use intus::tools::{Tool, ReadUrlTool};
use intus::rag::RagSystem;
use intus::ollama::OllamaClient;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use std::sync::{Arc, Mutex, OnceLock};

#[tokio::test]
async fn test_rag_end_to_end() {
    // 1. Setup Mock Server
    let mock_server = MockServer::start().await;
    
    // Mock page fetch
    Mock::given(method("GET"))
        .and(path("/test-page"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html><body><h1>RAG Test</h1><p>Important content to index.</p></body></html>"))
        .mount(&mock_server)
        .await;

    // Mock embedding generation
    // We return a fixed embedding so cosine similarity is high
    let embedding_response = json!({
        "embedding": vec![0.1; 768] // 768 dim vector
    });

    Mock::given(method("POST"))
        .and(path("/api/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(embedding_response))
        .mount(&mock_server)
        .await;

    // 2. Initialize Components
    let vector_index = Arc::new(Mutex::new(None));
    let client = OllamaClient::new(mock_server.uri(), "ollama".to_string(), "".to_string());
    let rag = Arc::new(RagSystem::new(
        client.clone(),
        "nomic-embed-text".to_string(),
        vector_index.clone(),
        None,
    ));

    let tool = ReadUrlTool {
        client: OnceLock::new(),
        rag: rag.clone(),
        browser: Arc::new(intus::tools::web::BrowserClient::new()),
    };

    // 3. Execute Tool (Fetch + Index)
    // We wrap in spawn_blocking because ReadUrlTool uses blocking reqwest
    let server_uri = mock_server.uri();
    let url = format!("{}/test-page", server_uri);
    
    let args = json!({ "url": url });
    
    let execution_result = tokio::task::spawn_blocking(move || {
        tool.execute(args)
    }).await.expect("Task failed").expect("Tool execution failed");

    println!("Tool Output: {}", execution_result);

    // 4. Verify Indexing
    {
        let guard = vector_index.lock().unwrap();
        let index = guard.as_ref().expect("Index should be initialized");
        assert!(!index.chunks.is_empty(), "Index should contain chunks");
        
        let found = index.chunks.iter().any(|c| c.content.contains("Important content"));
        assert!(found, "Should find a chunk with 'Important content'. Chunks: {:?}", index.chunks.iter().map(|c| &c.content).collect::<Vec<_>>());

    }

    // 5. Verify Search
    // We search for "content". Mock returns same embedding so it will match perfectly.
    // Since all chunks have same score (1.0), order might be strictly by insertion.
    // We request multiple results to find our target.
    let search_results = rag.search("content", 5, None).await.expect("Search failed");
    
    assert!(!search_results.is_empty(), "Search should return results");
    let found = search_results.iter().any(|s| s.contains("Important content"));
    assert!(found, "Search result should match one of the chunks. Results: {:?}", search_results);
}
