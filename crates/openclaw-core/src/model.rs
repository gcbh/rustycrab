use async_trait::async_trait;

use crate::error::Result;
use crate::types::{Message, ToolSchema};

/// Response from a model provider.
#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub message: Message,
    pub usage: Usage,
}

/// Token usage for a single request.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// Trait implemented by every model provider (e.g. Anthropic, OpenAI).
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Human-readable name of the provider.
    fn name(&self) -> &str;

    /// Send a conversation to the model and get back the next message.
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<ModelResponse>;
}
