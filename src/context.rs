//! Context management for intelligent conversation handling.
//!
//! This module provides:
//! - System resource detection (RAM)
//! - Optimal context window calculation based on model and system resources
//! - Conversation summarization when context limits are approached

use crate::ollama::{ChatMessage, ChatMessageRequest};
use sysinfo::System;

/// Manages context window sizing and conversation summarization.
#[derive(Debug, Clone)]
pub struct ContextManager {
    /// Threshold (0.0-1.0) at which to trigger summarization
    pub summarization_threshold: f32,
    /// Whether to use automatic context sizing
    pub auto_context: bool,
    /// Whether summarization is enabled
    pub summarization_enabled: bool,
}

impl Default for ContextManager {
    fn default() -> Self {
        Self {
            summarization_threshold: 0.8,
            auto_context: true,
            summarization_enabled: true,
        }
    }
}

/// System resource information
#[derive(Debug, Clone)]
pub struct SystemResources {
    /// Available RAM in MB
    pub available_ram_mb: u64,
    /// Total RAM in MB
    pub total_ram_mb: u64,
}

impl ContextManager {
    pub fn new(
        auto_context: bool,
        summarization_enabled: bool,
        summarization_threshold: f32,
    ) -> Self {
        Self {
            summarization_threshold,
            auto_context,
            summarization_enabled,
        }
    }

    /// Detect available system resources
    pub fn detect_system_resources() -> SystemResources {
        let mut sys = System::new_all();
        sys.refresh_memory();

        let available_ram_mb = sys.available_memory() / (1024 * 1024);
        let total_ram_mb = sys.total_memory() / (1024 * 1024);

        SystemResources {
            available_ram_mb,
            total_ram_mb,
        }
    }

    /// Calculate optimal context size based on model's native context and available resources.
    ///
    /// Heuristics:
    /// - Never exceed model's native context_length
    /// - Cap at available_ram_mb / 4 (rough estimate of 4 bytes per token)
    /// - Minimum of 2048 tokens
    pub fn get_optimal_context_size(model_context: Option<usize>, available_ram_mb: u64) -> usize {
        const MIN_CONTEXT: usize = 2048;
        const DEFAULT_CONTEXT: usize = 4096;

        // Calculate RAM-based limit (very rough: 4 bytes per token, 1KB headroom per token)
        // This is conservative - most models use less than 1KB per token for KV cache
        let ram_based_limit = (available_ram_mb / 4) as usize * 1024; // tokens

        let model_limit = model_context.unwrap_or(DEFAULT_CONTEXT);

        // Take the minimum of model limit and RAM limit, but at least MIN_CONTEXT
        let optimal = model_limit.min(ram_based_limit).max(MIN_CONTEXT);

        optimal
    }

    /// Check if the conversation should be summarized based on current token usage.
    pub fn should_summarize(&self, current_tokens: usize, limit: usize) -> bool {
        if !self.summarization_enabled || limit == 0 {
            return false;
        }

        let usage_ratio = current_tokens as f32 / limit as f32;
        usage_ratio >= self.summarization_threshold
    }

    /// Calculate current approximate token count for messages.
    pub fn estimate_token_count(messages: &[ChatMessage]) -> usize {
        messages
            .iter()
            .map(|m| {
                // Rough estimate: 1 token ≈ 4 chars, plus overhead for message structure
                (m.content.len() / 4) + 4
            })
            .sum()
    }

    /// Generate a prompt to summarize the conversation.
    /// The summary will be used to replace older messages.
    pub fn generate_summary_prompt(messages: &[ChatMessage]) -> String {
        let mut conversation = String::new();
        for msg in messages {
            conversation.push_str(&format!("{}: {}\n\n", msg.role.to_uppercase(), msg.content));
        }

        format!(
            r#"Please provide a concise summary of the following conversation. Focus on:
- Key topics discussed
- Important decisions or conclusions
- Any pending questions or tasks

Keep the summary brief but comprehensive enough to maintain conversation context.

CONVERSATION:
{}

SUMMARY:"#,
            conversation
        )
    }

    /// Apply a summary to the message history, replacing old messages.
    /// Keeps the most recent messages and replaces earlier ones with a summary.
    /// Generate a summary of the provided messages.
    /// Returns the summary string and the number of messages summarized.
    /// This is NON-DESTRUCTIVE - it does not modify the input messages.
    pub fn summarize_messages(messages: &[ChatMessage], keep_recent: usize) -> Option<(String, usize)> {
        if messages.len() <= keep_recent {
             return None;
        }

        let count_to_summarize = messages.len() - keep_recent;
        if count_to_summarize == 0 {
            return None;
        }
        
        let messages_to_summarize = &messages[0..count_to_summarize];
        let prompt = Self::generate_summary_prompt(messages_to_summarize);
        
        Some((prompt, count_to_summarize))
    }

    /// Build a request for generating a summary
    pub fn build_summary_request(
        messages: &[ChatMessage],
        system_prompt: &str,
    ) -> Vec<ChatMessageRequest> {
        let summary_prompt = Self::generate_summary_prompt(messages);

        vec![
            ChatMessageRequest {
                role: "system".to_string(),
                content: system_prompt.to_string(),
                images: None,
                tool_calls: None,
                tool_name: None,
            },
            ChatMessageRequest {
                role: "user".to_string(),
                content: summary_prompt,
                images: None,
                tool_calls: None,
                tool_name: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimal_context_low_ram() {
        // 8GB available RAM should cap context appropriately
        let optimal = ContextManager::get_optimal_context_size(Some(32768), 8000);
        // Should not exceed model's 32k context, but may be limited by RAM
        assert!(optimal >= 2048);
        assert!(optimal <= 32768);
    }

    #[test]
    fn test_optimal_context_high_ram() {
        // 64GB available RAM should allow using model's full context
        let optimal = ContextManager::get_optimal_context_size(Some(8192), 64000);
        assert_eq!(optimal, 8192); // Should use model's full context
    }

    #[test]
    fn test_optimal_context_no_model_info() {
        // When model context is unknown, use default
        let optimal = ContextManager::get_optimal_context_size(None, 16000);
        assert!(optimal >= 2048);
    }

    #[test]
    fn test_should_summarize_threshold() {
        let manager = ContextManager::new(true, true, 0.8);

        // At 80% capacity -> should summarize
        assert!(manager.should_summarize(800, 1000));

        // At 50% capacity -> should not
        assert!(!manager.should_summarize(500, 1000));

        // At 100% capacity -> should summarize
        assert!(manager.should_summarize(1000, 1000));
    }

    #[test]
    fn test_should_summarize_disabled() {
        let manager = ContextManager::new(true, false, 0.8);
        // Even at 100% capacity, don't summarize if disabled
        assert!(!manager.should_summarize(1000, 1000));
    }

    #[test]
    fn test_generate_summary_prompt() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                images: None,
                tool_calls: None,
                tool_name: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
                images: None,
                tool_calls: None,
                tool_name: None,
            },
        ];

        let prompt = ContextManager::generate_summary_prompt(&messages);
        assert!(prompt.contains("USER: Hello"));
        assert!(prompt.contains("ASSISTANT: Hi there!"));
        assert!(prompt.contains("SUMMARY:"));
    }

    #[test]
    fn test_summarize_messages() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Old message 1".to_string(),
                images: None,
                tool_calls: None,
                tool_name: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "Old response 1".to_string(),
                images: None,
                tool_calls: None,
                tool_name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Recent message".to_string(),
                images: None,
                tool_calls: None,
                tool_name: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "Recent response".to_string(),
                images: None,
                tool_calls: None,
                tool_name: None,
            },
        ];

        let result = ContextManager::summarize_messages(&messages, 2);

        // Should have summary string and count=2
        assert!(result.is_some());
        let (prompt, count) = result.unwrap();
        assert_eq!(count, 2);
        assert!(prompt.contains("Old message 1"));
    }

    #[test]
    fn test_estimate_token_count() {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello world this is a test message".to_string(), // ~35 chars
            images: None,
            tool_calls: None,
            tool_name: None,
        }];

        let count = ContextManager::estimate_token_count(&messages);
        // 35/4 + 4 = ~12-13 tokens
        assert!(count > 0);
        assert!(count < 20);
    }

    #[test]
    fn test_detect_system_resources() {
        let resources = ContextManager::detect_system_resources();
        // Just verify it returns something reasonable
        // Note: available_ram_mb can be 0 in some test environments (e.g., CI containers)
        assert!(resources.total_ram_mb > 0);
        assert!(resources.available_ram_mb <= resources.total_ram_mb);
    }
}
