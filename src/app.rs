use crate::config::Config;
use crate::ollama::{ChatMessage, ChatMessageRequest, ChatStreamEvent, OllamaClient, ToolCall};
use crate::tools::{CatTool, FileSearchTool, GrepTool, ListDirectoryTool, Tool};
use crossterm::event::{KeyCode, KeyModifiers};
use directories::{BaseDirs, ProjectDirs};
use futures::StreamExt;
use ratatui::style::Style;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;
use tokio::sync::mpsc;
use tui_textarea::{Input, TextArea};

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
}

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

        let mut tools: HashMap<String, Arc<dyn Tool>> = HashMap::new();
        tools.insert("find_files".to_string(), Arc::new(FileSearchTool));
        tools.insert("grep_files".to_string(), Arc::new(GrepTool));
        tools.insert("read_file".to_string(), Arc::new(CatTool));
        tools.insert("list_directory".to_string(), Arc::new(ListDirectoryTool));

        let mut app = Self {
            ollama_client: OllamaClient::new(config.ollama_url),
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

    fn build_context_window(&self) -> Vec<ChatMessageRequest> {
        let mut context_messages: Vec<ChatMessageRequest> = Vec::new();
        let system_prompt_tokens = self.estimate_tokens(&self.system_prompt);
        let mut current_tokens = system_prompt_tokens;

        // Iterate backwards through history to fit as many recent messages as possible
        for msg in self.messages.iter().rev() {
            let msg_tokens = self.estimate_tokens(&msg.content);
            if current_tokens + msg_tokens > self.context_token_limit {
                break;
            }
            context_messages.push(ChatMessageRequest {
                role: msg.role.clone(),
                content: msg.content.clone(),
                images: msg.images.clone(),
                tool_calls: msg.tool_calls.clone(),
            });
            current_tokens += msg_tokens;
        }

        // Restore chronological order
        context_messages.reverse();

        // Prepend system prompt
        context_messages.insert(
            0,
            ChatMessageRequest {
                role: "system".to_string(),
                content: self.system_prompt.clone(),
                images: None,
                tool_calls: None,
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
                let _ = fs::write(path, json);
            }
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
            });
            self.current_response_buffer.clear();
        }

        let context_messages = self.build_context_window();
        let model = self.models[self.selected_model].clone();
        let client = self.ollama_client.clone();
        let tx = self.action_tx.clone();

        let tool_definitions = if !self.tools.is_empty() {
            Some(self.tools.values().map(|t| t.definition()).collect())
        } else {
            None
        };

        tokio::spawn(async move {
            match client
                .chat(&model, context_messages, tool_definitions)
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
                }
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
                self.auto_scroll = false;
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
                self.messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: msg,
                    images: None,
                    tool_calls: None,
                });
                self.loading = true;
                self.scroll_to_bottom();
                self.save_session();
                let _ = self.action_tx.send(Action::RequestAiResponse);
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
                self.save_session();
                true
            }
            Action::AddToolCall(tool_call) => {
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
                            .push_str(&format!("\n[Tool Call: {}]", tool_call.function.name));

                        // EXECUTE TOOL
                        let tool_name = tool_call.function.name.clone();
                        let tool_args = tool_call.function.arguments.clone();

                        if let Some(tool) = self.tools.get(&tool_name) {
                            let tx = self.action_tx.clone();
                            let tool_arc = tool.clone();

                            tokio::spawn(async move {
                                let result = tool_arc.execute(tool_args);
                                let output = match result {
                                    Ok(s) => s,
                                    Err(e) => format!("Error: {}", e),
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
            Action::AddToolOutput(_name, output) => {
                self.messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: output,
                    images: None,
                    tool_calls: None,
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
                                let _ = self.action_tx.send(Action::Quit);
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                self.vertical_scroll = self.vertical_scroll.saturating_add(1);
                                self.auto_scroll = false;
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                self.vertical_scroll = self.vertical_scroll.saturating_sub(1);
                                self.auto_scroll = false;
                            }
                            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                let _ = self.action_tx.send(Action::EnterModelSelect);
                            }
                            KeyCode::F(1) => self.show_help = true,
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
                }
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
        };
        let mut app = App::new(tx, config, false, None);

        // Setup conversation state
        app.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: "Searching...".to_string(),
            images: None,
            tool_calls: None,
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
        assert!(last_msg.content.contains("[Tool Call: find_files]"));
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
            context_token_limit: 12,
            system_prompt: "HI".to_string(),
        };
        let mut app = App::new(tx, config, false, None);

        app.messages.push(ChatMessage {
            role: "user".to_string(),
            content: "msg1".to_string(),
            images: None,
            tool_calls: None,
        });
        app.messages.push(ChatMessage {
            role: "user".to_string(),
            content: "msg3".to_string(),
            images: None,
            tool_calls: None,
        });

        // Build context window from existing history
        let context = app.build_context_window();

        assert_eq!(context.len(), 2);
        assert_eq!(context[0].role, "system");
        assert_eq!(context[1].content, "msg3");
    }
}
