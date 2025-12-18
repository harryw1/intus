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
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_context_token_limit() -> usize {
    4096
}

fn default_system_prompt() -> String {
    "You are a helpful AI assistant.".to_string()
}

fn default_ignored_patterns() -> Vec<String> {
    vec![
        "target".to_string(),
        ".git".to_string(),
        "node_modules".to_string(),
        ".env".to_string(),
        ".DS_Store".to_string(),
    ]
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
        })
    }
}
