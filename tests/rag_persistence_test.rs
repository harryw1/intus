use intus::rag::RagSystem;
use intus::ollama::OllamaClient;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[tokio::test]
async fn test_rag_persistence() {
    let mock_server = MockServer::start().await;
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path().join("vectors.json");

    // Mock embedding generation
    let embedding_response = json!({
        "embedding": vec![0.5; 768]
    });
    Mock::given(method("POST"))
        .and(path("/api/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(embedding_response))
        .mount(&mock_server)
        .await;

    let client = OllamaClient::new(mock_server.uri());
    
    // 1. Create first system and add text
    {
        let vector_index = Arc::new(Mutex::new(None));
        let rag = RagSystem::new(
            client.clone(),
            "nomic-embed-text".to_string(),
            vector_index.clone(),
            Some(storage_path.clone()),
        );

        rag.add_text("Persistent Memory Test Content", None).await.expect("Add text failed");
        assert!(storage_path.exists(), "File should have been created");
    }

    // 2. Create a second system and load
    {
        let vector_index = Arc::new(Mutex::new(None));
        let rag = RagSystem::new(
            client.clone(),
            "nomic-embed-text".to_string(),
            vector_index.clone(),
            Some(storage_path.clone()),
        );

        rag.load().expect("Load failed");
        
        let guard = vector_index.lock().unwrap();
        let index = guard.as_ref().expect("Index should be loaded");
        assert!(!index.chunks.is_empty(), "Index should not be empty");
        assert!(index.chunks[0].content.contains("Persistent Memory"), "Content should match");
    }
}
