# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.2] - 2025-12-20

### Fixed
- Improved date/time context injection for time-sensitive web searches.

## [1.0.1] - 2025-12-20

### Added
- Homebrew formula for easy installation.
- Automated release workflow via GitHub Actions.

### Fixed
- Terminal cleanup on exit in release builds.

## [1.0.0] - 2025-12-19

### Added
- Initial release of Intus.
- Local RAG system with named knowledge bases and collection isolation.
- Background indexing for large directories.
- Tool suite: `read_file`, `edit_file`, `grep_files`, `run_command`, `web_search`, `read_url`, `semantic_search`, `remember`.
- Session auto-naming based on conversation context.
- Vim-style modal editing (Insert/Normal modes).
- Markdown rendering with syntax highlighting.
- Configuration via `~/.config/intus/config.toml`.

[1.0.2]: https://github.com/harryw1/intus/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/harryw1/intus/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/harryw1/intus/releases/tag/v1.0.0
