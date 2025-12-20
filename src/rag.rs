use anyhow::Result;
use crate::ollama::OllamaClient;
use crate::tools::{VectorIndex, TextChunk};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::fs;

#[derive(Clone)]
pub struct RagSystem {
    pub client: OllamaClient,
    pub embedding_model: String,
    pub index: Arc<Mutex<Option<VectorIndex>>>,
    pub storage_path: Option<PathBuf>,
}

impl RagSystem {
    pub fn new(client: OllamaClient, embedding_model: String, index: Arc<Mutex<Option<VectorIndex>>>, storage_path: Option<PathBuf>) -> Self {
        Self {
            client,
            embedding_model,
            index,
            storage_path,
        }
    }

    /// Split text into chunks (by newlines for simplicity)
    fn chunk_text(text: &str) -> Vec<String> {
        text.split('\n')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Add text to the RAG index
    pub async fn add_text(&self, text: &str, collection: Option<String>) -> Result<()> {
        let chunks = Self::chunk_text(text);
        if chunks.is_empty() {
             return Ok(());
        }

        let mut doc_chunks = Vec::new();
        let collection_name = collection.unwrap_or_else(|| "default".to_string());

        // 1. Create text chunks (embeddings generated next)
        for chunk_content in chunks {
            // For ad-hoc RAG text, we don't have file paths or line numbers easily.
            // We use a placeholder.
            doc_chunks.push(TextChunk {
                file_path: "session_memory".to_string(),
                content: chunk_content,
                start_line: 0,
                end_line: 0,
                embedding: Vec::new(),
                collection: collection_name.clone(),
            });
        }
        
        // 2. Generate embeddings
        for chunk in &mut doc_chunks {
            if let Ok(embedding) = self.client.generate_embeddings(&self.embedding_model, &chunk.content).await {
                chunk.embedding = embedding;
            }
        }
        
        // Remove failed embeddings
        doc_chunks.retain(|c| !c.embedding.is_empty());

        if doc_chunks.is_empty() {
            return Ok(());
        }

        // 3. Add to shared index
        self.add_chunks(doc_chunks).await
    }

    /// Explicitly add chunks to the index
    pub async fn add_chunks(&self, doc_chunks: Vec<TextChunk>) -> Result<()> {
        if doc_chunks.is_empty() {
            return Ok(());
        }

        {
            let mut guard = self.index.lock().unwrap();
            if let Some(index) = &mut *guard {
                index.chunks.extend(doc_chunks);
                index.indexed_at = std::time::SystemTime::now();
            } else {
                 *guard = Some(VectorIndex {
                    chunks: doc_chunks,
                    indexed_at: std::time::SystemTime::now(),
                });
            }
        }

        self.save()?;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        if let Some(path) = &self.storage_path {
            let guard = self.index.lock().unwrap();
            if let Some(index) = &*guard {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let json = serde_json::to_string(index)?;
                fs::write(path, json)?;
            }
        }
        Ok(())
    }

    pub fn load(&self) -> Result<()> {
        if let Some(path) = &self.storage_path {
            if path.exists() {
                let content = fs::read_to_string(path)?;
                let index: VectorIndex = serde_json::from_str(&content)?;
                let mut guard = self.index.lock().unwrap();
                *guard = Some(index);
            }
        }
        Ok(())
    }

    /// Search the RAG index using Cosine Similarity
    /// collection_filter: If Some, only search chunks belonging to this collection.
    pub async fn search(&self, query: &str, limit: usize, collection_filter: Option<&str>) -> Result<Vec<String>> {
        // Check if index exists and has chunks (fast check)
        {
            let guard = self.index.lock().unwrap();
            if let Some(index) = &*guard {
                if index.chunks.is_empty() {
                    return Ok(Vec::new());
                }
            } else {
                return Ok(Vec::new());
            }
        }

        let query_embedding = self.client.generate_embeddings(&self.embedding_model, query).await?;
        
        // Re-acquire lock to search
        let guard = self.index.lock().unwrap();
        if let Some(index) = &*guard {
             // Calculate similarity for each chunk
            let mut scored_chunks: Vec<(&TextChunk, f64)> = index.chunks.iter()
                .filter(|chunk| {
                    match collection_filter {
                        Some(filter) => chunk.collection == filter,
                        None => true // If no filter, search everything? Or default?
                                     // Ideally, if no filter is provided, we might want to search ONLY "default" 
                                     // to avoid polluting general queries with "Work" data?
                                     // For now, let's keep it broad: No filter = Search All.
                    }
                })
                .map(|chunk| {
                    let score = cosine_similarity(&query_embedding, &chunk.embedding);
                    (chunk, score)
                }).collect();

            // Sort by score descending
            scored_chunks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // Return top `limit` results
            Ok(scored_chunks.into_iter().take(limit).map(|(c, _)| c.content.clone()).collect())
        } else {
            Ok(Vec::new())
        }
    }
}

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot_product: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    
    dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 1e-6);
    }
}
