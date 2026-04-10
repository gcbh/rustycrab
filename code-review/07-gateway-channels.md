# Gateway & Channels Review

## CRITICAL

### GW-C1: Webhooks Accept Unauthenticated Payloads by Default
- **File:** `crates/rustykrab-channels/src/telegram.rs:289-296`, `crates/rustykrab-channels/src/signal.rs:205-211`
- **Description:** Both `parse_webhook_update` and `parse_webhook_payload` only validate the secret header `if let Some(ref secret) = self.webhook_secret`. With no secret configured (the default), any HTTP client can POST arbitrary payloads as legitimate messages. Auth middleware explicitly skips webhook routes.
- **Fix:** Require webhook secrets as mandatory (non-`Option`) or reject all payloads when no secret is configured.

### GW-C2: Signal Message Source Lost During Routing
- **File:** `crates/rustykrab-channels/src/signal.rs:248-258`
- **Description:** The sender's phone number is extracted and validated but then discarded. The `Message` pushed to `inbound_tx` has no sender information. Replies cannot be routed back. Compare with Telegram which wraps in `ChannelMessage { chat_id, message }`.
- **Fix:** Add sender metadata to the inbound message type.

## HIGH

### GW-H1: Security Headers Don't Apply to Error Responses
- **File:** `crates/rustykrab-gateway/src/lib.rs:42-63`
- **Description:** `add_security_headers` is applied via `map_response` after `.with_state()`. Auth/origin/rate-limit middleware return early with `Err(StatusCode)` which converts to responses before reaching `map_response`. 401, 403, 429 responses lack security headers.
- **Fix:** Move security headers to the innermost layer.

### GW-H2: Monitor Task Leaked on Normal Agent Completion
- **File:** `crates/rustykrab-gateway/src/routes.rs:217-260`
- **Description:** Heartbeat monitor spawned with no abort on normal completion. Dropping `JoinHandle` detaches (doesn't cancel) the task. Leaked monitor runs for up to 5 minutes per streaming request.
- **Fix:** Call `monitor.abort()` after the `select!`.

### GW-H3: Origin Policy Rejects HTTPS and IPv6 Loopback
- **File:** `crates/rustykrab-gateway/src/origin.rs:29-34`
- **Description:** Only allows `http://127.0.0.1:*` and `http://localhost:*`. Rejects `https://` variants and `http://[::1]:*`. Breaks behind TLS-terminating proxies.
- **Fix:** Also allow HTTPS variants and IPv6 loopback.

### GW-H4: Rate Limiter Prunes Under Mutex During Request
- **File:** `crates/rustykrab-gateway/src/rate_limit.rs:88-94`
- **Description:** When >10,000 entries, `records.retain(...)` iterates the entire HashMap while holding the Mutex lock. Creates latency spikes under the exact conditions (DDoS) when low latency matters most.
- **Fix:** Prune asynchronously or in a background task.

## MEDIUM

### GW-M1: New Auth Token Printed to stdout
- **File:** `crates/rustykrab-gateway/src/routes.rs:52`
- **Description:** `println!("New RUSTYKRAB_AUTH_TOKEN={new_token}")` writes raw token to stdout. Captured by logging infrastructure in containerized deployments.
- **Fix:** Write to a file or return through secure side channel.

### GW-M2: No CORS Headers in Responses
- **File:** `crates/rustykrab-gateway/src/lib.rs:24-39`
- **Description:** Origin check validates but doesn't add `Access-Control-Allow-Origin`. Legitimate cross-origin requests blocked by browsers. No OPTIONS handler for preflight.
- **Fix:** Add CORS response headers for allowed origins.

### GW-M3: Conversation Not Persisted on Streaming Failure
- **File:** `crates/rustykrab-gateway/src/routes.rs:253-257`
- **Description:** Only saved on success. Failures lose intermediate tool call messages.

### GW-M4: `delete_conversation` Returns 500 for Not-Found
- **File:** `crates/rustykrab-gateway/src/routes.rs:90-100`
- **Description:** All errors mapped to `INTERNAL_SERVER_ERROR`. Should return 404 for missing conversations.

### GW-M5: Signal Webhook No Replay Protection
- **File:** `crates/rustykrab-channels/src/signal.rs:199-215`
- **Description:** No timestamp validation. Old messages can be replayed indefinitely.
- **Fix:** Add timestamp window check (reject messages older than 5 minutes).

### GW-M6: Telegram `ChannelMessage` Exported from Generic Crate
- **File:** `crates/rustykrab-channels/src/lib.rs:8`
- **Description:** Telegram-specific type (with `chat_id: i64`) re-exported from the generic channel crate. Leaks platform-specific concerns.

### GW-M7: Telegram `split_message` UTF-8 Panic
- **File:** `crates/rustykrab-channels/src/telegram.rs:460-483`
- **Description:** Byte-level slicing at `max_len=4096` panics on multi-byte characters.

## LOW

### GW-L1: Redundant Profile Resolution in `prepare_agent`
- **File:** `crates/rustykrab-gateway/src/orchestrate.rs:122-134`
- **Description:** Profile resolved twice -- once in `build_and_inject_system_prompt` and again for `to_agent_config()`.

### GW-L2: `backoff_delay` Starts at 2 Seconds
- **File:** `crates/rustykrab-channels/src/telegram.rs:452-455`
- **Description:** `2u64.pow(consecutive_errors.min(6))` starts at 2s, not standard 1s.

### GW-L3: Duplicated `constant_time_eq` Implementations
- **File:** `crates/rustykrab-gateway/src/auth.rs:66-77`, `telegram.rs:524-535`, `signal.rs:332-343`
- **Description:** Same function copy-pasted in 3 locations.
- **Fix:** Extract to `rustykrab-core` or use `subtle::ConstantTimeEq`.

### GW-L4: `MAX_MESSAGE_SIZE` Uses Bytes Not Characters
- **File:** `crates/rustykrab-gateway/src/routes.rs:108-110`
- **Description:** Documentation says "100 KB" but `len()` is bytes. Unclear for multi-byte content.

### GW-L5: `/reset` Command Uses Sentinel String Coupling
- **File:** `crates/rustykrab-channels/src/telegram.rs:412-415`
- **Description:** Returns `None` so `/reset` passes as raw text. Agent loop must know to look for this string -- invisible coupling.
