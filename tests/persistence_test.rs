use ollama_tui::app::{Action, App};
use ollama_tui::config::Config;
use std::fs;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_persistence_lifecycle() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("history.json");

    let (tx, _rx) = mpsc::unbounded_channel();
    let config = Config {
        ollama_url: "dummy".to_string(),
        context_token_limit: 100,
        system_prompt: "Sys".to_string(),
        ignored_patterns: vec![],
        auto_context: true,
        summarization_enabled: true,
        summarization_threshold: 0.8,
        searxng_url: "http://localhost:8080".to_string(),
        embedding_model: "nomic-embed-text".to_string(),
    };

    // 2. User sends message
    let mut app = App::new(tx.clone(), config.clone(), true, Some(file_path.clone()));
    app.models = vec!["test".to_string()]; // Mock models for RequestAiResponse
    assert!(app.messages.is_empty());

    app.update(Action::AddUserMessage("Hello Persistence".to_string()))
        .await;

    // Verify file written
    let content = fs::read_to_string(&file_path).expect("File should exist");
    assert!(content.contains("Hello Persistence"));

    // 3. AI starts response (adds placeholder)
    app.update(Action::RequestAiResponse).await;

    // 4. AI streams response
    app.update(Action::AddAiToken("Thinking...".to_string()))
        .await;

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

#[tokio::test]
async fn test_prepare_quit_saves_session() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("sessions").join("default.json");

    // Create parent dir for session to ensure it exists
    fs::create_dir_all(dir.path().join("sessions")).unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let config = Config {
        ollama_url: "dummy".to_string(),
        context_token_limit: 100,
        system_prompt: "Sys".to_string(),
        ignored_patterns: vec![],
        auto_context: true,
        summarization_enabled: true,
        summarization_threshold: 0.8,
        searxng_url: "http://localhost:8080".to_string(),
        embedding_model: "nomic-embed-text".to_string(),
    };

    let mut app = App::new(tx.clone(), config.clone(), false, Some(file_path.clone()));
    app.models = vec!["test".to_string()];

    // Add a message (which triggers its own save, but we're testing PrepareQuit specifically)
    app.update(Action::AddUserMessage("Quit Test".to_string()))
        .await;

    // Clear the file to verify PrepareQuit saves
    fs::write(&file_path, "[]").unwrap();

    // PrepareQuit should save and send Quit
    app.update(Action::PrepareQuit).await;

    // Verify Quit action was sent
    let action = rx.recv().await.unwrap();
    // First we get RequestAiResponse from AddUserMessage
    assert_eq!(action, Action::RequestAiResponse);
    let action = rx.recv().await.unwrap();
    assert_eq!(action, Action::Quit);

    // Verify file was saved by PrepareQuit
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains("Quit Test"),
        "PrepareQuit should save the session"
    );
}

#[tokio::test]
async fn test_atomic_write_creates_backup() {
    let dir = tempdir().unwrap();
    let sessions_dir = dir.path().join("sessions");
    fs::create_dir_all(&sessions_dir).unwrap();
    let file_path = sessions_dir.join("default.json");

    let (tx, _rx) = mpsc::unbounded_channel();
    let config = Config {
        ollama_url: "dummy".to_string(),
        context_token_limit: 100,
        system_prompt: "Sys".to_string(),
        ignored_patterns: vec![],
        auto_context: true,
        summarization_enabled: true,
        summarization_threshold: 0.8,
        searxng_url: "http://localhost:8080".to_string(),
        embedding_model: "nomic-embed-text".to_string(),
    };

    // Create initial session with content
    let mut app = App::new(tx.clone(), config.clone(), false, Some(file_path.clone()));
    app.models = vec!["test".to_string()];
    app.update(Action::AddUserMessage("First Message".to_string()))
        .await;

    // Verify file exists
    assert!(file_path.exists());

    // Add another message which should create backup of previous state
    app.update(Action::AiResponseComplete).await; // Complete response to trigger save

    // Backup should exist after second save
    let backup_path = file_path.with_extension("json.bak");
    assert!(backup_path.exists(), "Backup file should be created");

    // Backup should contain original content
    let backup_content = fs::read_to_string(&backup_path).unwrap();
    assert!(
        backup_content.contains("First Message"),
        "Backup should contain original data"
    );
}
