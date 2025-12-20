use super::Tool;
use anyhow::Result;
use serde_json::Value;
use std::process::{Command, Stdio};
use std::sync::Arc;
use crate::process::ProcessTracker;

pub struct RunCommandTool {
    pub allowed_commands: Vec<String>,
    pub process_tracker: Arc<ProcessTracker>,
}

impl Tool for RunCommandTool {
    fn name(&self) -> &str {
        "run_command"
    }

    fn description(&self) -> &str {
        "USE THIS to execute shell commands. Safe commands like 'ls', 'git', 'cargo'. WARNING: Do NOT use `find` directly (use find_files instead) as it may hang or fail on macOS/BSD. Input: command, args."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command/program to run (e.g. 'git', 'cargo', 'ls')."
                },
                "args": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "List of arguments to pass to the command."
                }
            },
            "required": ["command", "args"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let command_name = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' argument"))?;

        let cmd_args: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| v.as_str().unwrap_or_default().to_string())
                    .collect()
            })
            .unwrap_or_default();

        if !self.allowed_commands.contains(&command_name.to_string()) {
            return Err(anyhow::anyhow!(
                "Command '{}' is not allowed. Allowed commands: {:?}",
                command_name,
                self.allowed_commands
            ));
        }
        
        let shell_operators = ["|", "&&", ";", ">", ">>", "<", "&"];
        let needs_shell = cmd_args.iter().any(|arg| {
            shell_operators.iter().any(|op| arg.contains(op))
        });

        let output = if needs_shell {
            let full_command = format!("{} {}", command_name, cmd_args.join(" "));
            
            let child = if cfg!(target_os = "windows") {
                Command::new("cmd")
                    .arg("/C")
                    .arg(&full_command)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()? 
            } else {
                Command::new("sh")
                    .arg("-c")
                    .arg(&full_command)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?
            };

            let pid = child.id();
            self.process_tracker.add_pid(pid);
            let result = child.wait_with_output();
            self.process_tracker.remove_pid(pid);
            result?
        } else {
            let mut cmd = Command::new(command_name);
            cmd.stdin(Stdio::null()); 
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            cmd.args(&cmd_args);
            
            let child = cmd.spawn()?;
            let pid = child.id();
            
            self.process_tracker.add_pid(pid);
            let result = child.wait_with_output();
            self.process_tracker.remove_pid(pid);
            result?
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let result = if output.status.success() {
            if stdout.trim().is_empty() {
                "Command succeeded with no output.".to_string()
            } else {
                stdout.to_string()
            }
        } else {
            format!("Command failed:\nstdout: {}\nstderr: {}", stdout, stderr)
        };

        if result.len() > 5000 {
            Ok(format!("{}... (truncated)", &result[..5000]))
        } else {
            Ok(result)
        }
    }

    fn requires_confirmation(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_command_shell_piping() -> Result<()> {
        let tracker = Arc::new(ProcessTracker::new());
        let tool = RunCommandTool {
            allowed_commands: vec!["echo".to_string(), "grep".to_string()],
            process_tracker: tracker,
        };
        let args = serde_json::json!({
            "command": "echo",
            "args": ["hello world", "|", "grep", "hello"]
        });

        let output = tool.execute(args)?;
        assert!(output.contains("hello world"));
        Ok(())
    }

    #[test]
    fn test_run_command_shell_piping_fail_allowlist() -> Result<()> {
        let tracker = Arc::new(ProcessTracker::new());
        let tool = RunCommandTool {
            allowed_commands: vec!["ls".to_string()],
            process_tracker: tracker,
        };
        let args = serde_json::json!({
            "command": "echo",
            "args": ["hello"]
        });

        let result = tool.execute(args);
        assert!(result.is_err());
        Ok(())
    }
}
