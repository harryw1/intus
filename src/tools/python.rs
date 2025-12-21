use super::Tool;
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use crate::python::PythonRuntime;

pub struct RunPythonTool {
    pub runtime: Arc<PythonRuntime>,
}

impl Tool for RunPythonTool {
    fn name(&self) -> &str {
        "run_python"
    }

    fn description(&self) -> &str {
        "USE THIS to execute Python scripts for advanced analysis, math, or complex data processing.
The environment has internet access.
If a script fails due to missing modules, I will attempt to install them automatically.
Returns the STDOUT and STDERR of the script."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "script": {
                    "type": "string",
                    "description": "The Python script to execute."
                }
            },
            "required": ["script"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let script = args
            .get("script")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'script' argument"))?;

        // Try running the script
        match self.runtime.run_script(script) {
            Ok(output) => {
                 // Check if it failed with a ModuleNotFoundError in the captured stderr (which run_script returns in its success Ok string for us to see)
                 // Wait, run_script returns a string containing stdout and optionally stderr.
                 // It only returns Err if the command *execution* failed (e.g. python not found), not if the script exited with error code.
                 // Actually, looking at my python.rs implementation: 
                 // It uses `cmd.output()`. If the process runs, it returns Ok(output).
                 // It only checks for `python_path` execution failure.
                 // So I need to parse the output here for common missing package errors if I want auto-install.
                 
                 if output.contains("ModuleNotFoundError: No module named") {
                     // Attempt to parse package name
                     // Format: "ModuleNotFoundError: No module named 'requests'"
                     if let Some(start) = output.find("No module named '") {
                         let rest = &output[start + 17..];
                         if let Some(end) = rest.find('\'') {
                             let package = &rest[..end];
                             let install_msg = format!("(Auto-installing missing package: '{}'...)\n", package);
                             
                             // Install
                             if let Err(e) = self.runtime.install_packages(&[package]) {
                                 return Ok(format!("{}Failed to auto-install package '{}': {}\n\nOriginal Output:\n{}", install_msg, package, e, output));
                             }
                             
                             // Retry script
                             match self.runtime.run_script(script) {
                                 Ok(retry_output) => return Ok(format!("{}Package installed successfully.\n\n{}", install_msg, retry_output)),
                                 Err(e) => return Err(e),
                             }
                         }
                     }
                 }
                 
                 Ok(output)
            },
            Err(e) => Err(e),
        }
    }
}
