//! Knowledge graph backed by SQLite.
//!
//! Migrated from sled to share the same SQLite database as the memory
//! system. Uses recursive CTEs for BFS traversal instead of manual
//! iteration, and SQL transactions instead of sled's multi-tree
//! transactions for atomic entity deletion.

use std::sync::Arc;

use chrono::Utc;
use rustykrab_core::orchestration::{EntityType, KnowledgeEntity, KnowledgeRelation, RelationType};
use rustykrab_core::Error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::SqliteMemoryStorage;

/// A subgraph extracted from the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubGraph {
    pub entities: Vec<KnowledgeEntity>,
    pub relations: Vec<KnowledgeRelation>,
}

/// Knowledge graph backed by SQLite tables.
///
/// Entity and relation storage lives alongside the memory tables in
/// the same database, sharing WAL mode and connection pooling.
#[derive(Clone)]
pub struct KnowledgeGraph {
    storage: Arc<SqliteMemoryStorage>,
}

impl KnowledgeGraph {
    pub fn new(storage: Arc<SqliteMemoryStorage>) -> Self {
        Self { storage }
    }

    // ── Entity operations ───────────────────────────────────────

    /// Add or update an entity in the graph.
    pub async fn upsert_entity(&self, entity: &KnowledgeEntity) -> Result<(), Error> {
        let e = entity.clone();
        self.storage
            .with_conn(move |conn| {
                let entity_type_str = entity_type_to_str(&e.entity_type);
                conn.execute(
                    "INSERT INTO kg_entities (id, entity_type, name, attributes, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET
                         entity_type = excluded.entity_type,
                         name = excluded.name,
                         attributes = excluded.attributes,
                         updated_at = excluded.updated_at",
                    rusqlite::params![
                        e.id.to_string(),
                        entity_type_str,
                        e.name,
                        e.attributes.to_string(),
                        e.created_at.to_rfc3339(),
                        e.updated_at.to_rfc3339(),
                    ],
                )
                .map_err(|e| Error::Storage(e.to_string()))?;
                Ok(())
            })
            .await
    }

    /// Get an entity by ID.
    pub async fn get_entity(&self, id: Uuid) -> Result<Option<KnowledgeEntity>, Error> {
        let id_str = id.to_string();
        self.storage
            .with_conn(move |conn| {
                let mut stmt = conn
                    .prepare("SELECT * FROM kg_entities WHERE id = ?1")
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let mut rows = stmt
                    .query_map(rusqlite::params![id_str], row_to_entity)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                match rows.next() {
                    Some(Ok(entity)) => Ok(Some(entity)),
                    Some(Err(e)) => Err(Error::Storage(e.to_string())),
                    None => Ok(None),
                }
            })
            .await
    }

    /// Find an entity by name (case-insensitive).
    pub async fn find_by_name(&self, name: &str) -> Result<Option<KnowledgeEntity>, Error> {
        let name_lower = name.to_lowercase();
        self.storage
            .with_conn(move |conn| {
                let mut stmt = conn
                    .prepare("SELECT * FROM kg_entities WHERE LOWER(name) = ?1 LIMIT 1")
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let mut rows = stmt
                    .query_map(rusqlite::params![name_lower], row_to_entity)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                match rows.next() {
                    Some(Ok(entity)) => Ok(Some(entity)),
                    Some(Err(e)) => Err(Error::Storage(e.to_string())),
                    None => Ok(None),
                }
            })
            .await
    }

    /// Search entities by type.
    pub async fn find_by_type(
        &self,
        entity_type: &EntityType,
    ) -> Result<Vec<KnowledgeEntity>, Error> {
        let type_str = entity_type_to_str(entity_type);
        self.storage
            .with_conn(move |conn| {
                let mut stmt = conn
                    .prepare("SELECT * FROM kg_entities WHERE entity_type = ?1")
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map(rusqlite::params![type_str], row_to_entity)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let mut results = Vec::new();
                for row in rows {
                    results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
                }
                Ok(results)
            })
            .await
    }

    /// Search entities by keyword in name or attributes.
    pub async fn search_entities(&self, query: &str) -> Result<Vec<KnowledgeEntity>, Error> {
        let pattern = format!("%{}%", query.to_lowercase());
        self.storage
            .with_conn(move |conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT * FROM kg_entities
                         WHERE LOWER(name) LIKE ?1 OR LOWER(attributes) LIKE ?1",
                    )
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map(rusqlite::params![pattern], row_to_entity)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let mut results = Vec::new();
                for row in rows {
                    results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
                }
                Ok(results)
            })
            .await
    }

    /// Delete an entity and all its relations atomically.
    pub async fn delete_entity(&self, id: Uuid) -> Result<(), Error> {
        let id_str = id.to_string();
        self.storage
            .with_conn(move |conn| {
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|e| Error::Storage(e.to_string()))?;
                tx.execute(
                    "DELETE FROM kg_relations WHERE from_id = ?1 OR to_id = ?1",
                    rusqlite::params![id_str],
                )
                .map_err(|e| Error::Storage(e.to_string()))?;
                tx.execute(
                    "DELETE FROM kg_entities WHERE id = ?1",
                    rusqlite::params![id_str],
                )
                .map_err(|e| Error::Storage(e.to_string()))?;
                tx.commit().map_err(|e| Error::Storage(e.to_string()))?;
                Ok(())
            })
            .await
    }

    // ── Relation operations ─────────────────────────────────────

    /// Add a relation between two entities.
    pub async fn add_relation(&self, relation: &KnowledgeRelation) -> Result<(), Error> {
        let r = relation.clone();
        self.storage
            .with_conn(move |conn| {
                let rel_type = relation_type_to_str(&r.relation_type);
                conn.execute(
                    "INSERT OR REPLACE INTO kg_relations (from_id, to_id, relation_type, metadata)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        r.from_id.to_string(),
                        r.to_id.to_string(),
                        rel_type,
                        r.metadata.as_ref().map(|m| m.to_string()),
                    ],
                )
                .map_err(|e| Error::Storage(e.to_string()))?;
                Ok(())
            })
            .await
    }

    /// Get all relations from a given entity.
    pub async fn relations_from(
        &self,
        entity_id: Uuid,
    ) -> Result<Vec<KnowledgeRelation>, Error> {
        let id_str = entity_id.to_string();
        self.storage
            .with_conn(move |conn| {
                let mut stmt = conn
                    .prepare("SELECT * FROM kg_relations WHERE from_id = ?1")
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map(rusqlite::params![id_str], row_to_relation)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let mut results = Vec::new();
                for row in rows {
                    results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
                }
                Ok(results)
            })
            .await
    }

    /// Get all relations involving a given entity (incoming and outgoing).
    pub async fn relations_for(
        &self,
        entity_id: Uuid,
    ) -> Result<Vec<KnowledgeRelation>, Error> {
        let id_str = entity_id.to_string();
        self.storage
            .with_conn(move |conn| {
                let mut stmt = conn
                    .prepare("SELECT * FROM kg_relations WHERE from_id = ?1 OR to_id = ?1")
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map(rusqlite::params![id_str], row_to_relation)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let mut results = Vec::new();
                for row in rows {
                    results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
                }
                Ok(results)
            })
            .await
    }

    /// Remove a specific relation.
    pub async fn remove_relation(
        &self,
        from_id: Uuid,
        to_id: Uuid,
        relation_type: &RelationType,
    ) -> Result<(), Error> {
        let from_str = from_id.to_string();
        let to_str = to_id.to_string();
        let rel_type = relation_type_to_str(relation_type);
        self.storage
            .with_conn(move |conn| {
                conn.execute(
                    "DELETE FROM kg_relations WHERE from_id = ?1 AND to_id = ?2 AND relation_type = ?3",
                    rusqlite::params![from_str, to_str, rel_type],
                )
                .map_err(|e| Error::Storage(e.to_string()))?;
                Ok(())
            })
            .await
    }

    // ── Graph traversal ─────────────────────────────────────────

    /// Retrieve the relevant subgraph for a given set of entity IDs
    /// using a recursive CTE for BFS traversal.
    pub async fn retrieve_subgraph(
        &self,
        seed_ids: &[Uuid],
        max_hops: usize,
    ) -> Result<SubGraph, Error> {
        if seed_ids.is_empty() {
            return Ok(SubGraph {
                entities: Vec::new(),
                relations: Vec::new(),
            });
        }

        let seeds: Vec<String> = seed_ids.iter().map(|id| id.to_string()).collect();
        let max_hops = max_hops as u32;

        self.storage
            .with_conn(move |conn| {
                // Build seed placeholders.
                let placeholders: String = seeds
                    .iter()
                    .map(|s| format!("'{s}'"))
                    .collect::<Vec<_>>()
                    .join(",");

                // Recursive CTE for BFS traversal.
                let entity_sql = format!(
                    "WITH RECURSIVE subgraph(id, depth) AS (
                        SELECT id, 0 FROM kg_entities WHERE id IN ({placeholders})
                        UNION
                        SELECT CASE
                            WHEN r.from_id = s.id THEN r.to_id
                            ELSE r.from_id
                        END, s.depth + 1
                        FROM subgraph s
                        JOIN kg_relations r ON r.from_id = s.id OR r.to_id = s.id
                        WHERE s.depth < ?1
                    )
                    SELECT DISTINCT e.* FROM kg_entities e
                    JOIN subgraph s ON e.id = s.id"
                );

                let mut stmt = conn
                    .prepare(&entity_sql)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let entity_rows = stmt
                    .query_map(rusqlite::params![max_hops], row_to_entity)
                    .map_err(|e| Error::Storage(e.to_string()))?;

                let mut entities = Vec::new();
                let mut entity_ids = std::collections::HashSet::new();
                for row in entity_rows {
                    let entity = row.map_err(|e| Error::Storage(e.to_string()))?;
                    entity_ids.insert(entity.id);
                    entities.push(entity);
                }

                // Fetch all relations between the entities in the subgraph.
                let mut relations = Vec::new();
                if !entity_ids.is_empty() {
                    let id_list: String = entity_ids
                        .iter()
                        .map(|id| format!("'{id}'"))
                        .collect::<Vec<_>>()
                        .join(",");
                    let rel_sql = format!(
                        "SELECT * FROM kg_relations
                         WHERE from_id IN ({id_list}) AND to_id IN ({id_list})"
                    );
                    let mut stmt = conn
                        .prepare(&rel_sql)
                        .map_err(|e| Error::Storage(e.to_string()))?;
                    let rel_rows = stmt
                        .query_map([], row_to_relation)
                        .map_err(|e| Error::Storage(e.to_string()))?;
                    for row in rel_rows {
                        relations.push(row.map_err(|e| Error::Storage(e.to_string()))?);
                    }
                }

                Ok(SubGraph {
                    entities,
                    relations,
                })
            })
            .await
    }

    /// Format a subgraph as context text suitable for injection into a prompt.
    pub fn subgraph_to_context(subgraph: &SubGraph) -> String {
        let mut lines = Vec::new();
        lines.push("Known entities:".to_string());

        for entity in &subgraph.entities {
            let attrs = if entity.attributes.is_null() || entity.attributes == serde_json::json!({})
            {
                String::new()
            } else {
                format!(" — {}", entity.attributes)
            };
            lines.push(format!(
                "- {} ({:?}){attrs}",
                entity.name, entity.entity_type
            ));
        }

        if !subgraph.relations.is_empty() {
            lines.push(String::new());
            lines.push("Relationships:".to_string());
            let names: std::collections::HashMap<Uuid, &str> = subgraph
                .entities
                .iter()
                .map(|e| (e.id, e.name.as_str()))
                .collect();

            for rel in &subgraph.relations {
                let from = names.get(&rel.from_id).unwrap_or(&"?");
                let to = names.get(&rel.to_id).unwrap_or(&"?");
                lines.push(format!("- {from} --[{:?}]--> {to}", rel.relation_type));
            }
        }

        lines.join("\n")
    }

    /// List all entities, sorted by most recently updated.
    pub async fn list_entities(&self) -> Result<Vec<KnowledgeEntity>, Error> {
        self.storage
            .with_conn(move |conn| {
                let mut stmt = conn
                    .prepare("SELECT * FROM kg_entities ORDER BY updated_at DESC")
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map([], row_to_entity)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let mut results = Vec::new();
                for row in rows {
                    results.push(row.map_err(|e| Error::Storage(e.to_string()))?);
                }
                Ok(results)
            })
            .await
    }
}

// ── Row mappers ─────────────────────────────────────────────────

fn row_to_entity(row: &rusqlite::Row) -> rusqlite::Result<KnowledgeEntity> {
    let id_str: String = row.get("id")?;
    let type_str: String = row.get("entity_type")?;
    let attrs_str: String = row.get("attributes")?;
    let created_str: String = row.get("created_at")?;
    let updated_str: String = row.get("updated_at")?;

    Ok(KnowledgeEntity {
        id: Uuid::parse_str(&id_str).unwrap_or_default(),
        entity_type: str_to_entity_type(&type_str),
        name: row.get("name")?,
        attributes: serde_json::from_str(&attrs_str).unwrap_or_default(),
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_relation(row: &rusqlite::Row) -> rusqlite::Result<KnowledgeRelation> {
    let from_str: String = row.get("from_id")?;
    let to_str: String = row.get("to_id")?;
    let type_str: String = row.get("relation_type")?;
    let meta_str: Option<String> = row.get("metadata")?;

    Ok(KnowledgeRelation {
        from_id: Uuid::parse_str(&from_str).unwrap_or_default(),
        to_id: Uuid::parse_str(&to_str).unwrap_or_default(),
        relation_type: str_to_relation_type(&type_str),
        metadata: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
    })
}

// ── Type conversion helpers ─────────────────────────────────────

fn entity_type_to_str(t: &EntityType) -> String {
    match t {
        EntityType::Person => "person".to_string(),
        EntityType::Project => "project".to_string(),
        EntityType::Event => "event".to_string(),
        EntityType::Preference => "preference".to_string(),
        EntityType::Task => "task".to_string(),
        EntityType::Location => "location".to_string(),
        EntityType::Organization => "organization".to_string(),
        EntityType::Topic => "topic".to_string(),
        EntityType::Custom(s) => format!("custom:{s}"),
    }
}

fn str_to_entity_type(s: &str) -> EntityType {
    match s {
        "person" => EntityType::Person,
        "project" => EntityType::Project,
        "event" => EntityType::Event,
        "preference" => EntityType::Preference,
        "task" => EntityType::Task,
        "location" => EntityType::Location,
        "organization" => EntityType::Organization,
        "topic" => EntityType::Topic,
        other => {
            if let Some(custom) = other.strip_prefix("custom:") {
                EntityType::Custom(custom.to_string())
            } else {
                EntityType::Custom(other.to_string())
            }
        }
    }
}

fn relation_type_to_str(t: &RelationType) -> String {
    match t {
        RelationType::WorksWith => "works_with".to_string(),
        RelationType::DependsOn => "depends_on".to_string(),
        RelationType::Prefers => "prefers".to_string(),
        RelationType::ScheduledFor => "scheduled_for".to_string(),
        RelationType::BelongsTo => "belongs_to".to_string(),
        RelationType::RelatedTo => "related_to".to_string(),
        RelationType::CreatedBy => "created_by".to_string(),
        RelationType::AssignedTo => "assigned_to".to_string(),
        RelationType::Custom(s) => format!("custom:{s}"),
    }
}

fn str_to_relation_type(s: &str) -> RelationType {
    match s {
        "works_with" => RelationType::WorksWith,
        "depends_on" => RelationType::DependsOn,
        "prefers" => RelationType::Prefers,
        "scheduled_for" => RelationType::ScheduledFor,
        "belongs_to" => RelationType::BelongsTo,
        "related_to" => RelationType::RelatedTo,
        "created_by" => RelationType::CreatedBy,
        "assigned_to" => RelationType::AssignedTo,
        other => {
            if let Some(custom) = other.strip_prefix("custom:") {
                RelationType::Custom(custom.to_string())
            } else {
                RelationType::Custom(other.to_string())
            }
        }
    }
}
