# Contributing to Intus

Thank you for your interest in contributing to Intus! This document provides guidelines and instructions for contributing.

## Getting Started

### Prerequisites

- **Rust** (stable): Install via [rustup](https://rustup.rs/)
- **Ollama**: [Download & Install](https://ollama.com/)
- **SearXNG** (optional): For web search features

### Building from Source

```bash
git clone https://github.com/harryw1/intus.git
cd intus
cargo build
```

### Running Tests

```bash
# Run all tests
cargo test

# Run a specific test
cargo test test_name
```

### Running the Application

```bash
# Development build
cargo run

# Release build (optimized)
cargo build --release
./target/release/intus
```

## Development Workflow

1. **Fork** the repository and create a feature branch.
2. **Make changes** following the coding style below.
3. **Test** your changes thoroughly.
4. **Commit** with clear, descriptive messages.
5. **Open a Pull Request** against `master`.

## Coding Style

- Follow standard Rust conventions (`rustfmt`).
- Use meaningful variable and function names.
- Add doc comments (`///`) for public functions.
- Keep functions focused and reasonably sized.

### Formatting

Before committing, run:

```bash
cargo fmt
cargo clippy
```

## Project Structure

```
src/
├── main.rs          # Entry point, terminal setup
├── app.rs           # Core application state and event handling
├── ui.rs            # TUI rendering (ratatui)
├── ollama.rs        # Ollama API client
├── config.rs        # Configuration loading
├── rag.rs           # RAG system core
├── context.rs       # Context management
├── persistence.rs   # Session persistence
├── theme.rs         # UI theming
├── logging.rs       # Application logging
├── process.rs       # Child process management
└── tools/           # Tool implementations
    ├── mod.rs
    ├── filesystem.rs
    ├── rag.rs
    ├── system.rs
    └── web.rs
```

## Adding a New Tool

1. Create or modify a file in `src/tools/`.
2. Implement the `Tool` trait.
3. Register the tool in `src/app.rs`.
4. Update the system prompt in `src/config.rs` if needed.
5. Add tests in `tests/`.

## Reporting Issues

When reporting bugs, please include:

- Intus version (`intus --version`)
- Ollama version
- Operating system
- Steps to reproduce
- Expected vs. actual behavior

## Questions?

Open an issue or start a discussion on GitHub.
