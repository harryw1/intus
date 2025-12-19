use anyhow::Result;
use crate::ollama::OllamaClient;

#[derive(Debug, Clone)]
pub struct DocumentChunk {
    pub content: String,
    pub embedding: Vec<f64>,
}

pub struct RagSystem {
    client: OllamaClient,
    embedding_model: String,
    chunks: Vec<DocumentChunk>,
}

impl RagSystem {
    pub fn new(client: OllamaClient, embedding_model: String) -> Self {
        Self {
            client,
            embedding_model,
            chunks: Vec::new(),
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
    pub async fn add_text(&mut self, text: &str) -> Result<()> {
        let chunks = Self::chunk_text(text);
        for chunk in chunks {
            if let Ok(embedding) = self.client.generate_embeddings(&self.embedding_model, &chunk).await {
                self.chunks.push(DocumentChunk {
                    content: chunk,
                    embedding,
                });
            }
        }
        Ok(())
    }

    /// Search the RAG index using Cosine Similarity
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        if self.chunks.is_empty() {
            return Ok(Vec::new());
        }

        let query_embedding = self.client.generate_embeddings(&self.embedding_model, query).await?;
        
        // Calculate similarity for each chunk
        let mut scored_chunks: Vec<(&DocumentChunk, f64)> = self.chunks.iter().map(|chunk| {
            let score = cosine_similarity(&query_embedding, &chunk.embedding);
            (chunk, score)
        }).collect();

        // Sort by score descending
        scored_chunks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top `limit` results
        Ok(scored_chunks.into_iter().take(limit).map(|(c, _)| c.content.clone()).collect())
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
