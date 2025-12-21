use crate::ollama::{ToolDefinition, ToolFunction};
use anyhow::Result;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Trait defining a tool that can be invoked by the AI.
pub trait Tool: Send + Sync {
    /// Returns the unique name of the tool (e.g., "read_file").
    fn name(&self) -> &str;
    /// Returns a description of what the tool does.
    fn description(&self) -> &str;
    /// Returns the JSON Schema definition of the tool's parameters.
    fn parameters(&self) -> Value;
    
    /// Returns the full tool definition for the OpenAI/Ollama API.
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            type_: "function".to_string(),
            function: ToolFunction {
                name: self.name().to_string(),
                description: self.description().to_string(),
                parameters: self.parameters(),
            },
        }
    }

    /// Executes the tool with the provided arguments.
    ///
    /// # Arguments
    ///
    /// * `args` - A JSON Value containing the arguments passed by the model.
    fn execute(&self, args: Value) -> Result<String>;

    /// Whether this tool requires explicit user confirmation before execution.
    fn requires_confirmation(&self) -> bool {
        false
    }
}

/// Expands `~` at the start of a path to the user's home directory.
pub fn expand_path(path: &str) -> String {
    let home = BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    if path == "~" {
        home.map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    } else if path.starts_with("~/") {
        home.map(|p| format!("{}{}", p.to_string_lossy(), &path[1..]))
            .unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TextChunk {
    pub file_path: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub embedding: Vec<f64>,
    #[serde(default = "default_collection")]
    pub collection: String,
}

fn default_collection() -> String {
    "default".to_string()
}

#[derive(Serialize, Deserialize)]
pub struct VectorIndex {
    pub chunks: Vec<TextChunk>,
    pub indexed_at: std::time::SystemTime,
}

// Export modules
pub mod filesystem;
pub mod web;
pub mod system;
pub mod rag;

// Re-export tools for easier access
pub use filesystem::{ListDirectoryTool, GrepTool, CatTool, WriteFileTool, ReplaceTextTool, EditFileTool, DeleteFileTool};
pub use web::{WebSearchTool, ReadUrlTool};
pub use system::RunCommandTool;
pub use rag::{SemanticSearchTool, MemoryTool};

pub type StatusSender = tokio::sync::mpsc::UnboundedSender<String>;
