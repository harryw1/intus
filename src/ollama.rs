use serde::{Deserialize, Serialize};
use reqwest::Client;
use anyhow::Result;
use futures::Stream;
use futures::stream::StreamExt;
use std::time::Duration;
use serde_json::Value;
use std::collections::HashMap;


#[derive(Debug, Clone)]
pub struct OllamaClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessageRequest>,
    stream: bool,
    options: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMessageRequest {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessageRequest,
    #[allow(dead_code)]
    done: bool,
}

#[derive(Deserialize)]
struct ModelsResponse {
    models: Vec<Model>,
}

#[derive(Deserialize)]
struct Model {
    name: String,
}

#[derive(Serialize)]
struct DeleteModelRequest {
    name: String,
}

#[derive(Serialize)]
struct PullModelRequest {
    name: String,
    stream: bool,
}

#[derive(Deserialize, Debug)]
pub struct PullModelResponse {
    pub status: String,
    pub digest: Option<String>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
}

impl OllamaClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(3600)) // 1 hour timeout
                .build()
                .unwrap_or_else(|_| Client::new()),
            base_url,
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        let response = self.client
            .get(&format!("{}/api/tags", self.base_url))
            .send()
            .await?
            .json::<ModelsResponse>()
            .await?;
        
        Ok(response.models.into_iter().map(|m| m.name).collect())
    }

    pub async fn delete_model(&self, name: &str) -> Result<()> {
        let request = DeleteModelRequest { name: name.to_string() };
        let response = self.client
            .delete(&format!("{}/api/delete", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Delete failed: {}", response.status()));
        }
        Ok(())
    }

    pub async fn pull_model(&self, name: &str) -> Result<impl Stream<Item = Result<PullModelResponse, anyhow::Error>>> {
        let request = PullModelRequest { name: name.to_string(), stream: true };
        let response = self.client
            .post(&format!("{}/api/pull", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
             return Err(anyhow::anyhow!("Pull failed: {}", response.status()));
        }

        let stream = response.bytes_stream();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            let mut stream = stream;
            let mut buffer = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                            let line_bytes: Vec<u8> = buffer.drain(..=pos).collect();
                            let s = String::from_utf8_lossy(&line_bytes);
                            if let Ok(json) = serde_json::from_str::<PullModelResponse>(&s) {
                                if let Err(_) = tx.send(Ok(json)) { return; }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("Chunk error: {}", e)));
                        return;
                    }
                }
            }
             if !buffer.is_empty() {
                let s = String::from_utf8_lossy(&buffer);
                if let Ok(json) = serde_json::from_str::<PullModelResponse>(&s) {
                    let _ = tx.send(Ok(json));
                }
            }
        });

        Ok(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
    }

    pub async fn chat(
        &self, 
        model: &str, 
        messages: Vec<ChatMessageRequest>
    ) -> Result<impl Stream<Item = Result<String, anyhow::Error>>> {
        let mut options = HashMap::new();
        options.insert("num_predict".to_string(), serde_json::json!(-1)); // -1 = Infinite generation

        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: true,
            options: Some(options),
        };


        let response = self.client
            .post(&format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Request failed with status: {}", response.status()));
        }

        let stream = response.bytes_stream();
        // We use a stream of bytes, but we need to buffer them to handle split lines.
        // Since we can't easily keep state in a map closure without a more complex stream combinator,
        // we'll use `async_stream` or just a custom stream impl if we could.
        // But simpler: let's map to Result<String> but handle buffering inside the consumer?
        // No, the signature returns `impl Stream<Item = Result<String>>`.
        // We can use `futures::stream::unfold` or `try_stream!` if available.
        // Given dependencies, let's use `async_stream::try_stream!` if we add the dependency, 
        // OR standard loop with `channel`.
        // Actually, let's use a channel to bridge the byte stream to a string stream with buffering.
        
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        
        tokio::spawn(async move {
            let mut stream = stream;
            let mut buffer = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        
                        // Process buffer for newlines
                        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                            let line_bytes: Vec<u8> = buffer.drain(..=pos).collect();
                            let s = String::from_utf8_lossy(&line_bytes);
                            if let Ok(json) = serde_json::from_str::<ChatResponse>(&s) {
                                if let Err(_) = tx.send(Ok(json.message.content)) {
                                    return; // Receiver dropped
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("Chunk error: {}", e)));
                        return;
                    }
                }
            }
            
            // Process any remaining buffer (e.g. if last chunk didn't have newline)
            if !buffer.is_empty() {
                let s = String::from_utf8_lossy(&buffer);
                if let Ok(json) = serde_json::from_str::<ChatResponse>(&s) {
                    let _ = tx.send(Ok(json.message.content));
                }
            }
        });

        // Convert receiver to stream
        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        Ok(stream)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use serde_json::json;

    #[test]
    fn test_client_creation() {
        let client = OllamaClient::new("http://localhost:11434".to_string());
        assert_eq!(client.base_url, "http://localhost:11434");
    }

    #[tokio::test]
    async fn test_list_models_success() {
        let mock_server = MockServer::start().await;
        let client = OllamaClient::new(mock_server.uri());

        let mock_response = json!({
            "models": [
                { "name": "llama2" },
                { "name": "mistral" }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_response))
            .mount(&mock_server)
            .await;

        let models = client.list_models().await.expect("Failed to list models");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0], "llama2");
        assert_eq!(models[1], "mistral");
    }

    #[tokio::test]
    async fn test_list_models_error() {
        let mock_server = MockServer::start().await;
        let client = OllamaClient::new(mock_server.uri());

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let result = client.list_models().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chat_stream_success() {
        let mock_server = MockServer::start().await;
        let client = OllamaClient::new(mock_server.uri());

        // We can simulate a stream by just returning a body, 
        // because reqwest stream will just read it. 
        // For distinct chunks, we'd need a more complex setup or just assume the client handles the stream of bytes correctly if they are valid JSONs.
        // Ollama sends JSON objects one by one.
        let chunk1 = json!({ "message": { "role": "assistant", "content": "Hello" }, "done": false });
        let chunk2 = json!({ "message": { "role": "assistant", "content": " World" }, "done": true });
        
        let body = format!("{}\n{}\n", chunk1.to_string(), chunk2.to_string());

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&mock_server)
            .await;

        let messages = vec![ChatMessageRequest { role: "user".to_string(), content: "Hi".to_string() }];
        let mut stream = client.chat("llama2", messages).await.expect("Failed to start chat");

        let mut response = String::new();
        while let Some(item) = stream.next().await {
            let token = item.expect("Failed to get token");
            response.push_str(&token);
        }

        assert_eq!(response, "Hello World");
    }


    #[tokio::test]
    async fn test_chat_error() {
        let mock_server = MockServer::start().await;
        let client = OllamaClient::new(mock_server.uri());

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let messages = vec![ChatMessageRequest { role: "user".to_string(), content: "Hi".to_string() }];
        let result = client.chat("llama2", messages).await;
        
        assert!(result.is_err());
    }
}