use serde::{Deserialize, Serialize};

use crate::runner::AgentConfig;

/// A serializable harness profile that bundles agent loop parameters
/// into a single, swappable configuration.
///
/// Profiles vary the agent loop's runtime behavior (iteration limits,
/// error thresholds, retry counts) without injecting different system
/// prompts. The system prompt is intentionally minimal and static.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HarnessProfile {
    /// Human-readable name for this profile.
    pub name: String,

    /// Agent identity (used by the prompt builder, though the prompt is
    /// currently static — kept for future customization).
    pub agent_name: String,
    pub agent_description: String,

    // --- Agent loop parameters ---
    /// Maximum iterations before the agent gives up.
    pub max_iterations: usize,
    /// Iteration count at which a soft warning is injected, nudging the agent
    /// to wrap up or save progress. Set to 0 to disable.
    pub soft_iteration_warning: usize,
    /// Consecutive errors before injecting a reflection prompt.
    pub max_consecutive_errors: usize,
    /// Max retries per failed tool call.
    pub max_tool_retries: u32,

    // --- Context budget ---
    /// Model's context window size in tokens.
    pub max_context_tokens: usize,
}

/// Task-type hint for agent loop parameter selection.
///
/// Used only for selecting runtime config presets (iteration limits,
/// retry counts). Does not affect the system prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    General,
    Coding,
    Research,
    Creative,
    Planning,
}

impl Default for HarnessProfile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            agent_name: "RustyKrab".to_string(),
            agent_description: "a personal AI agent".to_string(),
            max_iterations: 200,
            soft_iteration_warning: 150,
            max_consecutive_errors: 3,
            max_tool_retries: 2,
            max_context_tokens: 128_000,
        }
    }
}

impl HarnessProfile {
    /// Preset optimized for coding tasks.
    pub fn coding() -> Self {
        Self {
            name: "coding".to_string(),
            max_consecutive_errors: 2, // Reflect sooner on code errors.
            max_tool_retries: 3,
            ..Self::default()
        }
    }

    /// Preset optimized for research tasks.
    pub fn research() -> Self {
        Self {
            name: "research".to_string(),
            ..Self::default()
        }
    }

    /// Preset for creative tasks: fewer iterations.
    pub fn creative() -> Self {
        Self {
            name: "creative".to_string(),
            max_iterations: 100,
            soft_iteration_warning: 75,
            max_tool_retries: 1,
            ..Self::default()
        }
    }

    /// Convert this profile into an AgentConfig for the runner.
    pub fn to_agent_config(&self) -> AgentConfig {
        AgentConfig {
            max_iterations: self.max_iterations,
            soft_iteration_warning: self.soft_iteration_warning,
            max_consecutive_errors: self.max_consecutive_errors,
            max_tool_retries: self.max_tool_retries,
            max_context_tokens: self.max_context_tokens,
        }
    }
}
