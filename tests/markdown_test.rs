//! Tests for markdown rendering functionality
//! 
//! These tests verify that markdown content is parsed and rendered correctly,
//! including code blocks, inline code, and line wrapping calculations.

use ratatui::text::Text;

/// Test helper: estimate wrapped height (mirrors the function in ui.rs)
fn estimate_wrapped_height(text: &Text, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    let mut height = 0;
    for line in &text.lines {
        let line_width = line.width() as u16;
        if line_width == 0 {
            height += 1;
        } else {
            height += (line_width as f32 / width as f32).ceil() as u16;
        }
    }
    height
}

#[test]
fn test_code_block_parsing() {
    let markdown = r###"Here is some code:

```rust
fn main() {
    println!("Hello, world!");
}
```

And some text after."###;

    let text = tui_markdown::from_str(markdown);

    // Verify the text was parsed (has content)
    assert!(
        !text.lines.is_empty(),
        "Code block markdown should parse to non-empty text"
    );

    // Verify we got multiple lines (code block should create multiple lines)
    assert!(
        text.lines.len() > 3,
        "Code block should create multiple lines"
    );
}

#[test]
fn test_inline_code_parsing() {
    let markdown = "Use the `cargo build` command to compile.";

    let text = tui_markdown::from_str(markdown);

    assert!(!text.lines.is_empty(), "Inline code markdown should parse");

    // The line should contain "cargo build" somewhere
    let content: String = text
        .lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();

    assert!(
        content.contains("cargo build"),
        "Inline code should preserve content"
    );
}

#[test]
fn test_file_path_in_code() {
    let markdown = "Edit the file at `/Users/test/Documents/code/project/src/main.rs`";

    let text = tui_markdown::from_str(markdown);

    let content: String = text
        .lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();

    // File path should remain intact in the parsed output
    assert!(
        content.contains("/Users/test/Documents/code/project/src/main.rs"),
        "File paths in backticks should be preserved"
    );
}

#[test]
fn test_wrapped_height_simple() {
    // Create a simple text with known width
    let text = Text::raw("Hello World"); // 11 chars

    // Width of 20 should fit on one line
    assert_eq!(estimate_wrapped_height(&text, 20), 1);

    // Width of 5 should require 3 lines (11/5 = 2.2, ceil = 3)
    assert_eq!(estimate_wrapped_height(&text, 5), 3);
}

#[test]
fn test_wrapped_height_multiline() {
    let text = Text::raw("Line1\nLine2\nLine3");

    // Each line should count separately, assuming width is big enough
    assert_eq!(estimate_wrapped_height(&text, 50), 3);
}

#[test]
fn test_wrapped_height_zero_width() {
    let text = Text::raw("Any content");

    // Zero width should return 0 (edge case protection)
    assert_eq!(estimate_wrapped_height(&text, 0), 0);
}

#[test]
fn test_wrapped_height_empty_lines() {
    let text = Text::raw("\n\n");

    // Empty lines should still count as 1 height each
    let height = estimate_wrapped_height(&text, 50);
    assert!(height >= 2, "Empty lines should contribute to height");
}

#[test]
fn test_long_code_line_wrapping() {
    let markdown = r###"```
let very_long_variable_name = some_function_with_long_name(parameter1, parameter2, parameter3);
```"###;

    let text = tui_markdown::from_str(markdown);

    // With narrow width, should wrap to multiple lines
    let narrow_height = estimate_wrapped_height(&text, 40);
    let wide_height = estimate_wrapped_height(&text, 200);

    assert!(
        narrow_height > wide_height,
        "Narrow width should require more lines than wide width"
    );
}

#[test]
fn test_bold_italic_parsing() {
    let markdown = "This is **bold** and *italic* text.";

    let text = tui_markdown::from_str(markdown);

    let content: String = text
        .lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();

    assert!(content.contains("bold"), "Bold text should be preserved");
    assert!(
        content.contains("italic"),
        "Italic text should be preserved"
    );
}

#[test]
fn test_list_parsing() {
    let markdown = r###"- Item 1
- Item 2
- Item 3"###;

    let text = tui_markdown::from_str(markdown);

    // Should have at least 3 lines for the list items
    assert!(text.lines.len() >= 3, "List should create multiple lines");
}

#[test]
fn test_heading_parsing() {
    let markdown = "# Heading 1\n\nSome content";

    let text = tui_markdown::from_str(markdown);

    let content: String = text
        .lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();

    assert!(content.contains("Heading 1"), "Heading should be preserved");
}

#[test]
fn test_empty_content() {
    let markdown = "";

    let text = tui_markdown::from_str(markdown);

    // Should not panic, should return valid (possibly empty) text
    let _height = estimate_wrapped_height(&text, 50);
    // If we got here without panic, the test passes
}