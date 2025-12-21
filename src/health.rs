use std::process::Command;
use reqwest::Client;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Ok,
    Warning(String),
    Critical(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceStatus {
    pub name: String,
    pub status: HealthStatus,
}

impl ServiceStatus {
    pub fn new(name: &str, status: HealthStatus) -> Self {
        Self {
            name: name.to_string(),
            status,
        }
    }
}

pub async fn check_all(ollama_url: &str, searxng_url: &str) -> Vec<ServiceStatus> {
    let mut statuses = Vec::new();
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| Client::new());

    // 1. Check Ollama (Critical)
    statuses.push(check_ollama(&client, ollama_url).await);

    // 2. Check SearXNG (Warning)
    statuses.push(check_searxng(&client, searxng_url).await);

    // 3. Check uv (Warning/Info)
    statuses.push(check_tool("uv", "--version"));

    // 4. Check git (Critical for some ops, but usually present)
    statuses.push(check_tool("git", "--version"));

    statuses
}

async fn check_ollama(client: &Client, base_url: &str) -> ServiceStatus {
    match client.get(format!("{}/api/tags", base_url)).send().await {
        Ok(res) => {
            if res.status().is_success() {
                ServiceStatus::new("Ollama", HealthStatus::Ok)
            } else {
                ServiceStatus::new(
                    "Ollama",
                    HealthStatus::Critical(format!("API Error: {}", res.status())),
                )
            }
        }
        Err(e) => ServiceStatus::new(
            "Ollama",
            HealthStatus::Critical(format!("Connection Failed: {}", e)),
        ),
    }
}

async fn check_searxng(client: &Client, base_url: &str) -> ServiceStatus {
    // SearXNG usually has a health endpoint or just root
    match client.get(base_url).send().await {
        Ok(res) => {
            if res.status().is_success() {
                ServiceStatus::new("SearXNG", HealthStatus::Ok)
            } else {
                ServiceStatus::new(
                    "SearXNG",
                    HealthStatus::Warning(format!("Status: {}", res.status())),
                )
            }
        }
        Err(e) => ServiceStatus::new(
            "SearXNG",
            HealthStatus::Warning(format!("Not Reachable: {}", e)),
        ),
    }
}

fn check_tool(command: &str, arg: &str) -> ServiceStatus {
    match Command::new(command).arg(arg).output() {
        Ok(output) => {
            if output.status.success() {
                ServiceStatus::new(command, HealthStatus::Ok)
            } else {
                ServiceStatus::new(
                    command,
                    HealthStatus::Warning("Command failed".to_string()),
                )
            }
        }
        Err(_) => ServiceStatus::new(
            command,
            HealthStatus::Warning("Not installed".to_string()),
        ),
    }
}
