# Agent Orchestration Review

## CRITICAL

### ORCH-C1: Sub-task Executor Bypasses Capability Checks
- **File:** `crates/rustykrab-agent/src/orchestrator/executor.rs:271-277`
- **Description:** `execute_tool_for_subtask` hardcodes a fully permissive `SandboxPolicy` (`allow_net: true, allow_fs_read: true, allow_fs_write: true, allow_spawn: true`). No `capabilities.can_use_tool()` check exists. Any tool invoked through the orchestration pipeline bypasses session capability restrictions.
- **Fix:** Thread the session's `CapabilitySet` through to `ParallelExecutor` and derive policy from it.

## HIGH

### ORCH-H1: Token Accounting Only Captures Last Round
- **File:** `crates/rustykrab-agent/src/orchestrator/executor.rs:198, 233-239`
- **Description:** The `tokens` variable is overwritten each iteration. For a sub-task making 5 tool rounds, only the last round's tokens are counted.
- **Fix:** Accumulate: `total_tokens += response.usage.prompt_tokens + response.usage.completion_tokens`.

### ORCH-H2: Recursive Sub-calls Drop Parent Context
- **File:** `crates/rustykrab-agent/src/rlm/recursive_call.rs:186-189`
- **Description:** Child sub-calls always receive `context: None`, losing agent identity, system instructions, and permission context.
- **Fix:** Pass `context.clone()` to child calls.

### ORCH-H3: Sub-call Marker Replacement is Fragile
- **File:** `crates/rustykrab-agent/src/rlm/recursive_call.rs:234-239`
- **Description:** Reconstructed marker may not match original text due to whitespace trimming. Results silently discarded by `remove_unresolved_markers`.
- **Fix:** Store original marker text from parsing and use it for replacement.

## MEDIUM

### ORCH-M1: Context Budget Ignores Parent Budget
- **File:** `crates/rustykrab-agent/src/rlm/context_manager.rs:22-33`
- **Description:** `child_budget` computes from `config.sub_task_context_budget`, not `parent_budget`. At depth 0, child gets the full config budget (no reduction).
- **Fix:** Use `parent_budget` as the base: `(parent_budget as f64 * 0.75) as usize`.

### ORCH-M2: Completion Check False Negative on "NOT COMPLETE"
- **File:** `crates/rustykrab-agent/src/orchestrator/pipeline.rs:270-271`
- **Description:** `text.contains("COMPLETE") && !text.contains("INCOMPLETE")` -- "NOT COMPLETE" matches as complete.
- **Fix:** Check for "NOT COMPLETE" and "INCOMPLETE" first.

### ORCH-M3: Consistency Voter Uses Naive Word-Overlap
- **File:** `crates/rustykrab-agent/src/orchestrator/verifier.rs:179-194`
- **Description:** 30% one-directional word-overlap threshold. Common stop words easily push overlap above 30%. Feeds into critical pipeline decision (confidence >= 0.8).
- **Fix:** Use bidirectional Jaccard similarity with higher threshold.

### ORCH-M4: No Timeout on Pipeline Model Calls
- **File:** `crates/rustykrab-agent/src/orchestrator/pipeline.rs` (all), `decomposer.rs:104`, `refiner.rs:116,154`, `synthesizer.rs:93`, `verifier.rs:85,170`
- **Description:** None of the orchestration LLM calls have timeouts. A stalled provider blocks the entire pipeline forever.
- **Fix:** Wrap in `tokio::time::timeout()`. Add total pipeline timeout.

### ORCH-M5: Router Uses Only Keyword Heuristics (LLM Unused)
- **File:** `crates/rustykrab-agent/src/router.rs:80-82`
- **Description:** `classify_complexity` ignores the classifier model. "Delete all production data" (4 words) classified as Trivial. `COMPLEXITY_PROMPT` and `parse_complexity` are dead code.
- **Fix:** Use LLM classifier with keyword heuristics as fast-path fallback.

### ORCH-M6: Synthesizer Skips Synthesis When Failures Exist
- **File:** `crates/rustykrab-agent/src/orchestrator/synthesizer.rs:53-55`
- **Description:** If 3 sub-tasks run and 2 fail, returns the single successful result with no indication of incompleteness.
- **Fix:** Only short-circuit when `successful.len() == 1 && results.len() == 1`.

### ORCH-M7: Executor Silently Drops Panicked Tasks (Potential Infinite Loop)
- **File:** `crates/rustykrab-agent/src/orchestrator/executor.rs:101-110, 59-112`
- **Description:** Panicked tasks never added to `completed`. Dependent tasks can't proceed but the panicked task may be re-selected and re-spawned, creating an infinite loop.
- **Fix:** Insert failure `SubTaskResult` and mark task as `completed`.

### ORCH-M8: No Total Pipeline Timeout or Cancellation
- **File:** `crates/rustykrab-agent/src/orchestrator/pipeline.rs`
- **Description:** Once started, the pipeline runs to completion or error. A `run_critical` chain can make dozens of sequential LLM calls with no overall timeout.
- **Fix:** Add `tokio::time::timeout` wrapper and accept `CancellationToken`.

## LOW

### ORCH-L1: `enforce_sandbox_policy` Has Overlapping Match Arms
- **File:** `crates/rustykrab-agent/src/runner.rs:979-1028`
- **Description:** `image_generate` needs both `allow_fs_write` and `allow_net`, but only the first matching arm's check runs.
- **Fix:** Check all required capabilities per tool.

### ORCH-L2: Refinement Critique Fallback is "APPROVED"
- **File:** `crates/rustykrab-agent/src/orchestrator/refiner.rs:122`
- **Description:** Non-text critique responses default to "APPROVED", prematurely approving without review.

### ORCH-L3: Dead Code -- `COMPLEXITY_PROMPT` and `parse_complexity`
- **File:** `crates/rustykrab-agent/src/router.rs:41-51, 164-179`
- **Description:** Defined but never called. Suggests incomplete transition.
