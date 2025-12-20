use ratatui::text::Text;

#[test]
fn test_markdown_styling() {
    let markdown = "This is **bold** and 1. numbered list";
    let text = tui_markdown::from_str(markdown);
    
    for (i, line) in text.lines.iter().enumerate() {
        println!("Line {}:", i);
        for span in &line.spans {
            println!("  Span: '{}', Style: {:?}", span.content, span.style);
        }
    }
}
