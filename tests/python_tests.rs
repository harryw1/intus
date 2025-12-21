use intus::python::PythonRuntime;

#[test]
fn test_python_exec() {
    // This test requires `uv` to be installed.
    let runtime = PythonRuntime::new().expect("Failed to initialize PythonRuntime. Is `uv` installed?");
    
    let script = "print('Hello from Rust Test')";
    let output = runtime.run_script(script).expect("Failed to run script");
    
    assert!(output.contains("Hello from Rust Test"));
}

#[test]
fn test_python_install_and_import() {
    let runtime = PythonRuntime::new().expect("Failed to initialize PythonRuntime");
    
    // Install a lightweight package
    // 'requests' is standard enough.
    runtime.install_packages(&["requests"]).expect("Failed to install requests");
    
    let script = "import requests; print('requests version:', requests.__version__)";
    let output = runtime.run_script(script).expect("Failed to run script");
    
    println!("Output: {}", output);
    assert!(output.contains("requests version"));
}
