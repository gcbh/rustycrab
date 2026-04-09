use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustykrab_core::Result;
use uuid::Uuid;

use crate::types::{
    ExtractedFact, LifecycleStage, Memory, MemoryChunk, MemoryLink,
};

/// Abstract storage backend for the memory system.
///
/// All retrieval, write, and lifecycle operations go through this trait,
/// allowing different backends (sled, SQLite, PostgreSQL) to be swapped.
#[async_trait]
pub trait MemoryStorage: Send + Sync {
    // ── Memory CRUD ─────────────────────────────────────────────

    /// Insert or update a memory record.
    async fn upsert_memory(&self, memory: &Memory) -> Result<()>;

    /// Retrieve a memory by ID.
    async fn get_memory(&self, id: Uuid) -> Result<Option<Memory>>;

    /// Retrieve multiple memories by IDs.
    async fn get_memories(&self, ids: &[Uuid]) -> Result<Vec<Memory>>;

    /// Check for exact content duplicate within a time window.
    async fn find_by_content_hash(
        &self,
        agent_id: Uuid,
        content_hash: &str,
    ) -> Result<Option<Memory>>;

    /// List all valid memories for an agent in a given lifecycle stage.
    async fn list_by_stage(
        &self,
        agent_id: Uuid,
        stage: LifecycleStage,
    ) -> Result<Vec<Memory>>;

    /// List all valid, retrievable memories for an agent.
    async fn list_retrievable(&self, agent_id: Uuid) -> Result<Vec<Memory>>;

    /// List memories created within a time range, sorted by recency.
    async fn list_by_time_range(
        &self,
        agent_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Memory>>;

    /// Update lifecycle stage for a memory.
    async fn update_stage(&self, id: Uuid, stage: LifecycleStage) -> Result<()>;

    /// Record an access (increment access_count, update last_accessed_at).
    async fn record_access(&self, id: Uuid) -> Result<()>;

    /// Soft-delete: mark a memory as invalid.
    async fn invalidate(&self, id: Uuid, invalidated_by: Option<Uuid>) -> Result<()>;

    // ── Chunk operations ────────────────────────────────────────

    /// Store embedding chunks for a memory.
    async fn store_chunks(&self, chunks: &[MemoryChunk]) -> Result<()>;

    /// Retrieve all chunks for a memory.
    async fn get_chunks_for_memory(&self, memory_id: Uuid) -> Result<Vec<MemoryChunk>>;

    /// Retrieve all chunks with embeddings for an agent (for vector search).
    async fn get_all_chunk_embeddings(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<(Uuid, Vec<f32>)>>;

    // ── Extracted facts ─────────────────────────────────────────

    /// Store extracted facts.
    async fn store_facts(&self, facts: &[ExtractedFact]) -> Result<()>;

    /// Get facts extracted from a specific memory.
    async fn get_facts_for_memory(&self, memory_id: Uuid) -> Result<Vec<ExtractedFact>>;

    // ── Memory links (graph) ────────────────────────────────────

    /// Add or update a link between two memories.
    async fn upsert_link(&self, link: &MemoryLink) -> Result<()>;

    /// Get all outgoing links from a memory.
    async fn get_links_from(&self, source_id: Uuid) -> Result<Vec<MemoryLink>>;

    /// Get all links involving a memory (incoming + outgoing).
    async fn get_links_for(&self, memory_id: Uuid) -> Result<Vec<MemoryLink>>;

    // ── Bulk operations ─────────────────────────────────────────

    /// Batch update lifecycle stages (used by sweep).
    async fn batch_update_stages(
        &self,
        updates: &[(Uuid, LifecycleStage)],
    ) -> Result<u32>;
}

/// Sled-backed implementation of MemoryStorage.
///
/// Uses separate sled trees for memories, chunks, facts, and links.
/// This keeps the embedded, single-file-database approach consistent
/// with the rest of the RustyKrab storage layer.
pub struct SledMemoryStorage {
    memories: sled::Tree,
    chunks: sled::Tree,
    facts: sled::Tree,
    links: sled::Tree,
    /// Secondary index: content_hash → memory_id for dedup.
    hash_index: sled::Tree,
    /// Secondary index: memory_id → [chunk_id] for chunk lookup.
    memory_chunks: sled::Tree,
}

impl SledMemoryStorage {
    /// Open or create sled trees under the given database.
    pub fn open(db: &sled::Db) -> Result<Self> {
        Ok(Self {
            memories: db
                .open_tree("hybrid_memories")
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?,
            chunks: db
                .open_tree("hybrid_chunks")
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?,
            facts: db
                .open_tree("hybrid_facts")
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?,
            links: db
                .open_tree("hybrid_links")
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?,
            hash_index: db
                .open_tree("hybrid_hash_idx")
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?,
            memory_chunks: db
                .open_tree("hybrid_mem_chunks")
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?,
        })
    }

    fn serialize<T: serde::Serialize>(val: &T) -> Result<Vec<u8>> {
        serde_json::to_vec(val).map_err(rustykrab_core::Error::Serialization)
    }

    fn deserialize<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
        serde_json::from_slice(bytes)
            .map_err(rustykrab_core::Error::Serialization)
    }
}

#[async_trait]
impl MemoryStorage for SledMemoryStorage {
    async fn upsert_memory(&self, memory: &Memory) -> Result<()> {
        let key = memory.id.as_bytes().to_vec();
        let value = Self::serialize(memory)?;
        self.memories
            .insert(key, value)
            .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;

        // Update hash index.
        let hash_key = format!("{}:{}", memory.agent_id, memory.content_hash);
        self.hash_index
            .insert(hash_key.as_bytes(), memory.id.as_bytes().as_slice())
            .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;

        Ok(())
    }

    async fn get_memory(&self, id: Uuid) -> Result<Option<Memory>> {
        match self
            .memories
            .get(id.as_bytes())
            .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?
        {
            Some(bytes) => Ok(Some(Self::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    async fn get_memories(&self, ids: &[Uuid]) -> Result<Vec<Memory>> {
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(mem) = self.get_memory(*id).await? {
                results.push(mem);
            }
        }
        Ok(results)
    }

    async fn find_by_content_hash(
        &self,
        agent_id: Uuid,
        content_hash: &str,
    ) -> Result<Option<Memory>> {
        let hash_key = format!("{agent_id}:{content_hash}");
        match self
            .hash_index
            .get(hash_key.as_bytes())
            .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?
        {
            Some(id_bytes) => {
                let id_arr: [u8; 16] = id_bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| rustykrab_core::Error::Storage("invalid uuid bytes".into()))?;
                let id = Uuid::from_bytes(id_arr);
                self.get_memory(id).await
            }
            None => Ok(None),
        }
    }

    async fn list_by_stage(
        &self,
        agent_id: Uuid,
        stage: LifecycleStage,
    ) -> Result<Vec<Memory>> {
        let mut results = Vec::new();
        for item in self.memories.iter() {
            let (_, value) =
                item.map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
            let mem: Memory = Self::deserialize(&value)?;
            if mem.agent_id == agent_id && mem.lifecycle_stage == stage && mem.is_valid {
                results.push(mem);
            }
        }
        Ok(results)
    }

    async fn list_retrievable(&self, agent_id: Uuid) -> Result<Vec<Memory>> {
        let mut results = Vec::new();
        for item in self.memories.iter() {
            let (_, value) =
                item.map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
            let mem: Memory = Self::deserialize(&value)?;
            if mem.agent_id == agent_id
                && mem.is_valid
                && mem.lifecycle_stage.is_retrievable()
            {
                results.push(mem);
            }
        }
        Ok(results)
    }

    async fn list_by_time_range(
        &self,
        agent_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let mut results = Vec::new();
        for item in self.memories.iter() {
            let (_, value) =
                item.map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
            let mem: Memory = Self::deserialize(&value)?;
            if mem.agent_id == agent_id
                && mem.is_valid
                && mem.lifecycle_stage.is_retrievable()
                && mem.created_at >= from
                && mem.created_at <= to
            {
                results.push(mem);
            }
        }
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results.truncate(limit);
        Ok(results)
    }

    async fn update_stage(&self, id: Uuid, stage: LifecycleStage) -> Result<()> {
        if let Some(mut mem) = self.get_memory(id).await? {
            mem.lifecycle_stage = stage;
            self.upsert_memory(&mem).await?;
        }
        Ok(())
    }

    async fn record_access(&self, id: Uuid) -> Result<()> {
        if let Some(mut mem) = self.get_memory(id).await? {
            mem.access_count += 1;
            mem.last_accessed_at = Some(Utc::now());
            self.upsert_memory(&mem).await?;
        }
        Ok(())
    }

    async fn invalidate(&self, id: Uuid, invalidated_by: Option<Uuid>) -> Result<()> {
        if let Some(mut mem) = self.get_memory(id).await? {
            mem.is_valid = false;
            mem.invalidated_by = invalidated_by;
            mem.invalidated_at = Some(Utc::now());
            mem.lifecycle_stage = LifecycleStage::Tombstone;
            self.upsert_memory(&mem).await?;
        }
        Ok(())
    }

    async fn store_chunks(&self, chunks: &[MemoryChunk]) -> Result<()> {
        for chunk in chunks {
            let key = chunk.id.as_bytes().to_vec();
            let value = Self::serialize(chunk)?;
            self.chunks
                .insert(key, value)
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;

            // Update memory → chunks index.
            let idx_key = format!("{}:{}", chunk.memory_id, chunk.chunk_index);
            self.memory_chunks
                .insert(idx_key.as_bytes(), chunk.id.as_bytes().as_slice())
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
        }
        Ok(())
    }

    async fn get_chunks_for_memory(&self, memory_id: Uuid) -> Result<Vec<MemoryChunk>> {
        let prefix = format!("{memory_id}:");
        let mut chunks = Vec::new();
        for item in self.memory_chunks.scan_prefix(prefix.as_bytes()) {
            let (_, chunk_id_bytes) =
                item.map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
            if let Some(chunk_bytes) = self
                .chunks
                .get(&chunk_id_bytes)
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?
            {
                chunks.push(Self::deserialize(&chunk_bytes)?);
            }
        }
        chunks.sort_by_key(|c: &MemoryChunk| c.chunk_index);
        Ok(chunks)
    }

    async fn get_all_chunk_embeddings(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<(Uuid, Vec<f32>)>> {
        // First get all retrievable memory IDs for this agent.
        let memories = self.list_retrievable(agent_id).await?;
        let mem_ids: std::collections::HashSet<Uuid> =
            memories.iter().map(|m| m.id).collect();

        let mut results = Vec::new();
        for item in self.chunks.iter() {
            let (_, value) =
                item.map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
            let chunk: MemoryChunk = Self::deserialize(&value)?;
            if mem_ids.contains(&chunk.memory_id) && !chunk.embedding.is_empty() {
                // Return memory_id (not chunk_id) for RRF dedup at memory level.
                results.push((chunk.memory_id, chunk.embedding));
            }
        }
        Ok(results)
    }

    async fn store_facts(&self, facts: &[ExtractedFact]) -> Result<()> {
        for fact in facts {
            let key = fact.id.as_bytes().to_vec();
            let value = Self::serialize(fact)?;
            self.facts
                .insert(key, value)
                .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
        }
        Ok(())
    }

    async fn get_facts_for_memory(&self, memory_id: Uuid) -> Result<Vec<ExtractedFact>> {
        let mut results = Vec::new();
        for item in self.facts.iter() {
            let (_, value) =
                item.map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
            let fact: ExtractedFact = Self::deserialize(&value)?;
            if fact.source_memory_id == memory_id {
                results.push(fact);
            }
        }
        Ok(results)
    }

    async fn upsert_link(&self, link: &MemoryLink) -> Result<()> {
        let key = format!(
            "{}:{}:{:?}",
            link.source_id, link.target_id, link.link_type
        );
        let value = Self::serialize(link)?;
        self.links
            .insert(key.as_bytes(), value)
            .map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
        Ok(())
    }

    async fn get_links_from(&self, source_id: Uuid) -> Result<Vec<MemoryLink>> {
        let prefix = format!("{source_id}:");
        let mut results = Vec::new();
        for item in self.links.scan_prefix(prefix.as_bytes()) {
            let (_, value) =
                item.map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
            results.push(Self::deserialize(&value)?);
        }
        Ok(results)
    }

    async fn get_links_for(&self, memory_id: Uuid) -> Result<Vec<MemoryLink>> {
        let mut results = Vec::new();
        let id_str = memory_id.to_string();
        for item in self.links.iter() {
            let (key, value) =
                item.map_err(|e| rustykrab_core::Error::Storage(e.to_string()))?;
            let key_str = String::from_utf8_lossy(&key);
            if key_str.contains(&id_str) {
                results.push(Self::deserialize(&value)?);
            }
        }
        Ok(results)
    }

    async fn batch_update_stages(
        &self,
        updates: &[(Uuid, LifecycleStage)],
    ) -> Result<u32> {
        let mut count = 0u32;
        for &(id, stage) in updates {
            self.update_stage(id, stage).await?;
            count += 1;
        }
        Ok(count)
    }
}
