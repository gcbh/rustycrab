use async_trait::async_trait;
use openclaw_core::types::ToolSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// A skill is a composable unit: a system prompt plus a set of tools
/// that together give an agent a specific capability.
#[async_trait]
pub trait Skill: Send + Sync {
    /// Unique identifier for this skill.
    fn id(&self) -> &str;

    /// Human-readable name.
    fn name(&self) -> &str;

    /// System prompt fragment injected when this skill is active.
    fn system_prompt(&self) -> &str;

    /// Tool schemas this skill contributes.
    fn tools(&self) -> Vec<ToolSchema>;
}

/// Metadata describing a skill (for listing / UI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Registry that holds all available skills.
pub struct SkillRegistry {
    skills: HashMap<String, Arc<dyn Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    pub fn register(&mut self, skill: Arc<dyn Skill>) {
        self.skills.insert(skill.id().to_string(), skill);
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn Skill>> {
        self.skills.get(id)
    }

    pub fn list(&self) -> Vec<SkillManifest> {
        self.skills
            .values()
            .map(|s| SkillManifest {
                id: s.id().to_string(),
                name: s.name().to_string(),
                description: s.system_prompt().lines().next().unwrap_or("").to_string(),
            })
            .collect()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
