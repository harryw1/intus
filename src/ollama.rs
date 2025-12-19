use anyhow::Result;
use futures::stream::StreamExt;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OllamaClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Name of the tool when role is "tool" (for tool response messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessageRequest>,
    stream: bool,
    options: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChatMessageRequest {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Name of the tool when role is "tool" (for tool response messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessageResponse,
    #[allow(dead_code)]
    done: bool,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    #[allow(dead_code)]
    role: String,
    content: String,
    tool_calls: Option<Vec<ToolCall>>,
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
    #[allow(dead_code)]
    pub digest: Option<String>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
}

// Model Information Structures (from /api/show)
#[derive(Deserialize, Debug, Clone)]
pub struct ModelInfo {
    #[serde(default)]
    pub modelfile: String,
    #[serde(default)]
    pub parameters: String,
    #[serde(default)]
    pub template: String,
    pub details: Option<ModelDetails>,
    #[serde(default)]
    pub model_info: HashMap<String, Value>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

impl ModelInfo {
    /// Get the context length from model_info if available
    pub fn context_length(&self) -> Option<usize> {
        // Try common keys: "llama.context_length", "context_length", etc.
        if let Some(val) = self.model_info.get("llama.context_length") {
            return val.as_u64().map(|v| v as usize);
        }
        if let Some(val) = self.model_info.get("context_length") {
            return val.as_u64().map(|v| v as usize);
        }
        // Check in parameters string for "num_ctx"
        for line in self.parameters.lines() {
            if line.contains("num_ctx") {
                if let Some(val) = line.split_whitespace().last() {
                    if let Ok(n) = val.parse::<usize>() {
                        return Some(n);
                    }
                }
            }
        }
        None
    }
}

#[derive(Serialize)]
struct ShowModelRequest {
    name: String,
}

// Running Models Structures (from /api/ps)
#[derive(Deserialize, Debug, Clone)]
pub struct RunningModelsResponse {
    pub models: Vec<RunningModel>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RunningModel {
    pub name: String,
    pub model: String,
    pub size: u64,
    #[serde(default)]
    pub size_vram: u64,
    pub details: Option<ModelDetails>,
}

// Tooling Structures
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub type_: String,
    pub function: ToolFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub function: ToolCallFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatStreamEvent {
    Token(String),
    ToolCall(ToolCall),
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
        let response = self
            .client
            .get(&format!("{}/api/tags", self.base_url))
            .send()
            .await?
            .json::<ModelsResponse>()
            .await?;

        Ok(response.models.into_iter().map(|m| m.name).collect())
    }

    pub async fn delete_model(&self, name: &str) -> Result<()> {
        let request = DeleteModelRequest {
            name: name.to_string(),
        };
        let response = self
            .client
            .delete(&format!("{}/api/delete", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Delete failed: {}", response.status()));
        }
        Ok(())
    }

    /// Get detailed information about a model, including its context length
    pub async fn show_model(&self, name: &str) -> Result<ModelInfo> {
        let request = ShowModelRequest {
            name: name.to_string(),
        };
        let response = self
            .client
            .post(&format!("{}/api/show", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Show model failed: {}", response.status()));
        }

        let model_info = response.json::<ModelInfo>().await?;
        Ok(model_info)
    }

    /// List currently running models with their VRAM usage
    pub async fn list_running(&self) -> Result<Vec<RunningModel>> {
        let response = self
            .client
            .get(&format!("{}/api/ps", self.base_url))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "List running failed: {}",
                response.status()
            ));
        }

        let running = response.json::<RunningModelsResponse>().await?;
        Ok(running.models)
    }

    pub async fn pull_model(
        &self,
        name: &str,
    ) -> Result<impl Stream<Item = Result<PullModelResponse, anyhow::Error>>> {
        let request = PullModelRequest {
            name: name.to_string(),
            stream: true,
        };
        let response = self
            .client
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
                                if let Err(_) = tx.send(Ok(json)) {
                                    return;
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
        messages: Vec<ChatMessageRequest>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<impl Stream<Item = Result<ChatStreamEvent, anyhow::Error>>> {
        let mut options = HashMap::new();
        options.insert("num_predict".to_string(), serde_json::json!(-1)); // -1 = Infinite generation

        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: true,
            options: Some(options),
            tools,
        };

        let response = self
            .client
            .post(&format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Request failed with status: {}",
                response.status()
            ));
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

                        // Process buffer for newlines
                        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                            let line_bytes: Vec<u8> = buffer.drain(..=pos).collect();
                            let s = String::from_utf8_lossy(&line_bytes);
                            if let Ok(json) = serde_json::from_str::<ChatResponse>(&s) {
                                // Emit Token if content exists
                                if !json.message.content.is_empty() {
                                    if let Err(_) =
                                        tx.send(Ok(ChatStreamEvent::Token(json.message.content)))
                                    {
                                        return;
                                    }
                                }
                                // Emit ToolCall if exists
                                if let Some(calls) = json.message.tool_calls {
                                    for call in calls {
                                        if let Err(_) = tx.send(Ok(ChatStreamEvent::ToolCall(call)))
                                        {
                                            return;
                                        }
                                    }
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

            // Process any remaining buffer
            if !buffer.is_empty() {
                let s = String::from_utf8_lossy(&buffer);
                if let Ok(json) = serde_json::from_str::<ChatResponse>(&s) {
                    if !json.message.content.is_empty() {
                        let _ = tx.send(Ok(ChatStreamEvent::Token(json.message.content)));
                    }
                    if let Some(calls) = json.message.tool_calls {
                        for call in calls {
                            let _ = tx.send(Ok(ChatStreamEvent::ToolCall(call)));
                        }
                    }
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
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
        let chunk1 =
            json!({ "message": { "role": "assistant", "content": "Hello" }, "done": false });
        let chunk2 =
            json!({ "message": { "role": "assistant", "content": " World" }, "done": true });

        let body = format!("{}\n{}\n", chunk1.to_string(), chunk2.to_string());

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&mock_server)
            .await;

        let messages = vec![ChatMessageRequest {
            role: "user".to_string(),
            content: "Hi".to_string(),
            images: None,
            tool_calls: None,
            tool_name: None,
        }];
        let mut stream = client
            .chat("llama2", messages, None)
            .await
            .expect("Failed to start chat");

        let mut response = String::new();
        while let Some(item) = stream.next().await {
            let event = item.expect("Failed to get event");
            if let ChatStreamEvent::Token(token) = event {
                response.push_str(&token);
            }
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

        let messages = vec![ChatMessageRequest {
            role: "user".to_string(),
            content: "Hi".to_string(),
            images: None,
            tool_calls: None,
            tool_name: None,
        }];
        let result = client.chat("llama2", messages, None).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_show_model_returns_context_length() {
        let mock_server = MockServer::start().await;
        let client = OllamaClient::new(mock_server.uri());

        let mock_response = json!({
            "modelfile": "FROM llama2",
            "parameters": "num_ctx 8192",
            "template": "{{ .Prompt }}",
            "details": {
                "format": "gguf",
                "family": "llama",
                "parameter_size": "7B",
                "quantization_level": "Q4_0"
            },
            "model_info": {
                "llama.context_length": 8192,
                "general.architecture": "llama"
            }
        });

        Mock::given(method("POST"))
            .and(path("/api/show"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_response))
            .mount(&mock_server)
            .await;

        let model_info = client
            .show_model("llama2")
            .await
            .expect("Failed to get model info");

        // Test context_length extraction
        assert_eq!(model_info.context_length(), Some(8192));
        assert!(model_info.details.is_some());
        assert_eq!(
            model_info.details.as_ref().unwrap().family,
            Some("llama".to_string())
        );
    }

    #[tokio::test]
    async fn test_list_running_models() {
        let mock_server = MockServer::start().await;
        let client = OllamaClient::new(mock_server.uri());

        let mock_response = json!({
            "models": [
                {
                    "name": "llama2:latest",
                    "model": "llama2:latest",
                    "size": 5137025024_u64,
                    "size_vram": 4000000000_u64,
                    "details": {
                        "format": "gguf",
                        "family": "llama",
                        "parameter_size": "7B",
                        "quantization_level": "Q4_0"
                    }
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/api/ps"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_response))
            .mount(&mock_server)
            .await;

        let running = client
            .list_running()
            .await
            .expect("Failed to list running models");

        assert_eq!(running.len(), 1);
        assert_eq!(running[0].name, "llama2:latest");
        assert_eq!(running[0].size_vram, 4000000000);
    }
}
