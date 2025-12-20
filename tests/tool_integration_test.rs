use intus::tools::{ReplaceTextTool, Tool, WriteFileTool};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_integration_workflow() {
    // 1. Setup
    let dir = tempdir().expect("failed to create temp dir");
    let main_rs = dir.path().join("main.rs");

    let write_tool = WriteFileTool {
        ignored_patterns: vec![],
    };
    let replace_tool = ReplaceTextTool {
        ignored_patterns: vec![],
    };

    // 2. AI "Writes" a file
    let write_args = json!({
        "path": main_rs.to_str().unwrap(),
        "content": "fn main() {\n    println!(\"Hello World\");\n}"
    });
    let result = write_tool.execute(write_args).expect("Write failed");
    assert!(result.contains("Successfully wrote"));

    // Verify content
    let content = fs::read_to_string(&main_rs).expect("Failed to read");
    assert_eq!(content, "fn main() {\n    println!(\"Hello World\");\n}");

    // 3. AI "Refactors" the file (Replace Text)
    let replace_args = json!({
        "path": main_rs.to_str().unwrap(),
        "old_text": "println!(\"Hello World\");",
        "new_text": "println!(\"Hello Integration\");"
    });
    let result = replace_tool.execute(replace_args).expect("Replace failed");
    assert!(result.contains("Successfully modified"));

    // Verify change
    let new_content = fs::read_to_string(&main_rs).expect("Failed to read new content");
    assert_eq!(new_content, "fn main() {\n    println!(\"Hello Integration\");\n}");
}
