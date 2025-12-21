use super::Tool;
use anyhow::Result;
use serde_json::Value;
use std::sync::{Arc, OnceLock, Mutex};
use crate::rag::RagSystem;
use headless_chrome::{Browser, LaunchOptions};

pub struct WebSearchTool {
    pub searxng_url: String,
    pub client: OnceLock<reqwest::blocking::Client>,
    pub rag: Arc<RagSystem>,
}

impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "USE THIS to search the web for SNIPPETS.
IMPORTANT:
- This tool only gives you summaries. To read a full page, use `read_url` with the URL you find here.
- CATEGORY SELECTION:
  * 'news': Recent events, current weather.
  * 'it': Programming docs, libraries, technical specs (searches github, stackoverflow).
  * 'general': Everything else (default).
- Do NOT use 'it' for weather or general questions."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query. Required if 'url' is not provided."
                },
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from. If provided, 'query' is ignored."
                },
                "category": {
                    "type": "string",
                    "enum": ["general", "news", "it", "science", "files", "images", "videos", "music", "social_media"],
                    "description": "The category of search results. Use 'news' for current events, 'it' for programming/technical, 'general' for broad searches. Defaults to 'general'."
                }
            },
            "required": []
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let url_arg = args.get("url").and_then(|v| v.as_str());

        let client = self.client.get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new())
        });
        
        // Fix: usage of filter to ignore empty strings which cause builder errors
        if let Some(url) = url_arg.filter(|u| !u.is_empty()) {
             let response = client.get(url)
                 .header("User-Agent", "Mozilla/5.0 (compatible; Intus/1.0; +https://github.com/harryw1/intus)")
                 .send()?;
            if !response.status().is_success() {
                return Err(anyhow::anyhow!("Failed to fetch URL: {}", response.status()));
            }

            let html = response.text()?;
            let width = 120; // Reasonable width for TUI reading
            let text = html2text::from_read(html.as_bytes(), width);
            
            // Limit output size
            if text.len() > 20000 {
                return Ok(format!("{}\n... (truncated)", &text[..20000]));
            }
            return Ok(text);
        }

        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument (or 'url')"))?;

        let category = args
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("general");

        let mut url = self.searxng_url.clone();
        if !url.ends_with('/') {
            url.push('/');
        }
        url.push_str("search"); 

        let response = client
            .get(&url)
            .query(&[
                ("q", query), 
                ("format", "json"), 
                ("language", "en-US"),
                ("categories", category)
            ])
            .send()?;

        if !response.status().is_success() {
             return Err(anyhow::anyhow!("Search request failed: {}", response.status()));
        }

        let json: Value = response.json()?;
        
        if let Some(results) = json.get("results").and_then(|v| v.as_array()) {
            if results.is_empty() {
                return Ok(format!("No results found for query: '{}'.", query));
            }

            let mut output = String::new();
            let mut all_content = String::new();
            for (i, result) in results.iter().take(5).enumerate() {
                let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("No Title");
                let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("No URL");
                let content = result.get("content").and_then(|v| v.as_str()).unwrap_or("");
                
                output.push_str(&format!("{}. [{}]({})\n   {}\n\n", i + 1, title, url, content));
                all_content.push_str(&format!("Source: {}\nTitle: {}\nContent: {}\n\n", url, title, content));
            }

            // Auto-ingest snippets into RAG
            let handle = tokio::runtime::Handle::current();
            let _ = handle.block_on(self.rag.add_text(&all_content, Some("web".to_string())));

            Ok(output)
        } else {
            Ok("No results structure in response.".to_string())
        }
    }
}

pub struct BrowserClient {
    browser: Mutex<Option<Browser>>,
}

impl BrowserClient {
    pub fn new() -> Self {
        Self {
            browser: Mutex::new(None),
        }
    }

    pub fn get_content(&self, url: &str) -> Result<String> {
        let mut browser_guard = self.browser.lock().unwrap();
        
        if browser_guard.is_none() {
            let browser = Browser::new(LaunchOptions {
                headless: true,
                ..Default::default()
            })?;
            *browser_guard = Some(browser);
        }

        let browser = browser_guard.as_ref().unwrap();
        let tab = browser.new_tab()?;
        
        tab.navigate_to(url)?;
        tab.wait_until_navigated()?;
        
        // Wait a bit for JS to execute (naive)
        std::thread::sleep(std::time::Duration::from_millis(2000));

        let content = tab.find_element("body")?.get_inner_text()?;
        Ok(content)
    }
}

pub struct ReadUrlTool {
    pub client: OnceLock<reqwest::blocking::Client>,
    pub rag: Arc<RagSystem>,
    pub browser: Arc<BrowserClient>,
}

impl Tool for ReadUrlTool {
    fn name(&self) -> &str {
        "read_url"
    }

    fn description(&self) -> &str {
        "USE THIS to read the full content of a specific web page (URL). 
If the page is large, provide a 'query' to search for specific sections.
If NO query is provided, the tool returns the start of the page and INDEXES the full content for future searches."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from."
                },
                "query": {
                    "type": "string",
                    "description": "Optional: A specific query to search for within the page content (e.g., 'pricing', 'installation')."
                }
            },
            "required": ["url"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' argument"))?;
            
        let query = args.get("query").and_then(|v| v.as_str());

        let client = self.client.get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new())
        });
        
        // Primary Strategy: HTTP Request (Fast)
        // If it fails or returns little content, fallback to Browser (Slow but Robust)
        
        let mut text = String::new();
        let mut needs_browser = false;

        match client.get(url)
            .header("User-Agent", "Mozilla/5.0 (compatible; Intus/1.0; +https://github.com/harryw1/intus)")
            .send() 
        {
            Ok(response) => {
                if response.status().is_success() {
                    let html = response.text().unwrap_or_default();
                    let width = 120;
                    text = html2text::from_read(html.as_bytes(), width);
                    
                    // Simple heuristic: If text is too short or mentions "enable javascript", use browser
                    if text.len() < 500 || text.to_lowercase().contains("enable javascript") || text.to_lowercase().contains("you need to enable javascript") {
                        needs_browser = true;
                    }
                } else {
                    needs_browser = true; // Fallback on error (e.g. 403 blocking naive requests)
                }
            },
            Err(_) => {
                needs_browser = true;
            }
        }

        if needs_browser {
             match self.browser.get_content(url) {
                 Ok(browser_text) => {
                     text = browser_text;
                 },
                 Err(e) => {
                     // If browser fails, return whatever we had or the error
                     if text.is_empty() {
                         return Err(anyhow::anyhow!("Failed to read URL via HTTP and Browser: {}", e));
                     }
                     // Provide what we have with a warning
                     text = format!("(Warning: Browser rendering failed, showing raw extraction)\n{}", text);
                 }
             }
        }
        
        // Always ingest into RAG
        let handle = tokio::runtime::Handle::current();
        
        if let Err(e) = handle.block_on(self.rag.add_text(&text, Some("web".to_string()))) {
             return Ok(format!("Fetched content but failed to index: {}\n\n{}", e, &text.chars().take(2000).collect::<String>()));
        }

        if let Some(q) = query {
            let clean_query = q.trim_matches('\'').trim_matches('"');
            let results = handle.block_on(self.rag.search(clean_query, 5, Some("web")))?;
            
            if results.is_empty() {
                Ok(format!("Page indexed, but no sections found matching query '{}'.\nHere is the beginning of the page:\n\n{}", clean_query, &text.chars().take(2000).collect::<String>()))
            } else {
                Ok(format!("Found {} relevant sections for '{}' in {}:\n\n{}", results.len(), clean_query, url, results.join("\n\n---\n\n")))
            }
        } else {
            if text.len() > 20000 {
                Ok(format!("Page indexed successfully. Showing first 20k chars:\n\n{}... (truncated)\n\n(Tip: You can now use `semantic_search` or call `read_url` again with a `query` to find specific info on this page)", &text[..20000]))
            } else {
                Ok(format!("{}\n\n(Page content indexed)", text))
            }
        }
    }
}
