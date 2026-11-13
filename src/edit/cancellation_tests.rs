use super::{ReplaceRequest, ReplaceService, control, document, locks};
use crate::operation::{RequestWorkGuard, TestStage};
use rmcp::model::RequestId;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

fn request(path: &std::path::Path) -> ReplaceRequest {
    serde_json::from_value(serde_json::json!({
        "path": path, "pattern": "old", "replacement": "new"
    }))
    .unwrap()
}

fn text(response: &crate::ToolResponse) -> &str {
    match &response.content[0] {
        crate::ToolContent::Text(text) => text,
        _ => panic!("expected text"),
    }
}

#[test]
fn cancellation_during_planning_leaves_all_files_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("target.txt");
    std::fs::write(&path, b"old\n").unwrap();
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    let (_guard, operation) = RequestWorkGuard::new_with_hook(
        RequestId::Number(1),
        cancellation,
        Arc::new(move |stage| {
            if stage == TestStage::CandidateValidation {
                cancel.cancel();
            }
        }),
    );
    let response = ReplaceService::new().replace_with_operation(request(&path), 10, &operation);
    assert_eq!(std::fs::read(&path).unwrap(), b"old\n");
    assert!(
        text(&response).contains("request cancelled; 0 replacements written in 0 files"),
        "{response:?}"
    );
}

#[test]
fn cancellation_after_one_commit_preserves_that_commit_and_reports_exact_totals() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.txt"] {
        std::fs::write(temp.path().join(name), b"old\n").unwrap();
    }
    let cancellation = CancellationToken::new();
    let (_guard, operation) = RequestWorkGuard::new(RequestId::Number(2), cancellation.clone());
    document::set_before_commit_hook(move |_| cancellation.cancel());
    let response =
        ReplaceService::new().replace_with_operation(request(temp.path()), 10, &operation);
    let actual = ["a.txt", "b.txt"].map(|name| std::fs::read(temp.path().join(name)).unwrap());
    eprintln!("cancel during first atomic commit; disk={actual:?}; response={response:?}");
    assert_eq!(
        actual
            .iter()
            .filter(|bytes| bytes.as_slice() == b"new\n")
            .count(),
        1
    );
    assert_eq!(
        actual
            .iter()
            .filter(|bytes| bytes.as_slice() == b"old\n")
            .count(),
        1
    );
    assert!(text(&response).contains("request cancelled; 1 replacement written in 1 file"));
    assert!(!text(&response).contains("Complete:"));
}

#[test]
fn expired_deadline_before_planning_writes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("target.txt");
    std::fs::write(&path, b"old\n").unwrap();
    let (_guard, operation) = RequestWorkGuard::new(RequestId::Number(3), CancellationToken::new());
    let operation = operation.with_deadline(Instant::now() - Duration::from_secs(1));
    let response = ReplaceService::new().replace_with_operation(request(&path), 10, &operation);
    assert_eq!(std::fs::read(&path).unwrap(), b"old\n");
    assert!(
        text(&response).contains("timed out after 60 seconds; 0 replacements written in 0 files")
    );
    assert_eq!(control::REPLACE_TIMEOUT, Duration::from_secs(60));
}

#[test]
fn cancelled_cross_process_lock_wait_returns_before_holder_releases() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("target.txt");
    std::fs::write(&path, b"old\n").unwrap();
    let identity = locks::PathIdentity::for_name(&path).unwrap();
    let held = locks::FilePathLock::acquire(&identity, &path).unwrap();
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    let (_guard, operation) = RequestWorkGuard::new_with_hook(
        RequestId::Number(4),
        cancellation,
        Arc::new(move |stage| {
            if stage == TestStage::ReplaceLockWait {
                cancel.cancel();
            }
        }),
    );
    let response = ReplaceService::new().replace_with_operation(request(&path), 10, &operation);
    assert!(text(&response).contains("request cancelled; 0 replacements written in 0 files"));
    drop(held);
    assert_eq!(std::fs::read(&path).unwrap(), b"old\n");
}

#[test]
fn expired_in_process_lock_wait_does_not_resume_when_released() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("target.txt");
    std::fs::write(&path, b"old\n").unwrap();
    let service = ReplaceService::new();
    let identity = locks::PathIdentity::for_name(&path).unwrap();
    let lock = service.path_locks.for_identity(&identity);
    let held = lock.lock().unwrap();
    let _ = crate::budget::estimate_tokens("warm the tokenizer before timing lock contention");
    let waited = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let saw_wait = Arc::clone(&waited);
    let (_guard, operation) = RequestWorkGuard::new_with_hook(
        RequestId::Number(5),
        CancellationToken::new(),
        Arc::new(move |stage| {
            if stage == TestStage::ReplaceLockWait {
                saw_wait.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }),
    );
    let operation = operation.with_deadline(Instant::now() + Duration::from_millis(200));
    let response = service.replace_with_operation(request(&path), 10, &operation);
    assert!(waited.load(std::sync::atomic::Ordering::Relaxed));
    assert!(
        text(&response).contains("timed out after 60 seconds; 0 replacements written in 0 files")
    );
    drop(held);
    assert_eq!(std::fs::read(&path).unwrap(), b"old\n");
}

#[test]
fn cancellation_during_directory_walk_never_reaches_writes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("target.txt");
    std::fs::write(&path, b"old\n").unwrap();
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    let (_guard, operation) = RequestWorkGuard::new_with_hook(
        RequestId::Number(6),
        cancellation,
        Arc::new(move |stage| {
            if stage == TestStage::TraversalEntry {
                cancel.cancel();
            }
        }),
    );
    let response =
        ReplaceService::new().replace_with_operation(request(temp.path()), 10, &operation);
    assert_eq!(std::fs::read(path).unwrap(), b"old\n");
    assert!(text(&response).contains("request cancelled; 0 replacements written in 0 files"));
}
