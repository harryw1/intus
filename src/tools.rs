use crate::ollama::{ToolDefinition, ToolFunction};
use anyhow::Result;
use serde_json::Value;
use std::process::Command;

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            type_: "function".to_string(),
            function: ToolFunction {
                name: self.name().to_string(),
                description: self.description().to_string(),
                parameters: self.parameters(),
            },
        }
    }
    fn execute(&self, args: Value) -> Result<String>;
}

pub struct FileSearchTool;

impl Tool for FileSearchTool {
    fn name(&self) -> &str {
        "find_files"
    }

    fn description(&self) -> &str {
        "Find files matching a name pattern in the current directory or a specified path. Returns detailed file metadata."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The file name pattern to search for (e.g. '*.rs', 'config.json')"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in (default: current directory)."
                }
            },
            "required": ["name"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'name' argument"))?;

        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        // Use -ls to get metadata (permissions, size, time)
        let output = Command::new("find")
            .arg(path)
            .arg("-name")
            .arg(name)
            .arg("-ls")
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Find command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.trim().is_empty() {
            Ok("No files found.".to_string())
        } else {
            let lines: Vec<&str> = stdout.lines().take(20).collect();
            let result = lines.join("\n");
            if lines.len() < stdout.lines().count() {
                Ok(format!("{}\n... (and more)", result))
            } else {
                Ok(result)
            }
        }
    }
}

pub struct ListDirectoryTool;

impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List all files and directories in a specific path (equivalent to ls -la)."
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
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let output = Command::new("ls").arg("-la").arg(path).output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "ls command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.trim().is_empty() {
            Ok("Directory is empty.".to_string())
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

pub struct GrepTool;

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep_files"
    }

    fn description(&self) -> &str {
        "Search for a text pattern in files within the current directory or specified path using grep. Returns line numbers."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The text pattern or regex to search for."
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in (default: current directory)."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Whether to search recursively in subdirectories (default: true)."
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

        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let recursive = args
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut cmd = Command::new("grep");
        cmd.arg("--color=never");
        cmd.arg("-n"); // Line numbers
        if recursive {
            cmd.arg("-r");
        }
        cmd.arg(pattern);
        cmd.arg(path);

        let output = cmd.output()?;
        if !output.status.success() && output.status.code() != Some(1) {
            return Err(anyhow::anyhow!(
                "Grep failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.trim().is_empty() {
            Ok("No matches found.".to_string())
        } else {
            let lines: Vec<&str> = stdout.lines().take(50).collect();
            Ok(lines.join("\n"))
        }
    }
}

pub struct CatTool;

impl Tool for CatTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a specific file using the 'cat' command."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the file to read."
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;

        if path.contains("..") {
            return Err(anyhow::anyhow!("Security: '..' not allowed"));
        }

        let output = Command::new("cat").arg(path).output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Cat failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let content = String::from_utf8_lossy(&output.stdout);
        if content.len() > 5000 {
            Ok(format!("{}... (truncated)", &content[..5000]))
        } else {
            Ok(content.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_file_search_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_search.txt");
        File::create(&file_path)?;

        let tool = FileSearchTool;
        let args = serde_json::json!({
            "name": "test_search.txt",
            "path": dir.path().to_str().unwrap()
        });

        let output = tool.execute(args)?;
        assert!(output.contains("test_search.txt"));
        Ok(())
    }

    #[test]
    fn test_list_directory_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_ls.txt");
        File::create(&file_path)?;

        let tool = ListDirectoryTool;
        let args = serde_json::json!({ "path": dir.path().to_str().unwrap() });

        let output = tool.execute(args)?;
        assert!(output.contains("test_ls.txt"));
        Ok(())
    }

    #[test]
    fn test_cat_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("read_me.txt");
        let mut file = File::create(&file_path)?;
        writeln!(file, "Hello Tool World")?;

        let tool = CatTool;
        let args = serde_json::json!({ "path": file_path.to_str().unwrap() });

        let output = tool.execute(args)?;
        assert_eq!(output.trim(), "Hello Tool World");
        Ok(())
    }

    #[test]
    fn test_grep_tool() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("grep_me.txt");
        let mut file = File::create(&file_path)?;
        writeln!(file, "Line 1\nMatchThis\nLine 3")?;

        let tool = GrepTool;
        let args = serde_json::json!({
            "pattern": "MatchThis",
            "path": dir.path().to_str().unwrap()
        });

        let output = tool.execute(args)?;
        assert!(output.contains("MatchThis"));
        assert!(output.contains("grep_me.txt")); // Output typically includes filename
        Ok(())
    }
}
