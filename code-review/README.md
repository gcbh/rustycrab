# Comprehensive Code Review -- rustycrab

**Date:** 2026-04-10
**Scope:** Full workspace (20,800+ lines across 10 crates)
**Method:** 10 parallel specialized review agents

## Summary

| # | Report | CRIT | HIGH | MED | LOW | Total |
|---|--------|------|------|-----|-----|-------|
| 1 | [Security](01-security.md) | 3 | 7 | 7 | 4 | **21** |
| 2 | [Error Handling](02-error-handling.md) | 2 | 8 | 8 | 7 | **25** |
| 3 | [Memory Crate](03-memory-crate.md) | 2 | 6 | 8 | 7 | **23** |
| 4 | [Tools Crate](04-tools-crate.md) | 2 | 3 | 8 | 5 | **18** |
| 5 | [Agent Orchestration](05-agent-orchestration.md) | 1 | 3 | 8 | 3 | **15** |
| 6 | [Skills/Store/Core](06-skills-store-core.md) | 3 | 5 | 8 | 6 | **22** |
| 7 | [Gateway & Channels](07-gateway-channels.md) | 2 | 4 | 7 | 5 | **18** |
| 8 | [Providers](08-providers.md) | 3 | 5 | 6 | 5 | **19** |
| 9 | [Concurrency & Async](09-concurrency-async.md) | 1 | 5 | 6 | 3 | **15** |
| 10 | [Dependencies & Build](10-dependencies-build.md) | 3 | 5 | 6 | 4 | **18** |
| | **Totals (with overlap)** | **22** | **51** | **72** | **49** | **194** |

After deduplication across agents (several issues like the `unsafe transmute`, sandbox no-op, and missing timeouts were found by 3-4 agents independently), the unique issue count is approximately **145**.

## Top 10 Most Critical Findings

1. **SEC-C1: Command allowlist bypass via `$()`/backtick shell substitution** -- `exec.rs` runs commands through `sh -c`, making the allowlist trivially bypassable
2. **SEC-C2: Unsandboxed arbitrary Python code execution** -- `code_execution.rs` runs Python with full system access
3. **SEC-C3 / ORCH-C1: Sandbox is a no-op** -- `ProcessSandbox::execute()` returns input unchanged; orchestrator hardcodes all-permissive policy
4. **SKL-C1: Skill verification never invoked** -- ed25519 signing infrastructure exists but `load_skills_from_dir` never calls it
5. **SEC-H7: Unsafe transmute causes potential use-after-free** -- `orchestrate.rs` transmutes a bounded lifetime to `'static` for a spawned task
6. **GW-C1: Webhooks accept unauthenticated payloads by default** -- no secret configured means any HTTP client can inject messages
7. **PROV-C1/C2: No retry logic or timeouts on LLM API calls** -- single transient failure kills the entire agent pipeline; hung connections block forever
8. **MEM-C1: UTF-8 chunking panics** -- byte-offset slicing crashes on any non-ASCII input
9. **MEM-C2: Race conditions in sled storage** -- non-atomic read-modify-write causes lost updates
10. **SKL-C3: Master key leaks from Zeroizing wrapper** -- `clone()` defeats secure memory erasure

## Cross-Cutting Themes

### Missing Timeouts (affects 6+ crates)
No `reqwest::Client` in the codebase has an explicit timeout. Anthropic, Ollama, Signal, Telegram, nodes, canvas, TTS, and image tools all create clients with `Client::new()`.

### UTF-8 Boundary Violations (affects 4+ locations)
Byte-level string slicing without char boundary checks: `chunking.rs`, `telegram.rs:split_message`, `pipeline.rs:check_completion`, `exec.rs:truncate_output`.

### Blocking I/O on Async Runtime (affects 5+ locations)
`std::fs`, `std::net`, `std::process::Command` called directly from async context: `security.rs` (DNS), `gmail.rs` (IMAP), `browser.rs` (Chrome launch), `storage.rs` (sled), `skill_create.rs`.

### No Sandbox Enforcement (systemic)
`ProcessSandbox` is a no-op. The orchestrator hardcodes all-permissive policies. `enforce_sandbox_policy` has overlapping match arms. The exec tool's allowlist is bypassed by shell substitution.
