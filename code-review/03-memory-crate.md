# Memory Crate Review

## CRITICAL

### MEM-C1: Chunking Panics on Multi-byte UTF-8
- **File:** `crates/rustykrab-memory/src/chunking.rs:25-50`
- **Description:** `chunk_text` uses byte offsets (`content.len()`, `content[start..chunk_end]`). For any multi-byte UTF-8 content (accented characters, CJK, emoji), the slice panics at runtime.
- **Fix:** Use `char_indices()` for offset maps or snap to `is_char_boundary()`.

### MEM-C2: Race Condition in Sled Read-Modify-Write Operations
- **File:** `crates/rustykrab-memory/src/storage.rs:276-301`
- **Description:** `update_stage`, `record_access`, and `invalidate` perform non-atomic read-modify-write: `get_memory` then `upsert_memory` with no locking. Concurrent `record_access` calls both read `access_count=5`, increment to 6, write 6 -- losing one increment. Concurrent `record_access` can overwrite `lifecycle_stage` changes.
- **Fix:** Use sled's `fetch_and_update` or `compare_and_swap`.

## HIGH

### MEM-H1: BM25 Index Not Partitioned by Agent
- **File:** `crates/rustykrab-memory/src/bm25.rs` (entire), `retrieval.rs:190-191`
- **Description:** Single global BM25 index shared across all agents. Agent A's recall returns agent B's memories. Index grows without bound (never pruned on tombstone/archive). Lost on restart.
- **Fix:** Partition per agent (`HashMap<Uuid, Bm25Index>`) or filter results post-search.

### MEM-H2: Full Table Scan on Every Storage Operation
- **File:** `crates/rustykrab-memory/src/storage.rs:222-273, 339-358, 372-383, 408-419`
- **Description:** Every query deserializes the entire sled tree and filters in Rust. No secondary indexes for `agent_id`, `lifecycle_stage`, or `created_at`. Recall latency is O(total_memories) not O(agent_memories).
- **Fix:** Add compound prefix keys (e.g., `{agent_id}:{lifecycle_stage}:{memory_id}`) for prefix scans.

### MEM-H3: Blocking Sled I/O on Async Executor
- **File:** `crates/rustykrab-memory/src/storage.rs` (all async methods)
- **Description:** All `SledMemoryStorage` methods are `async` but perform synchronous sled I/O directly. Blocks the tokio executor thread.
- **Fix:** Wrap sled operations in `tokio::task::spawn_blocking`.

### MEM-H4: Double Full-Table Scan Per Recall
- **File:** `crates/rustykrab-memory/src/retrieval.rs:166, 212`
- **Description:** `retrieve_semantic` and `retrieve_graph` both independently call `get_all_chunk_embeddings`. Both in `tokio::join!`, so both scan the entire chunks tree. Doubles I/O on the hot path.
- **Fix:** Fetch embeddings once and pass as shared reference.

### MEM-H5: Fire-and-Forget `record_access` Silently Loses Errors
- **File:** `crates/rustykrab-memory/src/retrieval.rs:136-143`
- **Description:** Each recall result spawns `tokio::spawn` with `let _ = storage.record_access(id).await`. Errors discarded, no limit on spawned tasks, JoinHandle dropped (panics silent).
- **Fix:** Batch access updates into a single call after the loop.

### MEM-H6: Near-Duplicate Detection is O(n^2)
- **File:** `crates/rustykrab-memory/src/lifecycle.rs:146-224`
- **Description:** Compares every pair of memories. For 10,000 memories: ~50 million cosine similarity computations plus 10,000 storage round-trips. No batching or early termination.
- **Fix:** Use approximate nearest neighbor (ANN) or cap the number of memories processed.

## MEDIUM

### MEM-M1: RRF Normalization Destroys Ranking Discrimination
- **File:** `crates/rustykrab-memory/src/retrieval.rs:113`
- **Description:** `normalized_rrf = (*rrf_score * self.config.rrf_k).min(1.0)`. Any document found by 2+ retrieval arms gets RRF score clamped to 1.0, destroying ranking discrimination.
- **Fix:** Normalize by `max_rrf = (sum_of_all_weights) / rrf_k`.

### MEM-M2: `unsigned_abs()` Masks Clock Skew Bugs
- **File:** `crates/rustykrab-memory/src/types.rs:97`, `lifecycle.rs:79`
- **Description:** Converts negative durations (future timestamps) to positive, silently hiding clock skew or data corruption.
- **Fix:** Use `.max(0)` and log a warning on negative values.

### MEM-M3: Regex Compilation on Every Call
- **File:** `crates/rustykrab-memory/src/extraction.rs:22-109, 116-153`
- **Description:** Multiple regexes compiled from string patterns on every invocation. Expensive.
- **Fix:** Use `OnceLock` or `lazy_static!` for one-time compilation.

### MEM-M4: `estimate_tokens` Uses Byte Length Not Char Count
- **File:** `crates/rustykrab-memory/src/chunking.rs:3`
- **Description:** `text.len()` returns bytes, not characters. For CJK text (3 bytes/char), overestimates by ~3x.
- **Fix:** Use `text.chars().count()`.

### MEM-M5: Near-Duplicate Creates Unidirectional Links for Similar Memories
- **File:** `crates/rustykrab-memory/src/lifecycle.rs:201-213`
- **Description:** Similarity in [0.85, 0.95) creates unidirectional links only. Graph traversal depends on indexing order.
- **Fix:** Always create bidirectional links.

### MEM-M6: `rebuild_bm25_index` Doesn't Clear Old Entries
- **File:** `crates/rustykrab-memory/src/writer.rs:202-214`
- **Description:** Adds all memories without clearing first. Repeated calls double-index documents, corrupting BM25 scores.
- **Fix:** Clear the index before rebuilding.

### MEM-M7: `HashEmbedder` Can Produce NaN/Degenerate Vectors
- **File:** `crates/rustykrab-memory/src/embedding.rs:125-126`
- **Description:** `f32::from_bits(u32::from_le_bytes(bytes))` can produce NaN/Inf. The `is_finite()` check replaces with 0.0 but degenerate vectors defeat embedding purpose.
- **Fix:** Map `u32` to `[-1, 1]` deterministically.

### MEM-M8: `get_links_for` Uses String Containment on Key
- **File:** `crates/rustykrab-memory/src/storage.rs:408-419`
- **Description:** Full scan of links tree checking `key.contains(&id_str)`. O(total_links) per call.
- **Fix:** Add reverse index tree for efficient lookup.

## LOW

### MEM-L1: `sentiment_intensity` Substring Matching False Positives
- **File:** `crates/rustykrab-memory/src/scoring.rs:68-83`
- **Description:** `lower.contains(**w)` matches substrings ("must" matches "mustard", "love" matches "glove").
- **Fix:** Use word boundary matching.

### MEM-L2: `has_temporal_markers` Matches Common Word "may"
- **File:** `crates/rustykrab-memory/src/scoring.rs:113`
- **Description:** "may" (modal verb) in temporal markers. Also matches "maybe", "mayonnaise".

### MEM-L3: No Config Validation
- **File:** `crates/rustykrab-memory/src/config.rs`
- **Description:** `chunk_max_tokens: 0` causes division issues, `rrf_k: 0.0` causes division by zero.
- **Fix:** Add a `validate()` method.

### MEM-L4: `cosine_similarity` Uses `debug_assert` for Dimension Mismatch
- **File:** `crates/rustykrab-memory/src/embedding.rs:23`
- **Description:** Check stripped in release builds. Mismatched dimensions silently compute wrong results.

### MEM-L5: Link Key Uses `Debug` Formatting (Not Stable)
- **File:** `crates/rustykrab-memory/src/storage.rs:386-389`
- **Description:** `{:?}` for `LinkType` is not stable across compiler versions. Existing links become orphaned on rename.
- **Fix:** Implement `Display` with stable representations.

### MEM-L6: `token_count` Field in `ConversationTurn` Never Used
- **File:** `crates/rustykrab-memory/src/types.rs:130`
- **Description:** Dead metadata. Writer always uses `estimate_tokens` instead.

### MEM-L7: `chunk_by_turns` Speaker Detection Fragile
- **File:** `crates/rustykrab-memory/src/chunking.rs:93-104`
- **Description:** False positives on "Note: ...", "Error: ...". Misses speakers with spaces.
