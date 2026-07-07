# Realtime Token Usage Panel Implementation Plan

> **For agentic workers (advisory):** If available, superpowers:subagent-driven-development or superpowers:executing-plans is the recommended way to execute this plan task-by-task. Workers without those skills implement the tasks in order, following the repo workflow gates in `Agents.md`. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a session-scoped **Tokens** card to the realtime diagnostics page: cumulative token counts per model and token class (fresh text/audio input, cached input, text/audio output), rendered as one stacked bar per model, sharing the bottom row with the Phase Timeline.

**Architecture:** Token usage already exists server-side per call — the realtime model's `response.done` usage is extracted in `sideband_response_done.rs`, and the live-goal-formation judge's `ModelResponse` carries `ModelUsage` (currently discarded). A new `TokenUsageSnapshot` accumulator on `SessionRuntime` aggregates per *(role, model)* token-class counters; each recorded call publishes the full snapshot through a `watch` channel, exactly like the existing sideband-status / turn-context / volition-inspection pushes, and the events socket forwards it to the browser as a `kind: "token_usage"` message. The UI follows the page's strict reducer → selector → render shape: a parse function, one new action and state field, a pure `selectTokenUsagePanelModel` view-model, and a dumb render function. The persisted `ExchangeModelUse` schema is untouched — the audio/text split lives only in the diagnostics accumulator.

**Tech Stack:** Rust (axum, tokio watch channels, serde) in `crates/qsf_realtime_server/` and `crates/qsf_models/`; TypeScript (Vite, Vitest, Biome) in `crates/qsf_realtime_server/ui/`.

**Spec:** Design approved conversationally 2026-07-07 (three panel treatments and three page placements reviewed as browser mockups; "model rows with stacked bars" and "split the bottom row" chosen). No `Design.*.md` — this plan is the authoritative description.

**Design decisions resolved during brainstorming (surfaced per repo rule):**
- **Tokens, not dollars.** Raw counts per class answer "where do the tokens go" without a hand-maintained price table. A dollar view can be layered on later.
- **Session scope only.** Only model calls flowing through `qsf_realtime_server` during the current session; batch/sleep work in `qsf_app` is out of scope.
- **Audio/text split for the realtime model.** Audio tokens dominate realtime pricing (~10× text), so `input_token_details` / `output_token_details` are read; text-only models fall back to text classes.
- **Cached input is one class** (no cached-audio/cached-text split in the display) — five legend entries is the comfortable maximum.
- **Server aggregates, browser renders.** Full-snapshot push per completed call; the UI is stateless w.r.t. accumulation, so a reconnecting browser heals itself.
- **Stale/cancelled responses still count.** The provider billed them; the meter records usage before the stale early-return. Call counts therefore mean "provider responses", not "accepted turns". The same "provider spend" policy covers goal-formation calls whose responses later fail structured-output parsing or validation: usage is captured at the `ModelInvoker` seam, so billed spend survives post-response failures (review finding).
- **Row identity is *(role, model)*** — `realtime_voice` and `goal_formation` today — so a future call site is one `record_token_usage(...)` line away from appearing on the page.

**Documents to update (per `docs/ProjectFrame/ProjectWorkflow.md`):** routine diagnostics engineering with durable scope decisions → **no Experiment doc** (no simulation-mechanism question); one `docs/DecisionLog.md` entry (Task 4.1). No architecture doc describes the browser diagnostics card (same as `Plan.RealtimeWideLayoutAndLanePause.md`). `docs/Handoff.md` untouched. No trace contract — this slice makes no trace-based behavioral claims; its observability *is* the new panel plus unit-tested aggregation. This plan is ephemeral; durable docs and code must not cite its phase numbers.

## Global Constraints

- Rust gates after each Rust task: `cargo test -p <touched crate>`; on task completion `cargo build`, and at plan completion `cargo clippy --all-targets -- -D warnings` then `cargo fmt` from the repo root.
- UI gates: run all npm commands from `crates/qsf_realtime_server/ui/`. After each UI task `npm run test`; on task completion also `npm run check` (tsc + Biome) and `npm run fmt`. When launching npm through `Start-Process`, use `npm.cmd` explicitly.
- Reducers and selectors stay pure — no clock reads, no DOM access in `realtime.ts`.
- Wire format is snake_case (serde default); TypeScript properties are camelCase; the parse function does the mapping (same convention as `parseTurnContextMessage`).
- Existing `data-role` names are untouched; this plan adds exactly one: `token-usage-body`.
- UI testing policy: reducers, parse functions, and view-models only; no DOM-structure or render tests. Markup/CSS tasks gate on `npm run check` plus human verification.
- View-model numbers used in tests are written as the *same arithmetic expression* the selector computes (e.g. `(100 * 100) / 1_000`) so `toEqual` stays bit-exact.
- TDD for all logic tasks: write the failing test, watch it fail, implement, watch it pass, commit. One commit per task.

## File Structure

**Created:**
- `crates/qsf_realtime_server/src/realtime/token_usage.rs` — token-class counters, per-session snapshot/accumulator, and `response.done` usage extraction. One responsibility: turn provider usage payloads into the session's token ledger.

**Modified:**
- `crates/qsf_realtime_server/src/realtime/mod.rs` — declare the new module.
- `crates/qsf_realtime_server/src/state.rs` — `SessionRuntime` gains the snapshot field, watch sender, `record_token_usage`, `subscribe_token_usage`; tests for the watch contract.
- `crates/qsf_realtime_server/src/realtime/sideband_response_done.rs` — usage-number helper moves to `token_usage.rs`; realtime feed point before the stale early-return.
- `crates/qsf_realtime_server/src/realtime/sideband_tests.rs` — new feed-point test; goal-formation usage assertion appended to an existing test.
- `crates/qsf_realtime_server/src/realtime/live_goal_formation.rs` — goal-formation feed point (records captured usage before outcome parsing can fail) + billed-failure regression test.
- `crates/qsf_models/src/model_client.rs` — `UsageCapturingInvoker` / `CapturedModelUse`, a `ModelInvoker` that preserves usage across post-response failures.
- `crates/qsf_models/src/live_goal_formation.rs` — tests only: usage capture survives a validation failure. `LiveGoalFormationOutcome` and both judges unchanged.
- `crates/qsf_models/src/lib.rs` — export `UsageCapturingInvoker` and `CapturedModelUse` (next to the existing `DirectModelInvoker` export).
- `crates/qsf_realtime_server/src/realtime/routes.rs` — events socket subscribes to and pushes `token_usage` messages.
- `crates/qsf_realtime_server/ui/src/realtime.ts` — snapshot types, `parseTokenUsageMessage`, `token_usage_captured` action + reducer case + state field, `selectTokenUsagePanelModel`, `formatTokenCount`.
- `crates/qsf_realtime_server/ui/src/realtime.test.ts` — parse/reducer/view-model tests.
- `crates/qsf_realtime_server/ui/src/main.ts` — bottom-strip markup, socket hook, `renderTokenUsagePanel`, new ref.
- `crates/qsf_realtime_server/ui/src/styles.css` — bottom-strip grid, token panel styles, segment colors.
- `docs/DecisionLog.md` — one entry (Task 4.1).

---

## Phase 1 — Server-side capture and aggregation

Everything verifiable by `cargo test -p qsf_realtime_server` / `-p qsf_models`. No behavior visible in the browser yet.

### Task 1.1: Token ledger module with `response.done` extraction

**Files:**
- Create: `crates/qsf_realtime_server/src/realtime/token_usage.rs`
- Modify: `crates/qsf_realtime_server/src/realtime/mod.rs`

**Interfaces:**
- Consumes: `serde_json::Value` provider events (`response.done` shape with `response.usage`).
- Produces (later tasks rely on these exact names):
  - `pub struct TokenClassCounts { pub text_input: u64, pub audio_input: u64, pub cached_input: u64, pub text_output: u64, pub audio_output: u64 }` (`Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize`)
  - `pub struct ModelTokenUsage { pub model_id: String, pub role: String, pub calls: u32, pub counts: TokenClassCounts }`
  - `pub struct TokenUsageSnapshot { pub qsf_session_id: String, pub models: Vec<ModelTokenUsage> }` with `new(qsf_session_id)` and `record(&mut self, role, model_id, counts)`
  - `pub(crate) fn response_done_token_counts(event: &serde_json::Value) -> TokenClassCounts`
  - `pub(crate) fn usage_number(event: &serde_json::Value, path: &[&str]) -> Option<u64>`
- Extraction contract: `text_input + audio_input + cached_input <= input_tokens` for every provider-consistent payload — the full cached count is always subtracted from the fresh classes, never double-reported (review finding: a payload with `cached_tokens` but no `cached_tokens_details` must not leave cached tokens inside `text_input`).

- [ ] **Step 1: Write the module with failing tests**

Create `crates/qsf_realtime_server/src/realtime/token_usage.rs`:

```rust
//! Session-scoped token ledger for the diagnostics page. Aggregates provider-reported
//! token usage per (role, model) into class counters (fresh text/audio input, cached
//! input, text/audio output). Raw counts only — no price table, no dollar conversion
//! (see the DecisionLog entry on the diagnostics token meter).

use serde::{Deserialize, Serialize};

/// Token counts split by the classes the diagnostics panel displays. "Fresh" input
/// excludes cached tokens; `cached_input` is the full cached prefix (audio + text).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenClassCounts {
    pub text_input: u64,
    pub audio_input: u64,
    pub cached_input: u64,
    pub text_output: u64,
    pub audio_output: u64,
}

impl TokenClassCounts {
    pub fn add_assign_saturating(&mut self, other: TokenClassCounts) {
        self.text_input = self.text_input.saturating_add(other.text_input);
        self.audio_input = self.audio_input.saturating_add(other.audio_input);
        self.cached_input = self.cached_input.saturating_add(other.cached_input);
        self.text_output = self.text_output.saturating_add(other.text_output);
        self.audio_output = self.audio_output.saturating_add(other.audio_output);
    }
}

/// Accumulated usage of one (role, model) pair. `calls` counts provider responses,
/// including stale/cancelled ones — the provider billed them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelTokenUsage {
    pub model_id: String,
    pub role: String,
    pub calls: u32,
    pub counts: TokenClassCounts,
}

/// The full session ledger, pushed to the browser as one snapshot per recorded call.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenUsageSnapshot {
    pub qsf_session_id: String,
    pub models: Vec<ModelTokenUsage>,
}

impl TokenUsageSnapshot {
    pub fn new(qsf_session_id: String) -> Self {
        Self {
            qsf_session_id,
            models: Vec::new(),
        }
    }

    /// Accumulate one completed model call. Rows are keyed by (role, model_id) and keep
    /// first-seen order so the browser's row identity is stable across updates.
    pub fn record(&mut self, role: &str, model_id: &str, counts: TokenClassCounts) {
        if let Some(row) = self
            .models
            .iter_mut()
            .find(|row| row.role == role && row.model_id == model_id)
        {
            row.calls = row.calls.saturating_add(1);
            row.counts.add_assign_saturating(counts);
            return;
        }
        self.models.push(ModelTokenUsage {
            model_id: model_id.to_string(),
            role: role.to_string(),
            calls: 1,
            counts,
        });
    }
}

/// Numeric field under a `response.done` event's `response.usage`, or `None` when any
/// path segment is missing (provider payload shapes vary by model and API version).
pub(crate) fn usage_number(event: &serde_json::Value, path: &[&str]) -> Option<u64> {
    let mut current = event.get("response")?.get("usage")?;
    for segment in path {
        current = current.get(segment)?;
    }
    current.as_u64()
}

/// Token classes of one `response.done` event. Detail blocks are optional: with no
/// `input_token_details`, fresh input falls back to `input - cached` counted as text;
/// with no `output_token_details`, all output counts as text. The full cached count is
/// always subtracted from the fresh input classes — the reported cached text/audio
/// split first, any unattributed remainder text-first — so displayed input classes
/// never sum past the provider total (`text_input + audio_input + cached_input <=
/// input_tokens`).
pub(crate) fn response_done_token_counts(event: &serde_json::Value) -> TokenClassCounts {
    let input = usage_number(event, &["input_tokens"]).unwrap_or(0);
    let cached = usage_number(event, &["input_token_details", "cached_tokens"])
        .or_else(|| usage_number(event, &["cached_input_tokens"]))
        .unwrap_or(0);
    let output = usage_number(event, &["output_tokens"]).unwrap_or(0);

    let input_text = usage_number(event, &["input_token_details", "text_tokens"]);
    let input_audio = usage_number(event, &["input_token_details", "audio_tokens"]);
    let cached_text = usage_number(
        event,
        &["input_token_details", "cached_tokens_details", "text_tokens"],
    )
    .unwrap_or(0);
    let cached_audio = usage_number(
        event,
        &["input_token_details", "cached_tokens_details", "audio_tokens"],
    )
    .unwrap_or(0);
    let output_text = usage_number(event, &["output_token_details", "text_tokens"]);
    let output_audio = usage_number(event, &["output_token_details", "audio_tokens"]);

    let (text_input, audio_input) = match (input_text, input_audio) {
        (None, None) => (input.saturating_sub(cached), 0),
        (text, audio) => {
            // Subtract the full cached prefix from the fresh classes: the reported
            // cached text/audio split first, then any unattributed remainder (a
            // payload with `cached_tokens` but absent or partial
            // `cached_tokens_details`) text-first, spilling into audio. This keeps
            // the extraction contract: input classes never sum past `input_tokens`.
            let mut fresh_text = text.unwrap_or(0).saturating_sub(cached_text);
            let mut fresh_audio = audio.unwrap_or(0).saturating_sub(cached_audio);
            let mut remainder = cached.saturating_sub(cached_text.saturating_add(cached_audio));
            let from_text = remainder.min(fresh_text);
            fresh_text -= from_text;
            remainder -= from_text;
            fresh_audio = fresh_audio.saturating_sub(remainder);
            (fresh_text, fresh_audio)
        }
    };
    let (text_output, audio_output) = match (output_text, output_audio) {
        (None, None) => (output, 0),
        (text, audio) => (text.unwrap_or(0), audio.unwrap_or(0)),
    };

    TokenClassCounts {
        text_input,
        audio_input,
        cached_input: cached,
        text_output,
        audio_output,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_usage_splits_audio_text_and_cached() {
        let event = serde_json::json!({
            "response": {
                "usage": {
                    "total_tokens": 1000,
                    "input_tokens": 900,
                    "output_tokens": 100,
                    "input_token_details": {
                        "text_tokens": 300,
                        "audio_tokens": 600,
                        "cached_tokens": 500,
                        "cached_tokens_details": { "text_tokens": 200, "audio_tokens": 300 }
                    },
                    "output_token_details": { "text_tokens": 20, "audio_tokens": 80 }
                }
            }
        });
        assert_eq!(
            response_done_token_counts(&event),
            TokenClassCounts {
                text_input: 100,
                audio_input: 300,
                cached_input: 500,
                text_output: 20,
                audio_output: 80,
            }
        );
    }

    #[test]
    fn missing_details_fall_back_to_text_classes() {
        // Partial detail: text split present, no audio field, no cached detail block.
        // The detail arm runs — the full cached count (3) is subtracted from fresh
        // text since no cached split attributes it, audio defaults to 0 — while
        // output falls back to plain text.
        let partial = serde_json::json!({
            "response": {
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 4,
                    "input_token_details": { "text_tokens": 8, "cached_tokens": 3 }
                }
            }
        });
        assert_eq!(
            response_done_token_counts(&partial),
            TokenClassCounts {
                text_input: 5,
                audio_input: 0,
                cached_input: 3,
                text_output: 4,
                audio_output: 0,
            }
        );

        // No detail blocks at all (text-only model shape): fresh input falls back to
        // `input - cached`, counted as text.
        let bare = serde_json::json!({
            "response": { "usage": { "input_tokens": 10, "output_tokens": 4, "cached_input_tokens": 3 } }
        });
        assert_eq!(
            response_done_token_counts(&bare),
            TokenClassCounts {
                text_input: 7,
                audio_input: 0,
                cached_input: 3,
                text_output: 4,
                audio_output: 0,
            }
        );

        assert_eq!(
            response_done_token_counts(&serde_json::json!({})),
            TokenClassCounts::default()
        );
    }

    /// Regression for the double-count hazard: with `cached_tokens` reported but no
    /// `cached_tokens_details`, the cached prefix must still come out of the fresh
    /// classes so that `text_input + audio_input + cached_input <= input_tokens`.
    #[test]
    fn cached_without_cached_details_never_double_counts_input() {
        // Cached (5) exceeds fresh text (3 after nothing subtracted → 3): text-first
        // subtraction empties text and spills the remaining 2 into audio.
        let spilling = serde_json::json!({
            "response": {
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 1,
                    "input_token_details": { "text_tokens": 3, "audio_tokens": 7, "cached_tokens": 5 }
                }
            }
        });
        let counts = response_done_token_counts(&spilling);
        assert_eq!(
            counts,
            TokenClassCounts {
                text_input: 0,
                audio_input: 5,
                cached_input: 5,
                text_output: 1,
                audio_output: 0,
            }
        );
        assert!(counts.text_input + counts.audio_input + counts.cached_input <= 10);
    }

    #[test]
    fn record_accumulates_per_role_model_and_keeps_first_seen_order() {
        let mut snapshot = TokenUsageSnapshot::new("session-test".to_string());
        let counts = TokenClassCounts {
            text_input: 10,
            audio_input: 20,
            cached_input: 5,
            text_output: 3,
            audio_output: 7,
        };
        snapshot.record("realtime_voice", "gpt-realtime-2", counts);
        snapshot.record("goal_formation", "gpt-5-mini", counts);
        snapshot.record(
            "realtime_voice",
            "gpt-realtime-2",
            TokenClassCounts {
                text_input: 1,
                ..TokenClassCounts::default()
            },
        );

        assert_eq!(snapshot.models.len(), 2);
        assert_eq!(snapshot.models[0].role, "realtime_voice");
        assert_eq!(snapshot.models[0].calls, 2);
        assert_eq!(snapshot.models[0].counts.text_input, 11);
        assert_eq!(snapshot.models[0].counts.audio_input, 20);
        assert_eq!(snapshot.models[1].role, "goal_formation");
        assert_eq!(snapshot.models[1].calls, 1);
    }
}
```

- [ ] **Step 2: Declare the module**

In `crates/qsf_realtime_server/src/realtime/mod.rs`, add alongside the existing declarations (alphabetical placement, after `pub(crate) mod sideband;`-group ends — concretely between `mod sideband_turn_injection;` and `pub(crate) mod tools;`):

```rust
pub(crate) mod token_usage;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p qsf_realtime_server token_usage`
Expected: PASS (4 tests). The module is not yet referenced elsewhere; `pub(crate)` items may raise dead-code warnings under clippy until Tasks 1.2–1.3 wire them — defer the clippy gate to task completion order or add nothing; `cargo test` itself passes.

- [ ] **Step 4: Commit**

```bash
git add crates/qsf_realtime_server/src/realtime/token_usage.rs crates/qsf_realtime_server/src/realtime/mod.rs
git commit -m "realtime server: session token ledger with response.done class extraction"
```

### Task 1.2: `SessionRuntime` accumulator with watch-channel publication

**Files:**
- Modify: `crates/qsf_realtime_server/src/state.rs`

**Interfaces:**
- Consumes: `TokenClassCounts`, `TokenUsageSnapshot` (Task 1.1).
- Produces (later tasks rely on these exact names):
  - field `pub token_usage: TokenUsageSnapshot` on `SessionRuntime`
  - `pub fn record_token_usage(&mut self, role: &str, model_id: &str, counts: TokenClassCounts)`
  - `pub fn subscribe_token_usage(&self) -> watch::Receiver<Option<TokenUsageSnapshot>>`

- [ ] **Step 1: Write the failing test**

Append inside the `mod tests` block of `crates/qsf_realtime_server/src/state.rs` (mirroring `turn_context_watch_holds_value_for_late_subscriber`):

```rust
    #[test]
    fn token_usage_watch_holds_snapshot_for_late_subscriber() {
        use crate::realtime::token_usage::TokenClassCounts;

        let tempdir = tempfile::tempdir().expect("tempdir");
        let diagnostics =
            DiagnosticWriter::create(tempdir.path().join("test.jsonl")).expect("diagnostics");
        let mut runtime = SessionRuntime::new(
            "test-session".to_string(),
            BrowserSessionConfig::default(),
            diagnostics,
        );

        runtime.record_token_usage(
            "realtime_voice",
            "gpt-realtime-2",
            TokenClassCounts {
                text_input: 10,
                audio_input: 20,
                cached_input: 5,
                text_output: 3,
                audio_output: 7,
            },
        );
        runtime.record_token_usage(
            "realtime_voice",
            "gpt-realtime-2",
            TokenClassCounts {
                text_input: 1,
                ..TokenClassCounts::default()
            },
        );

        // A late-joining subscriber must immediately see the accumulated snapshot.
        let rx = runtime.subscribe_token_usage();
        let snapshot = rx
            .borrow()
            .clone()
            .expect("late subscriber must see stored snapshot");
        assert_eq!(snapshot.qsf_session_id, "test-session");
        assert_eq!(snapshot.models.len(), 1);
        assert_eq!(snapshot.models[0].calls, 2);
        assert_eq!(snapshot.models[0].counts.text_input, 11);
        assert_eq!(snapshot.models[0].counts.audio_output, 7);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p qsf_realtime_server token_usage_watch`
Expected: FAIL to compile — `record_token_usage` / `subscribe_token_usage` do not exist.

- [ ] **Step 3: Implement**

In `crates/qsf_realtime_server/src/state.rs`:

(a) Add the import next to the other `crate::realtime::` imports at the top:

```rust
use crate::realtime::token_usage::{TokenClassCounts, TokenUsageSnapshot};
```

(b) In `struct SessionRuntime`, after `pub(crate) live_goal_formation_queue: VecDeque<PendingLiveGoalFormation>,` add:

```rust
    /// Session token ledger for the diagnostics Tokens panel. Mutated only through
    /// `record_token_usage`, which also publishes the snapshot to `token_usage_tx`.
    pub token_usage: TokenUsageSnapshot,
```

and after `volition_inspection_tx: watch::Sender<Option<VolitionInspectionCapture>>,` add:

```rust
    token_usage_tx: watch::Sender<Option<TokenUsageSnapshot>>,
```

(c) In `SessionRuntime::new`, the constructor builds `session_state` from `qsf_session_id.clone()` on its first line; add the ledger before `Self { … }` and the two fields to the literal:

```rust
        let token_usage = TokenUsageSnapshot::new(qsf_session_id.clone());
```

```rust
            token_usage,
            token_usage_tx: watch::channel(None).0,
```

(`token_usage,` goes after `live_goal_formation_queue: VecDeque::new(),`; `token_usage_tx` after `volition_inspection_tx: watch::channel(None).0,`.)

(d) After `volition_inspection_sender`, add the accessor pair:

```rust
    /// Subscribe to token-usage snapshots. A late-joining subscriber immediately
    /// observes the most recent snapshot stored in the channel (watch channel
    /// guarantee), so a browser that reconnects mid-session shows correct totals.
    pub fn subscribe_token_usage(&self) -> watch::Receiver<Option<TokenUsageSnapshot>> {
        self.token_usage_tx.subscribe()
    }

    /// Record one completed model call in the session token ledger and publish the
    /// updated snapshot to any events-socket subscribers.
    pub fn record_token_usage(&mut self, role: &str, model_id: &str, counts: TokenClassCounts) {
        self.token_usage.record(role, model_id, counts);
        self.token_usage_tx.send_replace(Some(self.token_usage.clone()));
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p qsf_realtime_server token_usage`
Expected: PASS (Task 1.1's 4 tests + this one).

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/src/state.rs
git commit -m "realtime server: session runtime accumulates and publishes token usage"
```

### Task 1.3: Realtime feed point in the `response.done` handler

**Files:**
- Modify: `crates/qsf_realtime_server/src/realtime/sideband_response_done.rs`
- Test: `crates/qsf_realtime_server/src/realtime/sideband_tests.rs`

**Interfaces:**
- Consumes: `response_done_token_counts`, `usage_number` (Task 1.1); `record_token_usage` (Task 1.2).
- Produces: every provider `response.done` — stale or not — lands in `SessionRuntime::token_usage` under role `"realtime_voice"` and the session's configured model id.

- [ ] **Step 1: Write the failing tests**

Append to `crates/qsf_realtime_server/src/realtime/sideband_tests.rs` (same harness as `empty_store_turn_records_empty_context_and_promotes`). Two tests: the trusted-response path, and a stale-response regression that pins the feed point *above* the stale early-return — counting stale/cancelled responses is an explicit design decision, and without this test a future edit could move the feed below the guard with no test failing:

```rust
#[tokio::test]
async fn response_done_accumulates_realtime_token_usage() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    start_test_turn(&state, &allocation.qsf_session_id, &mut runtime_state, &outbound_tx).await;
    drain_outbound_texts(&mut outbound_rx);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-test",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-done",
            "response": {
                "id": "response-usage",
                "status": "completed",
                "output": [{
                    "content": [{ "type": "output_text", "text": "hi" }]
                }],
                "usage": {
                    "input_tokens": 900,
                    "output_tokens": 100,
                    "input_token_details": {
                        "text_tokens": 300,
                        "audio_tokens": 600,
                        "cached_tokens": 500,
                        "cached_tokens_details": { "text_tokens": 200, "audio_tokens": 300 }
                    },
                    "output_token_details": { "text_tokens": 20, "audio_tokens": 80 }
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("response done");

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    let row = guard
        .token_usage
        .models
        .iter()
        .find(|row| row.role == "realtime_voice")
        .expect("realtime response must be recorded in the token ledger");
    assert_eq!(row.model_id, guard.config.model);
    assert_eq!(row.calls, 1);
    assert_eq!(row.counts.text_input, 100);
    assert_eq!(row.counts.audio_input, 300);
    assert_eq!(row.counts.cached_input, 500);
    assert_eq!(row.counts.text_output, 20);
    assert_eq!(row.counts.audio_output, 80);
}

#[tokio::test]
async fn stale_response_done_records_token_usage_without_promoting() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    start_test_turn(&state, &allocation.qsf_session_id, &mut runtime_state, &outbound_tx).await;
    drain_outbound_texts(&mut outbound_rx);

    // Mark the response id stale before its response.done arrives, as a barge-in
    // cancellation does.
    runtime_state.stale_response_ids.insert("response-stale".to_string());

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-test",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-done-stale",
            "response": {
                "id": "response-stale",
                "status": "cancelled",
                "output": [],
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 2,
                    "input_token_details": {
                        "text_tokens": 10,
                        "cached_tokens": 4,
                        "cached_tokens_details": { "text_tokens": 4, "audio_tokens": 0 }
                    }
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("stale response done");

    // The stale early-return still ran: the event was diagnosed as stale and no
    // trusted exchange was promoted to continuity storage.
    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    assert!(
        records
            .iter()
            .any(|record| matches!(record, DiagnosticRecord::StaleProviderEvent { .. })),
        "the stale path must be the one exercised"
    );
    let continuity_dir = state.continuity_session_dir(&allocation.qsf_session_id);
    assert!(
        !continuity_dir.join("session-state.json").exists(),
        "a stale response must not promote an exchange"
    );

    // ...but the provider billed the call, so the ledger recorded it anyway.
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    let row = guard
        .token_usage
        .models
        .iter()
        .find(|row| row.role == "realtime_voice")
        .expect("stale response must still be recorded in the token ledger");
    assert_eq!(row.calls, 1);
    assert_eq!(row.counts.text_input, 6);
    assert_eq!(row.counts.cached_input, 4);
    assert_eq!(row.counts.text_output, 2);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p qsf_realtime_server token_usage`
Expected: both new tests FAIL — "…must be recorded in the token ledger" (nothing feeds the ledger yet); Task 1.1/1.2 tests still pass.

- [ ] **Step 3: Implement the feed point and DRY the helpers**

In `crates/qsf_realtime_server/src/realtime/sideband_response_done.rs`:

(a) Add the import next to the other `crate::realtime::` imports:

```rust
use crate::realtime::token_usage::{response_done_token_counts, usage_number};
```

(b) Delete the local `fn response_usage_number` (lines defining it) and change the three small extractors to call the moved helper — same bodies, `response_usage_number` → `usage_number`:

```rust
fn response_usage_input_tokens(event: &serde_json::Value) -> u32 {
    usage_number(event, &["input_tokens"]).unwrap_or(0) as u32
}

fn response_usage_cached_input_tokens(event: &serde_json::Value) -> u32 {
    usage_number(event, &["input_token_details", "cached_tokens"])
        .or_else(|| usage_number(event, &["cached_input_tokens"]))
        .unwrap_or(0) as u32
}

fn response_usage_output_tokens(event: &serde_json::Value) -> u32 {
    usage_number(event, &["output_tokens"]).unwrap_or(0) as u32
}
```

(c) In `handle_response_done_event`, immediately after the `let exchange_is_stale = …;` statement and **before** the `if response_is_stale || exchange_is_stale {` block, add:

```rust
    // Real provider spend regardless of staleness: a cancelled or superseded response
    // still consumed tokens, so the diagnostics ledger counts it before any stale
    // early-return below.
    let realtime_model_id = guard.config.model.clone();
    guard.record_token_usage(
        "realtime_voice",
        &realtime_model_id,
        response_done_token_counts(event),
    );
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p qsf_realtime_server`
Expected: PASS, including the pre-existing `response_usage_extractors_tolerate_missing_fields` (its subject functions now delegate to `usage_number`, same semantics) and all sideband/tool-loop tests (they drive `response.done` events whose usage now also lands in the ledger — no assertions there conflict).

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/src/realtime/sideband_response_done.rs crates/qsf_realtime_server/src/realtime/sideband_tests.rs
git commit -m "realtime server: feed realtime response usage into the session token ledger"
```

### Task 1.4: Goal-formation usage capture and feed point

**Files:**
- Modify: `crates/qsf_models/src/model_client.rs`
- Modify: `crates/qsf_models/src/lib.rs`
- Modify: `crates/qsf_realtime_server/src/realtime/live_goal_formation.rs` (feed point + tests module)
- Test: `crates/qsf_models/src/live_goal_formation.rs` (tests module), `crates/qsf_realtime_server/src/realtime/sideband_tests.rs`

**Interfaces:**
- Consumes: `ModelResponse { model_name, usage, .. }`, `ModelInvoker`, `invoke_model` (existing `qsf_models` types); `record_token_usage` (Task 1.2).
- Produces:
  - `pub struct CapturedModelUse { pub model_name: String, pub usage: ModelUsage }` (`Clone, Debug, PartialEq`) and `pub struct UsageCapturingInvoker { pub captured: Vec<CapturedModelUse> }` (`Default`, implements `ModelInvoker`), both exported from `qsf_models`
  - the realtime server records role `"goal_formation"` in the session ledger for every formation call the provider answered with usage — **including calls that fail after the response returns** (missing structured output, malformed JSON, duplicate candidate id, invalid contradictions). Billed spend is never dropped with the error.
  - `LiveGoalFormationOutcome` and both judges are untouched.

**Design note (review finding):** `form_and_detect` has four failure points *after* `invoker.invoke(...)` returns, and each such call is billed. Carrying usage on the success-only outcome would silently drop that spend. Capturing usage inside the invoker — the seam every model call already passes through — preserves the "provider spend" policy (the same one that counts stale realtime responses) without splitting the judge API or threading usage through error types, and any future invoker-based call site inherits the capability for free.

- [ ] **Step 1: Write the failing test (qsf_models)**

Append to the tests module of `crates/qsf_models/src/live_goal_formation.rs` (add `UsageCapturingInvoker` next to the existing `DirectModelInvoker` import):

```rust
    #[test]
    fn usage_capturing_invoker_preserves_usage_when_validation_fails_after_the_call() {
        // Fixture text that is not JSON: the provider "answered" (and billed the
        // call) but form_and_detect fails at structured-output parsing.
        let client = MockModelClient::default().with_fixture(
            crate::ModelRoleId::LiveGoalFormationJudge,
            "not json at all".to_string(),
        );
        let judge = ModelBackedLiveGoalFormationJudge::new(&client);
        let mut invoker = UsageCapturingInvoker::default();

        let result = judge.form_and_detect(&mut invoker, &[goal("goal-a")], "a turn");

        assert!(result.is_err());
        assert_eq!(
            invoker.captured.len(),
            1,
            "the billed call must be captured despite the post-response failure"
        );
        assert!(invoker.captured[0].usage.input_tokens > 0);
        assert!(!invoker.captured[0].model_name.is_empty());
    }
```

(Adjust the `with_fixture` argument path/signature to what `mock_model.rs` actually exposes if it differs.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p qsf_models usage_capturing`
Expected: FAIL to compile — `UsageCapturingInvoker` does not exist.

- [ ] **Step 3: Implement in qsf_models**

(a) In `crates/qsf_models/src/model_client.rs`, below the `DirectModelInvoker` impl:

```rust
/// One model call observed by `UsageCapturingInvoker`: which model answered and what
/// it consumed, per the provider's usage report.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedModelUse {
    pub model_name: String,
    pub usage: ModelUsage,
}

/// A `ModelInvoker` that calls the client like `DirectModelInvoker` and additionally
/// captures the usage of every response the provider returned. Callers whose work can
/// still fail *after* the provider billed the call (structured-output parsing, semantic
/// validation) read `captured` afterwards, so provider spend is never lost with the
/// error.
#[derive(Default)]
pub struct UsageCapturingInvoker {
    pub captured: Vec<CapturedModelUse>,
}

impl ModelInvoker for UsageCapturingInvoker {
    fn invoke(
        &mut self,
        client: &dyn ModelClient,
        request: &ModelRequest,
    ) -> anyhow::Result<ModelResponse> {
        let response = invoke_model(client, request)?;
        if let Some(usage) = &response.usage {
            self.captured.push(CapturedModelUse {
                model_name: response.model_name.clone(),
                usage: usage.clone(),
            });
        }
        Ok(response)
    }
}
```

(b) In `crates/qsf_models/src/lib.rs`, add `CapturedModelUse` and `UsageCapturingInvoker` to the existing `pub use model_client::{ … }` list (next to `DirectModelInvoker`).

- [ ] **Step 4: Run qsf_models tests**

Run: `cargo test -p qsf_models`
Expected: PASS, including the Step-1 test (the `MockModelClient` reports usage on every response).

- [ ] **Step 5: Write the failing feed-point tests (realtime server)**

(a) In `crates/qsf_realtime_server/src/realtime/sideband_tests.rs`, append to the existing `completed_trusted_turn_spawns_live_goal_formation` test, after the `performed` poll and its `assert!(matches!(…))`:

```rust
    // Formation usage is recorded as soon as the model call returns, before the
    // LiveGoalFormationPerformed diagnostic is written, so once that record is
    // observable the ledger row must exist.
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    let formation_row = guard
        .token_usage
        .models
        .iter()
        .find(|row| row.role == "goal_formation")
        .expect("goal formation call must be recorded in the token ledger");
    assert_eq!(formation_row.calls, 1);
    assert!(formation_row.counts.text_input + formation_row.counts.cached_input > 0);
```

(b) In the tests module of `crates/qsf_realtime_server/src/realtime/live_goal_formation.rs` (same harness as `a_failed_formation_call_writes_a_failure_diagnostic_and_leaves_state_untouched`), add the billed-failure regression the review asked for:

```rust
    #[tokio::test]
    async fn a_billed_call_that_fails_validation_still_lands_in_the_token_ledger() {
        let tempdir = TempDir::new().unwrap();
        let app_state = state(&tempdir);
        let allocation = app_state.create_session().await.unwrap();
        let session = app_state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .unwrap();

        // The provider answers (and bills the call) with output that fails
        // structured-output parsing, so run_live_goal_formation returns an error.
        let build_client = || -> anyhow::Result<Arc<dyn ModelClient>> {
            Ok(Arc::new(qsf_models::MockModelClient::default().with_fixture(
                qsf_models::ModelRoleId::LiveGoalFormationJudge,
                "not json at all".to_string(),
            )))
        };

        let result = run_live_goal_formation(
            session.clone(),
            &allocation.qsf_session_id,
            0,
            "a turn transcript".to_string(),
            None,
            build_client,
        )
        .await;
        assert!(result.is_err());

        let guard = session.lock().await;
        let row = guard
            .token_usage
            .models
            .iter()
            .find(|row| row.role == "goal_formation")
            .expect("a billed formation call must be recorded despite the failure");
        assert_eq!(row.calls, 1);
        assert!(row.counts.text_input + row.counts.cached_input > 0);
    }
```

Run: `cargo test -p qsf_realtime_server goal_formation`
Expected: both FAIL — "…must be recorded…" (nothing feeds the ledger yet).

- [ ] **Step 6: Implement the feed point**

In `crates/qsf_realtime_server/src/realtime/live_goal_formation.rs`, inside `run_live_goal_formation`, replace the `spawn_blocking` block (which currently returns `anyhow::Result<LiveGoalFormationOutcome>` via `??`) so the captured usage survives the error path:

```rust
    let (captured_model_use, outcome_result) = tokio::task::spawn_blocking(move || {
        let mut invoker = qsf_models::UsageCapturingInvoker::default();
        let result = (|| -> anyhow::Result<qsf_models::LiveGoalFormationOutcome> {
            let client = build_client()?;
            let judge = ModelBackedLiveGoalFormationJudge::new(client.as_ref());
            judge.form_and_detect(&mut invoker, &goal_set, &turn_transcript)
        })();
        (invoker.captured, result)
    })
    .await
    .map_err(|join_error| anyhow::anyhow!("live goal formation task panicked: {join_error}"))?;
    let formation_completed_at = OffsetDateTime::now_utc();

    // Provider spend is recorded before the outcome is even inspected: a call that
    // returned usage counts in the diagnostics ledger even when its response fails
    // structured-output parsing or validation below, and even when the outcome is
    // later discarded as stale — same policy as stale realtime responses.
    if !captured_model_use.is_empty() {
        let mut guard = session.lock().await;
        for model_use in &captured_model_use {
            guard.record_token_usage(
                "goal_formation",
                &model_use.model_name,
                crate::realtime::token_usage::TokenClassCounts {
                    text_input: u64::from(
                        model_use
                            .usage
                            .input_tokens
                            .saturating_sub(model_use.usage.cached_input_tokens),
                    ),
                    audio_input: 0,
                    cached_input: u64::from(model_use.usage.cached_input_tokens),
                    text_output: u64::from(model_use.usage.output_tokens),
                    audio_output: 0,
                },
            );
        }
    }
    let outcome = outcome_result?;
```

(The `qsf_models::DirectModelInvoker` reference in this function disappears; the rest of the function is unchanged and re-locks the session for outcome processing as before.)

- [ ] **Step 7: Run the tests, then the phase gates**

Run: `cargo test -p qsf_realtime_server` and `cargo test -p qsf_models` — Expected: PASS.
Run: `cargo build`, then `cargo clippy --all-targets -- -D warnings`, then `cargo fmt` — Expected: clean (all Task 1.1 items are referenced by now).

- [ ] **Step 8: Commit**

```bash
git add crates/qsf_models/src/model_client.rs crates/qsf_models/src/lib.rs crates/qsf_models/src/live_goal_formation.rs crates/qsf_realtime_server/src/realtime/live_goal_formation.rs crates/qsf_realtime_server/src/realtime/sideband_tests.rs
git commit -m "realtime server: record goal-formation model usage in the session token ledger"
```

---

## Phase 2 — Transport to the browser

### Task 2.1: Events socket pushes `token_usage` snapshots

**Files:**
- Modify: `crates/qsf_realtime_server/src/realtime/routes.rs`

**Interfaces:**
- Consumes: `subscribe_token_usage` (Task 1.2).
- Produces: server→browser events-socket messages of the shape (snake_case; Phase 3's parser consumes exactly this):

```json
{
  "kind": "token_usage",
  "qsf_session_id": "…",
  "models": [
    {
      "model_id": "gpt-realtime-2",
      "role": "realtime_voice",
      "calls": 3,
      "counts": { "text_input": 100, "audio_input": 300, "cached_input": 500, "text_output": 20, "audio_output": 80 }
    }
  ]
}
```

The watch contract is covered by Task 1.2's late-subscriber test and the select-loop fan-out is mechanical, but the browser parser (Task 3.1) depends on this exact flattened, snake_case wire shape — so `TokenUsageMessage` serialization gets a unit test pinning every field name `parseTokenUsageMessage` consumes (review finding: no existing routes test exercises socket-pushed message inventory, so a `#[serde(flatten)]` or naming mistake would otherwise only surface in manual browser verification). A websocket-level push test is not practical in the current routes test harness; the end-to-end path is human-verified in Task 3.3. All existing routes tests must keep passing.

- [ ] **Step 1: Write the failing wire-shape test**

Append to the existing `mod tests` at the bottom of `crates/qsf_realtime_server/src/realtime/routes.rs`:

```rust
    #[test]
    fn token_usage_message_serializes_the_wire_shape_the_browser_parses() {
        use crate::realtime::token_usage::{TokenClassCounts, TokenUsageSnapshot};

        let mut snapshot = TokenUsageSnapshot::new("session-1".to_string());
        snapshot.record(
            "realtime_voice",
            "gpt-realtime-2",
            TokenClassCounts {
                text_input: 100,
                audio_input: 300,
                cached_input: 500,
                text_output: 20,
                audio_output: 80,
            },
        );
        let message = TokenUsageMessage {
            kind: "token_usage",
            snapshot,
        };

        // parseTokenUsageMessage in the browser consumes exactly these snake_case
        // fields; the #[serde(flatten)] must keep the snapshot's fields top-level.
        assert_eq!(
            serde_json::to_value(&message).expect("serialize"),
            serde_json::json!({
                "kind": "token_usage",
                "qsf_session_id": "session-1",
                "models": [{
                    "model_id": "gpt-realtime-2",
                    "role": "realtime_voice",
                    "calls": 1,
                    "counts": {
                        "text_input": 100,
                        "audio_input": 300,
                        "cached_input": 500,
                        "text_output": 20,
                        "audio_output": 80
                    }
                }]
            })
        );
    }
```

Run: `cargo test -p qsf_realtime_server token_usage_message`
Expected: FAIL to compile — `TokenUsageMessage` does not exist yet.

- [ ] **Step 2: Extend the socket plumbing**

In `crates/qsf_realtime_server/src/realtime/routes.rs`, mirror the `VolitionInspectionCapture` handling in five places:

(a) Import the snapshot type (next to the other `crate::` imports):

```rust
use crate::realtime::token_usage::TokenUsageSnapshot;
```

(b) `subscribe_session` returns a 4-tuple — extend the signature and body:

```rust
async fn subscribe_session(
    state: &AppState,
    qsf_session_id: &str,
) -> Option<(
    watch::Receiver<SidebandStatus>,
    watch::Receiver<Option<TurnContextCapture>>,
    watch::Receiver<Option<VolitionInspectionCapture>>,
    watch::Receiver<Option<TokenUsageSnapshot>>,
)> {
    let session = state.session_runtime(qsf_session_id).await?;
    let guard = session.lock().await;
    Some((
        guard.subscribe_status(),
        guard.subscribe_turn_context(),
        guard.subscribe_volition_inspection(),
        guard.subscribe_token_usage(),
    ))
}
```

(c) In the socket handler, declare the receiver next to the others:

```rust
    let mut token_usage_rx: Option<watch::Receiver<Option<TokenUsageSnapshot>>> = None;
```

and at **both** `subscribe_session` call sites (the initial `session_hint` bind and the first-envelope rebind), destructure the 4-tuple `(srx, tcrx, virx, turx)` and add after the volition-inspection initial push:

```rust
                let initial_token_usage = turx.borrow().clone();
                if let Some(snapshot) = initial_token_usage {
                    push_token_usage(&mut socket, &snapshot).await;
                }
                token_usage_rx = Some(turx);
```

(d) Add the changed-future and select arm, mirroring `volition_inspection_changed`:

```rust
        let token_usage_changed = async {
            match token_usage_rx.as_mut() {
                Some(rx) => rx.changed().await,
                None => std::future::pending::<Result<(), watch::error::RecvError>>().await,
            }
        };
```

```rust
            tu_result = token_usage_changed => {
                match tu_result {
                    Ok(()) => {
                        let snapshot = token_usage_rx
                            .as_ref()
                            .expect("token usage receiver present when change observed")
                            .borrow()
                            .clone();
                        if let Some(snapshot) = snapshot {
                            push_token_usage(&mut socket, &snapshot).await;
                        }
                    }
                    // Sender dropped (session removed): stop watching, keep relaying.
                    Err(_) => token_usage_rx = None,
                }
            }
```

(e) Add the message type and push helper next to `push_volition_inspection`:

```rust
#[derive(Debug, Serialize)]
struct TokenUsageMessage {
    kind: &'static str,
    #[serde(flatten)]
    snapshot: TokenUsageSnapshot,
}

async fn push_token_usage(socket: &mut WebSocket, snapshot: &TokenUsageSnapshot) {
    let message = TokenUsageMessage {
        kind: "token_usage",
        snapshot: snapshot.clone(),
    };
    if let Ok(text) = serde_json::to_string(&message) {
        socket.send(Message::Text(text.into())).await.ok();
    }
}
```

(f) The rebind guard `if status_rx.is_none() { … }` covers all receivers as a set (unchanged behavior) — just make sure the destructuring inside it now names four receivers.

- [ ] **Step 3: Run the gates**

Run: `cargo test -p qsf_realtime_server` — Expected: PASS, including the Step-1 wire-shape test.
Run: `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt` — Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/qsf_realtime_server/src/realtime/routes.rs
git commit -m "realtime server: push token usage snapshots over the events socket"
```

---

## Phase 3 — Browser panel

### Task 3.1: Snapshot types, parser, action, reducer

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts`
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces:**
- Consumes: Task 2.1's wire format; existing `isRecord` helper in `realtime.ts`.
- Produces (later tasks rely on these exact names):
  - `export interface TokenClassCounts { textInput: number; audioInput: number; cachedInput: number; textOutput: number; audioOutput: number; }`
  - `export interface ModelTokenUsage { modelId: string; role: string; calls: number; counts: TokenClassCounts; }`
  - `export interface TokenUsageSnapshot { qsfSessionId: string; models: ModelTokenUsage[]; }`
  - `export function parseTokenUsageMessage(raw: string): TokenUsageSnapshot | null`
  - action `{ type: "token_usage_captured"; snapshot: TokenUsageSnapshot }`
  - state field `latestTokenUsage: TokenUsageSnapshot | null` (cleared on `session_allocated`, kept after stop for post-hoc review, session-guarded like the volition capture)

- [ ] **Step 1: Write the failing tests**

Append to `crates/qsf_realtime_server/ui/src/realtime.test.ts` (add `parseTokenUsageMessage` and `type TokenUsageSnapshot` to the file's imports from `./realtime`):

```ts
describe("token usage capture", () => {
  const snapshot: TokenUsageSnapshot = {
    qsfSessionId: "session-1",
    models: [
      {
        modelId: "gpt-realtime-2",
        role: "realtime_voice",
        calls: 3,
        counts: { textInput: 100, audioInput: 300, cachedInput: 500, textOutput: 20, audioOutput: 80 },
      },
    ],
  };

  it("parses a token_usage message", () => {
    const raw = JSON.stringify({
      kind: "token_usage",
      qsf_session_id: "session-1",
      models: [
        {
          model_id: "gpt-realtime-2",
          role: "realtime_voice",
          calls: 3,
          counts: { text_input: 100, audio_input: 300, cached_input: 500, text_output: 20, audio_output: 80 },
        },
      ],
    });
    expect(parseTokenUsageMessage(raw)).toEqual(snapshot);
  });

  it("returns null for other kinds, malformed models, and malformed counts", () => {
    expect(parseTokenUsageMessage("{not-json")).toBeNull();
    expect(parseTokenUsageMessage(JSON.stringify({ kind: "sideband_status" }))).toBeNull();
    expect(
      parseTokenUsageMessage(JSON.stringify({ kind: "token_usage", qsf_session_id: "s", models: "nope" })),
    ).toBeNull();
    expect(
      parseTokenUsageMessage(
        JSON.stringify({
          kind: "token_usage",
          qsf_session_id: "s",
          models: [{ model_id: "m", role: "r", calls: 1, counts: { text_input: "1" } }],
        }),
      ),
    ).toBeNull();
  });

  it("stores captures for the active session and ignores others", () => {
    const active = { ...INITIAL_STATE, sessionId: "session-1" };
    const captured = reduceConversationState(active, { type: "token_usage_captured", snapshot });
    expect(captured.latestTokenUsage).toEqual(snapshot);

    const mismatched = reduceConversationState(
      { ...INITIAL_STATE, sessionId: "other-session" },
      { type: "token_usage_captured", snapshot },
    );
    expect(mismatched.latestTokenUsage).toBeNull();
  });

  it("clears the snapshot when a new session is allocated and keeps it after stop", () => {
    const populated: ConversationState = {
      ...INITIAL_STATE,
      sessionId: "session-1",
      latestTokenUsage: snapshot,
    };
    const reallocated = reduceConversationState(populated, {
      type: "session_allocated",
      sessionId: "session-2",
    });
    expect(reallocated.latestTokenUsage).toBeNull();

    const stopped = reduceConversationState(populated, { type: "stopped", atMs: 1_000 });
    expect(stopped.latestTokenUsage).toEqual(snapshot);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run check` — Expected: FAIL (`parseTokenUsageMessage` / `TokenUsageSnapshot` not exported; `latestTokenUsage` not on state).
Run: `npm run test` — Expected: FAIL on the new describe block.

- [ ] **Step 3: Implement**

In `crates/qsf_realtime_server/ui/src/realtime.ts`:

(a) Types, next to `TurnContextCapture`:

```ts
/// Token counts split by the classes the Tokens panel displays. "Fresh" input
/// excludes cached tokens; cachedInput is the full cached prefix (audio + text).
export interface TokenClassCounts {
  textInput: number;
  audioInput: number;
  cachedInput: number;
  textOutput: number;
  audioOutput: number;
}

/// Accumulated usage of one (role, model) pair, as aggregated server-side.
export interface ModelTokenUsage {
  modelId: string;
  role: string;
  calls: number;
  counts: TokenClassCounts;
}

/// The session token ledger, pushed over the events socket with `kind: "token_usage"`.
/// The server sends the full snapshot on every recorded call, so latest-wins is the
/// only reducer logic needed.
export interface TokenUsageSnapshot {
  qsfSessionId: string;
  models: ModelTokenUsage[];
}
```

(b) Extend `ConversationState` with:

```ts
  latestTokenUsage: TokenUsageSnapshot | null;
```

`INITIAL_STATE` with `latestTokenUsage: null,`, and the `session_allocated` reducer case with `latestTokenUsage: null,` (next to `latestVolitionState: null,`).

(c) Add the action variant:

```ts
  | { type: "token_usage_captured"; snapshot: TokenUsageSnapshot };
```

and the reducer case (after `volition_state_captured`):

```ts
    case "token_usage_captured":
      // Ignore captures for a session other than the active one: a queued
      // message from a closed socket must not overwrite state after stop or
      // during a newly allocated session.
      if (action.snapshot.qsfSessionId !== state.sessionId) {
        return state;
      }
      return {
        ...state,
        latestTokenUsage: action.snapshot,
      };
```

(d) The parser, next to `parseVolitionStateMessage`:

```ts
function parseTokenClassCounts(value: unknown): TokenClassCounts | null {
  if (!isRecord(value)) {
    return null;
  }
  const { text_input, audio_input, cached_input, text_output, audio_output } = value;
  if (
    typeof text_input !== "number" ||
    typeof audio_input !== "number" ||
    typeof cached_input !== "number" ||
    typeof text_output !== "number" ||
    typeof audio_output !== "number"
  ) {
    return null;
  }
  return {
    textInput: text_input,
    audioInput: audio_input,
    cachedInput: cached_input,
    textOutput: text_output,
    audioOutput: audio_output,
  };
}

/// Parse a server→browser events-socket message, returning a token-usage snapshot
/// when the message has `kind: "token_usage"` and all required fields are present
/// and correctly typed. Returns `null` for any other message.
///
/// Wire format uses snake_case field names; this function maps them to camelCase
/// TypeScript properties.
export function parseTokenUsageMessage(raw: string): TokenUsageSnapshot | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isRecord(parsed) || parsed.kind !== "token_usage") {
    return null;
  }
  const qsfSessionId = parsed.qsf_session_id;
  const models = parsed.models;
  if (typeof qsfSessionId !== "string" || !Array.isArray(models)) {
    return null;
  }
  const parsedModels: ModelTokenUsage[] = [];
  for (const entry of models) {
    if (!isRecord(entry)) {
      return null;
    }
    const counts = parseTokenClassCounts(entry.counts);
    if (
      typeof entry.model_id !== "string" ||
      typeof entry.role !== "string" ||
      typeof entry.calls !== "number" ||
      counts === null
    ) {
      return null;
    }
    parsedModels.push({
      modelId: entry.model_id,
      role: entry.role,
      calls: entry.calls,
      counts,
    });
  }
  return { qsfSessionId, models: parsedModels };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test` — Expected: PASS.
Run: `npm run check` then `npm run fmt`.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/realtime.test.ts
git commit -m "realtime ui: parse and reduce token usage snapshots"
```

### Task 3.2: Tokens panel view-model

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts`
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces:**
- Consumes: `ConversationState.latestTokenUsage` (Task 3.1).
- Produces (Task 3.3 renders exactly these):
  - `export function formatTokenCount(tokens: number): string`
  - `export interface TokenUsageSegmentModel { className: string; label: string; tokens: number; exactLabel: string; widthPercent: number; }`
  - `export interface TokenUsageRowModel { name: string; totalLabel: string; barPercent: number; segments: TokenUsageSegmentModel[]; }`
  - `export interface TokenUsageLegendEntry { className: string; label: string; }`
  - `export interface TokenUsagePanelModel { kind: "empty" | "data"; heroLabel: string; heroDetail: string; legend: TokenUsageLegendEntry[]; rows: TokenUsageRowModel[]; }`
  - `export function selectTokenUsagePanelModel(state: ConversationState): TokenUsagePanelModel`
  - Class names emitted: `"audio-in" | "text-in" | "cached-in" | "audio-out" | "text-out"` (CSS suffixes in Task 3.3).

- [ ] **Step 1: Write the failing tests**

Append to `realtime.test.ts` (add `selectTokenUsagePanelModel` and `formatTokenCount` to the imports):

```ts
describe("selectTokenUsagePanelModel", () => {
  it("reports the empty state before any recorded call", () => {
    expect(selectTokenUsagePanelModel(INITIAL_STATE)).toEqual({
      kind: "empty",
      heroLabel: "0",
      heroDetail: "",
      legend: [],
      rows: [],
    });
  });

  it("orders rows by total, scales bars to the largest row, and drops zero classes", () => {
    // realtime row total: 100 + 300 + 500 + 20 + 80 = 1_000 tokens.
    // formation row total: 40 + 50 + 10 = 100 tokens. Grand total 1_100; 5 calls.
    const state: ConversationState = {
      ...INITIAL_STATE,
      sessionId: "session-1",
      latestTokenUsage: {
        qsfSessionId: "session-1",
        models: [
          {
            modelId: "gpt-5-mini",
            role: "goal_formation",
            calls: 2,
            counts: { textInput: 40, audioInput: 0, cachedInput: 50, textOutput: 10, audioOutput: 0 },
          },
          {
            modelId: "gpt-realtime-2",
            role: "realtime_voice",
            calls: 3,
            counts: { textInput: 100, audioInput: 300, cachedInput: 500, textOutput: 20, audioOutput: 80 },
          },
        ],
      },
    };

    const model = selectTokenUsagePanelModel(state);
    expect(model.kind).toBe("data");
    expect(model.heroLabel).toBe("1.1k");
    expect(model.heroDetail).toBe("session total · 5 model calls");
    expect(model.rows.map((row) => row.name)).toEqual([
      "gpt-realtime-2 · voice",
      "gpt-5-mini · goal formation",
    ]);
    expect(model.rows[0].totalLabel).toBe("1.0k");
    expect(model.rows[0].barPercent).toBe(100);
    expect(model.rows[1].barPercent).toBe((100 * 100) / 1_000);
    // Segment order follows the fixed class order; zero classes are dropped.
    expect(model.rows[0].segments.map((segment) => [segment.className, segment.widthPercent])).toEqual([
      ["audio-in", (300 * 100) / 1_000],
      ["text-in", (100 * 100) / 1_000],
      ["cached-in", (500 * 100) / 1_000],
      ["audio-out", (80 * 100) / 1_000],
      ["text-out", (20 * 100) / 1_000],
    ]);
    expect(model.rows[1].segments.map((segment) => segment.className)).toEqual([
      "text-in",
      "cached-in",
      "text-out",
    ]);
    expect(model.rows[0].segments[0].exactLabel).toBe("audio in — 300 tokens");
    // Legend lists every class that is nonzero in at least one row, in fixed order.
    expect(model.legend.map((entry) => entry.className)).toEqual([
      "audio-in",
      "text-in",
      "cached-in",
      "audio-out",
      "text-out",
    ]);
  });

  it("treats an all-zero snapshot as empty and singularizes one call", () => {
    const zero: ConversationState = {
      ...INITIAL_STATE,
      sessionId: "session-1",
      latestTokenUsage: {
        qsfSessionId: "session-1",
        models: [
          {
            modelId: "gpt-realtime-2",
            role: "realtime_voice",
            calls: 1,
            counts: { textInput: 0, audioInput: 0, cachedInput: 0, textOutput: 0, audioOutput: 0 },
          },
        ],
      },
    };
    expect(selectTokenUsagePanelModel(zero).kind).toBe("empty");

    const single: ConversationState = {
      ...INITIAL_STATE,
      sessionId: "session-1",
      latestTokenUsage: {
        qsfSessionId: "session-1",
        models: [
          {
            modelId: "gpt-realtime-2",
            role: "realtime_voice",
            calls: 1,
            counts: { textInput: 7, audioInput: 0, cachedInput: 0, textOutput: 2, audioOutput: 0 },
          },
        ],
      },
    };
    expect(selectTokenUsagePanelModel(single).heroDetail).toBe("session total · 1 model call");
  });

  it("formats token counts compactly", () => {
    expect(formatTokenCount(0)).toBe("0");
    expect(formatTokenCount(941)).toBe("941");
    expect(formatTokenCount(1_100)).toBe("1.1k");
    expect(formatTokenCount(241_900)).toBe("241.9k");
    expect(formatTokenCount(1_200_000)).toBe("1.20M");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run check` — Expected: FAIL (missing exports).
Run: `npm run test` — Expected: FAIL on the new block.

- [ ] **Step 3: Implement**

In `realtime.ts`, below the token-usage parser:

```ts
/// Fixed display order of token classes: inputs before outputs, audio before text
/// within each direction (audio dominates realtime cost), cached input between.
/// The className suffixes double as CSS hooks (`token-seg-<className>`).
const TOKEN_CLASS_ORDER = [
  { key: "audioInput", className: "audio-in", label: "audio in" },
  { key: "textInput", className: "text-in", label: "text in" },
  { key: "cachedInput", className: "cached-in", label: "cached in" },
  { key: "audioOutput", className: "audio-out", label: "audio out" },
  { key: "textOutput", className: "text-out", label: "text out" },
] as const;

/// Human labels for the server-side role ids; an unknown role falls back to its id
/// so a future call site is visible without a UI change.
const TOKEN_ROLE_LABELS: Record<string, string> = {
  realtime_voice: "voice",
  goal_formation: "goal formation",
};

export interface TokenUsageSegmentModel {
  className: string;
  label: string;
  tokens: number;
  /// Tooltip text with the exact count, e.g. "audio in — 48,231 tokens".
  exactLabel: string;
  /// Share of this row's total, in percent (segment widths within the bar).
  widthPercent: number;
}

export interface TokenUsageRowModel {
  name: string;
  totalLabel: string;
  /// This row's total as a percentage of the largest row's total (bar length).
  barPercent: number;
  segments: TokenUsageSegmentModel[];
}

export interface TokenUsageLegendEntry {
  className: string;
  label: string;
}

export interface TokenUsagePanelModel {
  kind: "empty" | "data";
  heroLabel: string;
  heroDetail: string;
  legend: TokenUsageLegendEntry[];
  rows: TokenUsageRowModel[];
}

/// Compact token-count formatting for headline and row totals: exact under 1k,
/// one decimal in k, two decimals in M.
export function formatTokenCount(tokens: number): string {
  if (tokens < 1_000) {
    return String(tokens);
  }
  if (tokens < 1_000_000) {
    return `${(tokens / 1_000).toFixed(1)}k`;
  }
  return `${(tokens / 1_000_000).toFixed(2)}M`;
}

function tokenClassTotal(counts: TokenClassCounts): number {
  return (
    counts.textInput + counts.audioInput + counts.cachedInput + counts.textOutput + counts.audioOutput
  );
}

/// View-model for the Tokens panel: rows sorted by total descending (stable, so
/// equal totals keep server order), bar lengths normalized to the largest row,
/// segments in fixed class order with zero classes dropped, and a legend listing
/// only the classes present anywhere. All formatting decisions live here; the
/// render function only builds DOM.
export function selectTokenUsagePanelModel(state: ConversationState): TokenUsagePanelModel {
  const models = state.latestTokenUsage?.models ?? [];
  const grandTotal = models.reduce((sum, model) => sum + tokenClassTotal(model.counts), 0);
  if (grandTotal === 0) {
    return { kind: "empty", heroLabel: "0", heroDetail: "", legend: [], rows: [] };
  }

  const totalCalls = models.reduce((sum, model) => sum + model.calls, 0);
  const sorted = [...models].sort(
    (a, b) => tokenClassTotal(b.counts) - tokenClassTotal(a.counts),
  );
  const maxTotal = tokenClassTotal(sorted[0].counts);

  const rows: TokenUsageRowModel[] = sorted.map((model) => {
    const rowTotal = tokenClassTotal(model.counts);
    const roleLabel = TOKEN_ROLE_LABELS[model.role] ?? model.role;
    const segments: TokenUsageSegmentModel[] = [];
    for (const tokenClass of TOKEN_CLASS_ORDER) {
      const tokens = model.counts[tokenClass.key];
      if (tokens === 0) {
        continue;
      }
      segments.push({
        className: tokenClass.className,
        label: tokenClass.label,
        tokens,
        exactLabel: `${tokenClass.label} — ${tokens.toLocaleString("en-US")} tokens`,
        widthPercent: (tokens * 100) / rowTotal,
      });
    }
    return {
      name: `${model.modelId} · ${roleLabel}`,
      totalLabel: formatTokenCount(rowTotal),
      barPercent: maxTotal === 0 ? 0 : (rowTotal * 100) / maxTotal,
      segments,
    };
  });

  const legend: TokenUsageLegendEntry[] = TOKEN_CLASS_ORDER.filter((tokenClass) =>
    models.some((model) => model.counts[tokenClass.key] > 0),
  ).map((tokenClass) => ({ className: tokenClass.className, label: tokenClass.label }));

  return {
    kind: "data",
    heroLabel: formatTokenCount(grandTotal),
    heroDetail: `session total · ${totalCalls} model call${totalCalls === 1 ? "" : "s"}`,
    legend,
    rows,
  };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test` — Expected: PASS.
Run: `npm run check` then `npm run fmt`.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/realtime.test.ts
git commit -m "realtime ui: tokens panel view-model"
```

### Task 3.3: Bottom-row split, panel markup, render, styles

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/main.ts`
- Modify: `crates/qsf_realtime_server/ui/src/styles.css`

**Interfaces:**
- Consumes: `parseTokenUsageMessage`, `selectTokenUsagePanelModel`, `TokenUsagePanelModel` (Tasks 3.1–3.2).
- Produces: the Tokens card in the bottom row; new `data-role="token-usage-body"`; no other `data-role` changes.

No unit tests (markup/render task per the repo's UI testing policy); gates are `npm run check` and human verification.

- [ ] **Step 1: Split the bottom row in the template**

In `main.ts` `root.innerHTML`, wrap the existing phase-strip section and add the Tokens card — replace the block from `<section class="panel phase-strip">` through its closing `</section>` with:

```html
    <section class="bottom-strip">
      <section class="panel phase-strip">
        <div class="phase-strip-header">
          <h2>Phase timeline</h2>
          <ul class="phase-lane-legend" aria-hidden="true">
            <li><i style="background: var(--phase-idle)"></i>idle</li>
            <li><i style="background: var(--phase-listening)"></i>listening</li>
            <li><i style="background: var(--phase-thinking)"></i>thinking</li>
            <li><i style="background: var(--phase-speaking)"></i>speaking</li>
            <li><i class="legend-gap"></i>skipped idle</li>
          </ul>
        </div>
        <div class="phase-lane-wrap">
          <canvas data-role="phase-lane" aria-label="Runtime phase timeline, last 60 seconds of activity"></canvas>
          <div data-role="phase-lane-tip" class="phase-lane-tip" hidden></div>
        </div>
      </section>

      <aside class="panel token-panel">
        <div class="panel-header">
          <h2>Tokens</h2>
          <span class="status-pill muted">Session totals</span>
        </div>
        <div data-role="token-usage-body" class="token-usage-body"></div>
      </aside>
    </section>
```

(The phase-strip markup is unchanged — it only gains the wrapper and the sibling card.)

- [ ] **Step 2: Wire the ref, the socket hook, and the render call**

In `main.ts`:

(a) Extend the imports from `./realtime` with `parseTokenUsageMessage`, `selectTokenUsagePanelModel`, and `type TokenUsagePanelModel`.

(b) Add to `UiRefs`:

```ts
  tokenUsageBody: HTMLElement;
```

and to `collectRefs`'s returned object:

```ts
    tokenUsageBody: query<HTMLElement>('[data-role="token-usage-body"]'),
```

(c) In the relay-socket `message` listener (after the `parseVolitionStateMessage` block):

```ts
      const tokenUsage = parseTokenUsageMessage(raw);
      if (tokenUsage !== null) {
        dispatch({ type: "token_usage_captured", snapshot: tokenUsage });
      }
```

(d) At the end of `render()` (after the `renderWhyThisAnswerPanel` call):

```ts
  renderTokenUsagePanel(refs.tokenUsageBody, selectTokenUsagePanelModel(state));
```

(e) Add the render function next to `renderWhyThisAnswerPanel`:

```ts
function renderTokenUsagePanel(container: HTMLElement, model: TokenUsagePanelModel) {
  container.replaceChildren();

  if (model.kind === "empty") {
    const empty = document.createElement("p");
    empty.className = "token-usage-empty";
    empty.textContent = "No model calls yet.";
    container.appendChild(empty);
    return;
  }

  const hero = document.createElement("p");
  hero.className = "token-hero";
  const heroValue = document.createElement("strong");
  heroValue.textContent = model.heroLabel;
  const heroDetail = document.createElement("span");
  heroDetail.textContent = model.heroDetail;
  hero.append(heroValue, heroDetail);
  container.appendChild(hero);

  const legend = document.createElement("ul");
  legend.className = "token-legend";
  for (const entry of model.legend) {
    const item = document.createElement("li");
    const chip = document.createElement("i");
    chip.className = `token-seg-${entry.className}`;
    const text = document.createElement("span");
    text.textContent = entry.label;
    item.append(chip, text);
    legend.appendChild(item);
  }
  container.appendChild(legend);

  for (const row of model.rows) {
    const rowElement = document.createElement("div");
    rowElement.className = "token-model-row";
    const head = document.createElement("div");
    head.className = "token-model-head";
    const name = document.createElement("span");
    name.className = "token-model-name";
    name.textContent = row.name;
    const total = document.createElement("span");
    total.className = "token-model-total";
    total.textContent = row.totalLabel;
    head.append(name, total);
    const bar = document.createElement("div");
    bar.className = "token-bar";
    bar.style.width = `${row.barPercent}%`;
    for (const segment of row.segments) {
      const segmentElement = document.createElement("i");
      segmentElement.className = `token-seg-${segment.className}`;
      segmentElement.style.width = `${segment.widthPercent}%`;
      segmentElement.title = segment.exactLabel;
      bar.appendChild(segmentElement);
    }
    rowElement.append(head, bar);
    container.appendChild(rowElement);
  }
}
```

- [ ] **Step 3: Add the styles**

In `styles.css`, after the `.phase-strip-header h2` rule block, add:

```css
.bottom-strip {
  display: grid;
  grid-template-columns: minmax(0, 2fr) minmax(0, 1fr);
  gap: 0.85rem;
  min-height: 0;
}

.token-panel {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
}

.token-usage-body {
  min-height: 0;
  overflow-y: auto;
  padding: 0.35rem 1rem 0.9rem;
}

.token-usage-empty {
  margin: 0.4rem 0 0;
  color: var(--muted);
  font-style: italic;
}

.token-hero {
  display: flex;
  align-items: baseline;
  gap: 0.6rem;
  margin: 0 0 0.55rem;
}

.token-hero strong {
  font-size: 1.7rem;
  line-height: 1.1;
}

.token-hero span {
  color: var(--muted);
  font-size: 0.78rem;
}

.token-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem 0.9rem;
  list-style: none;
  margin: 0 0 0.75rem;
  padding: 0;
  color: var(--muted);
  font-size: 0.72rem;
}

.token-legend li {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
}

.token-legend i {
  width: 0.6rem;
  height: 0.6rem;
  border-radius: 3px;
}

.token-model-row {
  margin-bottom: 0.7rem;
}

.token-model-head {
  display: flex;
  justify-content: space-between;
  gap: 0.6rem;
  font-size: 0.8rem;
  margin-bottom: 0.3rem;
}

.token-model-name {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.token-model-total {
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}

/* 2px surface gaps between class segments; rounded ends on the whole bar. */
.token-bar {
  display: flex;
  gap: 2px;
  height: 0.85rem;
  border-radius: 4px;
  overflow: hidden;
}

.token-bar i {
  display: block;
  height: 100%;
  min-width: 1px;
}

/* Class colors: blue pair for fresh input (audio dark, text light), amber for
   cached input, green pair for output. Chosen on the dark surface for adjacent
   distinguishability; identity never rides on color alone (legend + tooltips). */
.token-seg-audio-in {
  background: #5598e7;
}

.token-seg-text-in {
  background: #b7d3f6;
}

.token-seg-cached-in {
  background: #c98500;
}

.token-seg-audio-out {
  background: #199e70;
}

.token-seg-text-out {
  background: #7fd0b0;
}
```

and inside the `@media (max-width: 900px)` block add:

```css
  .bottom-strip {
    grid-template-columns: 1fr;
  }
```

- [ ] **Step 4: Run the gates**

Run: `npm run test` — Expected: PASS.
Run: `npm run check` — Expected: clean.
Run: `npm run fmt`.
Run: `npm run build` — Expected: clean production build.

- [ ] **Step 5: Human verification (external testing recommended)**

1. Run `./qsf.ps1 realtime`, open the page.
2. Before any turn: bottom row shows Phase Timeline (~2/3) and a Tokens card (~1/3) reading "No model calls yet."
3. Hold a short voice conversation (or send a text turn). After the first assistant response the Tokens card shows a headline total, a legend, and a `gpt-realtime-2 · voice` row whose bar has visible audio/text/cached segments.
4. After a trusted turn completes, a second row appears for the goal-formation judge (text/cached classes only), noticeably shorter than the realtime row.
5. Hover a segment: the native tooltip reports the class and exact count (e.g. "audio in — 48,231 tokens").
6. Speak more turns: totals grow monotonically; row order stays stable (largest on top); the cached-input segment should grow fastest once the prefix cache warms.
7. Reload the page mid-session and reconnect: totals reappear at their correct values (snapshot self-heal).
8. Stop the session: the card keeps its final totals for post-hoc review. Start a new session: the card resets to "No model calls yet."
9. Shrink the window below 900 px: the Tokens card stacks under the timeline; nothing is clipped.

- [ ] **Step 6: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/main.ts crates/qsf_realtime_server/ui/src/styles.css
git commit -m "realtime ui: tokens panel shares the bottom row with the phase timeline"
```

---

## Phase 4 — Decision log and final gates

### Task 4.1: Record the scope decision and close the gates

**Files:**
- Modify: `docs/DecisionLog.md`

- [ ] **Step 1: Add the decision entry**

Append to `docs/DecisionLog.md` (matching the file's entry template; adjust the date if implementation lands later):

```markdown
## 2026-07-07 - Realtime diagnostics token meter is session-scoped raw token counts
Decision: The realtime diagnostics page reports provider token consumption as raw token
counts per model and token class (fresh text/audio input, cached input, text/audio
output), scoped to the current realtime session. No dollar conversion is shown, and
every billed provider call counts — including stale or cancelled realtime responses and
goal-formation calls whose responses later fail parsing or validation, since the
provider billed them.
Context: Cost visibility was requested for the simulator's OpenAI usage. A price table
would allow a single combined dollar figure but must be hand-maintained per model; raw
class counts answer the operative question — where the tokens go — without that burden.
Audio tokens dominate realtime pricing, so the audio/text split is the load-bearing
distinction.
Consequences: The diagnostics meter never claims monetary cost. A new server-side model
call site must feed the session token ledger to appear on the page. A future dollar view
would build on these same class counts plus a maintained price table.
```

- [ ] **Step 2: Final repo gates**

From the repo root:

Run: `cargo build` — Expected: clean.
Run: `cargo clippy --all-targets -- -D warnings` — Expected: clean.
Run: `cargo fmt`
From `crates/qsf_realtime_server/ui/`: `npm run check`, `npm run fmt`, `npm run test` — Expected: all clean/pass.

- [ ] **Step 3: Commit**

```bash
git add docs/DecisionLog.md
git commit -m "docs: record session-scoped raw-token diagnostics meter decision"
```

## Success Criteria

- During a live session the Tokens card answers "where do the tokens go" at a glance: one stacked bar per (model, role), largest on top, classes color-coded with a legend and exact-count tooltips, headline session total with call count.
- The realtime model's audio/text/cached split is real provider data (`input_token_details` / `output_token_details`), not inference; text-only models and detail-less payloads degrade to text classes without error, and input classes never sum past the provider's `input_tokens` (regression-tested).
- Both current call sites feed the ledger: realtime `response.done` (including stale/cancelled responses, regression-tested) and the live-goal-formation judge (including billed calls whose responses fail post-response parsing or validation, regression-tested). Adding a future call site requires only one `record_token_usage(...)` call.
- The browser holds no accumulation state: full-snapshot push per recorded call; a reconnecting or late-joining socket immediately receives the current snapshot (watch-channel guarantee, unit-tested).
- The persisted session schema (`ExchangeModelUse`) is unchanged.
- Reducers, parsers, and the view-model are pure and unit-tested (parse validation, session guarding, clear-on-allocate, row ordering, bar normalization, zero-class dropping, count formatting, singular/plural call counts).
- All gates pass: `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`, `npm run test`, `npm run check`, `npm run fmt`, `npm run build`.
