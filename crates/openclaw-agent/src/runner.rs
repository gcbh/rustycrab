use std::sync::Arc;

use chrono::Utc;
use openclaw_core::model::{ModelProvider, ModelResponse};
use openclaw_core::types::{
    Conversation, Message, MessageContent, Role, ToolCall, ToolResult, ToolSchema,
};
use openclaw_core::{Error, Result, Tool};
use uuid::Uuid;

/// Runs the agent loop: send conversation to model, execute tool calls, repeat.
pub struct AgentRunner {
    provider: Arc<dyn ModelProvider>,
    tools: Vec<Arc<dyn Tool>>,
    max_iterations: usize,
}

impl AgentRunner {
    pub fn new(provider: Arc<dyn ModelProvider>, tools: Vec<Arc<dyn Tool>>) -> Self {
        Self {
            provider,
            tools,
            max_iterations: 20,
        }
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    /// Run the agent loop on a conversation until the model produces a final
    /// text response (no tool calls) or the iteration limit is reached.
    pub async fn run(&self, conv: &mut Conversation) -> Result<()> {
        let schemas: Vec<ToolSchema> = self.tools.iter().map(|t| t.schema()).collect();

        for _ in 0..self.max_iterations {
            let ModelResponse { message, usage: _ } =
                self.provider.chat(&conv.messages, &schemas).await?;

            conv.messages.push(message.clone());
            conv.updated_at = Utc::now();

            // If the model returned a tool call, execute it and loop.
            if let MessageContent::ToolCall(ref call) = message.content {
                let result = self.execute_tool(call).await?;
                let tool_msg = Message {
                    id: Uuid::new_v4(),
                    role: Role::Tool,
                    content: MessageContent::ToolResult(result),
                    created_at: Utc::now(),
                };
                conv.messages.push(tool_msg);
                conv.updated_at = Utc::now();
                continue;
            }

            // Otherwise it's a final text response — we're done.
            return Ok(());
        }

        Err(Error::Internal(format!(
            "agent exceeded max iterations ({})",
            self.max_iterations
        )))
    }

    async fn execute_tool(&self, call: &ToolCall) -> Result<ToolResult> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == call.name)
            .ok_or_else(|| Error::ToolExecution(format!("unknown tool: {}", call.name)))?;

        tracing::info!(tool = call.name, "executing tool");
        let output = tool.execute(call.arguments.clone()).await?;

        Ok(ToolResult {
            call_id: call.id.clone(),
            output,
        })
    }
}
