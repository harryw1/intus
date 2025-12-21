use super::Tool;
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use crate::python::PythonRuntime;
use regex::Regex;

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
You can optionally provide a list of PyPI `dependencies` to install before running the script.
If a script fails due to missing modules, I will attempt to install them automatically, but it is better to list them explicitly.
Returns the STDOUT and STDERR of the script."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "script": {
                    "type": "string",
                    "description": "The Python script to execute."
                },
                "dependencies": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of PyPI packages to install before running."
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

        // Install explicit dependencies first
        if let Some(deps) = args.get("dependencies").and_then(|v| v.as_array()) {
             let packages: Vec<&str> = deps.iter().filter_map(|v| v.as_str()).collect();
             if !packages.is_empty() {
                 self.runtime.install_packages(&packages)?;
             }
        }

        // Try running the script
        match self.runtime.run_script(script) {
            Ok(output) => {
                 // Check if it failed with a ModuleNotFoundError in the captured stderr
                 if output.contains("ModuleNotFoundError: No module named") {
                     // Regex to match: ModuleNotFoundError: No module named 'requests'
                     // or: ModuleNotFoundError: No module named 'PIL'
                     let re = Regex::new(r"ModuleNotFoundError: No module named ['\x22](.*?)['\x22]").unwrap();
                     
                     if let Some(caps) = re.captures(&output) {
                         if let Some(package_match) = caps.get(1) {
                             let package = package_match.as_str();
                             // Clean up package name if it has submodules (e.g., 'sklearn.model_selection' -> 'sklearn' - wait, usually we want to map this, but for now try direct install or guessing 'scikit-learn')
                             // Mapping is hard without a database. Let's try installing exactly what it says first.
                             
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
