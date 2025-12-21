use anyhow::{Context, Result};
use std::path::PathBuf;
use directories::BaseDirs;
use std::process::Command;
use std::fs;

pub struct PythonRuntime {
    venv_path: PathBuf,
}

impl PythonRuntime {
    pub fn new() -> Result<Self> {
        let base_dirs = BaseDirs::new().context("Could not find home directory")?;
        let venv_path = base_dirs.home_dir().join(".config/intus/venv");
        
        let runtime = Self { venv_path };
        runtime.ensure_venv()?;
        
        Ok(runtime)
    }

    fn ensure_venv(&self) -> Result<()> {
        if self.venv_path.exists() {
            return Ok(());
        }

        // Check if uv is installed
        let uv_check = Command::new("uv")
            .arg("--version")
            .output()
            .context("Failed to execute `uv`. Please ensure `uv` is installed and in your PATH.")?;

        if !uv_check.status.success() {
            return Err(anyhow::anyhow!("`uv` is not available. Please install it to use Python features."));
        }

        // Create venv
        let status = Command::new("uv")
            .arg("venv")
            .arg(&self.venv_path)
            .status()
            .context("Failed to create venv with `uv`")?;

        if !status.success() {
            return Err(anyhow::anyhow!("Failed to create Python virtual environment at {:?}", self.venv_path));
        }

        Ok(())
    }

    fn get_python_path(&self) -> PathBuf {
        if cfg!(windows) {
            self.venv_path.join("Scripts").join("python.exe")
        } else {
            self.venv_path.join("bin").join("python")
        }
    }

    pub fn install_packages(&self, packages: &[&str]) -> Result<String> {
        if packages.is_empty() {
            return Ok("No packages requested.".to_string());
        }

        let mut cmd = Command::new("uv");
        cmd.arg("pip")
           .arg("install")
           .arg("--quiet"); // Reduce noise

        // On some platforms/uv versions, explicitly setting VIRTUAL_ENV is safest
        cmd.env("VIRTUAL_ENV", &self.venv_path);
        
        // We can also point to target python explicitly to be sure
        let python_path = self.get_python_path();
        cmd.arg("-p").arg(python_path);

        cmd.args(packages);

        let output = cmd.output().context("Failed to run `uv pip install`")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(anyhow::anyhow!("Failed to install packages: {}", String::from_utf8_lossy(&output.stderr)))
        }
    }

    pub fn run_script(&self, script_content: &str) -> Result<String> {
        let base_dirs = BaseDirs::new().context("Could not find home directory")?;
        let script_path = base_dirs.home_dir().join(".config/intus/temp_script.py");
        
        fs::write(&script_path, script_content).context("Failed to write temporary python script")?;

        // Find python executable
        let python_path = self.get_python_path();

        let output = Command::new(python_path)
            .arg(&script_path)
            .output();

        // Cleanup temporary script
        let _ = fs::remove_file(&script_path);

        let output = output.context("Failed to execute python script")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut result = stdout;
        if !stderr.is_empty() {
            result.push_str("\n\nSTDERR:\n");
            result.push_str(&stderr);
        }

        Ok(result)
    }
}
