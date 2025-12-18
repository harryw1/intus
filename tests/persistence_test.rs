use ollama_tui::app::{App, Action};
use ollama_tui::config::Config;
use tokio::sync::mpsc;
use tempfile::tempdir;
use std::fs;

#[tokio::test]
async fn test_persistence_lifecycle() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("history.json");
    
    let (tx, _rx) = mpsc::unbounded_channel();
    let config = Config { 
        ollama_url: "dummy".to_string(), 
        context_token_limit: 100,
        system_prompt: "Sys".to_string() 
    };

    // 1. Start fresh
    let mut app = App::new(tx.clone(), config.clone(), true, Some(file_path.clone()));
    assert!(app.messages.is_empty());

    // 2. User sends message
    app.update(Action::AddUserMessage("Hello Persistence".to_string())).await;
    
    // Verify file written
    let content = fs::read_to_string(&file_path).expect("File should exist");
    assert!(content.contains("Hello Persistence"));

    // 3. AI streams response
    app.update(Action::AddAiToken("Thinking...".to_string())).await;
    
    // CURRENT BEHAVIOR: Partial stream NOT saved to disk yet
    // If we want to fix this, we'd expect it here. For now, let's assert current behavior 
    // or fix it. The user reported missing text, so we WANT to save here.
    
    // 4. Finish response
    app.update(Action::AiResponseComplete).await;
    let content_final = fs::read_to_string(&file_path).unwrap();
    assert!(content_final.contains("Thinking..."));

    // 5. Reload
    let app2 = App::new(tx.clone(), config.clone(), true, Some(file_path.clone()));
    assert_eq!(app2.messages.len(), 2);
    assert_eq!(app2.messages[1].content, "Thinking...");
}
