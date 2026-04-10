# Security Audit

## CRITICAL

### SEC-C1: Command Allowlist Bypass via Shell Substitution
- **File:** `crates/rustykrab-tools/src/exec.rs:62-113`
- **Description:** `validate_command` splits on `|`, `;`, `&&`, `||` but the command runs via `sh -c` (line 164). Shell features like `$()` and backticks completely bypass the allowlist. Example: `echo $(rm -rf /)` passes validation since `echo` is allowed, but the substitution executes arbitrary code.
- **Fix:** Don't use `sh -c`. Use `Command::new(program).args(args)` directly. Block `$()`, backticks, and process substitution patterns.

### SEC-C2: Unrestricted Arbitrary Code Execution
- **File:** `crates/rustykrab-tools/src/code_execution.rs:85-157`
- **Description:** `CodeExecutionTool` writes Python to a temp file and runs it with the system's `python3` with full filesystem/network/subprocess access. The `env_clear()` provides negligible security. Any prompt injection reaching tool execution can exfiltrate data or install malware.
- **Fix:** Execute inside a proper sandbox (Docker, nsjail, bubblewrap, or WASM). At minimum use seccomp-bpf or Linux namespaces.

### SEC-C3: ProcessSandbox is a No-Op
- **File:** `crates/rustykrab-agent/src/sandbox.rs:88-126`
- **Description:** `ProcessSandbox::execute` returns input `args` unchanged (line 114). The `SandboxPolicy` fields (`allow_fs_read`, `allow_fs_write`, `allow_net`, `allow_spawn`) are never inspected. Any tool executed through this sandbox has unrestricted access.
- **Fix:** Implement actual policy enforcement or remove the misleading abstraction.

## HIGH

### SEC-H1: Path Validation Incomplete -- Blocklist Too Narrow
- **File:** `crates/rustykrab-tools/src/security.rs:37-44`
- **Description:** Blocked paths are only `/etc/shadow`, `/etc/sudoers`, `/root/.ssh`, `/proc`, `/sys`, `/dev`. Missing: `/etc/passwd`, `~/.ssh/`, `~/.aws/`, `~/.kube/`, `~/.config/`, `~/.gnupg/`, `~/.bashrc`, `/var/run/docker.sock`, `/etc/cron.d/`.
- **Fix:** Use an allowlist approach. Constrain file operations to within `workspace_root()`.

### SEC-H2: `validate_path` Does Not Enforce Workspace Boundary
- **File:** `crates/rustykrab-tools/src/security.rs:26-90`
- **Description:** `workspace_root()` is defined (line 11) but never called from `validate_path`. Paths like `/home/user/.ssh/id_rsa` pass validation entirely.
- **Fix:** After canonicalization, verify the resolved path starts with `workspace_root()`.

### SEC-H3: SSRF Bypass via DNS Rebinding (TOCTOU)
- **File:** `crates/rustykrab-tools/src/security.rs:99-152`
- **Description:** `validate_url` resolves DNS and checks IPs, then the actual HTTP request does a second DNS resolution. Between the two calls, DNS rebinding can change the IP from public to `127.0.0.1` or `169.254.169.254`. Also, if DNS resolution fails (line 141: `if let Ok(addrs)`), the check is silently skipped.
- **Fix:** Pin the resolved IP for the actual connection. If DNS fails, block the request.

### SEC-H4: Browser Tool Arbitrary JavaScript Execution
- **File:** `crates/rustykrab-tools/src/browser.rs:551-568`
- **Description:** The `evaluate` action executes arbitrary JS in any page context with no restrictions. Combined with using the user's real Chrome profile, this enables session hijacking, credential theft, and keylogging.
- **Fix:** Use an isolated Chrome profile. Restrict JS evaluation to a limited API.

### SEC-H5: Gmail Attachment Path Traversal
- **File:** `crates/rustykrab-tools/src/gmail.rs:799-814`
- **Description:** Attachment filename from MIME headers is used directly in `download_dir.join(&filename)`. A filename like `../../../.bashrc` writes outside the download directory.
- **Fix:** Strip path separators and `..` components. Use UUID-based filenames.

### SEC-H6: Browser CSS Selector Injection to XSS
- **File:** `crates/rustykrab-tools/src/browser.rs:635-645`
- **Description:** User-provided `selector` and `value` are interpolated into JS strings with minimal escaping (only `'` and `"`). Backslashes, newlines, and null bytes are not escaped, allowing JS injection.
- **Fix:** Use proper JS string escaping or parameter passing instead of string interpolation.

### SEC-H7: Unsafe `transmute` to Extend Lifetime -- Use-After-Free
- **File:** `crates/rustykrab-gateway/src/orchestrate.rs:258-263`
- **Description:** `unsafe { std::mem::transmute(heartbeat_event) }` casts a bounded reference to `'static` for a spawned tokio task. If `run_agent_streaming` returns early (timeout/cancellation) before `keepalive.abort()` runs, the task holds a dangling reference. This is undefined behavior.
- **Fix:** Use `Arc<dyn Fn(AgentEvent) + Send + Sync>` or a channel-based approach.

## MEDIUM

### SEC-M1: Origin Check Bypassed by Omitting Origin Header
- **File:** `crates/rustykrab-gateway/src/origin.rs:49-63`
- **Description:** Requests without an `Origin` header are allowed through. Any non-browser tool can bypass origin protection.
- **Fix:** Require Origin header for sensitive endpoints or use additional auth.

### SEC-M2: Rate Limiter Bypass via Proxy
- **File:** `crates/rustykrab-gateway/src/rate_limit.rs:101-120`
- **Description:** Uses `ConnectInfo<SocketAddr>` for client IP. Behind a reverse proxy, all requests appear from the proxy's IP. No `X-Forwarded-For` support.
- **Fix:** Extract real client IP from trusted proxy headers when deployed behind a proxy.

### SEC-M3: CSP Allows `unsafe-inline` for Scripts
- **File:** `crates/rustykrab-gateway/src/lib.rs:34`
- **Description:** `script-src 'unsafe-inline'` defeats CSP XSS protection.
- **Fix:** Use nonce-based or hash-based CSP.

### SEC-M4: Exec Allowlist Contains Dangerous Commands
- **File:** `crates/rustykrab-tools/src/exec.rs:12-25`
- **Description:** Allowlist includes `ssh`, `scp`, `docker`, `kubectl`, `kill`, `chmod`, `chown`, `curl`, `wget`, `python3`, `node`, `ruby` -- enabling network pivoting, container escape, and arbitrary code execution.
- **Fix:** Significantly reduce allowlist. Move dangerous operations behind explicit user approval.

### SEC-M5: `thread_rng()` Used for Cryptographic Key Generation
- **File:** `crates/rustykrab-gateway/src/auth.rs:83`, `crates/rustykrab-store/src/secret.rs:108-109`
- **Description:** While `thread_rng()` in rand 0.8 delegates to a CSPRNG, using `OsRng` explicitly makes security intent clear and avoids regression risk.
- **Fix:** Use `rand::rngs::OsRng` for all cryptographic operations.

### SEC-M6: Exec Tool Leaks `HOME` Environment Variable
- **File:** `crates/rustykrab-tools/src/exec.rs:167-171`
- **Description:** After `env_clear()`, `HOME` is re-added from the host, enabling access to `~/.ssh/`, `~/.aws/credentials`, etc.
- **Fix:** Set HOME to a sandboxed directory like `/tmp/rustykrab-home`.

### SEC-M7: Process Tool Can Kill Arbitrary PIDs
- **File:** `crates/rustykrab-tools/src/process.rs:143-172`
- **Description:** The stop action accepts any PID > 1 with no ownership validation. An agent could kill any system process.
- **Fix:** Track spawned PIDs and only allow killing owned processes.

## LOW

### SEC-L1: Output Truncation at Non-UTF8 Boundary
- **File:** `crates/rustykrab-tools/src/exec.rs:46-54`
- **Description:** `s[..MAX_OUTPUT_BYTES]` slices without checking UTF-8 char boundaries. Panics on multi-byte characters.
- **Fix:** Use `floor_char_boundary()`.

### SEC-L2: IPv6 Private Address Check Incomplete
- **File:** `crates/rustykrab-tools/src/security.rs:166-174`
- **Description:** Missing checks for unique local (fc00::/7) and link-local (fe80::/10) IPv6 addresses.
- **Fix:** Add manual checks for these ranges.

### SEC-L3: Cookie Values Partially Exposed
- **File:** `crates/rustykrab-tools/src/browser.rs:296-302`
- **Description:** `mask_cookie_value` shows first 8 characters, which may leak predictable session token prefixes.
- **Fix:** Show fewer characters or only return cookie names/domains.

### SEC-L4: IMAP Search Query Injection
- **File:** `crates/rustykrab-tools/src/gmail.rs:126-131`
- **Description:** Minimal escaping on IMAP `X-GM-RAW` queries. Should also strip CRLF sequences.
- **Fix:** Validate query does not contain `\r\n`.
