use intus::python::PythonRuntime;
use intus::tools::{RunPythonTool, Tool};
use std::sync::Arc;
use serde_json::json;

#[test]
fn test_python_explicit_deps() {
    let runtime = Arc::new(PythonRuntime::new().expect("Failed to init runtime"));
    let tool = RunPythonTool { runtime: runtime.clone() };

    // We'll try to install a small, safe package: `requests` if not already there, 
    // or maybe `packaging` which is small. `requests` is good.
    // To prove it works, we can try to import it in the script.
    
    let args = json!({
        "script": "import packaging; print('packaging installed')",
        "dependencies": ["packaging"]
    });

    let result = tool.execute(args).expect("Tool execution failed");
    println!("Tool output: {}", result);
    assert!(result.contains("packaging installed"));
}

#[test]
fn test_python_auto_install() {
    let runtime = Arc::new(PythonRuntime::new().expect("Failed to init runtime"));
    let tool = RunPythonTool { runtime: runtime.clone() };

    // This relies on `contextlib2` NOT being installed by default suitable for a quick test?
    // Or just `colorama`.
    let script = "import colorama; print('colorama auto-installed')";
    
    // We intentionally do NOT provide dependencies
    let args = json!({
        "script": script
    });

    // This might fail if colorama is already there, but that's fine, it should still run.
    // If it's NOT there, it should auto-install.
    // To test auto-install logic specifically, we'd need a clean venv, but we can't easily guarantee that here without wiping usage.
    // Let's rely on the fact that if it fails, it returns an error, if it succeeds (after install), it returns output.
    
    let result = tool.execute(args).expect("Tool execution failed");
    println!("Tool output: {}", result);
    assert!(result.contains("colorama auto-installed"));
}
