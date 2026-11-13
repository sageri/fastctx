use fastctx::ToolContent;
use fastctx::read_tool::{ReadRequest, read_file};

fn assert_empty_file_complete(view: Option<&str>) {
    let file = tempfile::NamedTempFile::new().unwrap();
    let response = read_file(ReadRequest {
        file_path: Some(file.path().to_string_lossy().into_owned()),
        files: None,
        offset: None,
        limit: None,
        pages: None,
        pdf_mode: None,
        encoding: None,
        view: view.map(str::to_owned),
    });

    assert!(!response.is_error);
    assert_eq!(
        response.content,
        vec![ToolContent::Text(
            "Warning: the file exists but is empty.\n\n(Complete: empty file.)".to_string()
        )]
    );
}

#[test]
fn empty_file_default_view_reports_complete() {
    assert_empty_file_complete(None);
}

#[test]
fn empty_file_hex_view_reports_complete() {
    assert_empty_file_complete(Some("hex"));
}
