use intus::tools::{Tool, WebSearchTool};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_web_search_url_fetching() {
    // Start a mock server
    let mock_server = MockServer::start().await;

    // Configure the mock to return HTML when accessed
    Mock::given(method("GET"))
        .and(path("/test-page"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html><body><h1>Hello Fetched World</h1><p>Some content</p></body></html>"))
        .mount(&mock_server)
        .await;

    let server_uri = mock_server.uri();
    let url = format!("{}/test-page", server_uri);

    // Initial dummy tool
    // We wrap execution in spawn_blocking because WebSearchTool uses reqwest::blocking
    // and we are in an async runtime.
    let result = tokio::task::spawn_blocking(move || {
        let tool = WebSearchTool {
            searxng_url: "http://localhost:8080".to_string(),
            client: std::sync::OnceLock::new(),
            rag: std::sync::Arc::new(intus::rag::RagSystem::new(
                intus::ollama::OllamaClient::new("http://localhost".to_string()),
                "dummy".to_string(),
                std::sync::Arc::new(std::sync::Mutex::new(None)),
                None,
            )),
        };

        let args = json!({
            "url": url
        });

        tool.execute(args)
    }).await.expect("Task failed").expect("Tool execution failed");

    // html2text should convert h1 to # or similar bold text, depending on width
    println!("DEBUG: Tool Output: {}", result);
    assert!(result.contains("Hello Fetched World"));
    assert!(result.contains("Some content"));
}
