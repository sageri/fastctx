mod common;

use common::McpSession;
use serde_json::json;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn mcp_cancelled_replace_does_not_write_after_worker_is_released() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("target.txt");
    std::fs::write(&path, b"old\n").unwrap();
    let barrier = temp.path().join("barrier");
    std::fs::create_dir(&barrier).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fastctx"));
    command
        .arg("serve")
        .env("FASTCTX_TEST_TOOL_BARRIER_DIR", &barrier);
    let mut session = McpSession::start(command);
    let id = session.begin_call(
        "replace",
        json!({"path":path, "pattern":"old", "replacement":"new"}),
    );
    let deadline = Instant::now() + Duration::from_secs(4);
    while !barrier.join("participant-0").exists() {
        assert!(
            Instant::now() < deadline,
            "replace worker did not reach the barrier"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    session.notify(
        "notifications/cancelled",
        json!({"requestId":id, "reason":"issue 3 regression"}),
    );
    assert!(session.list_tools().iter().any(|name| name == "replace"));
    std::fs::write(barrier.join("participant-release"), b"").unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let actual = std::fs::read(&path).unwrap();
    eprintln!("input=old\\n; replace old -> new; cancelled at worker barrier; disk={actual:?}");
    assert_eq!(
        actual, b"old\n",
        "cancelled worker must not write after release"
    );
    assert!(session.close().success());
}
