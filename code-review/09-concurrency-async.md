# Concurrency & Async Patterns Review

## CRITICAL

### ASYNC-C1: Blocking IMAP Connection in Async `action_setup`
- **File:** `crates/rustykrab-tools/src/gmail.rs:87-89`
- **Description:** `action_setup` is `async fn` but calls `self.connect_imap()` directly, performing blocking TCP/TLS handshake via `std::net::TcpStream` and `native_tls`. Blocks the entire tokio worker thread. Other actions in the same file correctly use `spawn_blocking`.
- **Fix:** Wrap in `tokio::task::spawn_blocking`.

## HIGH

### ASYNC-H1: Blocking DNS Resolution in SSRF Validator
- **File:** `crates/rustykrab-tools/src/security.rs:141`
- **Description:** `std::net::ToSocketAddrs::to_socket_addrs()` blocks the tokio worker thread. Called from all HTTP tool paths.
- **Fix:** Use `tokio::net::lookup_host` or `spawn_blocking`.

### ASYNC-H2: `std::sync::Mutex` in `HttpSessionTool`
- **File:** `crates/rustykrab-tools/src/http_session.rs:31, 119`
- **Description:** `self.sessions.lock().unwrap()` with `std::sync::Mutex` from async context. `.unwrap()` panics on poison.
- **Fix:** Switch to `tokio::sync::Mutex` or handle poison case.

### ASYNC-H3: Blocking Filesystem Operations in BrowserTool
- **File:** `crates/rustykrab-tools/src/browser.rs:131, 167, 183, 203`
- **Description:** `launch_chrome()` performs `std::fs` operations and `std::process::Command::spawn()` on the async executor.
- **Fix:** Wrap in `spawn_blocking` or use `tokio::fs` / `tokio::process::Command`.

### ASYNC-H4: Blocking `which` Command in Async Context
- **File:** `crates/rustykrab-tools/src/pdf.rs:222-237`
- **Description:** `std::process::Command::new("which")` called per-request from async context.
- **Fix:** Cache the result or use `tokio::process::Command`.

### ASYNC-H5: Dropped JoinHandles for Critical Tasks
- **Files:** `crates/rustykrab-cli/src/main.rs:196,252,275,346`, `crates/rustykrab-gateway/src/routes.rs:191`, `crates/rustykrab-memory/src/retrieval.rs:140`, `crates/rustykrab-memory/src/writer.rs:157`
- **Description:** Multiple `tokio::spawn` calls with dropped `JoinHandle`s. Panics silently swallowed. The agent streaming task (routes.rs:191) is fire-and-forget -- if it panics, client gets hung connection. Polling tasks (main.rs) silently stop on panic.
- **Fix:** Store handles for long-lived tasks. Add `catch_unwind` for fire-and-forget work.

## MEDIUM

### ASYNC-M1: Unbounded Task Spawning in Orchestration
- **Files:** `crates/rustykrab-agent/src/runner.rs:577`, `rlm/recursive_call.rs:187`, `orchestrator/executor.rs:95`, `orchestrator/verifier.rs:68`
- **Description:** All spawn concurrent tasks without semaphore. No global concurrency limiter. Pathological workloads spawn dozens of concurrent LLM calls.
- **Fix:** Add `tokio::sync::Semaphore` for concurrent LLM/tool calls.

### ASYNC-M2: SSE `try_send` Silently Drops Events
- **File:** `crates/rustykrab-gateway/src/routes.rs:210`
- **Description:** `try_send` on bounded channel (128) with `let _ =`. Slow clients see gaps.

### ASYNC-M3: `std::sync::RwLock` for Auth Token
- **File:** `crates/rustykrab-gateway/src/state.rs:18`, `auth.rs:48`
- **Description:** Currently safe (guard dropped before await), but fragile. `rotate_token()` blocks on write.
- **Fix:** Consider `tokio::sync::RwLock` for future-proofing.

### ASYNC-M4: `std::sync::Mutex` in ExecutionTracer
- **File:** `crates/rustykrab-agent/src/trace.rs:53`
- **Description:** Short critical sections acceptable now, but `summary_for_prompt` holds lock during iteration and formatting.

### ASYNC-M5: Rate Limiter `std::sync::Mutex` with Inline Pruning
- **File:** `crates/rustykrab-gateway/src/rate_limit.rs:43`
- **Description:** Pruning iterates entire HashMap under lock. Briefly blocks runtime under heavy load.
- **Fix:** Shard with `dashmap` or limit pruning work per call.

### ASYNC-M6: Blocking `std::fs` in Skill Create Tool
- **File:** `crates/rustykrab-tools/src/skill_create.rs:158, 162`
- **Description:** `std::fs::create_dir_all` and `std::fs::write` from async `execute`.
- **Fix:** Use `tokio::fs`.

## LOW

### ASYNC-L1: No Backpressure on Telegram Inbound Channel
- **File:** `crates/rustykrab-channels/src/telegram.rs:55`
- **Description:** 256-slot channel. If full, send blocks polling task, causing Telegram to re-deliver.

### ASYNC-L2: Process Spawned Without Tracking
- **File:** `crates/rustykrab-tools/src/process.rs:118-128`
- **Description:** Child handle dropped immediately. Process becomes orphan.

### ASYNC-L3: `WebChatChannel::receive` Panics
- **File:** `crates/rustykrab-channels/src/webchat.rs:48`
- **Description:** `unimplemented!()` is a latent runtime panic.
