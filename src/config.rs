use anyhow::Result;
use directories::{BaseDirs, ProjectDirs};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    #[serde(default = "default_context_token_limit")]
    pub context_token_limit: usize,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default = "default_ignored_patterns")]
    pub ignored_patterns: Vec<String>,
    /// Whether to automatically detect optimal context size based on system resources
    #[serde(default = "default_auto_context")]
    pub auto_context: bool,
    /// Whether to enable automatic conversation summarization
    #[serde(default = "default_summarization_enabled")]
    pub summarization_enabled: bool,
    /// Threshold (0.0-1.0) at which to trigger summarization
    #[serde(default = "default_summarization_threshold")]
    pub summarization_threshold: f32,
    #[serde(default = "default_searxng_url")]
    pub searxng_url: String,
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_max_consecutive_tool_calls")]
    pub max_consecutive_tool_calls: usize,
    #[serde(default = "default_max_history_messages")]
    pub max_history_messages: usize,
    /// Optional location string (e.g. "New York, USA") for context-aware responses
    pub location: Option<String>,
    /// Whether to enable automatic IP-based geolocation (privacy warning: exposes IP to third-party)
    #[serde(default = "default_enable_geolocation")]
    pub enable_geolocation: bool,
    /// Map of named knowledge bases to their directory paths (e.g. "work" -> "~/Documents/Work")
    #[serde(default = "default_knowledge_bases")]
    pub knowledge_bases: HashMap<String, String>,
    #[serde(default = "default_enable_session_autonaming")]
    pub enable_session_autonaming: bool,
}

fn default_enable_session_autonaming() -> bool {
    true
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_enable_geolocation() -> bool {
    false
}

fn default_max_consecutive_tool_calls() -> usize {
    3
}

fn default_max_history_messages() -> usize {
    1000
}

fn default_searxng_url() -> String {
    "http://localhost:8080".to_string()
}

fn default_embedding_model() -> String {
    "nomic-embed-text".to_string()
}

fn default_context_token_limit() -> usize {
    131072
}

fn default_knowledge_bases() -> HashMap<String, String> {
    HashMap::new()
}

fn default_system_prompt() -> String {
    r#"You are `tenere`, a highly capable AI assistant that functions as a proactive System Sidecar. You have direct access to the file system, web search, and local knowledge bases.

## CORE INSTRUCTIONS
1. **PROACTIVE CLARIFICATION**: If a user's request is vague (e.g., "Search my notes" or "Find that file"), **DO NOT GUESS**. Ask clarifying questions: "Which notes? Work or Personal?" or "What topic are you looking for?".
2. **ACTION FIRST**: When the task is clear, use tools IMMEDIATELY. Do not plan out loud unless the task is complex.
3. **NO HALLUCINATIONS**: ONLY use the tools listed below. Do not invent tools.
4. **VERIFY**: Check file contents (`read_file`) before editing.
5. **ARGUMENTS**: Provide exact arguments. Do not use placeholders.

## KNOWLEDGE BASES & SEARCH
- You can access named knowledge bases (directories) via `semantic_search`.
- If the user asks to search "work notes" or "personal docs", check if a corresponding knowledge base exists.
- **Always index** a directory before searching it if it's new to the conversation.

## AVAILABLE TOOLS
- `web_search(query, category="general"|"news"|"it", domain=null)`: Search the web.
  * Use this for: "Check weather", "News", "Find docs", "General knowledge".
- `read_url(url)`: Read the content of a specific URL.
- `remember(fact)`: Save important facts to long-term memory.
  * Use for: User preferences, project ports, specific file paths they mention often.

- `grep_files(query, path=".")`: Search for string content in files.
- `read_file(path)`: Read exact file content.
- `list_directory(path)`: List files in a folder.
- `run_command(command)`: Execute shell commands (git, ls, cargo, mkdir, etc).
- `write_file(path, content)`: Create or overwrite a file.
- `edit_file(path, start_line, end_line, content)`: Replace lines in a file. **PREFERRED for code edits** as it avoids whitespace issues.
- `replace_text(path, old_content, new_content)`: Replace a precise string block. Use only for simple, unique text.
- `semantic_search(query, index_path=null, refresh=false)`: Search local knowledge.
  * **USE THIS for conceptual questions**: "What notes do I have on X?", "Recall Y".
  * **index_path**: Can be a literal path ("~/Documents") OR a knowledge base name ("work").
  * **Auto-Ingestion**: Remembers what you read and search.

## RESPONSE FORMATTING
- **Markdown Only**: Use headers, lists, code blocks.
- **Concise**: Be helpful and direct.
- **Path Handling**: Use absolute paths returned by tools.

## COMMON PATTERNS
- **Vague Request** -> **Ask Question**: "Search notes" -> "Which notes folder should I check?"
- **Research** -> `web_search` -> `read_url` -> Synthesize.
- **Knowledge Retrieval**:
  1. `semantic_search(query, index_path="work")` (if "work" is a known base).
  2. `read_file(path)` to verify details.
"#
        .to_string()
}

fn default_ignored_patterns() -> Vec<String> {
    vec![
        "target".to_string(),
        ".git".to_string(),
        "node_modules".to_string(),
        ".env".to_string(),
        ".DS_Store".to_string(),
        "Library".to_string(),
        "Music".to_string(),
        "Movies".to_string(),
        "Pictures".to_string(),
        "dist".to_string(),
        "build".to_string(),
    ]
}

fn default_auto_context() -> bool {
    true
}

fn default_summarization_enabled() -> bool {
    true
}

fn default_summarization_threshold() -> f32 {
    0.8
}

impl Config {
    /// Loads the configuration from the standard config directory.
    ///
    /// On macOS/Linux, this defaults to `~/.config/tenere/config.toml`.
    /// On Windows, it uses the roaming app data directory.
    ///
    /// If the file does not exist, default settings are returned.
    pub fn load() -> Result<Self> {
        let config_path = if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            // Force ~/.config/tenere/config.toml for macOS and Linux
            BaseDirs::new().map(|base| {
                base.home_dir()
                    .join(".config")
                    .join("tenere")
                    .join("config.toml")
            })
        } else {
            // Fallback to standard directories for other OSs (like Windows)
            ProjectDirs::from("com", "tenere", "tenere")
                .map(|proj_dirs| proj_dirs.config_dir().join("config.toml"))
        };

        if let Some(path) = config_path {
            if path.exists() {
                let contents = fs::read_to_string(&path)?;
                let config: Config = toml::from_str(&contents)?;
                return Ok(config);
            }
        }

        // Return default if file doesn't exist or directories fails
        Ok(Config {
            ollama_url: default_ollama_url(),
            context_token_limit: default_context_token_limit(),
            system_prompt: default_system_prompt(),
            ignored_patterns: default_ignored_patterns(),
            auto_context: default_auto_context(),
            summarization_enabled: default_summarization_enabled(),
            summarization_threshold: default_summarization_threshold(),
            searxng_url: default_searxng_url(),
            embedding_model: default_embedding_model(),
            max_consecutive_tool_calls: default_max_consecutive_tool_calls(),
            max_history_messages: default_max_history_messages(),
            location: None,
            enable_geolocation: default_enable_geolocation(),
            knowledge_bases: default_knowledge_bases(),
            enable_session_autonaming: default_enable_session_autonaming(),
        })
    }

    /// Returns the path to the application's configuration directory.
    pub fn get_config_dir(&self) -> Option<std::path::PathBuf> {
        if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            BaseDirs::new().map(|base| {
                base.home_dir()
                    .join(".config")
                    .join("tenere")
            })
        } else {
            ProjectDirs::from("com", "tenere", "tenere")
                .map(|proj_dirs| proj_dirs.config_dir().to_path_buf())
        }
    }

    pub fn new_test_config() -> Self {
        Self {
            ollama_url: default_ollama_url(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: default_searxng_url(),
            embedding_model: default_embedding_model(),
            max_consecutive_tool_calls: default_max_consecutive_tool_calls(),
            max_history_messages: default_max_history_messages(),
            location: None,
            enable_geolocation: false,
            knowledge_bases: HashMap::new(),
            enable_session_autonaming: false,
        }
    }
}
