# Error Handling & Robustness Review

## CRITICAL

### ERR-C1: `unimplemented!()` Panic in WebChatChannel::receive()
- **File:** `crates/rustykrab-channels/src/webchat.rs:48`
- **Description:** `receive()` calls `unimplemented!()`, which panics unconditionally. Since this implements the `Channel` trait, any generic code calling `.receive()` on a `dyn Channel` crashes the async runtime.
- **Fix:** Return a proper error or redesign the trait so `WebChatChannel` doesn't implement `Channel`.

### ERR-C2: `last_err.unwrap()` Can Panic if Retry Logic Changes
- **File:** `crates/rustykrab-agent/src/orchestrator/executor.rs:316`, `crates/rustykrab-agent/src/runner.rs:860`
- **Description:** `last_err.unwrap()` after retry loops. Currently safe but fragile -- any code change that skips setting `last_err` causes a panic.
- **Fix:** Use `.unwrap_or_else(|| ...)` for defense-in-depth.

## HIGH

### ERR-H1: No Request Timeout on Anthropic API Calls
- **File:** `crates/rustykrab-providers/src/anthropic.rs:25-26`
- **Description:** `reqwest::Client::new()` has no timeout. Calls to the Anthropic API can hang indefinitely.
- **Fix:** Use `Client::builder().timeout(Duration::from_secs(300)).connect_timeout(Duration::from_secs(10)).build()`.

### ERR-H2: No Request Timeout on Ollama API Calls
- **File:** `crates/rustykrab-providers/src/ollama.rs:72-75`
- **Description:** Same issue. Ollama inference can take very long, and a hung connection blocks the task indefinitely.
- **Fix:** Same as ERR-H1 with a longer timeout for model loading.

### ERR-H3: No Request Timeout on Signal Channel HTTP Calls
- **File:** `crates/rustykrab-channels/src/signal.rs:51-52`
- **Description:** `reqwest::Client::new()` without timeout. Polling loop hangs indefinitely if signal-cli-rest-api is unresponsive.
- **Fix:** Add client-side timeout.

### ERR-H4: No Request Timeout on Telegram Channel HTTP Calls
- **File:** `crates/rustykrab-channels/src/telegram.rs:57-58`
- **Description:** The `timeout=30` query parameter is server-side only. No client-side reqwest timeout exists.
- **Fix:** Add client-side timeout.

### ERR-H5: Spawned Tasks Silently Lose Results on Panic
- **File:** `crates/rustykrab-agent/src/orchestrator/executor.rs:101-111`
- **Description:** When a sub-task panics, the error is logged but the task result is never added to the `results` map. Downstream code never knows the task existed, producing incomplete results silently.
- **Fix:** Insert a failure `SubTaskResult` for panicked tasks.

### ERR-H6: Signal Polling Loop Never Terminates
- **File:** `crates/rustykrab-channels/src/signal.rs:133-156`
- **Description:** `start_polling()` runs in an infinite `loop` with no `CancellationToken` or shutdown signal. On application shutdown, continues running until process is killed.
- **Fix:** Accept a `CancellationToken` parameter.

### ERR-H7: Telegram Polling Loop Never Terminates
- **File:** `crates/rustykrab-channels/src/telegram.rs:215-278`
- **Description:** Same issue as signal. No graceful shutdown mechanism.
- **Fix:** Accept a `CancellationToken` parameter.

### ERR-H8: `expect()` in Telegram HMAC Creation
- **File:** `crates/rustykrab-channels/src/telegram.rs:435`
- **Description:** `HmacSha256::new_from_slice(...).expect(...)` in a network-facing webhook handler. If HMAC impl changes, this panics in production.
- **Fix:** Use `?` operator instead of `.expect()`.

## MEDIUM

### ERR-M1: Silent Error Swallowing in Decomposer Fallback
- **File:** `crates/rustykrab-agent/src/orchestrator/decomposer.rs:116-119`
- **Description:** Decomposition failures are swallowed and logged at `debug` level. Model misconfigurations are silently masked.
- **Fix:** Log at `warn` level and include the original error.

### ERR-M2: `unwrap_or("")` Hides Non-Text Model Responses
- **Files:** `crates/rustykrab-agent/src/orchestrator/pipeline.rs:117-118`, `verifier.rs:174-176`, `executor.rs:222-224`, `synthesizer.rs:97-99`, `rlm/recursive_call.rs:273-277`
- **Description:** If the model returns a non-text response (e.g., a tool call when text was expected), `as_text().unwrap_or("")` silently produces an empty string throughout the pipeline.
- **Fix:** Log unexpected content types at `warn` level.

### ERR-M3: Conversation Not Saved on Agent Error
- **File:** `crates/rustykrab-gateway/src/routes.rs:254-257`
- **Description:** `conv.save()` is only called on success. On failure, intermediate messages (tool calls, results) are lost. Retrying re-executes already-completed tool calls.
- **Fix:** Save conversation state on both success and failure.

### ERR-M4: SSE Event Channel Overflow Silently Drops Events
- **File:** `crates/rustykrab-gateway/src/routes.rs:210`
- **Description:** `try_send` on a 128-slot channel with `let _ =` discards events silently when the channel is full. Client sees gaps in streaming response.
- **Fix:** Use `.send().await` with timeout, increase buffer, or log drops.

### ERR-M5: Rate Limiter Memory Growth
- **File:** `crates/rustykrab-gateway/src/rate_limit.rs:88-94`
- **Description:** Stale entries only pruned at 10,000 IPs. Below that, memory leaks slowly. Pruning holds mutex during full iteration.
- **Fix:** Use periodic background cleanup or a more efficient data structure.

### ERR-M6: `truncate_for_classification` May Split UTF-8
- **File:** `crates/rustykrab-agent/src/router.rs:150-161`
- **Description:** `.unwrap_or(200)` fallback could reference a non-char-boundary byte position, causing panic.
- **Fix:** Use `.unwrap_or(0)`.

### ERR-M7: Telegram `split_message` Panics on Multi-byte UTF-8
- **File:** `crates/rustykrab-channels/src/telegram.rs:475-476`
- **Description:** `&remaining[..max_len]` slices at byte position 4096. Multi-byte UTF-8 characters cause panic.
- **Fix:** Use `char_indices()` or `floor_char_boundary()`.

### ERR-M8: Pipeline `check_completion` Truncation Splits Multi-byte Chars
- **File:** `crates/rustykrab-agent/src/orchestrator/pipeline.rs:227-229`
- **Description:** `&r.output[..500]` slices at byte 500 without checking character boundaries.
- **Fix:** Use `floor_char_boundary()`.

## LOW

### ERR-L1: No Logging for Empty Consistency Voter Responses
- **File:** `crates/rustykrab-agent/src/orchestrator/verifier.rs:96-99`
- **Description:** Non-text model responses silently skipped with no logging.

### ERR-L2: `delete_webhook` Response Status Not Checked
- **File:** `crates/rustykrab-channels/src/telegram.rs:329-337`
- **Description:** HTTP status never checked. Failed deletions (401) silently ignored.

### ERR-L3: `send_text` Error from Command Handler Silently Ignored
- **File:** `crates/rustykrab-channels/src/telegram.rs:366`
- **Description:** `let _ = self.send_text(chat_id, &reply).await;` discards errors.

### ERR-L4: Dead Code -- `parse_complexity` Function
- **File:** `crates/rustykrab-agent/src/router.rs:164-179`
- **Description:** Defined but never called from production code. Suggests incomplete transition from LLM to keyword-based classification.

### ERR-L5: Sandbox `ProcessSandbox` Does Not Enforce Policy
- **File:** `crates/rustykrab-agent/src/sandbox.rs:88-126`
- **Description:** Just returns `args` unchanged. Acknowledged in comments but still a gap.

### ERR-L6: Static Header Values Use `.parse().unwrap()`
- **File:** `crates/rustykrab-gateway/src/lib.rs:26-37`
- **Description:** Won't fail on static strings but fragile if strings are modified.

### ERR-L7: No HTTP Status Check on canvas.rs and tts.rs Responses
- **File:** `crates/rustykrab-tools/src/canvas.rs:75-91`, `crates/rustykrab-tools/src/tts.rs:94-106`
- **Description:** 4xx/5xx responses treated as success. Corrupt data written to disk.
