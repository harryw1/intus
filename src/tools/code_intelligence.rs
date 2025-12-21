use super::{expand_path, Tool};
use anyhow::Result;
use ignore::WalkBuilder;
use regex::Regex;
use serde_json::Value;
use std::fs;

pub struct SymbolSearchTool {
    pub ignored_patterns: Vec<String>,
}

impl Tool for SymbolSearchTool {
    fn name(&self) -> &str {
        "find_symbol"
    }

    fn description(&self) -> &str {
        "USE THIS to find where a code symbol (Function, Class, Struct) or Markdown Header is DEFINED. 
        It searches for DEFINITIONS, not references.
        Optimized for: Rust (fn, struct, trait), Python (def, class), Markdown (# Header)."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The symbol name to find (e.g., 'App', 'main', 'Installation')."
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Default: current directory."
                },
                "file_extension": {
                    "type": "string",
                    "description": "Optional: Filter by file extension (e.g., 'rs', 'md', 'py')."
                }
            },
            "required": ["query"]
        })
    }

    fn execute(&self, args: Value) -> Result<String> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;

        let path_str = expand_path(args.get("path").and_then(|v| v.as_str()).unwrap_or("."));
        let extension = args.get("file_extension").and_then(|v| v.as_str());

        // Regex patterns for different languages
        // We compile them dynamically based on the query to search SPECIFIC symbols
        // Rust: fn name, struct name, trait name, enum name, type name
        // Python: def name, class name
        // Markdown: # ... name ... (Headers)
        
        let rust_pattern = format!(r"(fn\s+{}\b|struct\s+{}\b|trait\s+{}\b|enum\s+{}\b|type\s+{}\b|impl.*{}\b)", query, query, query, query, query, query);
        let py_pattern = format!(r"(def\s+{}\b|class\s+{}\b)", query, query);
        let md_pattern = format!(r"(^#+\s+.*{}\b)", query); // Matches headers containing the query

        let re_rust = Regex::new(&rust_pattern).unwrap();
        let re_py = Regex::new(&py_pattern).unwrap();
        let re_md = Regex::new(&md_pattern).unwrap();

        let walker = WalkBuilder::new(&path_str)
            .hidden(false) 
            .ignore(false) // We manually check ignored patterns for flexibility or just use standard .gitignore
            .git_ignore(true)
            .build();

        let mut results = Vec::new();

        for result in walker {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if !path.is_file() { continue; }

                    // Apply extension filter if present
                    if let Some(ext) = extension {
                        if path.extension().and_then(|e| e.to_str()) != Some(ext) {
                            continue;
                        }
                    }

                    // Manually check ignored patterns from config
                    let path_lossy = path.to_string_lossy();
                    if self.ignored_patterns.iter().any(|ignore| path_lossy.contains(ignore)) {
                        continue;
                    }

                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        let re = match ext {
                            "rs" => &re_rust,
                            "py" => &re_py,
                            "md" => &re_md,
                            _ => continue, // Skip unsupported files for now? Or fallback to simple grep?
                            // Let's restrict to supported types to avoid noise.
                        };

                        if let Ok(content) = fs::read_to_string(path) {
                            for (i, line) in content.lines().enumerate() {
                                if re.is_match(line) {
                                    results.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
                                    if results.len() >= 20 { break; } // Limit results per search
                                }
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
            if results.len() >= 20 { break; }
        }

        if results.is_empty() {
            Ok(format!("No definitions found for symbol '{}'.", query))
        } else {
            Ok(results.join("\n"))
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
    fn test_find_symbol_rust() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_code.rs");
        let mut file = File::create(&file_path)?;
        writeln!(file, "fn my_func() {{}}")?;
        writeln!(file, "struct MyStruct {{}}")?;

        let tool = SymbolSearchTool { ignored_patterns: vec![] };
        
        // Find function
        let args = serde_json::json!({
            "query": "my_func",
            "path": dir.path().to_str().unwrap()
        });
        let output = tool.execute(args)?;
        assert!(output.contains("fn my_func"));

        // Find struct
        let args = serde_json::json!({
            "query": "MyStruct",
            "path": dir.path().to_str().unwrap()
        });
        let output = tool.execute(args)?;
        assert!(output.contains("struct MyStruct"));
        
        Ok(())
    }

    #[test]
    fn test_find_symbol_markdown() -> Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("README.md");
        let mut file = File::create(&file_path)?;
        writeln!(file, "# Installation Guide")?;

        let tool = SymbolSearchTool { ignored_patterns: vec![] };
        
        let args = serde_json::json!({
            "query": "Installation",
            "path": dir.path().to_str().unwrap()
        });
        let output = tool.execute(args)?;
        assert!(output.contains("# Installation Guide"));
        
        Ok(())
    }
}
