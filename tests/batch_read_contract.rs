use fastctx::ToolContent;
use fastctx::read_tool::{ReadRequest, read_file};
use serde_json::{Value, json};

fn read(request: Value) -> String {
    let response = read_file(serde_json::from_value::<ReadRequest>(request).unwrap());
    assert!(!response.is_error, "{response:?}");
    match response.content.as_slice() {
        [ToolContent::Text(output)] => output.clone(),
        other => panic!("unexpected response: {other:?}"),
    }
}

fn continuation(output: &str) -> Value {
    let json = output
        .split_once("Continue with files=")
        .and_then(|(_, tail)| tail.strip_suffix(".)"))
        .unwrap_or_else(|| panic!("missing continuation: {output}"));
    serde_json::from_str(json).unwrap()
}

fn assert_continuation_limits(inherit_limit: bool) {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a.txt");
    let b = temp.path().join("b.txt");
    std::fs::write(&a, "a1\na2\na3\na4\na5\n").unwrap();
    std::fs::write(&b, "b1\nb2\nb3\nb4\nb5\n").unwrap();
    let mut first = json!({"path": a, "encoding": "utf-8"});
    if !inherit_limit {
        first["limit"] = json!(2);
    }
    let mut request = json!({"files": [first, {"path": b, "limit": 1}]});
    if inherit_limit {
        request["limit"] = json!(2);
    }
    let output = read(request);
    eprintln!("first page: {output}");
    let pending = continuation(&output);
    assert_eq!(pending[0]["limit"], 2, "{output}");
    assert_eq!(pending[1]["limit"], 1, "{output}");
    assert_eq!(pending[0]["offset"], 3);
    assert_eq!(pending[1]["offset"], 2);
    assert_eq!(pending[0]["encoding"], "utf-8");

    let next = read(json!({"files": pending}));
    eprintln!("second page: {next}");
    let lines = next
        .lines()
        .filter(|line| line.contains('\t'))
        .collect::<Vec<_>>();
    assert_eq!(lines, ["3\ta3", "4\ta4", "2\tb2"]);
}

#[test]
fn batch_continuation_preserves_per_entry_limits() {
    assert_continuation_limits(false);
}

#[test]
fn batch_continuation_preserves_inherited_limit_and_entry_override() {
    assert_continuation_limits(true);
}

#[test]
fn batch_counts_completed_windows_even_when_files_have_more_lines() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a.txt");
    let b = temp.path().join("b.txt");
    std::fs::write(&a, "a1\na2\na3\n").unwrap();
    std::fs::write(&b, "b1\nb2\nb3\n").unwrap();
    let output = read(json!({"files": [
        {"path": a, "limit": 2},
        {"path": b, "limit": 1}
    ]}));
    eprintln!("completed windows: {output}");
    assert!(
        output.contains("(Partial: 2 of 2 entries processed."),
        "{output}"
    );
    let pending = continuation(&output);
    assert_eq!(pending.as_array().unwrap().len(), 2);
}

#[test]
fn batch_does_not_count_budget_truncated_or_unread_windows() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a.txt");
    let b = temp.path().join("b.txt");
    let c = temp.path().join("c.txt");
    std::fs::write(&a, "a1\na2\n").unwrap();
    std::fs::write(&b, format!("{}\n", "word ".repeat(80)).repeat(10_000)).unwrap();
    std::fs::write(&c, "c1\nc2\n").unwrap();
    let output = read(json!({"files": [
        {"path": a, "limit": 1},
        {"path": b, "limit": 10_000},
        {"path": c, "limit": 1}
    ]}));
    let terminal = output.lines().last().unwrap();
    eprintln!("budget-truncated windows: {terminal}");
    assert!(
        terminal.starts_with("(Partial: 1 of 3 entries processed."),
        "{terminal}"
    );
    let pending = continuation(&output);
    assert_eq!(pending[0]["offset"], 2);
    let next_offset = pending[1]["offset"].as_u64().unwrap();
    assert!(next_offset > 1 && next_offset < 10_001, "{terminal}");
    assert_eq!(pending[1]["limit"], 10_001 - next_offset);
    assert!(pending[2].get("offset").is_none());
}
