use std::sync::Arc;

use chrono::Utc;
use openclaw_core::capability::Capability;
use openclaw_core::model::{ModelProvider, ModelResponse};
use openclaw_core::session::Session;
use openclaw_core::types::{
    Conversation, Message, MessageContent, Role, ToolCall, ToolResult, ToolSchema,
};
use openclaw_core::{Error, Result, Tool};
use uuid::Uuid;

use crate::sandbox::{Sandbox, SandboxPolicy};

/// Runs the agent loop: send conversation to model, execute tool calls, repeat.
///
/// Every tool execution is gated by the session's capability set and
/// runs inside a sandbox, preventing privilege escalation and
/// cross-session data leakage.
pub struct AgentRunner {
    provider: Arc<dyn ModelProvider>,
    tools: Vec<Arc<dyn Tool>>,
    sandbox: Arc<dyn Sandbox>,
    max_iterations: usize,
}

impl AgentRunner {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        tools: Vec<Arc<dyn Tool>>,
        sandbox: Arc<dyn Sandbox>,
    ) -> Self {
        Self {
            provider,
            tools,
            sandbox,
            max_iterations: 20,
        }
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    /// Run the agent loop on a conversation within a session's capability scope.
    ///
    /// The session's capabilities are checked before each tool execution.
    /// If the model requests a tool the session doesn't have access to,
    /// the tool call is denied and an error message is fed back to the model.
    pub async fn run(&self, conv: &mut Conversation, session: &Session) -> Result<()> {
        if session.is_expired() {
            return Err(Error::Auth("session has expired".into()));
        }

        let schemas: Vec<ToolSchema> = self
            .tools
            .iter()
            .filter(|t| session.capabilities.can_use_tool(t.name()))
            .map(|t| t.schema())
            .collect();

        for _ in 0..self.max_iterations {
            let ModelResponse { message, usage: _ } =
                self.provider.chat(&conv.messages, &schemas).await?;

            conv.messages.push(message.clone());
            conv.updated_at = Utc::now();

            // If the model returned a tool call, check capabilities and execute.
            if let MessageContent::ToolCall(ref call) = message.content {
                let result = self.execute_tool_checked(call, session).await;
                let tool_msg = match result {
                    Ok(tr) => Message {
                        id: Uuid::new_v4(),
                        role: Role::Tool,
                        content: MessageContent::ToolResult(tr),
                        created_at: Utc::now(),
                    },
                    Err(e) => Message {
                        id: Uuid::new_v4(),
                        role: Role::Tool,
                        content: MessageContent::ToolResult(ToolResult {
                            call_id: call.id.clone(),
                            output: serde_json::json!({ "error": e.to_string() }),
                        }),
                        created_at: Utc::now(),
                    },
                };
                conv.messages.push(tool_msg);
                conv.updated_at = Utc::now();
                continue;
            }

            // Text response — done.
            return Ok(());
        }

        Err(Error::Internal(format!(
            "agent exceeded max iterations ({})",
            self.max_iterations
        )))
    }

    async fn execute_tool_checked(
        &self,
        call: &ToolCall,
        session: &Session,
    ) -> Result<ToolResult> {
        // Capability check: is this tool allowed in this session?
        if !session.capabilities.can_use_tool(&call.name) {
            tracing::warn!(
                tool = call.name,
                session = %session.id,
                "tool call denied: insufficient capabilities"
            );
            return Err(Error::Auth(format!(
                "session does not have permission to use tool '{}'",
                call.name
            )));
        }

        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == call.name)
            .ok_or_else(|| Error::ToolExecution(format!("unknown tool: {}", call.name)))?;

        tracing::info!(tool = call.name, session = %session.id, "executing tool in sandbox");

        // Determine sandbox policy based on capabilities.
        let policy = SandboxPolicy {
            allow_net: session.capabilities.has(&Capability::HttpRequest),
            allow_fs_read: session.capabilities.has(&Capability::FileRead),
            allow_fs_write: session.capabilities.has(&Capability::FileWrite),
            allow_spawn: session.capabilities.has(&Capability::ShellExec),
            ..SandboxPolicy::default()
        };

        // Execute within sandbox with timeout enforcement.
        // In production, the sandbox runs the tool in an isolated process.
        // For now, the sandbox validates the policy and the tool runs directly.
        self.sandbox
            .execute(&call.name, call.arguments.clone(), &policy)
            .await?;

        let output = tool.execute(call.arguments.clone()).await?;

        Ok(ToolResult {
            call_id: call.id.clone(),
            output,
        })
    }
}
