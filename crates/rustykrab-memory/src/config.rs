use serde::{Deserialize, Serialize};

/// Configuration for the memory system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    // Chunking
    /// Maximum tokens per chunk (estimated at ~3.5 chars/token).
    pub chunk_max_tokens: usize,
    /// Overlap between chunks as a fraction (0.0–1.0). 0.15 = 15% overlap.
    pub chunk_overlap_ratio: f64,

    // Retrieval
    /// Number of candidates to over-fetch from each retrieval arm before fusion.
    pub retrieval_candidates_per_arm: usize,
    /// RRF fusion constant k (default 60).
    pub rrf_k: f64,
    /// Weight for semantic retrieval in RRF.
    pub rrf_weight_semantic: f64,
    /// Weight for keyword/BM25 retrieval in RRF.
    pub rrf_weight_keyword: f64,
    /// Weight for graph-based retrieval in RRF.
    pub rrf_weight_graph: f64,
    /// Weight for temporal retrieval in RRF.
    pub rrf_weight_temporal: f64,
    /// Default number of results to return from recall.
    pub default_recall_limit: usize,

    // Lifecycle
    /// Default decay rate for new memories (1.0 = 37% after one idle week).
    pub default_decay_rate: f64,
    /// Default importance for new memories.
    pub default_importance: f64,
    /// Minimum effective score below which episodic memories get archived.
    pub archive_score_threshold: f64,
    /// Minimum access count to promote episodic → semantic.
    pub promote_min_access_count: u32,
    /// Minimum age in days before episodic → semantic promotion.
    pub promote_min_age_days: u32,
    /// Days of idleness before archival → tombstone.
    pub tombstone_idle_days: u32,
    /// Importance threshold below which archival memories get tombstoned.
    pub tombstone_importance_threshold: f64,

    // Deduplication
    /// Cosine similarity threshold for auto-merge (≥0.95).
    pub dedup_auto_merge_threshold: f64,
    /// Cosine similarity below which memories are considered distinct.
    pub dedup_distinct_threshold: f64,

    // Embedding
    /// Dimensionality of embedding vectors.
    pub embedding_dimensions: usize,
    /// Model version string for provenance tracking.
    pub embedding_model_version: String,

    // Session flushing
    /// Seconds of inactivity before Working memories auto-flush to Episodic.
    pub idle_flush_timeout_secs: u64,
    /// Maximum age in seconds for Working memories before forced flush,
    /// regardless of activity. Prevents unbounded growth during long sessions.
    pub max_working_age_secs: u64,
    /// How often the background flusher checks for idle/max-age (seconds).
    pub flush_check_interval_secs: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            // Chunking: 512 tokens with 15% overlap (NVIDIA benchmark optimal)
            chunk_max_tokens: 512,
            chunk_overlap_ratio: 0.15,

            // Retrieval
            retrieval_candidates_per_arm: 50,
            rrf_k: 60.0,
            rrf_weight_semantic: 1.0,
            rrf_weight_keyword: 1.0,
            rrf_weight_graph: 0.8,
            rrf_weight_temporal: 0.6,
            default_recall_limit: 10,

            // Lifecycle
            default_decay_rate: 1.0,
            default_importance: 0.5,
            archive_score_threshold: 0.05,
            promote_min_access_count: 3,
            promote_min_age_days: 7,
            tombstone_idle_days: 180,
            tombstone_importance_threshold: 0.3,

            // Dedup
            dedup_auto_merge_threshold: 0.95,
            dedup_distinct_threshold: 0.85,

            // Embedding (Nomic-embed-text-v1.5 default)
            embedding_dimensions: 768,
            embedding_model_version: "nomic-embed-text-v1.5".to_string(),

            // Session flushing
            idle_flush_timeout_secs: 300,     // 5 minutes idle → flush
            max_working_age_secs: 1800,       // 30 minutes max → force flush
            flush_check_interval_secs: 30,    // check every 30 seconds
        }
    }
}
