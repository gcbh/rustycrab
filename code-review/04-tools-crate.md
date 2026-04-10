# Tools Crate Review (Non-Security)

## CRITICAL

### TOOL-C1: Zombie Process Leak in ProcessTool
- **File:** `crates/rustykrab-tools/src/process.rs:118-134`
- **Description:** The `child` from `tokio::process::Command::spawn()` is dropped without being awaited or stored. The process becomes orphaned/zombied. No tracking structure exists. The `stop` action uses shell `kill` which doesn't reap the child.
- **Fix:** Maintain a `HashMap<u32, Child>` to hold handles and properly manage lifecycle.

### TOOL-C2: Recursion Depth Guard is Client-Controlled and Bypassable
- **File:** `crates/rustykrab-tools/src/sessions_spawn.rs:57-63`, `crates/rustykrab-tools/src/subagents.rs:63-69`
- **Description:** Both tools read `_depth` from `args` JSON (untrusted input). A caller can omit or set to 0, defeating fork-bomb protection entirely.
- **Fix:** Track depth server-side in `SessionManager` state.

## HIGH

### TOOL-H1: No Size Limit on Image Download or TTS Response
- **File:** `crates/rustykrab-tools/src/image.rs:67-75`, `crates/rustykrab-tools/src/tts.rs:99-102`
- **Description:** `resp.bytes().await` reads entire response into memory with no size cap. A malicious URL could serve gigabytes, causing OOM.
- **Fix:** Use streaming with a size limit (e.g., 50MB).

### TOOL-H2: SSRF DNS Resolution Blocks Async Runtime
- **File:** `crates/rustykrab-tools/src/security.rs:141-149`
- **Description:** `std::net::ToSocketAddrs::to_socket_addrs()` is blocking DNS resolution in async context. Can cause latency spikes or deadlocks under load.
- **Fix:** Use `tokio::net::lookup_host()` or `spawn_blocking`.

### TOOL-H3: Process Command Allowlist Bypass via Path
- **File:** `crates/rustykrab-tools/src/process.rs:44-53`
- **Description:** Validation extracts command name via `rsplit('/')` then checks allowlist, but the full path is passed to `Command::new`. A binary at `/tmp/python` passes the check.
- **Fix:** Reject paths entirely or resolve to verify it's the expected binary.

## MEDIUM

### TOOL-M1: `process.rs` "list" Leaks All System Processes
- **File:** `crates/rustykrab-tools/src/process.rs:174-205`
- **Description:** Runs `ps aux` returning ALL processes, not just tool-started ones. Exposes PIDs, command-line args (may contain secrets).
- **Fix:** Only list tracked processes.

### TOOL-M2: No HTTP Request Timeout in Multiple Tools
- **File:** `crates/rustykrab-tools/src/nodes.rs:12-16`, `canvas.rs`, `tts.rs`, `image.rs`
- **Description:** `reqwest::Client::new()` with default settings. No explicit timeout. Calls can hang indefinitely.
- **Fix:** Set explicit timeouts on all clients.

### TOOL-M3: `env_clear()` in process.rs Breaks Most Commands
- **File:** `crates/rustykrab-tools/src/process.rs:123-126`
- **Description:** Only sets `PATH`, `HOME`, `LANG`. Commands like docker, kubectl, npm need `DOCKER_HOST`, `KUBECONFIG`, `CARGO_HOME` etc.
- **Fix:** Preserve essential environment variables or document the limitation.

### TOOL-M4: Image Tool Encodes Base64 Then Discards It
- **File:** `crates/rustykrab-tools/src/image.rs:86-93`
- **Description:** Reads image, base64-encodes (large allocation), but only returns `b64_len` and "Image loaded successfully". The `prompt` parameter is also ignored.
- **Fix:** Either return the base64 data or skip the encoding.

### TOOL-M5: Gateway Config Ambiguous Key/Value Semantics
- **File:** `crates/rustykrab-tools/src/gateway.rs:86-114`
- **Description:** `key=None, value=Some(...)` silently discards the value with no error.
- **Fix:** Return an error for contradictory states.

### TOOL-M6: `sanitize.rs` Collects All Chars into Vec
- **File:** `crates/rustykrab-tools/src/sanitize.rs:21`
- **Description:** `html.chars().collect::<Vec<char>>()` roughly quadruples memory for large HTML (4 bytes per char).
- **Fix:** Iterate over chars directly with peekable.

### TOOL-M7: `sanitize.rs` extract_href Byte Indexing After Lowercase
- **File:** `crates/rustykrab-tools/src/sanitize.rs:177-204`
- **Description:** Searches `lower` for position, indexes into original `tag_content`. Multi-byte UTF-8 before `href=` causes wrong slice or panic.
- **Fix:** Search in original string case-insensitively.

### TOOL-M8: CGNAT Range Check Operator Precedence
- **File:** `crates/rustykrab-tools/src/security.rs:164`
- **Description:** `&&` vs `||` precedence is correct but fragile without parentheses.
- **Fix:** Add explicit parentheses.

## LOW

### TOOL-L1: Magic String "current" as Default Session ID
- **File:** `crates/rustykrab-tools/src/session_status.rs:51`
- **Description:** `unwrap_or("current")` passes a magic value that `SessionManager` must handle specially.

### TOOL-L2: `collapse_whitespace` Doesn't Reset on Newlines
- **File:** `crates/rustykrab-tools/src/sanitize.rs:252-254`
- **Description:** Space before newline is preserved when it should be collapsed.

### TOOL-L3: Inconsistent Error Handling Between Tool Files
- **Description:** Some tools wrap errors with `Error::ToolExecution`, others pass through backend errors directly.

### TOOL-L4: Process Allowlist vs Shell Metachar Block Inconsistency
- **File:** `crates/rustykrab-tools/src/process.rs:7-12, 35-42`
- **Description:** Blocks shell metacharacters AND doesn't allow shell, but many allowed commands need complex args.

### TOOL-L5: Unnecessary `secrets.clone()` in lib.rs
- **File:** `crates/rustykrab-tools/src/lib.rs:187`
- **Description:** Could be micro-optimized by reordering to avoid one clone.
