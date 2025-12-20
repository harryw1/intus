# Intus Project Context

## Project Overview
`intus` is a robust, privacy-first Local Autonomous Agent and System Sidecar written in Rust. It integrates with [Ollama](https://ollama.com/) to provide an AI assistant that lives in your terminal, capable of proactive assistance, local knowledge management, and web research.

## Key Technologies
- **Language:** Rust
- **TUI Framework:** `ratatui`
- **Terminal Backend:** `crossterm`
- **Async Runtime:** `tokio`
- **HTTP Client:** `reqwest`
- **Serialization:** `serde`, `serde_json`
- **Config:** `toml`
- **Local Search:** `ignore` (ripgrep-like walking)

## Architecture & Codebase Structure

### `src/` Directory
- **`main.rs`**: Entry point. Sets up the terminal, loads configuration, and starts the main event loop.
- **`app.rs`**: Core application state and event handling logic (`Action` enum). Manages session auto-naming and tool outputs.
- **`ui.rs`**: TUI rendering logic using `ratatui`.
- **`ollama.rs`**: Client for the Ollama API.
- **`config.rs`**: Configuration loading. Defaults to `~/.config/intus/config.toml`.
- **`context.rs`**: Context management and system context generation.
- **`persistence.rs`**: Session persistence and loading.
- **`rag.rs`**: RAG system core. Manages vector storage with **Collection Isolation** (work/personal/web).
- **`theme.rs`**: UI theming and color definitions.
- **`process.rs`**: Child process management.
- **`logging.rs`**: Application logging.
- **`lib.rs`**: Library exports.

### `src/tools/` Directory
- **`mod.rs`**: Tool trait definition and exports.
- **`filesystem.rs`**: `read_file` (line-numbered), `edit_file` (line-based), `grep_files`, `list_directory`, `write_file`, `replace_text`.
- **`rag.rs`**: `semantic_search` (background indexing with status updates), `remember`.
- **`web.rs`**: `web_search`, `read_url` (isolated "web" collection).
- **`system.rs`**: `run_command`.

### Other Directories
- **`tests/`**: Integration and unit tests.
- **`homebrew/`**: Homebrew formula for installation.
- **`.github/workflows/`**: CI/CD workflows.

## Key Features

### 1. The "Sidecar" Workflow
- **Background Indexing**: Large knowledge bases index in the background without freezing the UI.
- **Auto-Naming**: Sessions are automatically named based on context (e.g., `fix_rust_bug`) after the first exchange.
- **Status Updates**: Transient notifications keep the user informed of background tasks.

### 2. Knowledge Management
- **Collections**: Data is strictly isolated. "Work" docs are never mixed with "Web" search results.
- **Named Bases**: Users define `[knowledge_bases]` in config (e.g., `work = "~/Docs/Work"`).
- **Smart Search**: `semantic_search(query, index_path="work")` targets specific memory.

### 3. Robust Tooling
- **Safe Edits**: `edit_file` uses line numbers to prevent code corruption.
- **Web Isolation**: Web searches are sandboxed in a "web" vector collection.

## Setup & Usage

### Prerequisites
1. **Ollama**: Running locally.
2. **SearXNG** (Optional): For web search (`docker run -d -p 8080:8080 searxng/searxng`).

### Configuration
- **Location**: `~/.config/intus/config.toml`
- **Key Settings**:
  ```toml
  [knowledge_bases]
  work = "~/Documents/Work"
  notes = "~/Notes"
  ```

### Building
```bash
cargo build --release
cargo run
```

### Testing
```bash
cargo test
```