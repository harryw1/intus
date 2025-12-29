use anyhow::Result;
use directories::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tracing::{info, warn};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Base URL for the Ollama API (e.g., "http://localhost:11434").
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    
    /// Type of API to use ("ollama" or "openai").
    #[serde(default = "default_api_type")]
    pub api_type: String, 
    
    /// Optional API Key for OpenAI-compatible endpoints.
    #[serde(default)]
    pub api_key: String,
    
    /// Maximum number of tokens for the context window.
    #[serde(default = "default_context_token_limit")]
    pub context_token_limit: usize,
    
    /// The default system prompt to use for new sessions.
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    
    /// List of file/directory patterns to ignore in file operations.
    #[serde(default = "default_ignored_patterns")]
    pub ignored_patterns: Vec<String>,
    
    /// Whether to automatically detect optimal context size based on system resources.
    #[serde(default = "default_auto_context")]
    pub auto_context: bool,
    
    /// Whether to enable automatic conversation summarization.
    #[serde(default = "default_summarization_enabled")]
    pub summarization_enabled: bool,
    
    /// Threshold (0.0-1.0) at which to trigger summarization.
    #[serde(default = "default_summarization_threshold")]
    pub summarization_threshold: f32,
    
    /// URL for the SearXNG instance used for web searches.
    #[serde(default = "default_searxng_url")]
    pub searxng_url: String,
    
    /// Name of the model to use for generating embeddings.
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    
    /// Maximum number of consecutive tool calls allowed before stopping.
    #[serde(default = "default_max_consecutive_tool_calls")]
    pub max_consecutive_tool_calls: usize,
    
    /// Maximum number of messages to keep in the conversation history.
    #[serde(default = "default_max_history_messages")]
    pub max_history_messages: usize,
    
    /// Optional location string (e.g. "New York, USA") for context-aware responses.
    pub location: Option<String>,
    
    /// Whether to enable automatic IP-based geolocation (privacy warning: exposes IP to third-party).
    #[serde(default = "default_enable_geolocation")]
    pub enable_geolocation: bool,
    
    /// Map of named knowledge bases to their directory paths (e.g. "work" -> "~/Documents/Work").
    #[serde(default = "default_knowledge_bases")]
    pub knowledge_bases: HashMap<String, String>,
    
    /// Whether to enable automatic session renaming based on conversation content.
    #[serde(default = "default_enable_session_autonaming")]
    pub enable_session_autonaming: bool,
}

fn default_enable_session_autonaming() -> bool {
    true
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_api_type() -> String {
    "ollama".to_string()
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
    r#"You are `intus`, a highly capable AI assistant that functions as a proactive System Sidecar. You have direct access to the file system, web search, and local knowledge bases.

## CORE INSTRUCTIONS
1. **ENVIRONMENT AWARENESS**: On your first turn in a new session, or if you are unsure about the user's setup, proactively check for available tools using `run_command` (e.g., `which brew`, `which uv`, `which cargo`). This helps you use the user's preferred workflows (e.g., using `uv` for Python instead of plain `python3`).
2. **PROACTIVE CLARIFICATION**: If a user's request is vague (e.g., "Search my notes" or "Find that file"), **DO NOT GUESS**. Ask clarifying questions: "Which notes? Work or Personal?" or "What topic are you looking for?".
3. **ACTION FIRST**: When the task is clear, use tools IMMEDIATELY. Do not plan out loud unless the task is complex.
4. **NO HALLUCINATIONS**: ONLY use the tools listed below. Do not invent tools.
5. **REAL-TIME DATA**: You do not have internal knowledge of current events, weather, or time-sensitive data. You MUST use `web_search` for these queries. **DO NOT GUESS**. If you are asked about the weather, news, or recent updates, you MUST use a tool.
6. **INTERNAL MONOLOGUE**: Before taking complex actions or answering difficult questions, use `<thought>` tags to plan your approach or analyze the problem. For example: `<thought>I need to check the file structure first.</thought>`. The user receives this as a distinct UI element.
7. **VERIFY**: Check file contents (`read_file`) before editing.
8. **ARGUMENTS**: Provide exact arguments. Do not use placeholders.
9. **AVOID LOOPS**: If a tool fails or returns the same results, try a DIFFERENT strategy or ASK the user. Do not repeatedly run the same search.

## KNOWLEDGE BASES & SEARCH
- You can access named knowledge bases (directories) via `semantic_search`.
- If the user asks to search "work notes" or "personal docs", check if a corresponding knowledge base exists.
- **Always index** a directory before searching it if it's new to the conversation.

## AVAILABLE TOOLS
- `web_search(query, category="general"|"news"|"it", domain=null)`: Search the web.
  * Use this for: "Check weather", "News", "Find docs", "General knowledge".
  * **IMPORTANT**: For "latest news" or time-sensitive queries, INCLUDE the current date (from [System Context]) in your query string (e.g. "SpaceX launch Dec 20 2024").
- `read_url(url)`: Read the content of a specific URL. Required after `web_search` to get page details.
- `remember(fact)`: Save important facts to long-term memory.
  * Use for: User preferences, project ports, specific file paths they mention often.

- `grep_files(query, path=".")`: Search for string content in files.
- `read_file(path)`: Read exact file content.
- `list_directory(path)`: List files in a folder.
- `run_command(command)`: Execute shell commands (git, cargo, curl, jq, python3, etc).
  * **CURL/WGET**: When using `curl` or `wget` to fetch external data, **ALWAYS** include a browser-like User-Agent header (e.g., `-A "Mozilla/5.0..."`) and common headers to avoid being blocked by anti-bot measures.
  * **Data Processing**: Use `jq` for JSON, `sed`/`awk` for text, or `python3`/`node` for complex calculations.
  * **Visualization**: Use `tree` to show directory structures clearly.
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
- **Research**:
  1. `web_search(query)`.
  2. ANALYZE results. If relevant, `read_url(url)`.
  3. STOP searching if you have enough info.
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
    /// On macOS/Linux, this defaults to `~/.config/intus/config.toml`.
    /// On Windows, it uses the roaming app data directory.
    ///
    /// If the file does not exist, default settings are returned.
    pub fn load() -> Result<Self> {
        // First check for config.toml in the current directory
        let local_config_path = std::path::Path::new("config.toml");
        if local_config_path.exists() {
            info!("Loading configuration from local ./config.toml");
            let contents = fs::read_to_string(local_config_path)?;
            let config: Config = toml::from_str(&contents)?;
            return Ok(config);
        }

        let config_path = if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            // Force ~/.config/intus/config.toml for macOS and Linux
            BaseDirs::new().map(|base| {
                base.home_dir()
                    .join(".config")
                    .join("intus")
                    .join("config.toml")
            })
        } else {
            // Fallback to standard directories for other OSs (like Windows)
            ProjectDirs::from("com", "intus", "intus")
                .map(|proj_dirs| proj_dirs.config_dir().join("config.toml"))
        };

        if let Some(path) = &config_path {
            if path.exists() {
                info!("Loading configuration from {:?}", path);
                let contents = fs::read_to_string(path)?;
                let config: Config = toml::from_str(&contents)?;
                return Ok(config);
            }
        }

        info!("Configuration file not found. Using defaults.");

        // Return default if file doesn't exist or directories fails
        let default_config = Config {
            ollama_url: default_ollama_url(),
            api_type: default_api_type(),
            api_key: String::new(),
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
        };

        // Try to save the default config
        if let Some(path) = &config_path {
            if let Some(parent) = path.parent() {
                match fs::create_dir_all(parent) {
                    Ok(_) => {
                        if let Ok(toml_string) = toml::to_string_pretty(&default_config) {
                             if let Ok(_) = fs::write(path, toml_string) {
                                 info!("Created default configuration file at {:?}", path);
                             }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to create configuration directory: {}", e);
                    }
                }
            }
        }

        Ok(default_config)
    }

    /// Returns the path to the application's configuration directory.
    pub fn get_config_dir(&self) -> Option<std::path::PathBuf> {
        if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            BaseDirs::new().map(|base| {
                base.home_dir()
                    .join(".config")
                    .join("intus")
            })
        } else {
            ProjectDirs::from("com", "intus", "intus")
                .map(|proj_dirs| proj_dirs.config_dir().to_path_buf())
        }
    }

    pub fn new_test_config() -> Self {
        Self {
            ollama_url: default_ollama_url(),
            api_type: default_api_type(),
            api_key: String::new(),
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