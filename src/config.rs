use anyhow::Result;
use directories::{BaseDirs, ProjectDirs};
use serde::Deserialize;
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
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
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

fn default_system_prompt() -> String {
    r#"You are a helpful AI coding assistant with access to file system tools.

## CRITICAL: ACTION FIRST
When the user asks you to do something with files or search for something:
1. USE YOUR TOOLS IMMEDIATELY - do not explain how you would use them
2. Execute the search/action first, THEN explain what you found

## Response Formatting
- Use Markdown for all responses
- Wrap code in fenced code blocks with language tags
- Be concise - avoid unnecessary verbosity

## Important Rules
1. EXECUTE tools immediately when asked to search/find something
2. Use case_insensitive=true when user asks for case-insensitive search
3. Use ~ or ~/Documents for user document searches
4. After getting results, summarize them clearly
5. Do NOT call the same tool with same arguments twice
6. VERIFY facts by reading the actual file or URL content. Do not guess based on filenames or search snippets."#
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
    pub fn load() -> Result<Self> {
        let config_path = if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            // Force ~/.config/ollama-tui/config.toml for macOS and Linux
            BaseDirs::new().map(|base| {
                base.home_dir()
                    .join(".config")
                    .join("ollama-tui")
                    .join("config.toml")
            })
        } else {
            // Fallback to standard directories for other OSs (like Windows)
            ProjectDirs::from("com", "ollama-tui", "ollama-tui")
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
        })
    }
    pub fn get_config_dir(&self) -> Option<std::path::PathBuf> {
        if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            BaseDirs::new().map(|base| {
                base.home_dir()
                    .join(".config")
                    .join("ollama-tui")
            })
        } else {
            ProjectDirs::from("com", "ollama-tui", "ollama-tui")
                .map(|proj_dirs| proj_dirs.config_dir().to_path_buf())
        }
    }
}
