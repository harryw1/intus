# Intus

> **Intus**: (Latin) Within, inside, inward.


**Intus** is a robust, privacy-first Local Autonomous Agent and System Sidecar for your terminal. It empowers you to interact with your local file system, knowledge bases, and the web through a context-aware AI interface, all while keeping your data strictly local (via [Ollama](https://ollama.com/)).

![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)

## ✨ Key Features

- **🛡️ Privacy First**: Built for local models. Your data stays on your machine.
- **🧠 Local RAG (Retrieval-Augmented Generation)**:
  - **Named Knowledge Bases**: Define "Work", "Personal", or "Code" folders in your config.
  - **Context Isolation**: Search results are strictly segregated to prevent data leaks.
  - **Background Indexing**: Add massive folders without freezing the UI.
## Local Installation (Developers)

To install the current local version via Homebrew (useful for testing):

```bash
make install
```

## Release Process (Maintainers)
- **⚡ Autonomous Tools**:
  - **Safe Code Editing**: Line-based editing (`edit_file`) prevents "hallucinated" file corruption.
  - **Web Research**: Search the web and read pages (via SearXNG) with auto-summarization.
  - **System Control**: Execute shell commands, manage git, and inspect files.
- **🎨 Polished UX**:
  - **Auto-Naming Sessions**: "fix_bug_ui" instead of "Session 1".
  - **Transient Notifications**: Real-time status updates for background tasks.
  - **Rich TUI**: Markdown rendering, syntax highlighting, and smooth scrolling.

## 🚀 Getting Started

### Prerequisites

1.  **Rust**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2.  **Ollama**: [Download & Install](https://ollama.com/).
3.  **SearXNG** (Optional): For web search capabilities.

### Installation

### Homebrew

```bash
# Register the tap (if you create one)
brew tap harryw1/intus
brew install intus

# Or install directly from the formula
brew install --HEAD https://raw.githubusercontent.com/harryw1/intus/master/homebrew/intus.rb
```

### Binaries

Download the latest pre-built binary for macOS or Linux from the [Releases](https://github.com/harryw1/intus/releases) page.

### From Source

```bash
git clone https://github.com/harryw1/intus.git
cd intus
cargo build --release
./target/release/intus
```

### Configuration

Intus creates a config file at `~/.config/intus/config.toml` on first run.

**Recommended Setup:**

```toml
[knowledge_bases]
work = "~/Documents/Work"
personal = "~/Notes"
projects = "~/Code"

[server]
ollama_url = "http://localhost:11434"
searxng_url = "http://localhost:8080"
```

## ⌨️ Shortcuts

| Key | Action |
| --- | --- |
| `Ctrl+o` | Select Model |
| `Ctrl+r` | Manage Sessions |
| `Ctrl+s` | Edit System Prompt |
| `Ctrl+l` | Clear History |
| `F1` | Help Menu |
| `Esc` | Normal Mode (Vim-style navigation) |
| `i` | Insert Mode |

## 🛠️ Architecture

Intus is built with the Rust TUI ecosystem:
- **Ratatui**: UI Rendering.
- **Tokio**: Async runtime for non-blocking tools.
- **Local Embeddings**: Uses `nomic-embed-text` (via Ollama) for vector search.

## License

MIT
