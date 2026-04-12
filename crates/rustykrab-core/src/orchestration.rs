//! Types for the orchestration and RLM layers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How complex a task is — used for RLM routing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskComplexity {
    /// Simple acknowledgment or lookup — direct response.
    Trivial,
    /// Single tool call or straightforward answer.
    Simple,
    /// Needs decomposition via RLM.
    Moderate,
    /// Full RLM treatment with sub-queries.
    Complex,
    /// High-stakes: RLM + self-consistency voting.
    Critical,
}

/// Configuration for the orchestration/RLM system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OrchestrationConfig {
    /// Maximum recursion depth for RLM pattern.
    pub max_recursion_depth: usize,
    /// Maximum tool-call rounds per RLM call.
    pub max_tool_rounds: usize,
    /// Number of samples for self-consistency voting.
    pub consistency_samples: usize,
    /// Temperature spread for voting.
    pub consistency_temperature_spread: f32,
    /// Maximum number of concurrent LLM/tool tasks spawned in parallel.
    pub max_concurrent_tasks: usize,
    /// Timeout in seconds for individual model/LLM calls.
    pub model_call_timeout_secs: u64,
    /// Timeout in seconds for the entire pipeline.
    pub pipeline_timeout_secs: u64,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            max_recursion_depth: 3,
            max_tool_rounds: 10,
            consistency_samples: 3,
            consistency_temperature_spread: 0.1,
            max_concurrent_tasks: 10,
            model_call_timeout_secs: 120,
            pipeline_timeout_secs: 1800,
        }
    }
}

/// A node in the recursive call tree (RLM pattern).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecursiveCall {
    /// Unique call ID.
    pub id: Uuid,
    /// Parent call (None for root).
    pub parent_id: Option<Uuid>,
    /// The prompt/question for this call.
    pub prompt: String,
    /// Context budget for this call.
    pub context_budget: usize,
    /// Current recursion depth.
    pub depth: usize,
    /// Child calls spawned by this call.
    pub children: Vec<Uuid>,
    /// Result once resolved.
    pub result: Option<String>,
}

impl RecursiveCall {
    pub fn root(prompt: impl Into<String>, context_budget: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id: None,
            prompt: prompt.into(),
            context_budget,
            depth: 0,
            children: Vec::new(),
            result: None,
        }
    }

    pub fn child(
        parent_id: Uuid,
        prompt: impl Into<String>,
        context_budget: usize,
        depth: usize,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id: Some(parent_id),
            prompt: prompt.into(),
            context_budget,
            depth,
            children: Vec::new(),
            result: None,
        }
    }
}

/// Result of a self-consistency vote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteResult {
    /// The winning answer.
    pub answer: String,
    /// Number of samples that agreed with the winner.
    pub agreement_count: usize,
    /// Total number of samples.
    pub total_samples: usize,
    /// Whether the vote was unanimous.
    pub unanimous: bool,
    /// All individual responses for inspection.
    pub responses: Vec<String>,
    /// Confidence score (0.0-1.0) based on agreement ratio.
    pub confidence: f64,
}

/// Voting strategy for self-consistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VotingStrategy {
    /// Take the most common answer.
    Majority,
    /// Require all samples to agree, otherwise escalate.
    UnanimousOrEscalate,
}
