use super::{expand_path, Tool, TextChunk};
use anyhow::Result;
use serde_json::Value;
use std::process::Command;
use std::fs::OpenOptions;
use std::sync::Arc;
use crate::rag::RagSystem;

pub struct ListDirectoryTool {
    pub ignored_patterns: Vec<String>,
}

impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "USE THIS to see what files and folders exist in a directory. This is essential for exploring the file system. Shows file sizes, dates, and permissions. Use this FIRST to explore an unfamiliar directory before using other tools or if you need to check if a file exists."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The directory path to list (default: current directory)."
                }
            }
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let path = expand_path(args.get("path").and_then(|v| v.as_str()).unwrap_or("."));

        let mut cmd = Command::new("find");
        cmd.arg(path);
        cmd.arg("-maxdepth").arg("1");

        // Ignore patterns
        for ignore in &self.ignored_patterns {
            cmd.arg("-not");
            cmd.arg("-path");
            cmd.arg(format!("*/{}/*", ignore));
            cmd.arg("-not");
            cmd.arg("-path");
            cmd.arg(format!("*/{}", ignore));
        }

        cmd.arg("-ls");

        let output = cmd.output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "List directory failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.trim().is_empty() {
            Ok("Directory is empty or all items ignored.".to_string())
        } else {
            let lines: Vec<&str> = stdout.lines().take(50).collect();
            let result = lines.join("\n");
            if lines.len() < stdout.lines().count() {
                Ok(format!("{}\n... (truncated)", result))
            } else {
                Ok(result)
            }
        }
    }
}

pub struct GrepTool {
    pub ignored_patterns: Vec<String>,
}

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep_files"
    }

    fn description(&self) -> &str {
        "USE THIS to search for text patterns INSIDE files. This is your primary code search tool. Use it to find function definitions, variable usages, error messages, or specific strings. Supports case-insensitivity. Example: pattern='fn main', case_insensitive=false."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The text pattern to search for (e.g., 'TODO', 'fn main', 'error')."
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Use '~' for home directory, '~/Documents' for documents. Default: current directory."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Whether to search recursively in subdirectories (default: true)."
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Set to true for case-insensitive matching (default: false)."
                }
            },
            "required": ["pattern"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' argument"))?;

        let path = expand_path(args.get("path").and_then(|v| v.as_str()).unwrap_or("."));
        let recursive = args
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let case_insensitive = args
            .get("case_insensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Try `rg` (ripgrep) first
        let mut cmd = Command::new("rg");
        cmd.arg("--color=never");
        cmd.arg("--vimgrep"); 
        cmd.arg("--smart-case"); 
        
        if case_insensitive {
            cmd.arg("-i");
        } else {
            cmd.arg("-s"); 
        }

        if !recursive {
            cmd.arg("--max-depth=1");
        }

        for ignore in &self.ignored_patterns {
            cmd.arg("-g");
            cmd.arg(format!("!{}", ignore));
            cmd.arg("-g");
            cmd.arg(format!("!{}/", ignore));
        }

        cmd.arg("--max-columns=1000"); 
        cmd.arg("-I"); 

        cmd.arg(pattern);
        cmd.arg(&path);

        match cmd.output() {
            Ok(output) => {
                 let exit_code = output.status.code().unwrap_or(-1);
                 
                 if exit_code == 0 || exit_code == 1 {
                     let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                     if stdout.trim().is_empty() {
                         return Ok("No matches found.".to_string());
                     } else {
                        let lines: Vec<&str> = stdout.lines().take(50).collect();
                        let result = lines.join("\n");
                         if lines.len() < stdout.lines().count() {
                             return Ok(format!("{}\n... (truncated)", result));
                         } else {
                             return Ok(result);
                         }
                     }
                 }
            }
            Err(_) => {
                // Fallback to grep
            }
        }

        let mut cmd = Command::new("grep");
        cmd.arg("--color=never");
        cmd.arg("-n"); 
        cmd.arg("-I"); 
        cmd.arg("-H"); 
        if recursive {
            cmd.arg("-r");
        }
        if case_insensitive {
            cmd.arg("-i");
        }

        for ignore in &self.ignored_patterns {
            cmd.arg(format!("--exclude-dir={}", ignore));
            cmd.arg(format!("--exclude={}", ignore));
            cmd.arg(format!("--exclude={}/", ignore)); 
        }

        cmd.arg(pattern);
        cmd.arg(path);

        let output = cmd.output()?;
        
        let exit_code = output.status.code().unwrap_or(-1);
        if exit_code != 0 && exit_code != 1 && exit_code != 2 {
            return Err(anyhow::anyhow!(
                "Grep failed with exit code {}: {}",
                exit_code,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        
        if stdout.trim().is_empty() {
            Ok("No matches found.".to_string())
        } else {
            let lines: Vec<&str> = stdout.lines().take(50).collect();
            let result = lines.join("\n");
            
            if exit_code == 2 {
                Ok(format!("{}\n\n(Note: Some directories were skipped due to permission errors)", result))
            } else {
                Ok(result)
            }
        }
    }
}

pub struct CatTool {
    pub ignored_patterns: Vec<String>,
    pub rag: Arc<RagSystem>,
}

impl Tool for CatTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "USE THIS to read the full contents of a file. Use this AFTER you have located the file. Returns the complete file text content. If the file is extremely large, it will be truncated."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the file to read."
                },
                "numbered": {
                    "type": "boolean",
                    "description": "Whether to include line numbers in the output (default: true). Useful for editing."
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let raw_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;
        
        let numbered = args
            .get("numbered")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let path = expand_path(raw_path);

        if path.contains("..") {
            return Err(anyhow::anyhow!("Security: '..' not allowed"));
        }

        for ignore in &self.ignored_patterns {
            if path.contains(ignore) {
                return Err(anyhow::anyhow!(
                    "Access denied: Path contains ignored pattern '{}'",
                    ignore
                ));
            }
        }

        let output = Command::new("cat").arg(&path).output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Cat failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let content = String::from_utf8_lossy(&output.stdout);
        let content_str = content.to_string();
        
        let handle = tokio::runtime::Handle::current();
        let chunks = self.chunk_file_for_rag(&path, &content_str);
        let _ = handle.block_on(self.rag.add_chunks(chunks));

        let display_content = if numbered {
            content_str.lines()
                .enumerate()
                .map(|(i, line)| format!("{:4} | {}", i + 1, line))
                .collect::<Vec<String>>()
                .join("\n")
        } else {
            content_str.clone()
        };

        if display_content.len() > 50000 {
            Ok(format!("{}... (truncated at 50k chars)", &display_content[..50000]))
        } else {
            Ok(display_content)
        }
    }
}

impl CatTool {
    fn chunk_file_for_rag(&self, file_path: &str, content: &str) -> Vec<TextChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let chunk_size = 50;
        let overlap = 10;
        let mut chunks = Vec::new();

        let mut start = 0;
        while start < lines.len() {
            let end = std::cmp::min(start + chunk_size, lines.len());
            let chunk_content = lines[start..end].join("\n");
            if chunk_content.len() > 50 { 
                chunks.push(TextChunk {
                    file_path: file_path.to_string(),
                    content: chunk_content,
                    start_line: start + 1,
                    end_line: end,
                    embedding: vec![],
                    collection: "default".to_string(),
                });
            }
            if end == lines.len() { break; }
            start += chunk_size - overlap;
        }
        chunks
    }
}

pub struct WriteFileTool {
    pub ignored_patterns: Vec<String>,
}

impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "USE THIS to create NEW files, OVERWRITE existing files, or APPEND to files. Input: path, content, append (boolean, default false). Set append=true to add to the end of file."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to write to."
                },
                "content": {
                    "type": "string",
                    "description": "The content to write."
                },
                "append": {
                    "type": "boolean",
                    "description": "If true, appends content to the end of the file. If false (default), overwrites the file."
                }
            },
            "required": ["path", "content"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let raw_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' argument"))?;
        
        let append = args
            .get("append")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let path = expand_path(raw_path);

        if path.contains("..") {
            return Err(anyhow::anyhow!("Security: '..' not allowed"));
        }

        for ignore in &self.ignored_patterns {
            if path.contains(ignore) {
                return Err(anyhow::anyhow!(
                    "Access denied: Path contains ignored pattern '{}'",
                    ignore
                ));
            }
        }

        let path_obj = std::path::Path::new(&path);
        if let Some(parent) = path_obj.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if append {
            let mut file = OpenOptions::new()
                .write(true)
                .append(true)
                .create(true)
                .open(&path)?;
            use std::io::Write; 
            file.write_all(content.as_bytes())?;
            Ok(format!("Successfully appended to {}", path))
        } else {
            std::fs::write(&path, content)?;
            Ok(format!("Successfully wrote to {}", path))
        }
    }

    fn requires_confirmation(&self) -> bool {
        true
    }
}

pub struct EditFileTool {
    pub ignored_patterns: Vec<String>,
}

impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "USE THIS to replace a range of lines in a file. This is safer than 'replace_text' for code edits.
        - `start_line`: 1-based line number to start replacing.
        - `end_line`: 1-based line number to stop replacing (inclusive).
        - `content`: The new content to insert.
        To DELETE lines, provide empty `content`.
        To INSERT, set `start_line` and `end_line` to the same value (the new content will replace that line, so if you want to insert *after*, you might need to include the original line in `content`)."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to edit."
                },
                "start_line": {
                    "type": "integer",
                    "description": "The 1-based line number to start the edit."
                },
                "end_line": {
                    "type": "integer",
                    "description": "The 1-based line number to end the edit (inclusive)."
                },
                "content": {
                    "type": "string",
                    "description": "The new content to put in place of the specified lines."
                }
            },
            "required": ["path", "start_line", "end_line", "content"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let raw_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;
        let start_line = args
            .get("start_line")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'start_line' argument"))? as usize;
        let end_line = args
            .get("end_line")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'end_line' argument"))? as usize;
        let new_content = args
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or(""); // Allow empty content for deletion

        if start_line == 0 || end_line < start_line {
            return Err(anyhow::anyhow!("Invalid line range: {}-{}", start_line, end_line));
        }

        let path = expand_path(raw_path);

        if path.contains("..") {
            return Err(anyhow::anyhow!("Security: '..' not allowed"));
        }

        for ignore in &self.ignored_patterns {
            if path.contains(ignore) {
                return Err(anyhow::anyhow!(
                    "Access denied: Path contains ignored pattern '{}'",
                    ignore
                ));
            }
        }

        let current_content = std::fs::read_to_string(&path)?;
        let lines: Vec<&str> = current_content.lines().collect();
        
        if start_line > lines.len() + 1 {
             return Err(anyhow::anyhow!("Start line {} is beyond end of file ({} lines)", start_line, lines.len()));
        }

        let mut new_lines = Vec::new();
        
        // Add lines before the edit
        for i in 0..start_line - 1 {
            if i < lines.len() {
                new_lines.push(lines[i].to_string());
            }
        }

        // Add the new content (if not empty)
        if !new_content.is_empty() {
             new_lines.push(new_content.to_string());
        }

        // Add lines after the edit
        for i in end_line..lines.len() {
            new_lines.push(lines[i].to_string());
        }

        let final_content = new_lines.join("\n");
        // Ensure trailing newline if original had it? 
        // For simplicity, join("\n") adds newlines between. 
        // We might want a trailing newline.
        let final_content = if final_content.ends_with('\n') { final_content } else { final_content + "\n" };

        std::fs::write(&path, final_content)?;

        Ok(format!("Successfully updated lines {}-{} in {}", start_line, end_line, path))
    }

    fn requires_confirmation(&self) -> bool {
        true
    }
}

pub struct ReplaceTextTool {
    pub ignored_patterns: Vec<String>,
}

impl Tool for ReplaceTextTool {
    fn name(&self) -> &str {
        "replace_text"
    }

    fn description(&self) -> &str {
        "USE THIS to replace specific text in a file. Input: path, old_text (exact match), new_text. Will fail if old_text is not found or not unique."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to edit."
                },
                "old_text": {
                    "type": "string",
                    "description": "The exact text block to be replaced."
                },
                "new_text": {
                    "type": "string",
                    "description": "The new text to insert in place of old_text."
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let raw_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;
        let old_text = args
            .get("old_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'old_text' argument"))?;
        let new_text = args
            .get("new_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'new_text' argument"))?;

        let path = expand_path(raw_path);

        if path.contains("..") {
            return Err(anyhow::anyhow!("Security: '..' not allowed"));
        }

        for ignore in &self.ignored_patterns {
            if path.contains(ignore) {
                return Err(anyhow::anyhow!(
                    "Access denied: Path contains ignored pattern '{}'",
                    ignore
                ));
            }
        }

        let current_content = std::fs::read_to_string(&path)?;

        let matches: Vec<_> = current_content.match_indices(old_text).collect();

        if matches.is_empty() {
             return Err(anyhow::anyhow!("Could not find exact match for specified old_text"));
        }
        if matches.len() > 1 {
             return Err(anyhow::anyhow!("Found multiple matches for old_text. Please provide more context to make it unique."));
        }

        let new_content = current_content.replace(old_text, new_text);
        std::fs::write(&path, new_content)?;

        Ok(format!("Successfully modified {}", path))
    }

    fn requires_confirmation(&self) -> bool {
        true
    }
}

pub struct DeleteFileTool {
    pub ignored_patterns: Vec<String>,
}

impl Tool for DeleteFileTool {
    fn name(&self) -> &str {
        "delete_file"
    }

    fn description(&self) -> &str {
        "USE THIS to DELETE a file. WARNING: This is permanent. Input: path. Checks for ignored patterns."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to delete."
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let raw_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;

        let path = expand_path(raw_path);

        if path.contains("..") {
            return Err(anyhow::anyhow!("Security: '..' not allowed"));
        }

        for ignore in &self.ignored_patterns {
            if path.contains(ignore) {
                return Err(anyhow::anyhow!(
                    "Access denied: Path contains ignored pattern '{}'",
                    ignore
                ));
            }
        }

        if !std::path::Path::new(&path).exists() {
             return Err(anyhow::anyhow!("File does not exist: {}", path));
        }

        std::fs::remove_file(&path)?;

        Ok(format!("Successfully deleted {}", path))
    }

    fn requires_confirmation(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;
    use crate::ollama::OllamaClient;
    use std::sync::Mutex;

    #[test]
    fn test_list_directory_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_ls.txt");
        File::create(&file_path)?;

        let tool = ListDirectoryTool {
            ignored_patterns: vec![],
        };
        let args = serde_json::json!({ "path": dir.path().to_str().unwrap() });

        let output = tool.execute(args)?;
        assert!(output.contains("test_ls.txt"));
        Ok(())
    }

    #[tokio::test]
    async fn test_cat_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("read_me.txt");
        let mut file = File::create(&file_path)?;
        writeln!(file, "Hello Tool World")?;

        let tool = Arc::new(CatTool {
            ignored_patterns: vec![],
            rag: Arc::new(crate::rag::RagSystem::new(
                OllamaClient::new("http://localhost".to_string()),
                "dummy".to_string(),
                Arc::new(Mutex::new(None)),
                None,
            )),
        });
        let args = serde_json::json!({ 
            "path": file_path.to_str().unwrap(),
            "numbered": false 
        });

        let output = tokio::task::spawn_blocking(move || {
            tool.execute(args)
        }).await??;

        assert_eq!(output.trim(), "Hello Tool World");
        Ok(())
    }

    #[test]
    fn test_grep_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("grep_me.txt");
        let mut file = File::create(&file_path)?;
        writeln!(file, "Line 1\nMatchThis\nLine 3")?;

        let tool = GrepTool {
            ignored_patterns: vec![],
        };
        let args = serde_json::json!({
            "pattern": "MatchThis",
            "path": dir.path().to_str().unwrap()
        });

        let output = tool.execute(args)?;
        assert!(output.contains("MatchThis"));
        Ok(())
    }

    #[test]
    fn test_grep_tool_multi_file() -> Result<()> {
        let dir = tempdir()?;
        let file1 = dir.path().join("file1.txt");
        {
            let mut f1 = File::create(&file1)?;
            writeln!(f1, "MatchThis")?;
        }

        let file2 = dir.path().join("file2.txt");
        {
            let mut f2 = File::create(&file2)?;
            writeln!(f2, "NoMatch")?;
        }

        let tool = GrepTool {
            ignored_patterns: vec![],
        };
        let args = serde_json::json!({
            "pattern": "MatchThis",
            "path": dir.path().to_str().unwrap()
        });

        let output = tool.execute(args)?;
        assert!(output.contains("MatchThis"));
        Ok(())
    }

    #[test]
    fn test_write_file_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("new_file.txt");
        
        let tool = WriteFileTool {
            ignored_patterns: vec![],
        };
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "Hello Writer"
        });

        let output = tool.execute(args)?;
        assert!(output.contains("Successfully wrote"));
        
        let content = std::fs::read_to_string(&file_path)?;
        assert_eq!(content, "Hello Writer");
        Ok(())
    }

    #[test]
    fn test_replace_text_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("code.rs");
        std::fs::write(&file_path, "fn main() { println!(\"Old\"); }")?;

        let tool = ReplaceTextTool {
            ignored_patterns: vec![],
        };
        
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "println!(\"Old\")",
            "new_text": "println!(\"New\")"
        });

        let output = tool.execute(args)?;
        assert!(output.contains("Successfully modified"));

        let content = std::fs::read_to_string(&file_path)?;
        assert_eq!(content, "fn main() { println!(\"New\"); }");
        Ok(())
    }

    #[test]
    fn test_replace_text_not_found() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("fail.txt");
        std::fs::write(&file_path, "content")?;

        let tool = ReplaceTextTool {
            ignored_patterns: vec![],
        };

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "missing",
            "new_text": "replaced"
        });

        let result = tool.execute(args);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_write_file_append() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("append_test.txt");
        std::fs::write(&file_path, "Initial")?;

        let tool = WriteFileTool {
            ignored_patterns: vec![],
        };
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": " + Appended",
            "append": true
        });

        let output = tool.execute(args)?;
        assert!(output.contains("Successfully appended"));

        let content = std::fs::read_to_string(&file_path)?;
        assert_eq!(content, "Initial + Appended");
        Ok(())
    }

    #[test]
    fn test_delete_file_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("delete_me.txt");
        File::create(&file_path)?;

        let tool = DeleteFileTool {
            ignored_patterns: vec![],
        };
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });

        let output = tool.execute(args)?;
        assert!(output.contains("Successfully deleted"));
        assert!(!file_path.exists());
        Ok(())
    }
}
