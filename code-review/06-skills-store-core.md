# Skills, Store & Core Review

## CRITICAL

### SKL-C1: Skill Verification Never Called During Loading
- **File:** `crates/rustykrab-skills/src/loader.rs:15-57`
- **Description:** `load_skills_from_dir` loads SKILL.md files and parses them into executable skill definitions, but **never calls `SkillVerifier::verify()` or `verify_skill_bundle()`**. The entire ed25519 verification module is decorative. Any file placed in the skills directory is loaded as trusted instructions.
- **Fix:** `load_single_skill` should accept a `&SkillVerifier`, read a companion `.sig` file, and verify before proceeding.

### SKL-C2: `verify_skill_bundle` Concatenation Ambiguity
- **File:** `crates/rustykrab-skills/src/verify.rs:54-64`
- **Description:** Signs/verifies `manifest_bytes || code_bytes` without length delimiter. An attacker can shift bytes between manifest and code boundaries while keeping the concatenation identical, forging a valid signature.
- **Fix:** Prepend `manifest_bytes.len()` as a fixed-width (8-byte LE) integer before the manifest.

### SKL-C3: Master Key Cloned Out of `Zeroizing` Wrapper
- **File:** `crates/rustykrab-store/src/lib.rs:73`
- **Description:** `(*self.master_key).clone()` creates a non-zeroized `Vec<u8>` copy that persists in freed heap memory. The entire point of `Zeroizing<Vec<u8>>` is defeated.
- **Fix:** Change `SecretStore.master_key` to `Zeroizing<Vec<u8>>` or use `Arc<Zeroizing<Vec<u8>>>`.

## HIGH

### SKL-H1: Serde Format Change Breaks Persisted Conversations
- **File:** `crates/rustykrab-core/src/types.rs:23-27`
- **Description:** `MessageContent` changed from `#[serde(untagged)]` to `#[serde(tag = "type", content = "data")]`. Existing stored conversations silently fail to deserialize -- permanent data loss on upgrade.
- **Fix:** Implement migration: try current format, fall back to old, re-save.

### SKL-H2: Prompt Injection via Skill Body
- **File:** `crates/rustykrab-skills/src/prompt.rs:205-211`
- **Description:** `with_active_skill` escapes skill `name` but injects `body` raw into XML. A body containing `</skill_instructions>` breaks out and injects arbitrary system-level prompt content.
- **Fix:** Use CDATA wrapping: `<![CDATA[{body}]]>` (escaping `]]>` within).

### SKL-H3: Persistent Prompt Injection via Poisoned Memories
- **File:** `crates/rustykrab-skills/src/prompt.rs:112-119`
- **Description:** `[END RECALLED MEMORIES]` markers in the summary are not sanitized. A poisoned memory can break out of the fence and inject instructions appearing as system prompt.
- **Fix:** Strip or escape marker strings from the summary.

### SKL-H4: Knowledge Graph Name Index Inconsistency on Rename
- **File:** `crates/rustykrab-store/src/knowledge_graph.rs:46-59`
- **Description:** `upsert_entity` inserts new name->ID mapping but never removes old mapping on rename. Stale entries accumulate forever.
- **Fix:** Fetch existing entity, compare names, remove old index entry if changed.

### SKL-H5: No Symlink Protection in Skill Loader
- **File:** `crates/rustykrab-skills/src/loader.rs:24-35`
- **Description:** Directory iteration follows symlinks. A symlink `skills/evil -> /etc/` causes reading arbitrary files.
- **Fix:** Check `entry.file_type()?.is_symlink()` and skip, or canonicalize and verify path.

## MEDIUM

### SKL-M1: Skill Registry Silent Override on Name Collision
- **File:** `crates/rustykrab-skills/src/skill.rs:48-58`
- **Description:** `register()` uses `skill.id()`, `register_md()` uses `skill.frontmatter.name`. Collisions silently overwrite -- could shadow legitimate skills with malicious ones.
- **Fix:** Check for existing entries and error on collision.

### SKL-M2: Knowledge Graph Entity Deletion Not Atomic
- **File:** `crates/rustykrab-store/src/knowledge_graph.rs:125-162`
- **Description:** Three separate operations (remove entity, remove name index, remove relations) without transactions. Crash between ops leaves orphaned relations. Deserialization errors silently swallowed during relation scan.
- **Fix:** Use sled's transactional API.

### SKL-M3: Memory Store `recall` Full-Table Scan
- **File:** `crates/rustykrab-store/src/memory.rs:75-113`
- **Description:** Scans all memory entries across all conversations to filter by `conversation_id`. O(N) on total memories.
- **Fix:** Use composite key `conversation_id || memory_id` with prefix scan.

### SKL-M4: `with_available_skills` Leaks Filesystem Paths
- **File:** `crates/rustykrab-skills/src/prompt.rs:184-199`
- **Description:** Full filesystem paths injected into system prompt, exposing server structure.
- **Fix:** Use sanitized relative paths or opaque identifiers.

### SKL-M5: `SecretStore` Derived Keys Not Zeroized
- **File:** `crates/rustykrab-store/src/secret.rs:31`
- **Description:** `derive_key` returns `[u8; 32]` on stack without zeroization. Derived keys linger in memory.
- **Fix:** Use `Zeroizing<[u8; 32]>`.

### SKL-M6: Skill Name Not Validated
- **File:** `crates/rustykrab-skills/src/skill_md.rs:71-98`
- **Description:** No validation on skill name content. Slashes, null bytes, newlines, or extreme lengths could cause issues.
- **Fix:** Validate with pattern `^[a-zA-Z0-9_-]{1,128}$`.

### SKL-M7: `ConversationStore::save` Flushes on Every Write
- **File:** `crates/rustykrab-store/src/conversation.rs:38-39`
- **Description:** Forces synchronous fsync after every insert. Significant I/O bottleneck for frequent saves.
- **Fix:** Remove per-write flush; let callers use `Store::flush()` at checkpoints.

### SKL-M8: `relations_from` Prefix Scan Fragility
- **File:** `crates/rustykrab-store/src/knowledge_graph.rs:177-186`
- **Description:** No defensive assertion after deserialization. If key structure changes, silent bugs appear.
- **Fix:** Add `debug_assert_eq!(rel.from_id, entity_id)`.

## LOW

### SKL-L1: Skill Description Uses First Line of Body
- **File:** `crates/rustykrab-skills/src/skill.rs:74-83`
- **Description:** `lines().next()` on markdown body gives headings like `# Instructions` instead of the frontmatter `description` field.
- **Fix:** Expose `description()` from the trait.

### SKL-L2: `extract_keywords` Non-Deterministic Order
- **File:** `crates/rustykrab-store/src/memory.rs:158-187`
- **Description:** `HashSet` iteration order is non-deterministic.
- **Fix:** Use `BTreeSet` or sort.

### SKL-L3: `which_bin` Doesn't Check Execute Permission
- **File:** `crates/rustykrab-skills/src/loader.rs:98-109`
- **Description:** Checks `is_file()` but not the execute bit.

### SKL-L4: `SkillMdFrontmatter.extra` Absorbs Unknown YAML Keys
- **File:** `crates/rustykrab-skills/src/skill_md.rs:24-25`
- **Description:** `#[serde(flatten)]` catch-all silently absorbs keys like `trusted: true` that future code might check.

### SKL-L5: `ConversationStore::delete` Doesn't Confirm Existence
- **File:** `crates/rustykrab-store/src/conversation.rs:67-72`
- **Description:** Silently succeeds when ID doesn't exist.

### SKL-L6: Inconsistent Flush Behavior Across Stores
- **File:** `crates/rustykrab-store/src/memory.rs:48-55`
- **Description:** `MemoryStore::save` doesn't flush; `ConversationStore::save` does. Inconsistent durability.
