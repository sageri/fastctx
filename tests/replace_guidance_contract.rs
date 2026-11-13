use fastctx::edit::{ReplaceRequest, ReplaceService};
use fastctx::server::{FastCtxServer, ServerOptions};

#[test]
fn replacement_guidance_explains_deadline_and_safe_retry() {
    let tools = FastCtxServer::new().tool_definitions();
    let replace = tools.iter().find(|tool| tool.name == "replace").unwrap();
    let description = replace.description.as_deref().unwrap();
    for required in [
        "60 seconds including queue time",
        "cancellation",
        "atomic commit",
        "completed writes remain",
        "before retrying",
        "Serialize replacements",
    ] {
        assert!(
            description.contains(required),
            "missing {required}: {description}"
        );
    }
}

#[test]
fn replacement_guidance_distinguishes_json_newlines_from_literal_backslashes() {
    let tools = FastCtxServer::with_options(ServerOptions::all()).tool_definitions();
    let replace = tools.iter().find(|tool| tool.name == "replace").unwrap();
    let description = replace.input_schema["properties"]["replacement"]["description"]
        .as_str()
        .unwrap();
    eprintln!("replacement guidance: {description}");
    for required in [
        "actual newline",
        "not unescaped",
        r#""a\nb""#,
        r#""a\\nb""#,
        "$1/${name}",
        "$$",
    ] {
        assert!(
            description.contains(required),
            "missing {required}: {description}"
        );
    }
}

#[test]
fn replacement_json_newlines_literals_and_captures_preserve_written_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let service = ReplaceService::new();
    for (name, original, pattern, wire_replacement, expected) in [
        ("literal", "old\n", "old", r#""a\\nb""#, "a\\nb\n"),
        ("lf", "old\n", "old", r#""a\nb""#, "a\nb\n"),
        ("crlf", "old\r\n", "old", r#""a\nb""#, "a\r\nb\r\n"),
        (
            "capture",
            "old\n",
            "(old)",
            r#""$1\\nnext""#,
            "old\\nnext\n",
        ),
        (
            "named",
            "old\n",
            "(?P<word>old)",
            r#""${word}$$""#,
            "old$\n",
        ),
    ] {
        let path = temp.path().join(format!("{name}.txt"));
        std::fs::write(&path, original).unwrap();
        let replacement: String = serde_json::from_str(wire_replacement).unwrap();
        let request: ReplaceRequest = serde_json::from_value(serde_json::json!({
            "path": path, "pattern": pattern, "replacement": replacement
        }))
        .unwrap();
        let response = service.replace(request);
        assert!(!response.is_error, "{name}: {response:?}");
        let actual = std::fs::read(&path).unwrap();
        eprintln!("{name}: wire={wire_replacement}, bytes={actual:?}");
        assert_eq!(actual, expected.as_bytes(), "{name}");
    }
}
