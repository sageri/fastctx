//! Replacement lifecycle checks at the async admission boundary.

use crate::operation::RequestWorkGuard;
use crate::server_support::run_blocking_replace;
use rmcp::model::RequestId;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn queued_replace_deadline_or_cancellation_never_starts_writing() {
    for timed_out in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("target.txt");
        std::fs::write(&path, b"old\n").unwrap();
        let permits = Arc::new(Semaphore::new(0));
        let cancellation = CancellationToken::new();
        let (guard, operation) = RequestWorkGuard::new(RequestId::Number(10), cancellation.clone());
        let operation = operation.with_deadline(
            Instant::now()
                + if timed_out {
                    Duration::from_millis(20)
                } else {
                    Duration::from_secs(60)
                },
        );
        let write_path = path.clone();
        let task = tokio::spawn(run_blocking_replace(
            crate::session::SessionContext::library_default(),
            Arc::clone(&permits),
            guard,
            operation,
            || None,
            move |_| {
                std::fs::write(write_path, b"new\n").unwrap();
                crate::ToolResponse::text("unexpected write")
            },
        ));
        tokio::task::yield_now().await;
        if !timed_out {
            cancellation.cancel();
        }
        let response = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(
            serialized.contains(if timed_out {
                "timed out after 60 seconds"
            } else {
                "request cancelled"
            }),
            "{serialized}"
        );
        assert!(serialized.contains("0 replacements written in 0 files"));
        permits.add_permits(1);
        assert_eq!(std::fs::read(path).unwrap(), b"old\n");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_replace_future_cancels_its_blocking_sibling() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("target.txt");
    std::fs::write(&path, b"old\n").unwrap();
    let permits = Arc::new(Semaphore::new(1));
    let (guard, operation) = RequestWorkGuard::new(RequestId::Number(11), CancellationToken::new());
    let operation = operation.with_deadline(Instant::now() + Duration::from_secs(60));
    let (started_tx, started_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);
    let (done_tx, done_rx) = mpsc::sync_channel(0);
    let write_path = path.clone();
    let task = tokio::spawn(run_blocking_replace(
        crate::session::SessionContext::library_default(),
        Arc::clone(&permits),
        guard,
        operation,
        || None,
        move |operation| {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let request = serde_json::from_value(serde_json::json!({
                "path":write_path, "pattern":"old", "replacement":"new"
            }))
            .unwrap();
            let response =
                crate::edit::ReplaceService::new().replace_with_operation(request, 10, &operation);
            done_tx.send(response.clone()).unwrap();
            response
        },
    ));
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    release_tx.send(()).unwrap();
    let response = done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(format!("{response:?}").contains("request cancelled"));
    assert_eq!(std::fs::read(path).unwrap(), b"old\n");
    tokio::time::timeout(Duration::from_secs(2), async {
        while permits.available_permits() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
