# Provider Implementations Review

## CRITICAL

### PROV-C1: No Retry Logic for Transient Failures
- **File:** `crates/rustykrab-providers/src/anthropic.rs:229-238`, `ollama.rs:269-281`
- **Description:** Neither provider retries on transient errors. A single 429 (rate limit), 529 (overloaded), or network blip fails the entire request. Mid-pipeline failures lose the whole tool-use chain.
- **Fix:** Add retry with exponential backoff + jitter for 429, 500, 502, 503, 529. Parse `retry-after` header. Max 3-5 retries.

### PROV-C2: No Request Timeouts
- **File:** `crates/rustykrab-providers/src/anthropic.rs:27`, `ollama.rs:74`
- **Description:** `reqwest::Client::new()` has no timeout. LLM requests can hang indefinitely, freezing the agent loop.
- **Fix:** `Client::builder().timeout(Duration::from_secs(300)).connect_timeout(Duration::from_secs(10))`.

### PROV-C3: API Key Stored as Plain String
- **File:** `crates/rustykrab-providers/src/anthropic.rs:19`
- **Description:** API key is a plain `String` on the heap. Core dumps, accidental logging, or `/proc/<pid>/mem` expose it.
- **Fix:** Use `secrecy::SecretString`. Never derive `Debug` without redacting.

## HIGH

### PROV-H1: No Streaming Implementation
- **File:** Both providers
- **Description:** Neither provider overrides `chat_stream()`. The default calls `chat()` synchronously and emits the entire result as a single `TextDelta`. No output until full generation completes (30+ seconds). No mid-stream cancellation.
- **Fix:** Implement streaming for both (Anthropic SSE, Ollama newline-delimited JSON).

### PROV-H2: Anthropic `system` Parameter Format Prevents Caching
- **File:** `crates/rustykrab-providers/src/anthropic.rs:219-221`
- **Description:** System prompt set as plain string. Array format required for prompt caching (`cache_control` on blocks). Prevents one of the biggest cost-saving features for agentic loops.
- **Fix:** Use `body["system"] = json!([{"type": "text", "text": sys}])`.

### PROV-H3: Tool Result Content Double-Serialized
- **File:** `crates/rustykrab-providers/src/anthropic.rs:103-104`
- **Description:** `serde_json::to_string(&result.output)` converts JSON Value to an escaped string. The model sees `"{\"files\": [\"foo.rs\"]}"` instead of readable content.
- **Fix:** Use string values directly; pretty-print structured values.

### PROV-H4: No Error Differentiation
- **File:** `crates/rustykrab-providers/src/anthropic.rs:241-245`, `ollama.rs:283-288`
- **Description:** All non-2xx responses become opaque `Error::ModelProvider(String)`. No distinction between 400, 401, 429, 500. Callers can't implement smart retry or show specific errors.
- **Fix:** Parse status code and map to specific error variants.

### PROV-H5: Mixed Tool-Call + Text Response Content Lost
- **File:** `crates/rustykrab-providers/src/anthropic.rs:156-173`
- **Description:** When Anthropic returns both text and tool_use blocks, the `if !tool_calls.is_empty()` branch returns early, discarding all text. Model explanations before tool calls are lost.
- **Fix:** Extend `MessageContent` to support text + tool calls together.

## MEDIUM

### PROV-M1: Ollama Ignores `done_reason` Field
- **File:** `crates/rustykrab-providers/src/ollama.rs:183-233`
- **Description:** `done_reason: "length"` (truncated output) is not parsed. Always returns `StopReason::EndTurn` even on truncation.
- **Fix:** Map `"length"` to `StopReason::MaxTokens`.

### PROV-M2: Ollama `num_parallel` Config Never Sent
- **File:** `crates/rustykrab-providers/src/ollama.rs:250-259`
- **Description:** Field exists in config but never included in request body.
- **Fix:** Remove from config (it's a server-level setting) or add to request options.

### PROV-M3: `unwrap_or_default()` Silently Swallows Serialization Failures
- **File:** `crates/rustykrab-providers/src/anthropic.rs:104`, `ollama.rs:146`
- **Description:** Produces empty string on failure, causing model to think tool returned nothing.
- **Fix:** Propagate the error with `?`.

### PROV-M4: Empty Message List Not Validated
- **File:** Both providers
- **Description:** Empty `messages` array sent to API results in 400 from Anthropic. Should validate locally with descriptive error.

### PROV-M5: Cache Token Fields Not Captured
- **File:** `crates/rustykrab-providers/src/anthropic.rs:316-320`
- **Description:** `cache_read_input_tokens` and `cache_creation_input_tokens` not deserialized. Loses billing data.
- **Fix:** Add with `#[serde(default)]`.

### PROV-M6: `reqwest::Client` Created Per-Provider
- **File:** Both providers
- **Description:** Each provider creates own client, missing connection pooling benefits. Multiple instances waste sockets.
- **Fix:** Accept a shared `reqwest::Client` in constructor.

## LOW

### PROV-L1: Multiple System Messages -- Last One Wins
- **File:** `crates/rustykrab-providers/src/anthropic.rs:50-54`
- **Description:** Multiple system messages silently overwritten. No warning.

### PROV-L2: `filter_map` in Ollama Never Filters
- **File:** `crates/rustykrab-providers/src/ollama.rs:102-153`
- **Description:** Every match arm returns `Some`. Should be plain `.map()`.

### PROV-L3: `ResponseBlock` Missing Catch-All Variant
- **File:** `crates/rustykrab-providers/src/anthropic.rs:303-314`
- **Description:** Only handles `text` and `tool_use`. New block types (e.g., `thinking`) crash deserialization.
- **Fix:** Add `#[serde(other)] Unknown` variant.

### PROV-L4: Hardcoded Default Model String
- **File:** `crates/rustykrab-providers/src/anthropic.rs:29`
- **Description:** `"claude-sonnet-4-20250514"` is hardcoded. No runtime/compile-time staleness check.

### PROV-L5: No `is_error` Flag on Tool Results
- **File:** `crates/rustykrab-providers/src/anthropic.rs:100-106`
- **Description:** Anthropic supports `is_error: bool` on tool_result but it's not implemented. Model can't distinguish tool failures from successes.
