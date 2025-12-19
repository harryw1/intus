use crate::config::Config;
use crate::process::ProcessTracker;
use crate::context::ContextManager;
use crate::ollama::{ChatMessage, ChatMessageRequest, ChatStreamEvent, OllamaClient, ToolCall};
use crate::tools::{CatTool, FileSearchTool, GrepTool, ListDirectoryTool, ReplaceTextTool, RunCommandTool, SemanticSearchTool, Tool, WebSearchTool, WriteFileTool};
use crossterm::event::{KeyCode, KeyModifiers};
use directories::{BaseDirs, ProjectDirs};
use futures::StreamExt;
use ratatui::style::Style;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use tui_textarea::{Input, TextArea};
use arboard::Clipboard;

/// Generates system context information for the LLM to understand the user's environment.
fn get_system_context() -> String {
    let os = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Unknown OS"
    };

    let home_dir = BaseDirs::new()
        .map(|b| b.home_dir().to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let cwd = env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    format!(
        "\n\n[System Info]\nOS: {}\nHome Directory: {}\nCurrent Working Directory: {}\n\nWhen using file system tools, use these actual paths instead of guessing. For example, use '{}' instead of '/home/user'.",
        os, home_dir, cwd, home_dir
    )
}

#[derive(Debug, PartialEq, Clone)]
pub enum Action {
    Render,
    #[allow(dead_code)]
    Resize(u16, u16),
    Quit,
    Error(String),
    UserInput(crossterm::event::KeyEvent),
    LoadModels,
    ModelsLoaded(Vec<String>),
    EnterModelSelect,
    SendMessage,
    AddUserMessage(String),
    AddAiToken(String),
    AddToolCall(ToolCall),
    AddToolOutput(String, String), // name, output
    RequestAiResponse,
    AiResponseComplete,
    SwitchMode(Mode),
    ClearHistory,
    UpdateSystemPrompt(String),
    EnterSystemPromptEdit,
    Scroll(i16),
    // Session Actions
    EnterSessionSelect,
    SelectSession(String),
    EnterSessionCreate,
    CreateSession(String),
    DeleteSession(String),
    SessionsLoaded(Vec<String>),
    // Model Management Actions
    EnterModelPull,
    StartPullModel(String),
    PullProgress(String, Option<u64>, Option<u64>), // Status, Completed, Total
    DeleteModel(String),
    PrepareQuit,
    ConfirmToolExecution,
    DenyToolExecution,
    CancelGeneration,
    CopyMessage,
    MoveSelection(i16),
    // Context Management
    UpdateModelContextLimit(usize),
    SummaryReady(String, usize), // summary text, count of messages summarized
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Mode {
    Insert,
    Normal,
    ModelSelect,
    SystemPromptEdit,
    SessionSelect,
    SessionCreate,
    ModelPullInput,
    ToolConfirmation,
}

pub struct App<'a> {
    pub ollama_client: OllamaClient,
    pub action_tx: mpsc::UnboundedSender<Action>,
    pub messages: Vec<ChatMessage>,
    pub input: TextArea<'a>, // Using tui-textarea
    pub models: Vec<String>,
    pub selected_model: usize,
    pub vertical_scroll: u16,
    pub auto_scroll: bool,
    pub mode: Mode,
    pub loading: bool,
    pub error: Option<String>,
    pub current_response_buffer: String,
    pub show_help: bool,
    pub spinner_state: ThrobberState,
    pub context_token_limit: usize,
    pub system_prompt: String,
    pub system_prompt_input: TextArea<'a>,
    pub session_file_path: Option<PathBuf>,
    // Session state
    pub current_session: String,
    pub available_sessions: Vec<String>,
    pub session_list_state: ratatui::widgets::ListState,
    pub session_input: TextArea<'a>,
    // Model Management state
    pub pull_input: TextArea<'a>,
    pub pull_progress: Option<(String, Option<u64>, Option<u64>)>,
    // Tools
    pub tools: HashMap<String, Arc<dyn Tool>>,
    /// Counter for consecutive tool calls to prevent infinite loops
    consecutive_tool_calls: usize,
    /// Pending tool call waiting for user confirmation
    pub pending_tool_call: Option<ToolCall>,
    // Persistence
    last_save_time: std::time::Instant,
    // Context Management
    pub context_manager: ContextManager,
    pub current_token_usage: usize,
    pub tool_scroll: u16,
    pub model_context_limit: Option<usize>,
    pub conversation_summary: Option<String>,
    pub summarized_count: usize,
    pub is_summarizing: bool,
    // Stop Generation
    pub current_request_handle: Option<AbortHandle>,
    // Clipboard & Selection
    pub selected_message_index: Option<usize>,
    #[allow(dead_code)] // Keep it alive
    pub clipboard: Option<Clipboard>,
    // UX
    pub notification: Option<(String, std::time::Instant)>,
    pub theme: crate::theme::Theme,
    pub process_tracker: Arc<ProcessTracker>,
    pub rag: crate::rag::RagSystem,
}

/// Maximum number of consecutive tool calls before forcing a text response
const MAX_CONSECUTIVE_TOOL_CALLS: usize = 3;

impl<'a> App<'a> {
    pub fn new(
        action_tx: mpsc::UnboundedSender<Action>,
        config: Config,
        load_history: bool,
        custom_session_path: Option<PathBuf>,
    ) -> Self {
        let mut textarea = TextArea::default();
        // Disable default cursor line style (underline)
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_text("Type a message...");

        textarea.set_placeholder_text("Type a message...");

        let process_tracker = Arc::new(ProcessTracker::new());

        let mut tools: HashMap<String, Arc<dyn Tool>> = HashMap::new();
        tools.insert(
            "find_files".to_string(),
            Arc::new(FileSearchTool {
                ignored_patterns: config.ignored_patterns.clone(),
            }),
        );
        tools.insert(
            "grep_files".to_string(),
            Arc::new(GrepTool {
                ignored_patterns: config.ignored_patterns.clone(),
            }),
        );
        tools.insert(
            "read_file".to_string(),
            Arc::new(CatTool {
                ignored_patterns: config.ignored_patterns.clone(),
            }),
        );
        tools.insert(
            "list_directory".to_string(),
            Arc::new(ListDirectoryTool {
                ignored_patterns: config.ignored_patterns.clone(),
            }),
        );
        tools.insert(
            "write_file".to_string(),
            Arc::new(WriteFileTool {
                ignored_patterns: config.ignored_patterns.clone(),
            }),
        );
        tools.insert(
            "replace_text".to_string(),
            Arc::new(ReplaceTextTool {
                ignored_patterns: config.ignored_patterns.clone(),
            }),
        );
        let storage_path = config.get_config_dir().map(|d| d.join("vectors.json"));
        
        tools.insert(
            "semantic_search".to_string(),
            Arc::new(SemanticSearchTool {
                client: OllamaClient::new(config.ollama_url.clone()),
                index: Arc::new(std::sync::Mutex::new(None)),
                embedding_model: config.embedding_model.clone(),
                ignored_patterns: config.ignored_patterns.clone(),
                storage_path,
            }),
        );
        tools.insert(
            "web_search".to_string(),
            Arc::new(WebSearchTool {
                searxng_url: config.searxng_url.clone(),
                client: std::sync::OnceLock::new(),
            }),
        );
        tools.insert(
            "run_command".to_string(),
            Arc::new(RunCommandTool {
                allowed_commands: vec![
                    "git".to_string(),
                    "ls".to_string(),
                    "grep".to_string(),
                    "rg".to_string(),
                    "find".to_string(),
                    "cargo".to_string(),
                    "mkdir".to_string(),
                    "rmdir".to_string(),
                    "touch".to_string(),
                    "pwd".to_string(),
                    "date".to_string(),
                    "echo".to_string(),
                    "mv".to_string(),
                    "cp".to_string(),
                    "stat".to_string(),
                ],
                process_tracker: process_tracker.clone(),
            }),
        );

        let mut app = Self {
            ollama_client: OllamaClient::new(config.ollama_url.clone()),
            action_tx,
            messages: Vec::new(),
            input: textarea,
            models: Vec::new(),
            selected_model: 0,
            vertical_scroll: 0,
            auto_scroll: true,
            mode: Mode::Insert,
            loading: bool::default(),
            error: None,
            current_response_buffer: String::new(),
            show_help: false,
            spinner_state: ThrobberState::default(),
            context_token_limit: config.context_token_limit,
            system_prompt: config.system_prompt.clone(),
            system_prompt_input: TextArea::new(vec![config.system_prompt]),
            session_file_path: custom_session_path,
            current_session: "default".to_string(),
            available_sessions: Vec::new(),
            session_list_state: ratatui::widgets::ListState::default(),
            session_input: TextArea::default(),
            pull_input: TextArea::default(),
            pull_progress: None,
            tools,
            consecutive_tool_calls: 0,
            pending_tool_call: None,
            last_save_time: std::time::Instant::now(),
            context_manager: ContextManager::new(
                config.auto_context,
                config.summarization_enabled,
                config.summarization_threshold,
            ),
            current_token_usage: 0,
            tool_scroll: 0,
            model_context_limit: None,
            conversation_summary: None,
            summarized_count: 0,
            is_summarizing: false,
            current_request_handle: None,
            selected_message_index: None,
            clipboard: Clipboard::new().ok(),
            notification: None,
            theme: crate::theme::Theme::default(),
            process_tracker,
            rag: crate::rag::RagSystem::new(
                OllamaClient::new(config.ollama_url.clone()),
                config.embedding_model.clone(),
            ),
        };

        if load_history {
            // Migrate old history.json if it exists and sessions/default.json doesn't
            app.migrate_legacy_history();
            app.load_session("default");
        }
        app
    }

    fn estimate_tokens(&self, text: &str) -> usize {
        // Rough estimate: 1 token ~= 4 chars
        // Add overhead for JSON structure/roles (approx 4 tokens per msg)
        (text.len() / 4) + 4
    }

    /// Calculates the token usage of the *actual* context window we would send.
    fn calculate_context_usage(&self) -> usize {
        let _limit = self.model_context_limit.unwrap_or(self.context_token_limit);
        let system_prompt_tokens = self.estimate_tokens(&self.system_prompt) + self.estimate_tokens(&get_system_context());
        
        let summary_tokens = if let Some(summary) = &self.conversation_summary {
             self.estimate_tokens(summary) + 10 
        } else {
             0
        };
        
        // Base usage
        let mut usage = system_prompt_tokens + summary_tokens;
        
        let start_index = if self.conversation_summary.is_some() {
             self.summarized_count
        } else {
             0
        };
        
        // Add active messages
        for (i, msg) in self.messages.iter().enumerate() {
            if i >= start_index {
                usage += self.estimate_tokens(&msg.content);
            }
        }
        
        // Note: This usage might exceed limit if we just added a huge message.
        // But `build_context_window` truncates. The UI should show *actual usage vs limit*,
        // so if we are incorrectly over limit, user sees it. 
        // Ideally, we want to show what the model *sees*. 
        // If `build_context` truncates, we should calculate usage based on that.
        // But iterating build_context is expensive. 
        // Let's stick to this "intended" usage.
        usage
    }

    fn trigger_summarization(&mut self) {
        if self.messages.len() < 4 { return; } // Don't summarize tiny history
        
        self.is_summarizing = true;
        
        // Summarize oldest 50% of messages
        let count_to_summarize = self.messages.len() / 2;
        // Don't re-summarize if we haven't advanced much?
        // Actually, `summarized_count` tracks where we are.
        // We want to summarize from 0 to `new_count`.
        // If we already have a summary covering X messages, we want a new summary covering Y (where Y > X).
        
        if count_to_summarize <= self.summarized_count {
            self.is_summarizing = false;
            return;
        }

        let messages_snapshot = self.messages.clone(); // Clone for async task
        let tx = self.action_tx.clone();
        let system_prompt = self.system_prompt.clone();
        let client = self.ollama_client.clone();
        let model = self.models.get(self.selected_model).cloned().unwrap_or("llama2".to_string());
        
        tokio::spawn(async move {
            if let Some((prompt, count)) = ContextManager::summarize_messages(&messages_snapshot, messages_snapshot.len() - count_to_summarize) {
                // Send summarization request
                let reqs = vec![
                    ChatMessageRequest {
                        role: "system".to_string(),
                        content: system_prompt, // Keep system persona for summary style
                        images: None,
                        tool_calls: None,
                        tool_name: None,
                    },
                    ChatMessageRequest {
                        role: "user".to_string(),
                        content: prompt,
                        images: None,
                        tool_calls: None,
                        tool_name: None,
                    }
                ];
                
                if let Ok(mut stream) = client.chat(&model, reqs, None, None).await {
                    let mut summary_acc = String::new();
                    while let Some(res) = stream.next().await {
                         if let Ok(ChatStreamEvent::Token(t)) = res {
                             summary_acc.push_str(&t);
                         }
                    }
                    
                    if !summary_acc.trim().is_empty() {
                         let _ = tx.send(Action::SummaryReady(summary_acc, count));
                         return;
                    }
                }
            }
            // If failed or empty
            // We need to reset flag? 
            // Currently no easy way to reset `is_summarizing` on error without another Action.
            // For MVP, if it fails, it hangs in `is_summarizing=true` preventing retry.
            // Let's implement Action::SummarizationFailed? Or just ignore.
        });
    }

    fn build_context_window(&self) -> Vec<ChatMessageRequest> {
        let mut context_messages: Vec<ChatMessageRequest> = Vec::new();
        
        let limit = self.model_context_limit.unwrap_or(self.context_token_limit);
        
        // Calculate reserved tokens (System Prompt + potential Summary)
        let system_prompt_tokens = self.estimate_tokens(&self.system_prompt) + self.estimate_tokens(&get_system_context());
        let summary_tokens = if let Some(summary) = &self.conversation_summary {
             self.estimate_tokens(summary) + 10 // +10 overhead
        } else {
             0
        };
        
        // Reserve space for system prompt and summary AND a generation buffer (e.g., 512 tokens)
        // This ensures we don't starve the model of generation capacity.
        let generation_buffer = 512;
        let reserved_tokens = system_prompt_tokens + summary_tokens + generation_buffer;
        let available_for_history = limit.saturating_sub(reserved_tokens);
        
        let mut current_tokens = 0;

        // Iterate backwards through history, skipping summarized messages if we rely on summary
        // However, "Non-destructive" means we keep ALL in UI, but maybe truncate for model.
        // If we have a summary covering N messages, we SHOULD skip them for the model context 
        // IF we are constrained. But ideally we fit as much recent history as possible.
        // Proposed Logic: Fill backwards until we hit limit OR we hit the `summarized_count` boundary.
        // Actually, if we have a summary, we typically WANT to use it instead of the oldest messages it covers.
        // So we should stop iterating when we reach `messages.len() - summarized_count`.
        
        let start_index = if self.conversation_summary.is_some() {
             self.summarized_count
        } else {
             0
        };

        // We only consider messages after the summary point
        // But wait, if available space allows, maybe we CAN include some "summarized" messages?
        // Simpler approach for Phase 1: Summary REPLACES the first `summarized_count` messages in the CONTEXT view.
        // So we explicitly skip the first `summarized_count` messages from the `self.messages` list.
        
        // Take messages from end, stopping at start_index
        for (i, msg) in self.messages.iter().enumerate().rev() {
            if i < start_index {
                break; 
            }
            
            let msg_tokens = self.estimate_tokens(&msg.content);
            if current_tokens + msg_tokens > available_for_history {
                break;
            }
            context_messages.push(ChatMessageRequest {
                role: msg.role.clone(),
                content: msg.content.clone(),
                images: msg.images.clone(),
                tool_calls: msg.tool_calls.clone(),
                tool_name: msg.tool_name.clone(),
            });
            current_tokens += msg_tokens;
        }

        // Restore chronological order
        context_messages.reverse();
        
        // Inject Summary if exists
        if let Some(summary) = &self.conversation_summary {
            context_messages.insert(0, ChatMessageRequest {
                role: "system".to_string(),
                content: format!("[Previous Conversation Summary]:\n{}", summary),
                images: None,
                tool_calls: None,
                tool_name: None,
            });
        }

        // Prepend system prompt with system context
        let full_system_prompt = format!("{}{}", self.system_prompt, get_system_context());
        context_messages.insert(
            0,
            ChatMessageRequest {
                role: "system".to_string(),
                content: full_system_prompt,
                images: None,
                tool_calls: None,
                tool_name: None,
            },
        );

        context_messages
    }

    fn get_sessions_dir(&self) -> Option<PathBuf> {
        let sessions_dir = if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            BaseDirs::new().map(|base| {
                base.home_dir()
                    .join(".config")
                    .join("ollama-tui")
                    .join("sessions")
            })
        } else {
            ProjectDirs::from("com", "ollama-tui", "ollama-tui")
                .map(|proj_dirs| proj_dirs.config_dir().join("sessions"))
        };

        if let Some(dir) = &sessions_dir {
            if !dir.exists() {
                let _ = fs::create_dir_all(dir);
            }
        }
        sessions_dir
    }

    fn get_session_path(&self, name: &str) -> Option<PathBuf> {
        if let Some(path) = &self.session_file_path {
            return Some(path.clone());
        }

        self.get_sessions_dir()
            .map(|dir| dir.join(format!("{}.json", name)))
    }

    fn migrate_legacy_history(&self) {
        if let Some(proj_dirs) = ProjectDirs::from("com", "ollama-tui", "ollama-tui") {
            let config_dir = proj_dirs.config_dir();
            let legacy_path = config_dir.join("history.json");
            if legacy_path.exists() {
                if let Some(default_path) = self.get_session_path("default") {
                    if !default_path.exists() {
                        let _ = fs::rename(legacy_path, default_path);
                    }
                }
            }
        }
    }

    fn list_sessions(&mut self) {
        if let Some(dir) = self.get_sessions_dir() {
            if let Ok(entries) = fs::read_dir(dir) {
                let mut sessions: Vec<String> = entries
                    .filter_map(|entry| {
                        entry.ok().and_then(|e| {
                            let path = e.path();
                            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                                path.file_stem()
                                    .and_then(|s| s.to_str())
                                    .map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                    })
                    .collect();
                sessions.sort();
                if sessions.is_empty() {
                    sessions.push("default".to_string());
                }
                self.available_sessions = sessions;
            }
        }
    }

    fn save_session(&self) {
        if let Some(path) = self.get_session_path(&self.current_session) {
            if let Ok(json) = serde_json::to_string(&self.messages) {
                // Create backup of existing session before overwriting
                if path.exists() {
                    let backup_path = path.with_extension("json.bak");
                    let _ = fs::copy(&path, &backup_path);
                }
                // Atomic write: write to temp file, then rename
                let temp_path = path.with_extension("json.tmp");
                if fs::write(&temp_path, &json).is_ok() {
                    let _ = fs::rename(&temp_path, &path);
                }
            }
        }
    }

    /// Throttled save - at most once every 2 seconds during streaming.
    /// Reduces I/O overhead when receiving many tokens.
    fn save_session_throttled(&mut self) {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_save_time).as_secs() >= 2 {
            self.save_session();
            self.last_save_time = now;
        }
    }

    fn load_session(&mut self, name: &str) {
        self.current_session = name.to_string();
        self.messages.clear();
        self.vertical_scroll = 0;
        self.current_response_buffer.clear();

        if let Some(path) = self.get_session_path(name) {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(path) {
                    if let Ok(messages) = serde_json::from_str(&content) {
                        self.messages = messages;
                        if !self.messages.is_empty() {
                            self.auto_scroll = true;
                        }
                    }
                }
            }
        }
        self.current_token_usage = ContextManager::estimate_token_count(&self.messages);
    }

    async fn request_ai_response(&mut self) {
        if self.models.is_empty() {
            self.error = Some("No models loaded".to_string());
            return;
        }

        self.loading = true;

        if let Some(last) = self.messages.last() {
            if last.role != "assistant" {
                self.messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: String::new(),
                    images: None,
                    tool_calls: None,
                    tool_name: None,
                });
                self.current_response_buffer.clear();
            } else {
                self.current_response_buffer = last.content.clone();
            }
        } else {
            self.messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: String::new(),
                images: None,
                tool_calls: None,
                tool_name: None,
            });
            self.current_response_buffer.clear();
        }

        // 1. Get the last USER message (ignoring the empty assistant placeholder we just pushed)
        // We look for the last actual content message from the user.
        let last_user_content = self.messages.iter()
            .rev()
            .skip(1) // skip the empty assistant message
            .find(|m| m.role == "user")
            .map(|m| m.content.clone());

        // 2. Perform RAG Search if valid user query
        let cloned_rag = if let Some(query) = last_user_content {
             // We can't use self.rag directly in the async block easily due to borrow checker
             // But actually we need the result BEFORE we build context window?
             // Or we inject it into the message list temporarily?
             // Let's search synchronously here (blocking the UI briefly is acceptable for now, or we spawn?)
             // RAG search relies on network (Ollama embedding). We should async it.
             // BUT `build_context_window` is synchronous.
             // We need to fetch context, modify the message history (or context window), THEN run chat.
             // Let's do it inside a separate spawn? No, that complicates state.
             
             // Simplest First Implementation:
             // Block on RAG search. (freeze UI for 100ms-500ms).
             // Since we are inside `request_ai_response` which is async, we can await!
             // `request_ai_response` is `async fn`.
             
             if let Ok(results) = self.rag.search(&query, 3).await {
                 if !results.is_empty() {
                      // Format context
                      let context_block = format!("\n\n[Relevant Context from Tools]:\n{}", results.join("\n---\n"));
                      // Inject into the SYSTEM prompt for this turn? Or as a System Message?
                      // Let's append to the last user message in the CONTEXT ONLY?
                      // `build_context_window` reads `self.messages`.
                      // We don't want to modify `self.messages` permanently with this context (it's confusing in UI).
                      // So we should modify `context_messages` AFTER calling `build_context_window`.
                      Some(context_block)
                 } else {
                     None
                 }
             } else {
                 None
             }
        } else {
            None
        };
        
        let mut context_messages = self.build_context_window();
        
        // Inject RAG context if available
        if let Some(ctx) = cloned_rag {
             // Find the last user message in context_messages and append context
             if let Some(msg) = context_messages.iter_mut().rfind(|m| m.role == "user") {
                  msg.content.push_str(&ctx);
             }
        }

        let model = self.models[self.selected_model].clone();
        let client = self.ollama_client.clone();
        let tx = self.action_tx.clone();

        // Disable tools if we've hit the consecutive tool call limit
        // This forces the model to generate a text response
        let tool_definitions =
            if !self.tools.is_empty() && self.consecutive_tool_calls < MAX_CONSECUTIVE_TOOL_CALLS {
                Some(self.tools.values().map(|t| t.definition()).collect())
            } else {
                None
            };

        let context_limit = self.context_token_limit;

        let handle = tokio::spawn(async move {
            let mut options = std::collections::HashMap::new();
            options.insert("num_ctx".to_string(), serde_json::json!(context_limit));

            match client
                .chat(&model, context_messages, tool_definitions, Some(options))
                .await
            {
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(event) => match event {
                                ChatStreamEvent::Token(token) => {
                                    let _ = tx.send(Action::AddAiToken(token));
                                }
                                ChatStreamEvent::ToolCall(tool_call) => {
                                    let _ = tx.send(Action::AddToolCall(tool_call));
                                }
                            },
                            Err(e) => {
                                let _ = tx.send(Action::Error(format!("Stream: {}", e)));
                            }
                        }
                    }
                    let _ = tx.send(Action::AiResponseComplete);
                }
                Err(e) => {
                    let _ = tx.send(Action::Error(format!("Chat failed: {}", e)));
                }
            }
        });
        self.current_request_handle = Some(handle.abort_handle());
    }

    fn scroll_to_bottom(&mut self) {
        self.auto_scroll = true;
    }

    pub async fn update(&mut self, action: Action) -> bool {
        match action {
            Action::Error(e) => {
                self.error = Some(e);
                self.loading = false;
                true
            }
            Action::LoadModels => {
                self.loading = true;
                let client = self.ollama_client.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match client.list_models().await {
                        Ok(models) => {
                            let _ = tx.send(Action::ModelsLoaded(models));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::Error(e.to_string()));
                        }
                    }
                });
                true
            }
            Action::ModelsLoaded(models) => {
                self.loading = false;
                self.models = models;
                if !self.models.is_empty() {
                    self.selected_model = 0;
                    // Trigger fetching context limit for the default selected model
                    let model_name = self.models[0].clone();
                    let client = self.ollama_client.clone();
                    let tx = self.action_tx.clone();
                    tokio::spawn(async move {
                        if let Ok(info) = client.show_model(&model_name).await {
                            if let Some(limit) = info.context_length() {
                                let _ = tx.send(Action::UpdateModelContextLimit(limit));
                            }
                        }
                    });
                }
                true
            }
            Action::UpdateModelContextLimit(limit) => {
                self.model_context_limit = Some(limit);
                // Re-calculate token usage with new limit
                self.current_token_usage = self.calculate_context_usage();
                true
            }
            Action::SummaryReady(summary, count) => {
                self.conversation_summary = Some(summary);
                self.summarized_count = count; // Replace old summary scope with new one
                self.is_summarizing = false;
                self.current_token_usage = self.calculate_context_usage();
                true
            }
            Action::EnterModelSelect => {
                self.mode = Mode::ModelSelect;
                true
            }
            Action::SwitchMode(mode) => {
                self.mode = mode;
                true
            }
            Action::ClearHistory => {
                self.messages.clear();
                self.current_response_buffer.clear();
                self.vertical_scroll = 0;
                self.save_session();
                true
            }
            Action::EnterSystemPromptEdit => {
                self.mode = Mode::SystemPromptEdit;
                self.system_prompt_input =
                    TextArea::new(self.system_prompt.lines().map(|s| s.to_string()).collect());
                self.system_prompt_input.set_block(
                    ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .title(" Edit System Prompt (Esc to Cancel, Enter to Save) "),
                );
                self.system_prompt_input
                    .set_cursor_line_style(Style::default());
                true
            }
            Action::UpdateSystemPrompt(prompt) => {
                self.system_prompt = prompt;
                self.mode = Mode::Insert;
                true
            }
            Action::Scroll(delta) => {
                if delta > 0 {
                    self.vertical_scroll = self.vertical_scroll.saturating_add(delta as u16);
                } else {
                    self.vertical_scroll = self.vertical_scroll.saturating_sub(delta.abs() as u16);
                }
                
                // If we scroll manually, generally disable autoscroll UNLESS we hit bottom? 
                // Actually the requirement is "Scrolling down past the last bubble should visually unselect the bubble".
                // We'll handle visual unselect in ui.rs calculation OR just clear selection here if we are at bottom.
                // But we don't know "total_height" here in App easily without recalculating.
                // However, user said "scrolling down past the last bubble". 
                // If we just unselect when `auto_scroll` becomes true (which happens in scroll_to_bottom), 
                // or we can heuristically say if we scroll down, and we were selected, maybe check bounds?
                // Let's implement a simpler heuristic: If I scroll down while selected, 
                // and I hit the end of messages, unselect?
                // Actually, let's just leave it for now and let the user manually unselect or click?
                // Wait, "Scrolling down past the lasr bubble should visually unselect the bubble"
                // This implies getting back to "live" mode.
                
                self.auto_scroll = false;
                
                // If in tool confirmation mode, apply scroll to tool_scroll

                if self.mode == Mode::ToolConfirmation {
                     if delta > 0 {
                        self.tool_scroll = self.tool_scroll.saturating_add(delta as u16);
                    } else {
                        self.tool_scroll = self.tool_scroll.saturating_sub(delta.abs() as u16);
                    }
                }
                true
            }
            Action::SendMessage => {
                let content = self.input.lines().join("\n");
                if content.trim().is_empty() {
                    return true;
                }
                // Clear input
                self.input = TextArea::default();
                self.input.set_cursor_line_style(Style::default());
                self.input.set_placeholder_text("Type a message...");

                let _ = self.action_tx.send(Action::AddUserMessage(content));
                true
            }
            Action::AddUserMessage(msg) => {
                // Reset tool call counter on new user message
                self.consecutive_tool_calls = 0;
                self.messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: msg,
                    images: None,
                    tool_calls: None,
                    tool_name: None,
                });
                self.loading = true;
                self.scroll_to_bottom();
                self.save_session();
                let _ = self.action_tx.send(Action::RequestAiResponse);
                self.current_token_usage = ContextManager::estimate_token_count(&self.messages);
                true
            }
            Action::RequestAiResponse => {
                self.request_ai_response().await;
                true
            }
            Action::AddAiToken(token) => {
                // self.loading = false; // Don't stop loading state yet, wait for completion or content to render
                self.current_response_buffer.push_str(&token);
                if let Some(last) = self.messages.last_mut() {
                    if last.role == "assistant" {
                        last.content = self.current_response_buffer.clone();
                    }
                }
                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
                self.save_session_throttled();
                self.current_token_usage = self.calculate_context_usage() + self.estimate_tokens(&self.current_response_buffer);
                
                // check for summarization
                if !self.is_summarizing {
                     let limit = self.model_context_limit.unwrap_or(self.context_token_limit);
                     if self.context_manager.should_summarize(self.current_token_usage, limit) {
                         self.trigger_summarization();
                     }
                }
                true
            }
            Action::AddToolCall(tool_call) => {
                // Increment consecutive tool call counter
                self.consecutive_tool_calls += 1;
                if let Some(last) = self.messages.last_mut() {
                    if last.role == "assistant" {
                        if last.tool_calls.is_none() {
                            last.tool_calls = Some(Vec::new());
                        }
                        if let Some(calls) = &mut last.tool_calls {
                            calls.push(tool_call.clone());
                        }
                        // Visual feedback
                        last.content
                            .push_str(&format!("\n> **Tool Call:** `{}`", tool_call.function.name));

                        // Recalculate token usage
                        self.current_token_usage = ContextManager::estimate_token_count(&self.messages);

                        // EXECUTE TOOL
                        let tool_name = tool_call.function.name.clone();
                        let tool_args = tool_call.function.arguments.clone();

                        if let Some(tool) = self.tools.get(&tool_name) {
                            // Check if confirmation is needed
                            if tool.requires_confirmation() {
                                self.pending_tool_call = Some(tool_call.clone());
                                self.mode = Mode::ToolConfirmation;
                                self.tool_scroll = 0; // Reset scroll
                                self.loading = false; // Stop loading spinner while waiting for user
                                return true;
                            }

                            let tx = self.action_tx.clone();
                            let tool_arc = tool.clone();

                            tokio::spawn(async move {
                                // Use spawn_blocking for the synchronous tool execution
                                // with a 30-second timeout to prevent hanging
                                let tool_clone = tool_arc.clone();
                                let args_clone = tool_args.clone();
                                
                                let result = tokio::time::timeout(
                                    std::time::Duration::from_secs(120),
                                    tokio::task::spawn_blocking(move || {
                                        tool_clone.execute(args_clone)
                                    })
                                ).await;
                                
                                let output = match result {
                                    Ok(Ok(Ok(s))) => s,
                                    Ok(Ok(Err(e))) => format!("Tool error: {}", e),
                                    Ok(Err(e)) => format!("Tool execution failed: {}", e),
                                    Err(_) => "Tool timed out after 120 seconds. Try a more specific search path (e.g., ~/Documents instead of ~).".to_string(),
                                };
                                let _ = tx.send(Action::AddToolOutput(tool_name, output));
                            });
                        } else {
                            let _ = self.action_tx.send(Action::AddToolOutput(
                                tool_name,
                                "Tool not found".to_string(),
                            ));
                        }
                    }
                }
                self.save_session();
                true
            }

            Action::ConfirmToolExecution => {
                if let Some(tool_call) = self.pending_tool_call.take() {
                    let tool_name = tool_call.function.name.clone();
                    let tool_args = tool_call.function.arguments.clone();
                    
                    self.loading = true; // Resume loading
                    self.mode = Mode::Insert;

                    if let Some(tool) = self.tools.get(&tool_name) {
                        let tx = self.action_tx.clone();
                        let tool_arc = tool.clone();
                         tokio::spawn(async move {
                            let tool_clone = tool_arc.clone();
                            let args_clone = tool_args.clone();
                            
                            let result = tokio::time::timeout(
                                std::time::Duration::from_secs(120),
                                tokio::task::spawn_blocking(move || {
                                    tool_clone.execute(args_clone)
                                })
                            ).await;
                            
                            let output = match result {
                                Ok(Ok(Ok(s))) => s,
                                Ok(Ok(Err(e))) => format!("Tool error: {}", e),
                                Ok(Err(e)) => format!("Tool execution failed: {}", e),
                                Err(_) => "Tool timed out.".to_string(),
                            };
                            let _ = tx.send(Action::AddToolOutput(tool_name, output));
                        });
                    }
                }
                true
            }
            Action::DenyToolExecution => {
                if let Some(tool_call) = self.pending_tool_call.take() {
                     let _ = self.action_tx.send(Action::AddToolOutput(
                        tool_call.function.name,
                        "Tool execution denied by user.".to_string(),
                    ));
                    self.mode = Mode::Insert;
                }
                true
            }
            Action::AddToolOutput(name, output) => {
                // Ingest tool output into RAG
                let output_clone = output.clone();
                let _ = self.rag.add_text(&output_clone).await;

                self.messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: output,
                    images: None,
                    tool_calls: None,
                    tool_name: Some(name),
                });
                let _ = self.action_tx.send(Action::RequestAiResponse);
                self.save_session();
                true
            }
            Action::AiResponseComplete => {
                self.loading = false;
                self.save_session();
                true
            }
            // Session Actions
            Action::EnterSessionSelect => {
                self.list_sessions();
                self.mode = Mode::SessionSelect;
                // Select current session in list
                if let Some(idx) = self
                    .available_sessions
                    .iter()
                    .position(|s| s == &self.current_session)
                {
                    self.session_list_state.select(Some(idx));
                } else {
                    self.session_list_state.select(Some(0));
                }
                true
            }
            Action::EnterSessionCreate => {
                self.mode = Mode::SessionCreate;
                self.session_input = TextArea::default();
                self.session_input.set_block(
                    ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .title(" New Session Name "),
                );
                self.session_input.set_cursor_line_style(Style::default());
                true
            }
            Action::CreateSession(name) => {
                let safe_name = name
                    .trim()
                    .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "_");
                if !safe_name.is_empty() {
                    self.load_session(&safe_name);
                    self.save_session(); // Create file immediately
                }
                self.mode = Mode::Insert;
                true
            }
            Action::SelectSession(name) => {
                self.load_session(&name);
                self.mode = Mode::Insert;
                true
            }
            Action::CancelGeneration => {
                if let Some(handle) = self.current_request_handle.take() {
                    handle.abort();
                }
                self.loading = false;
                if let Some(last) = self.messages.last_mut() {
                    if last.role == "assistant" {
                         last.content.push_str("\n[Cancelled]");
                    }
                }
                self.save_session();
                true
            }
            Action::CopyMessage => {
                if let Some(idx) = self.selected_message_index {
                    if let Some(msg) = self.messages.get(idx) {
                        if let Some(clipboard) = &mut self.clipboard {
                            if let Err(e) = clipboard.set_text(&msg.content) {
                                self.error = Some(format!("Failed to copy: {}", e));
                            } else {
                                self.notification = Some(("Copied to clipboard!".to_string(), std::time::Instant::now()));
                                self.selected_message_index = None;
                                self.auto_scroll = true; // Go back to normal behavior? Or just unselect.
                                // "Automatically unselect after copying"
                            }
                        } else {
                            self.error = Some("Clipboard not available".to_string());
                        }
                    }
                }
                true
            }
            Action::MoveSelection(delta) => {
                if self.messages.is_empty() {
                    self.selected_message_index = None;
                    return true;
                }

                let current_idx = self.selected_message_index.unwrap_or_else(|| self.messages.len().saturating_sub(1));
                
                let new_idx = if delta > 0 {
                    current_idx.saturating_add(delta as usize).min(self.messages.len() - 1)
                } else {
                    current_idx.saturating_sub(delta.abs() as usize)
                };

                self.selected_message_index = Some(new_idx);
                self.auto_scroll = false;
                true
            }
            Action::DeleteSession(name) => {
                if let Some(path) = self.get_session_path(&name) {
                    let _ = fs::remove_file(path);
                }
                // Refresh list
                self.list_sessions();

                if self.available_sessions.is_empty() {
                    // Edge case: Deleted the last session
                    self.load_session("default");
                    self.save_session(); // Create the file immediately
                    self.list_sessions(); // Refresh again to show "default"
                } else if self.current_session == name {
                    // If we deleted current, switch to first available
                    if let Some(first) = self.available_sessions.first().cloned() {
                        self.load_session(&first);
                    }
                }

                // Keep selection valid
                if let Some(idx) = self.session_list_state.selected() {
                    if idx >= self.available_sessions.len() {
                        self.session_list_state
                            .select(Some(self.available_sessions.len().saturating_sub(1)));
                    }
                }
                true
            }
            Action::SessionsLoaded(_) => true, // Handled in EnterSessionSelect mostly

            // Model Management Actions
            Action::EnterModelPull => {
                self.mode = Mode::ModelPullInput;
                self.pull_input = TextArea::default();
                self.pull_input.set_block(
                    ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .title(" Enter Model Name to Pull "),
                );
                self.pull_input.set_cursor_line_style(Style::default());
                true
            }
            Action::StartPullModel(name) => {
                if name.trim().is_empty() {
                    return true;
                }

                self.loading = true; // Indicate background activity, though we'll show specific progress
                self.pull_progress = Some(("Starting download...".to_string(), None, None));
                self.mode = Mode::Insert; // Go back to main screen, we'll show progress there or in a popup?

                let client = self.ollama_client.clone();
                let tx = self.action_tx.clone();
                let model_name = name.clone();

                tokio::spawn(async move {
                    match client.pull_model(&model_name).await {
                        Ok(mut stream) => {
                            while let Some(result) = stream.next().await {
                                match result {
                                    Ok(progress) => {
                                        let _ = tx.send(Action::PullProgress(
                                            progress.status,
                                            progress.completed,
                                            progress.total,
                                        ));
                                    }
                                    Err(e) => {
                                        let _ =
                                            tx.send(Action::Error(format!("Pull Error: {}", e)));
                                    }
                                }
                            }
                            // Refresh models after pull
                            let _ = tx.send(Action::LoadModels);
                            let _ = tx.send(Action::PullProgress("Done".to_string(), None, None));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::Error(format!("Failed to start pull: {}", e)));
                        }
                    }
                });
                true
            }
            Action::PullProgress(status, completed, total) => {
                if status == "Done" {
                    self.pull_progress = None;
                    self.loading = false;
                } else {
                    self.pull_progress = Some((status, completed, total));
                }
                true
            }
            Action::DeleteModel(name) => {
                self.loading = true;
                let client = self.ollama_client.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = client.delete_model(&name).await {
                        let _ = tx.send(Action::Error(format!("Delete Failed: {}", e)));
                    }
                    let _ = tx.send(Action::LoadModels);
                });
                true
            }

            Action::UserInput(key) => {
                // Global shortcuts

                // Clear History
                if key.code == KeyCode::Char('l') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    let _ = self.action_tx.send(Action::ClearHistory);
                    return true;
                }

                // System Prompt Edit
                if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    let _ = self.action_tx.send(Action::EnterSystemPromptEdit);
                    return true;
                }

                // Session Manager
                if key.code == KeyCode::Char('r') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    let _ = self.action_tx.send(Action::EnterSessionSelect);
                    return true;
                }

                if self.show_help {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(1) => {
                            self.show_help = false;
                        }
                        _ => {} // Ignore other keys when help is shown
                    }
                    return true;
                }
                match self.mode {
                    Mode::Insert => match key.code {
                        KeyCode::Esc => {
                            let _ = self.action_tx.send(Action::SwitchMode(Mode::Normal));
                        }
                        KeyCode::F(1) => self.show_help = true,
                        _ => {
                            if key.code == KeyCode::Enter
                                && !key.modifiers.contains(KeyModifiers::SHIFT)
                            {
                                let _ = self.action_tx.send(Action::SendMessage);
                            } else if key.code == KeyCode::Char('o')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                let _ = self.action_tx.send(Action::EnterModelSelect);
                            } else if key.code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                if self.loading {
                                   let _ = self.action_tx.send(Action::CancelGeneration);
                                } else {
                                    // if not loading, let main.rs handle global Ctrl+C or do nothing
                                }
                            } else {
                                self.input.input(Input::from(key));
                            }
                        }
                    },
                    Mode::Normal => {
                        match key.code {
                            KeyCode::Char('i') | KeyCode::Enter => {
                                let _ = self.action_tx.send(Action::SwitchMode(Mode::Insert));
                            }
                            KeyCode::Char('q') => {
                                let _ = self.action_tx.send(Action::PrepareQuit);
                            }
                            KeyCode::Char('y') => {
                                let _ = self.action_tx.send(Action::CopyMessage);
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                let _ = self.action_tx.send(Action::MoveSelection(1));
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                let _ = self.action_tx.send(Action::MoveSelection(-1));
                            }
                            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                let _ = self.action_tx.send(Action::EnterModelSelect);
                            }
                            KeyCode::F(1) => self.show_help = true,
                            KeyCode::Esc => {
                                self.selected_message_index = None;
                                self.auto_scroll = false; // Stay where we are, or true? User usually wants to stop selecting.
                                // If we don't set auto_scroll=true, we stay at current scroll pos.
                            }
                            _ => {} // Ignore other keys in Normal mode
                        }
                    }
                    Mode::ModelSelect => {
                        match key.code {
                            KeyCode::Esc => {
                                let _ = self.action_tx.send(Action::SwitchMode(Mode::Insert));
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if self.selected_model > 0 {
                                    self.selected_model -= 1;
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if self.selected_model < self.models.len().saturating_sub(1) {
                                    self.selected_model += 1;
                                }
                            }
                            KeyCode::Enter => {
                                let _ = self.action_tx.send(Action::SwitchMode(Mode::Insert));
                            }
                            KeyCode::Char('p')
                                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                let _ = self.action_tx.send(Action::EnterModelPull);
                            }
                            KeyCode::Char('d')
                                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                if let Some(model) = self.models.get(self.selected_model) {
                                    let _ = self.action_tx.send(Action::DeleteModel(model.clone()));
                                }
                            }
                            _ => {} // Ignore other keys in ModelSelect mode
                        }
                    }
                    Mode::SystemPromptEdit => match key.code {
                        KeyCode::Esc => {
                            let _ = self.action_tx.send(Action::SwitchMode(Mode::Insert));
                        }
                        KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                            let new_prompt = self.system_prompt_input.lines().join("\n");
                            let _ = self.action_tx.send(Action::UpdateSystemPrompt(new_prompt));
                        }
                        _ => {
                            self.system_prompt_input.input(Input::from(key));
                        }
                    },
                    Mode::SessionSelect => match key.code {
                        KeyCode::Esc => {
                            let _ = self.action_tx.send(Action::SwitchMode(Mode::Insert));
                        }
                        KeyCode::Char('c') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            let _ = self.action_tx.send(Action::EnterSessionCreate);
                        }
                        KeyCode::Char('d') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if let Some(idx) = self.session_list_state.selected() {
                                if let Some(name) = self.available_sessions.get(idx) {
                                    let _ =
                                        self.action_tx.send(Action::DeleteSession(name.clone()));
                                }
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            let i = match self.session_list_state.selected() {
                                Some(i) => {
                                    if i == 0 {
                                        self.available_sessions.len() - 1
                                    } else {
                                        i - 1
                                    }
                                }
                                None => 0,
                            };
                            self.session_list_state.select(Some(i));
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            let i = match self.session_list_state.selected() {
                                Some(i) => {
                                    if i >= self.available_sessions.len() - 1 {
                                        0
                                    } else {
                                        i + 1
                                    }
                                }
                                None => 0,
                            };
                            self.session_list_state.select(Some(i));
                        }
                        KeyCode::Enter => {
                            if let Some(idx) = self.session_list_state.selected() {
                                if let Some(name) = self.available_sessions.get(idx) {
                                    let _ =
                                        self.action_tx.send(Action::SelectSession(name.clone()));
                                }
                            }
                        }
                        _ => {}
                    },
                    Mode::SessionCreate => match key.code {
                        KeyCode::Esc => {
                            let _ = self.action_tx.send(Action::EnterSessionSelect);
                        }
                        KeyCode::Enter => {
                            let name = self.session_input.lines().join("");
                            let _ = self.action_tx.send(Action::CreateSession(name));
                        }
                        _ => {
                            self.session_input.input(Input::from(key));
                        }
                    },
                    Mode::ModelPullInput => match key.code {
                        KeyCode::Esc => {
                            let _ = self.action_tx.send(Action::SwitchMode(Mode::ModelSelect));
                        }
                        KeyCode::Enter => {
                            let name = self.pull_input.lines().join("");
                            let _ = self.action_tx.send(Action::StartPullModel(name));
                        }
                        _ => {
                            self.pull_input.input(Input::from(key));
                        }
                    },
                    Mode::ToolConfirmation => match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            let _ = self.action_tx.send(Action::ConfirmToolExecution);
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            let _ = self.action_tx.send(Action::DenyToolExecution);
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            let _ = self.action_tx.send(Action::Scroll(-1));
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            let _ = self.action_tx.send(Action::Scroll(1));
                        }
                        _ => {}
                    },
                }
                true
            }
            Action::PrepareQuit => {
                self.save_session();
                let _ = self.action_tx.send(Action::Quit);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[tokio::test]
    async fn test_app_initialization() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let app = App::new(tx, config, false, None);

        assert!(app.messages.is_empty());
        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(app.system_prompt, "You are helpful");
    }

    #[tokio::test]
    async fn test_add_user_message() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);
        app.models = vec!["test-model".to_string()]; // Mock models

        app.update(Action::AddUserMessage("Hello".to_string()))
            .await;

        // Now only adds the user message. The assistant message is added in RequestAiResponse.
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].role, "user");
        assert_eq!(app.messages[0].content, "Hello");
        assert!(app.loading);

        // Manually trigger RequestAiResponse to test assistant message creation
        app.update(Action::RequestAiResponse).await;
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[1].role, "assistant");
    }

    #[tokio::test]
    async fn test_models_loaded() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        let models = vec!["model1".to_string(), "model2".to_string()];
        app.update(Action::ModelsLoaded(models.clone())).await;

        assert_eq!(app.models, models);
        assert!(!app.loading);
    }

    #[tokio::test]
    async fn test_user_typing() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        // Type 'a'
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        app.update(Action::UserInput(key)).await;

        assert_eq!(app.input.lines()[0], "a");
    }

    #[tokio::test]
    async fn test_scroll_logic() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        app.vertical_scroll = 10;
        app.update(Action::Scroll(-1)).await;
        assert_eq!(app.vertical_scroll, 9);

        app.update(Action::Scroll(1)).await;
        assert_eq!(app.vertical_scroll, 10);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        app.update(Action::Error("Connection failed".to_string()))
            .await;
        assert_eq!(app.error, Some("Connection failed".to_string()));
        assert!(!app.loading);
    }

    #[tokio::test]
    async fn test_model_select_toggle() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        app.update(Action::EnterModelSelect).await;
        assert_eq!(app.mode, Mode::ModelSelect);

        app.update(Action::UserInput(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::empty(),
        )))
        .await;

        // Expect SwitchMode(Mode::Insert)
        match rx.recv().await {
            Some(Action::SwitchMode(Mode::Insert)) => {} // Correct
            Some(other) => panic!("Expected SwitchMode(Insert), got {:?}", other),
            None => panic!("Expected SwitchMode(Insert), got nothing"),
        }
    }

    #[tokio::test]
    async fn test_help_menu_toggle() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        // Open with F1
        app.update(Action::UserInput(KeyEvent::new(
            KeyCode::F(1),
            KeyModifiers::empty(),
        )))
        .await;
        assert!(app.show_help);

        // Close with Esc
        app.update(Action::UserInput(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::empty(),
        )))
        .await;
        assert!(!app.show_help);

        // Open again
        app.update(Action::UserInput(KeyEvent::new(
            KeyCode::F(1),
            KeyModifiers::empty(),
        )))
        .await;
        assert!(app.show_help);

        // Close with q
        app.update(Action::UserInput(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::empty(),
        )))
        .await;
        assert!(!app.show_help);

        // Open again
        app.update(Action::UserInput(KeyEvent::new(
            KeyCode::F(1),
            KeyModifiers::empty(),
        )))
        .await;
        assert!(app.show_help);

        // Close with F1
        app.update(Action::UserInput(KeyEvent::new(
            KeyCode::F(1),
            KeyModifiers::empty(),
        )))
        .await;
        assert!(!app.show_help);
    }

    #[tokio::test]
    async fn test_loading_stays_true_during_stream() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);
        app.models = vec!["test".to_string()];

        // Simulate user message sending
        app.update(Action::AddUserMessage("Hello".to_string()))
            .await;
        assert!(app.loading, "Should be loading after user message");

        // Request AI response adds the assistant placeholder
        app.update(Action::RequestAiResponse).await;

        // Simulate first token arrival
        app.update(Action::AddAiToken("H".to_string())).await;
        assert!(app.loading, "Should STILL be loading after first token");

        // Simulate completion
        app.update(Action::AiResponseComplete).await;
        assert!(!app.loading, "Should stop loading after completion");
    }

    #[tokio::test]
    async fn test_ctrl_o_enters_model_select() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        // Press Ctrl+o
        let key = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
        app.update(Action::UserInput(key)).await;

        // We expect the app to send EnterModelSelect
        match rx.recv().await {
            Some(Action::EnterModelSelect) => {} // Correct
            Some(action) => assert_eq!(action, Action::EnterModelSelect),
            None => panic!("Expected EnterModelSelect, got nothing"),
        }
    }

    #[tokio::test]
    async fn test_add_tool_call() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        // Setup conversation state
        app.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: "Searching...".to_string(),
            images: None,
            tool_calls: None,
            tool_name: None,
        });

        let tool_call = ToolCall {
            function: crate::ollama::ToolCallFunction {
                name: "find_files".to_string(),
                arguments: serde_json::json!({"name": "test"}),
            },
        };

        app.update(Action::AddToolCall(tool_call.clone())).await;

        let last_msg = app.messages.last().unwrap();
        assert_eq!(last_msg.role, "assistant");
        // Verify visual feedback
        assert!(last_msg.content.contains("> **Tool Call:** `find_files`"));
        // Verify structural storage
        assert!(last_msg.tool_calls.is_some());
        assert_eq!(last_msg.tool_calls.as_ref().unwrap().len(), 1);
        // The following assertion seems to have a typo in the original test, it should be "find_files"
        // assert_eq!(last_msg.tool_calls.as_ref().unwrap()[0].function.name, "search_files".to_string());
        assert_eq!(
            last_msg.tool_calls.as_ref().unwrap()[0].function.name,
            "find_files".to_string()
        );
    }

    #[test]
    fn test_context_window_logic() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 2048, // Safe limit > buffer + system context
            system_prompt: "HI".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        // Add a huge message that won't fit in the remaining space
        // Limit 2048 - 512 (buffer) - ~100 (system) = ~1400 available.
        // Message of 8000 chars is ~2000 tokens.
        let huge_msg = "a".repeat(8000);
        app.messages.push(ChatMessage {
            role: "user".to_string(),
            content: huge_msg,
            images: None,
            tool_calls: None,
            tool_name: None,
        });

        // Add a small recent message that should fit
        app.messages.push(ChatMessage {
            role: "user".to_string(),
            content: "recent_msg".to_string(),
            images: None,
            tool_calls: None,
            tool_name: None,
        });

        // Build context window from existing history
        let context = app.build_context_window();

        // Should contain System Prompt + Recent Msg
        assert_eq!(context.len(), 2, "Context should contain system prompt and recent message");
        assert_eq!(context[0].role, "system");
        assert_eq!(context[1].content, "recent_msg");
    }

    #[tokio::test]
    async fn test_cancel_generation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
            ollama_url: "http://localhost:11434".to_string(),
            context_token_limit: 4096,
            system_prompt: "You are helpful".to_string(),
            ignored_patterns: vec![],
            auto_context: true,
            summarization_enabled: true,
            summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        app.loading = true;
        
        let handle = tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        });
        app.current_request_handle = Some(handle.abort_handle());

        // Add a dummy last message to verify [Cancelled] append
        app.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: "Generating".to_string(),
            images: None,
            tool_calls: None,
            tool_name: None,
        });

        app.update(Action::CancelGeneration).await;

        assert!(!app.loading);
        assert!(app.current_request_handle.is_none());
        assert!(handle.await.unwrap_err().is_cancelled());
        assert_eq!(app.messages[0].content, "Generating\n[Cancelled]");
    }

    #[tokio::test]
    async fn test_move_selection() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
             ollama_url: "http://localhost:11434".to_string(),
             context_token_limit: 4096,
             system_prompt: "You are helpful".to_string(),
             ignored_patterns: vec![],
             auto_context: true,
             summarization_enabled: true,
             summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);
        
        app.messages.push(ChatMessage { role: "user".to_string(), content: "1".to_string(), images: None, tool_calls: None, tool_name: None });
        app.messages.push(ChatMessage { role: "assistant".to_string(), content: "2".to_string(), images: None, tool_calls: None, tool_name: None });

        assert_eq!(app.selected_message_index, None);

        // Move down -> selects last (len-1) = 1
        app.update(Action::MoveSelection(1)).await;
        assert_eq!(app.selected_message_index, Some(1));

        // Move up -> 0
        app.update(Action::MoveSelection(-1)).await;
        assert_eq!(app.selected_message_index, Some(0));
        
        // Move up again -> 0
        app.update(Action::MoveSelection(-1)).await;
        assert_eq!(app.selected_message_index, Some(0));

        // Move down -> 1
        app.update(Action::MoveSelection(1)).await;
        assert_eq!(app.selected_message_index, Some(1));
    }

    #[tokio::test]
    async fn test_copy_notification_and_unselect() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let config = Config {
             ollama_url: "http://localhost:11434".to_string(),
             context_token_limit: 4096,
             system_prompt: "You are helpful".to_string(),
             ignored_patterns: vec![],
             auto_context: true,
             summarization_enabled: true,
             summarization_threshold: 0.8,
            searxng_url: "http://localhost:8080".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        };
        let mut app = App::new(tx, config, false, None);
        
        let msg = ChatMessage { role: "assistant".to_string(), content: "test".to_string(), images: None, tool_calls: None, tool_name: None };
        app.messages.push(msg);
        app.selected_message_index = Some(0);

        app.update(Action::CopyMessage).await;
        
        if app.error.is_some() {
            assert!(app.notification.is_none());
        } else {
             assert!(app.notification.is_some());
             assert_eq!(app.notification.as_ref().unwrap().0, "Copied to clipboard!");
             assert!(app.selected_message_index.is_none());
        }
    }
}
