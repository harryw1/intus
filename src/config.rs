use serde::Deserialize;
use std::fs;
use directories::ProjectDirs;
use anyhow::Result;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    #[serde(default = "default_context_token_limit")]
    pub context_token_limit: usize,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
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

impl Config {
    pub fn load() -> Result<Self> {
        if let Some(proj_dirs) = ProjectDirs::from("com", "ollama-tui", "ollama-tui") {
            let config_dir = proj_dirs.config_dir();
            let config_path = config_dir.join("config.toml");

            if config_path.exists() {
                let contents = fs::read_to_string(&config_path)?;
                let config: Config = toml::from_str(&contents)?;
                return Ok(config);
            }
        }
        
        // Return default if file doesn't exist or directories fails
        Ok(Config {
            ollama_url: default_ollama_url(),
            context_token_limit: default_context_token_limit(),
            system_prompt: default_system_prompt(),
        })
    }
}
