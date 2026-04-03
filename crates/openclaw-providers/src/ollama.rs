use async_trait::async_trait;
use chrono::Utc;
use openclaw_core::error::Result;
use openclaw_core::model::{ModelProvider, ModelResponse, Usage};
use openclaw_core::types::{
    Message, MessageContent, Role, ToolCall, ToolSchema,
};
use openclaw_core::Error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Ollama provider for local models (Qwen, Llama, Mistral, etc.).
///
/// Connects to a local Ollama instance (default `http://localhost:11434`).
/// Supports tool calling via the Ollama chat API, which is compatible
/// with models that have been fine-tuned for function calling.
///
/// Recommended models:
/// - `qwen3:32b` — best tool-use quality per GB
/// - `llama4-scout` — good MoE option if you have 32GB+ VRAM
/// - `mistral-small` — lightweight fallback
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "http://localhost:11434".to_string(),
            model: model.into(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Convert internal messages to Ollama chat format.
    fn build_messages(messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .filter_map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };

                match &msg.content {
                    MessageContent::Text(text) => Some(OllamaMessage {
                        role: role.to_string(),
                        content: Some(text.clone()),
                        tool_calls: None,
                    }),
                    MessageContent::ToolCall(call) => Some(OllamaMessage {
                        role: role.to_string(),
                        content: None,
                        tool_calls: Some(vec![OllamaToolCall {
                            function: OllamaFunction {
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            },
                        }]),
                    }),
                    MessageContent::ToolResult(result) => Some(OllamaMessage {
                        role: role.to_string(),
                        content: Some(
                            serde_json::to_string(&result.output).unwrap_or_default(),
                        ),
                        tool_calls: None,
                    }),
                }
            })
            .collect()
    }

    /// Convert tool schemas to Ollama's tool format.
    fn build_tools(tools: &[ToolSchema]) -> Vec<OllamaTool> {
        tools
            .iter()
            .map(|t| OllamaTool {
                r#type: "function".to_string(),
                function: OllamaToolDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    /// Parse Ollama response into internal types.
    fn parse_response(resp: OllamaResponse) -> Result<ModelResponse> {
        let msg = resp.message;

        // Check for tool calls first.
        if let Some(tool_calls) = msg.tool_calls {
            if let Some(tc) = tool_calls.into_iter().next() {
                return Ok(ModelResponse {
                    message: Message {
                        id: Uuid::new_v4(),
                        role: Role::Assistant,
                        content: MessageContent::ToolCall(ToolCall {
                            id: Uuid::new_v4().to_string(),
                            name: tc.function.name,
                            arguments: tc.function.arguments,
                        }),
                        created_at: Utc::now(),
                    },
                    usage: Usage {
                        prompt_tokens: resp.prompt_eval_count.unwrap_or(0),
                        completion_tokens: resp.eval_count.unwrap_or(0),
                    },
                });
            }
        }

        // Text response.
        Ok(ModelResponse {
            message: Message {
                id: Uuid::new_v4(),
                role: Role::Assistant,
                content: MessageContent::Text(msg.content.unwrap_or_default()),
                created_at: Utc::now(),
            },
            usage: Usage {
                prompt_tokens: resp.prompt_eval_count.unwrap_or(0),
                completion_tokens: resp.eval_count.unwrap_or(0),
            },
        })
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<ModelResponse> {
        let ollama_messages = Self::build_messages(messages);
        let ollama_tools = Self::build_tools(tools);

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": ollama_messages,
            "stream": false,
        });

        if !ollama_tools.is_empty() {
            body["tools"] = serde_json::to_value(&ollama_tools)
                .map_err(|e| Error::Serialization(e))?;
        }

        tracing::debug!(model = %self.model, base_url = %self.base_url, "calling Ollama chat API");

        let url = format!("{}/api/chat", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                Error::ModelProvider(format!(
                    "failed to connect to Ollama at {}: {e}. Is Ollama running?",
                    self.base_url
                ))
            })?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(Error::ModelProvider(format!(
                "Ollama API returned {status}: {error_body}"
            )));
        }

        let ollama_resp: OllamaResponse = resp
            .json()
            .await
            .map_err(|e| Error::ModelProvider(format!("failed to parse Ollama response: {e}")))?;

        Self::parse_response(ollama_resp)
    }
}

// --- Ollama API wire types (private) ---

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Serialize, Deserialize)]
struct OllamaToolCall {
    function: OllamaFunction,
}

#[derive(Serialize, Deserialize)]
struct OllamaFunction {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Serialize)]
struct OllamaTool {
    r#type: String,
    function: OllamaToolDef,
}

#[derive(Serialize)]
struct OllamaToolDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaResponseMessage,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OllamaToolCall>>,
}
