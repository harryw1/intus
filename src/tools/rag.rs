use super::{expand_path, Tool, TextChunk, VectorIndex, StatusSender};
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use std::collections::HashMap;
use crate::rag::RagSystem;

pub struct SemanticSearchTool {
    pub rag: Arc<RagSystem>,
    pub ignored_patterns: Vec<String>,
    pub knowledge_bases: HashMap<String, String>,
    pub status_tx: Option<StatusSender>,
}

impl Tool for SemanticSearchTool {
    fn name(&self) -> &str {
        "semantic_search"
    }

    fn description(&self) -> &str {
        "USE THIS to find code, notes, or web search results by CONCEPT. Auto-indexes workspace on first use. Can also index specific directories or named knowledge bases."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The conceptual search query (e.g. 'authentication logic')."
                },
                "index_path": {
                    "type": "string",
                    "description": "Optional: A directory path OR knowledge base name (e.g. 'work') to index/search."
                },
                "refresh": {
                    "type": "boolean",
                    "description": "Force re-indexing of the workspace (default false)."
                }
            },
            "required": ["query"]
        })
    }

            fn execute(&self, args: Value) -> Result<String> {

                let query = args.get("query").and_then(|v| v.as_str())

                    .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?;

                

                let refresh = args.get("refresh").and_then(|v| v.as_bool()).unwrap_or(false);

                let index_path_raw = args.get("index_path").and_then(|v| v.as_str());

        

                // Resolve index_path and determine collection name

                let (index_path_arg, collection_name) = if let Some(raw) = index_path_raw {

                    if let Some(path) = self.knowledge_bases.get(raw) {

                        (Some(expand_path(path)), Some(raw.to_string()))

                    } else {

                        (Some(expand_path(raw)), Some("default".to_string()))

                    }

                } else {

                    (None, None)

                };

        

                // 1. Handle Lazy Load

                {

                    let guard = self.rag.index.lock().unwrap();

                    if guard.is_none() {

                        drop(guard);

                        let _ = self.rag.load();

                    }

                }

        

                // 2. Background Indexing

                if refresh || index_path_arg.is_some() {

                     let path = index_path_arg.unwrap_or_else(|| ".".to_string());

                     let coll = collection_name.clone().unwrap_or("default".to_string());

                     

                     // Spawn detached task for indexing

                     let rag_clone = self.rag.clone();

                     let status_tx_clone = self.status_tx.clone();

                     let path_clone = path.clone();

                     let coll_clone = coll.clone();

        

                     // Notify start

                     if let Some(tx) = &status_tx_clone {

                         let _ = tx.send(format!("Started indexing '{}' into collection '{}'...", path_clone, coll_clone));

                     }

        

                     tokio::spawn(async move {

                         match SemanticSearchTool::index_directory_logic(rag_clone, &path_clone, &coll_clone, status_tx_clone.clone()).await {

                             Ok(count) => {

                                 if let Some(tx) = status_tx_clone {

                                     let _ = tx.send(format!("Completed indexing '{}': {} chunks added.", path_clone, count));

                                 }

                             }

                             Err(e) => {

                                 if let Some(tx) = status_tx_clone {

                                     let _ = tx.send(format!("Indexing failed for '{}': {}", path_clone, e));

                                 }

                             }

                         }

                     });

                }

        

                // 3. Search (Immediate, on existing data)

                let handle = tokio::runtime::Handle::current();

                let filter = collection_name.as_deref();

                let results = handle.block_on(self.rag.search(query, 5, filter))?;

        

                if results.is_empty() {

                     if refresh || index_path_raw.is_some() {

                         Ok("No matches yet (Indexing in progress in background). Check status messages.".to_string())

                     } else {

                         Ok("No relevant conceptual matches found.".to_string())

                     }

                } else {

                    let mut output = format!("Top conceptual matches for '{}' (Collection: {}):\n\n", query, filter.unwrap_or("ALL"));

                    for (i, res) in results.into_iter().enumerate() {

                        output.push_str(&format!("{}. {}\n\n", i + 1, res.trim()));

                    }

                     if refresh || index_path_raw.is_some() {

                         output.push_str("\n(Note: Background indexing is active, results may improve shortly.)");

                     }

                    Ok(output)

                }

            }

        }

        

        impl SemanticSearchTool {

            async fn index_directory_logic(rag: Arc<RagSystem>, dir_path: &str, collection: &str, status_tx: Option<StatusSender>) -> Result<usize> {

                let dir_path_owned = dir_path.to_string();

                

                let walker_files = tokio::task::spawn_blocking(move || {

                    let mut files = Vec::new();

                    let walker = ignore::WalkBuilder::new(&dir_path_owned).standard_filters(true).build();

                    for result in walker {

                        if let Ok(entry) = result {

                            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {

                                 files.push(entry.path().to_owned());

                            }

                        }

                    }

                    files

                }).await?;

        

                let mut chunks = Vec::new();

                let collection_owned = collection.to_string();

                

                for file_path in walker_files {

                     if let Ok(content) = std::fs::read_to_string(&file_path) {

                         let lines: Vec<&str> = content.lines().collect();

                         let chunk_size = 30;

                         let overlap = 5;

                         

                         let mut start = 0;

                         while start < lines.len() {

                             let end = std::cmp::min(start + chunk_size, lines.len());

                             let chunk_text = lines[start..end].join("\n");

                             

                             if chunk_text.len() > 50 {

                                 chunks.push(TextChunk {

                                     file_path: file_path.to_string_lossy().to_string(),

                                     content: chunk_text,

                                     start_line: start + 1,

                                     end_line: end,

                                     embedding: vec![],

                                     collection: collection_owned.clone(),

                                 });

                             }

                             if end == lines.len() { break; }

                             start += chunk_size - overlap;

                         }

                     }

                }

                

                if chunks.is_empty() {

                    return Ok(0);

                }

        

                if let Some(tx) = &status_tx {

                    let _ = tx.send(format!("Generating embeddings for {} chunks...", chunks.len()));

                }

        

                // Process in batches

                for batch in chunks.chunks_mut(10) {

                     for chunk in batch.iter_mut() {

                         if let Ok(emb) = rag.client.generate_embeddings(&rag.embedding_model, &chunk.content).await {

                             chunk.embedding = emb;

                         }

                     }

                }

                

                chunks.retain(|c| !c.embedding.is_empty());

                let count = chunks.len();

        

                let mut guard = rag.index.lock().unwrap();

                if let Some(index) = &mut *guard {

                    index.chunks.extend(chunks);

                    index.indexed_at = std::time::SystemTime::now();

                } else {

                    *guard = Some(VectorIndex {

                        chunks,

                        indexed_at: std::time::SystemTime::now(),

                    });

                }

                drop(guard);

                

                rag.save()?;

        

                Ok(count)

            }

        

            // Retaining dummy chunk_file for now as it's not used by execute anymore but might be by other methods? 

            // Actually, index_directory_logic duplicates chunking logic because it's static/async. 

            // We can remove `index_directory` and `chunk_file` instance methods if unused.

            // But `chunk_file` is used by the old implementation. 

            // We are REPLACING `execute` and `impl SemanticSearchTool`.

            

            // We need to be careful to remove the OLD `index_directory` and `chunk_file`.

            // The previous `replace` failed, so `execute` is still the old one.

            // The `old_string` I provide below must match the CURRENT file content EXACTLY.

            

            // I will target the `execute` method implementation block.

        }

        

    

    pub struct MemoryTool {

        pub rag: Arc<RagSystem>,

    }

    

    impl Tool for MemoryTool {

        fn name(&self) -> &str {

            "remember"

        }

    

        fn description(&self) -> &str {

            "USE THIS to explicitly save an important fact, note, or piece of information to your long-term memory. This helps you remember things across conversations or after a restart."

        }

    

        fn parameters(&self) -> Value {

            serde_json::json!({

                "type": "object",

                "properties": {

                    "fact": {

                        "type": "string",

                        "description": "The fact or information to remember (e.g. 'The user prefers dark mode', 'The project uses port 8081')."

                    }

                },

                "required": ["fact"]

            })

        }

    

        fn execute(&self, args: Value) -> Result<String> {

            let fact = args.get("fact").and_then(|v| v.as_str())

                .ok_or_else(|| anyhow::anyhow!("Missing 'fact' argument"))?;

    

            let handle = tokio::runtime::Handle::current();

            // Use "memory" collection

            handle.block_on(self.rag.add_text(fact, Some("memory".to_string())))?;

    

            Ok(format!("Successfully remembered: {}", fact))

        }

    }

    