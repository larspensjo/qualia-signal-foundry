# Live Memory Capture And Retrieval Quality Implementation Plan

**Goal:** Make the live memory loop store the important facts shown by the Ari/Lars/volition QA session, stop irrelevant memories from being retrieved and reinforced, and add a repeatable evaluation path that catches regressions.

**Why now:** The `runs/2026-05-24-185308-multi-turn-text-loop` evaluation showed that the live loop can capture and reuse a narrow assistant-name memory, but it failed to persist the user's name and the explicit "remember this" volition discussion. It also retrieved and reinforced the Ari memory on unrelated turns because retrieval selected the top record even when it had no direct relevance signal.

**Architecture:** Five core phases plus an optional resume-check phase. Phase 1 adds a retrieval relevance gate and reinforcement eligibility. Phase 2 extracts live memory capture into a small pure module and adds user-identity capture. Phase 3 resolves explicit "remember this" turns against recent context. Phase 4 prevents truncated warm summaries from becoming continuity state. Phase 5 adds the QA fixture/evaluation path and updates architecture/docs. Phase 6 is optional closeout for integration with the paused association-proposer work.

**Tech Stack:** Rust workspace (`crates/qsf_app`, `crates/qsf_memory`), existing JSONL event/trace observability, `serde` persistence, `engine_logging` for operational logs, existing mock model test scaffolding.

**Relationship to current work:** Pause `Plan.AssociativeRecallAndDropDrivenAssociations.md` Phase 5 until this plan has at least Phases 1-3 complete. Association proposers become more useful once the live store contains multiple meaningful memories and retrieval no longer reinforces noise.

---

## Review-Pass Adjustments (2026-05-25)

Findings from `docs/Reviews/Review.LiveMemoryCaptureAndRetrievalQuality.2026-05-25.md` applied to this plan:

- Replaced the nonexistent `SessionTurn` type in the live-capture sketch with narrow previous-turn string inputs so `memory/live_capture.rs` does not depend on session state.
- Chose a concrete skip-reason diagnostic shape: `RetrievedMemory.skip_reason: Option<String>`.
- Reframed Phase 3 to store a bounded source excerpt plus topic/source metadata instead of promising a hand-curated rule-based semantic summary.
- Dropped the proposed `TurnSummaryResult` wrapper in favor of using the existing `ModelResponse.finish_reason`.
- Clarified that hints already cannot be reinforced because reinforcement is driven by `retrieval.selected`; the relevance gate is the actual fix.
- Made candidate kind / id suffix part of the live-capture contract so multiple candidate kinds from one turn cannot collide.
- Clarified that retrieval gating is shared, while live capture remains wired only into `multi_turn_text_loop` for this plan.

---

## Evidence From Last QA Run

Artifacts inspected:

- `state/qa-memory-browser-real/memory-store.json`
- `state/qa-memory-browser-real/session-state.json`
- `runs/2026-05-24-185308-multi-turn-text-loop/events.jsonl`
- `runs/2026-05-24-185308-multi-turn-text-loop/traces.jsonl`
- `runs/2026-05-24-185308-multi-turn-text-loop/multi-turn-text-loop.md`

Observed behavior:

- Only one durable memory was stored: `Assistant name: Ari`.
- `My name is Lars.` did not become a durable user identity memory.
- `Interesting, please remember this for future discussions!` did not persist the prior volition-system discussion.
- The Ari memory was selected on turns with zero matched terms because retrieval always returned the top N records.
- The Ari memory ended with high reinforcement mostly because it was the only available record.
- No associations were created because there was only one durable memory endpoint.
- One warm summary hit `finish_reason: max_tokens` and was persisted as a truncated continuity summary.

Acceptance target for this plan:

```text
I want you to use the name Ari.                  -> stores assistant name Ari
My name is Lars.                                 -> stores user name Lars
Interesting, please remember this...             -> stores remembered volition topic with source excerpt
What is your name?                               -> retrieves Ari
What is my name?                                 -> retrieves Lars
What did I ask you to remember about volition?   -> retrieves the remembered volition topic
Unrelated volition turns                         -> do not retrieve Ari solely by importance/recency
```

---

## Open Questions And Proposed Defaults

These are surfaced up front so implementation does not silently resolve them.

| # | Question | Proposed default for this plan |
|---|---|---|
| 1 | Rule-based capture or model-assisted extraction? | Rule-based only for this plan. Add model-assisted extraction later after the baseline is observable. |
| 2 | Where should user profile facts live? | Same durable memory store for now, tagged distinctly as `user_identity` / `profile`. No schema split yet. |
| 3 | How should "remember this" resolve context? | Use the immediately preceding assistant response plus the preceding user topic. Store a bounded source excerpt with topic tags and source-turn metadata; high-quality compression is a follow-up. |
| 4 | What if the user later corrects a name? | Capture the new fact, but do not implement supersession in this plan. Add an explicit follow-up. |
| 5 | Should zero-signal memories ever retrieve? | Only if they are explicitly classified as always-on profile/identity and the query is profile/identity-shaped. Otherwise no. |
| 6 | Should hints be reinforced? | No. Reinforce direct memory selections only, and only when they pass the relevance gate. |
| 7 | What happens when summary retry still truncates? | Log the failure and leave the turn unsummarized rather than persisting a truncated summary. |
| 8 | Should this affect the voice loop? | Retrieval relevance is shared because it lives in `memory/retrieval.rs`. New live capture initially targets `multi_turn_text_loop` only; the helper may be importable, but the voice loop must not call it until voice-specific behavior is planned. |

---

## Core Invariants

- Preserve unidirectional flow: input -> action/side effect -> event -> reducer -> state -> render.
- Keep reducers pure. Memory capture, persistence, and model calls remain side effects that emit events.
- Retrieval must be inspectable in traces: selected, omitted, and relevance-skip reasons should be visible. Use one stable diagnostic shape: `RetrievedMemory.skip_reason: Option<String>`, populated for omitted candidates that fail relevance or exceed the retrieval limit.
- Capture rules must be pure and unit-testable.
- Defaults exercise the new code path. Do not hide relevance gating or capture behind an off-by-default flag.
- Durable memories should be concise, tagged, and deduplicated before persistence.
- Do not solve semantic contradiction/supersession in this plan; leave a follow-up if implementation exposes the need.

---

## File Structure

### New files

- `crates/qsf_app/src/memory/live_capture.rs` - pure live-memory candidate extraction.
- `docs/Experiments/Experiment.LiveMemoryCaptureQuality.md` - repeatable QA scenario and success criteria.

### Modified files

- `crates/qsf_app/src/memory/retrieval.rs` - relevance gating and `RetrievedMemory.skip_reason`.
- `crates/qsf_app/src/memory/mod.rs` - export live capture helpers.
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` - use live capture helper, avoid reinforcing irrelevant selections, detect truncated summaries.
- `crates/qsf_app/src/observability/event_log.rs` - add event type only if existing payloads cannot express capture/skips clearly.
- `docs/Architecture/Architecture.MemorySystem.md` - update Implementation Status after implementation.
- `docs/Architecture/Architecture.RuntimeLoop.md` - update memory capture/retrieval steps after implementation.
- `docs/Architecture/Architecture.StateAndObservability.md` - add one line about retrieval skip reasons in traces if `RetrievedMemory.skip_reason` becomes part of trace payloads.
- `docs/EngineeringDiary.md` - one entry per code-touching phase.
- `docs/DecisionLog.md` - only if a durable rule is adopted beyond this plan's implementation details.

---

## Documents To Update

Per `docs/ProjectFrame/ProjectWorkflow.md`:

- `docs/EngineeringDiary.md` - add one entry per implementation phase. Plan-only edits do not need a diary entry.
- `docs/Experiments/Experiment.LiveMemoryCaptureQuality.md` - create in Phase 5 to make the QA session repeatable and interpretable.
- `docs/Architecture/Architecture.MemorySystem.md` - update after Phases 1-3 because memory capture and retrieval semantics change.
- `docs/Architecture/Architecture.RuntimeLoop.md` - update after Phases 1-4 because live memory capture and warm summary failure handling change runtime behavior.
- `docs/Architecture/Architecture.StateAndObservability.md` - update after Phase 1 if retrieval skip reasons become an explicit trace field.
- `docs/DecisionLog.md` - add an entry only if the project wants to commit to a durable rule such as "zero-signal memories are not retrieved by default."
- `docs/Plans/Plan.AssociativeRecallAndDropDrivenAssociations.md` - add a short note before Phase 5 if this plan changes the recommended order of work.

---

## Standard Verification

Every code phase ends with:

```powershell
cargo test -p qsf_app
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt
```

If a phase changes `qsf_memory`, also run:

```powershell
cargo test -p qsf_memory
```

No `crates/qsf_browser_server/ui/` changes are expected. If that changes, run `npm run check` and `npm run fmt` from that directory.

---

## Phase 1 - Retrieval Relevance Gate And Reinforcement Eligibility

**Goal:** Stop irrelevant memories from being selected and reinforced.

### Task 1.1: Add a relevance gate to `KeywordTag` retrieval

**Files:**

- Modify: `crates/qsf_app/src/memory/retrieval.rs`

- [ ] Add tests for zero-signal retrieval:

```rust
#[test]
fn keyword_tag_omits_zero_signal_memory() {
    // One high-importance memory with no query term overlap should not be selected.
}

#[test]
fn keyword_tag_keeps_direct_keyword_or_tag_match() {
    // A memory matching "name" or a curated tag should still be selected.
}
```

- [ ] Implement `is_relevant_for_strategy(record, strategy, score, matched_terms, association_paths, query_terms)`.
- [ ] For `RetrievalStrategy::KeywordTag`, select records only when at least one relevance signal is present:
  - text keyword match
  - tag match
  - association path, if association expansion is active for the strategy
  - explicit identity/profile allowance from Task 1.2
- [ ] Add the diagnostic field shape up front: `RetrievedMemory.skip_reason: Option<String>`.
- [ ] Set `skip_reason = None` for selected memories.
- [ ] Set `skip_reason = Some("relevance gate: no keyword, tag, association, or profile signal")` for records filtered by the relevance gate.
- [ ] Set `skip_reason = Some("retrieval limit exceeded")` for relevant records omitted only because the limit was reached.
- [ ] Ensure `RetrievalResult.omitted` includes both relevance-filtered records and relevant-over-limit records so traces remain complete.
- [ ] Verify:

```powershell
cargo test -p qsf_app keyword_tag_omits_zero_signal_memory keyword_tag_keeps_direct_keyword_or_tag_match
```

Expected: both pass.

### Task 1.2: Add a minimal profile/identity allowance

**Files:**

- Modify: `crates/qsf_app/src/memory/retrieval.rs`

- [ ] Add tests:

```rust
#[test]
fn profile_query_can_retrieve_identity_memory() {
    // "what is your name" can retrieve assistant_identity/name.
}

#[test]
fn unrelated_query_does_not_retrieve_identity_memory_by_importance_only() {
    // "tell me about volition goals" does not retrieve Ari unless there is a real match.
}
```

- [ ] Define a tiny helper, for example `query_is_identity_shaped(query_terms)`, that recognizes terms like `name`, `called`, `who`, `what` only when paired with identity/profile tags.
- [ ] Keep this helper conservative. It is a relevance exception, not a general fallback.
- [ ] Verify the tests above.

### Task 1.3: Reinforce only eligible retrieval selections

**Files:**

- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] Add a regression test using a small store where one memory is omitted by relevance gating.
- [ ] Keep reinforcement driven by `retrieval.selected`; do not feed context-assembly selections back into reinforcement.
- [ ] Confirm hints cannot enter `retrieval.selected` today because hints are created after retrieval by `expand_neighbors`. Add a debug assertion only if it clarifies the invariant.
- [ ] Rely on the relevance gate to keep irrelevant memories out of `retrieval.selected`; no separate reinforcement cap change is required because reinforcement contribution is already capped in scoring.
- [ ] Emit `MemoryReinforced` with skipped counts or skipped ids if practical.
- [ ] Verify Ari-style no-match retrieval does not increase reinforcement.

### Phase 1 verification

- [ ] Run the standard verification block.
- [ ] Add a diary entry summarizing retrieval gating and reinforcement eligibility.
- [ ] Human testing recommended: rerun the prior Ari/Lars/volition transcript and confirm Ari is not printed on unrelated volition turns.

---

## Phase 2 - Live Capture Module And User Identity Memory

**Goal:** Move live memory capture out of one-off assistant-name parsing and persist user identity facts such as "My name is Lars."

### Task 2.1: Extract pure live capture helpers

**Files:**

- Create: `crates/qsf_app/src/memory/live_capture.rs`
- Modify: `crates/qsf_app/src/memory/mod.rs`
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] Introduce:

```rust
pub struct LiveCaptureInput<'a> {
    pub user_input: &'a str,
    pub assistant_response: &'a str,
    pub previous_turn_index: Option<usize>,
    pub previous_user_input: Option<&'a str>,
    pub previous_assistant_response: Option<&'a str>,
}

pub struct LiveMemoryCandidate {
    pub candidate_kind: LiveMemoryCandidateKind,
    pub id_suffix: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub importance: f64,
    pub source_turn_index: Option<usize>,
}
```

Do not pass `session::Turn` or any experiment-local turn type into `memory/live_capture.rs`. The helper should receive only the strings and small metadata it needs, keeping `memory/` free of a dependency on session state.

- [ ] Return `Vec<LiveMemoryCandidate>` rather than a single optional candidate so future turns can produce more than one memory without changing the persistence loop again.
- [ ] Move existing assistant-name extraction into the helper unchanged first.
- [ ] Preserve existing assistant-name tests and add one pure helper test.
- [ ] Keep persistence in `multi_turn_text_loop.rs`; the helper only proposes candidates.
- [ ] Make the persisted id template per candidate: `memory.live.{session}.turn-{NNN}.{candidate.id_suffix}`.
- [ ] Ensure `id_suffix` includes the candidate kind, for example `assistant-name`, `user-name`, or `remembered-topic`, so multiple candidates from the same turn cannot collide.
- [ ] Keep the helper importable from `memory/`, but only call it from `multi_turn_text_loop` in this plan. Do not wire live capture into `text_owned_voice_loop` yet.

### Task 2.2: Capture user name statements

**Files:**

- Modify: `crates/qsf_app/src/memory/live_capture.rs`
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] Add tests:

```rust
#[test]
fn captures_user_name_statement() {
    // "My name is Lars." -> title "User name: Lars"
}

#[test]
fn user_name_and_assistant_name_have_distinct_tags_and_ids() {
    // Prevent "name" memories from overwriting each other through duplicate detection.
}
```

- [ ] Recognize conservative patterns:
  - `my name is <Name>`
  - `I am <Name>` only if the statement is short and name-like
  - `call me <Name>`
- [ ] Store with tags such as `user_identity`, `profile`, `name`.
- [ ] Use a distinct candidate kind and id suffix, for example `user-name`, not `assistant-name`.
- [ ] Keep duplicate detection scoped by normalized title and summary so Ari and Lars do not conflict.
- [ ] Emit `MemoryStorePersisted` with candidate kind/count in the payload.

### Task 2.3: End-to-end identity continuity test

**Files:**

- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] Add or extend an in-crate test that runs turns:

```text
I want you to use the name Ari.
My name is Lars.
What is your name?
What is my name?
```

- [ ] Assert the persisted memory store contains two records:
  - assistant identity Ari
  - user identity Lars
- [ ] Assert the relevant questions retrieve the matching memory.
- [ ] Assert unrelated non-identity queries do not retrieve either identity memory solely by importance.

### Phase 2 verification

- [ ] Run the standard verification block.
- [ ] Add a diary entry summarizing the live capture extraction and user identity capture.
- [ ] Human testing recommended: run the text loop manually and ask both "What is your name?" and "What is my name?"

---

## Phase 3 - Explicit Remember-This Capture

**Goal:** Persist useful source content when the user explicitly asks the system to remember the current topic.

**Scope decision from review:** This phase uses the "lower the bar" approach. It does not pretend to produce a high-quality semantic summary from arbitrary assistant prose. Instead, it stores a bounded excerpt of the previous assistant response, topic tags from the previous user input/current request, and source-turn metadata. Model-assisted compression and richer semantic summaries are follow-ups.

### Task 3.1: Detect explicit remember requests

**Files:**

- Modify: `crates/qsf_app/src/memory/live_capture.rs`

- [ ] Add pure tests:

```rust
#[test]
fn detects_explicit_remember_request() {
    // "Interesting, please remember this for future discussions!"
}

#[test]
fn ordinary_memory_word_does_not_trigger_capture() {
    // "How does memory influence goal selection?" should not trigger remember-this capture.
}
```

- [ ] Recognize conservative patterns:
  - `remember this`
  - `please remember this`
  - `remember that for future`
  - `keep this in mind`
- [ ] Require the request to be imperative or directed at the assistant. Avoid triggering on ordinary discussion of memory.

### Task 3.2: Resolve "this" against recent context

**Files:**

- Modify: `crates/qsf_app/src/memory/live_capture.rs`
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] Add tests with a previous volition turn:

```text
Previous user: Tell me more what you think how a volition system should work.
Previous assistant: A good volition system should include needs/drives, goals, arbitration...
Current user: Interesting, please remember this for future discussions!
```

Expected candidate summary shape:

```text
The user asked to remember the prior discussion for future conversations. Topic: volition system. Source excerpt: A good volition system for simulations should...
```

- [ ] Implement a deterministic resolver:
  - Prefer the immediately previous assistant response as the content source.
  - Use the immediately previous user input as the topic source.
  - If no previous assistant response exists, skip capture and emit a trace/event explaining why.
- [ ] Add tags from obvious topic terms when present, for example `volition_system`, `goals`, `simulation`, `remembered_topic`.
- [ ] Store a bounded source excerpt rather than a hand-curated semantic summary. Use a named constant for the character cap.
- [ ] Include source-turn metadata in either `source_reference` or candidate metadata, enough to identify which previous turn supplied the excerpt.
- [ ] Do not call a model in this phase.
- [ ] Add a follow-up note in the experiment doc that summary quality is intentionally limited until model-assisted compression exists.
- [ ] Deduplicate against existing remembered-topic memories.

### Task 3.3: End-to-end remembered-topic test

**Files:**

- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] Add a test using the Ari/Lars/volition mini-transcript.
- [ ] Assert the store contains a remembered-topic memory after the explicit remember turn.
- [ ] Assert the remembered-topic memory contains the volition topic tag and a bounded source excerpt from the previous assistant response.
- [ ] Assert a later query like `What did I ask you to remember about volition?` retrieves that memory.
- [ ] Assert the assistant-name memory is not selected for the volition recall query unless it has a direct signal.

### Phase 3 verification

- [ ] Run the standard verification block.
- [ ] Add a diary entry summarizing explicit remember capture.
- [ ] Human testing recommended: run the exact transcript from the QA session plus a follow-up question about the remembered volition topic.

---

## Phase 4 - Warm Summary Truncation Guard

**Goal:** Prevent truncated summaries from silently becoming continuity state.

### Task 4.1: Expose summary finish reason to the aging path

**Files:**

- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
- Modify: model test scaffolding only if needed.

- [ ] Use the existing `ModelResponse.finish_reason`; do not add a redundant wrapper unless downstream code proves it needs one.
- [ ] Update `summarize_turn` to return either `(TurnSummary, Option<String>)` or to store finish reason directly on `TurnSummary`. Prefer the smaller change after inspecting call sites.
- [ ] Keep trace output including the raw model finish reason.

### Task 4.2: Retry or fail closed on truncation

**Files:**

- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] Add tests with a fake model client:
  - first summarizer response returns `finish_reason = max_tokens`
  - retry returns `finish_reason = stop`
  - state stores the retry summary
- [ ] Add a second test:
  - both attempts return `max_tokens`
  - the turn remains unsummarized
  - an error event/log is emitted with `session_id` and `turn_index`
- [ ] Increase the first-pass summary cap if useful, but do not rely on a larger cap alone.
- [ ] On final truncation, do not call `TurnSummarized`; leave the turn hot so the system can try again later.

### Phase 4 verification

- [ ] Run the standard verification block.
- [ ] Add a diary entry summarizing warm summary truncation handling.
- [ ] Human testing recommended: run a long answer that forces summarization and inspect `multi-turn-text-loop.md` for complete warm summaries.

---

## Phase 5 - QA Fixture, Experiment Doc, And Architecture Updates

**Goal:** Make the Ari/Lars/volition session a repeatable regression scenario and update docs to match implemented behavior.

### Task 5.1: Add the experiment document

**Files:**

- Create: `docs/Experiments/Experiment.LiveMemoryCaptureQuality.md`

- [ ] Define:
  - hypothesis
  - transcript fixture
  - expected durable memories
  - expected retrievals
  - expected non-retrievals
  - measurements from events/traces
  - human testing notes
  - result fields to fill after running

### Task 5.2: Add an automated or semi-automated QA fixture

**Files:**

- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`, or add a fixture under `docs/Experiments/Fixtures/` if that matches existing test style.

- [ ] Prefer an automated in-crate test if the current fake model harness supports it.
- [ ] If a full live-model test is required, add a documented manual command instead and keep deterministic unit coverage for capture/retrieval helpers.
- [ ] The fixture should validate:
  - memory count and titles
  - selected memory IDs per follow-up query
  - no zero-signal Ari retrieval on unrelated volition turns
  - no truncated warm summaries in persisted state

### Task 5.3: Update architecture docs

**Files:**

- Modify: `docs/Architecture/Architecture.MemorySystem.md`
- Modify: `docs/Architecture/Architecture.RuntimeLoop.md`
- Modify: `docs/Architecture/Architecture.StateAndObservability.md` if `RetrievedMemory.skip_reason` is emitted in traces.

- [ ] Update Implementation Status with:
  - relevance-gated retrieval
  - live capture categories implemented
  - explicit remember-this source-excerpt capture
  - summary truncation guard
- [ ] In `Architecture.StateAndObservability.md`, document retrieval skip reasons as part of memory-retrieval trace payloads if the field lands.
- [ ] Refresh `Last reviewed:` dates.
- [ ] Keep aspirational sections clearly marked as candidate design.

### Task 5.4: Decide whether a DecisionLog entry is needed

**Files:**

- Maybe modify: `docs/DecisionLog.md`

- [ ] If the team wants a durable rule, add an entry like:

```text
2026-05-25 - Zero-signal memories are not retrieved by default
Decision: Live retrieval does not select or reinforce durable memories that have no query, tag, association, or explicit profile relevance signal.
```

- [ ] If this remains implementation detail, do not add a decision entry.

### Task 5.5: Note interaction with the association plan

**Files:**

- Modify: `docs/Plans/Plan.AssociativeRecallAndDropDrivenAssociations.md`

- [ ] Add a short note before Phase 5:
  - complete live capture/retrieval quality first
  - then resume `AssociationProposer` implementation
  - explain that proposer quality depends on meaningful memory endpoints

### Phase 5 verification

- [ ] Run the standard verification block if code changed in Task 5.2.
- [ ] Confirm docs are internally consistent:

```powershell
rg -n "LiveMemoryCaptureQuality|zero-signal memories|user identity|retrieval skip reason|Last reviewed" docs
```

- [ ] Human testing recommended: run the full transcript with a real model and inspect state plus run traces.

---

## Phase 6 - Optional Association-Proposer Resume Check

**Goal:** Resume `Plan.AssociativeRecallAndDropDrivenAssociations.md` Phase 5 with better memory inputs.

### Task 6.1: Rerun the association-oriented QA session

- [ ] Start from a clean `QSF_STATE_DIR`.
- [ ] Run the Ari/Lars/volition transcript.
- [ ] Ask:
  - `What is your name?`
  - `What is my name?`
  - `What did I ask you to remember about volition?`
- [ ] Confirm the store has at least three meaningful memory records.
- [ ] Confirm any associations formed have valid endpoints and interpretable reasons.

### Task 6.2: Resume proposer work

- [ ] Continue `Plan.AssociativeRecallAndDropDrivenAssociations.md` Phase 5.
- [ ] Use the new QA fixture as one sanity check for sleep safety-net/proposer behavior.

---

## Risks And Follow-Ups

- Rule-based capture will miss many valid memories. This is acceptable for the baseline; model-assisted capture should be a follow-up after traces prove what the rules miss.
- User identity and assistant identity both match `name`. The relevance gate must prevent cross-contamination in "your name" vs "my name" queries as much as possible.
- Explicit remember capture can under-compress rich assistant answers because it stores a bounded source excerpt rather than a semantic summary. The first slice should preserve useful source text and keep the source turn identifiable; summary quality is a follow-up.
- Multiple live-memory candidates from one turn can collide if ids are built only from `session_id + turn_index`. Candidate kind must be part of the id suffix.
- Processed ranges may mark turns as association-processed before later memory records are created from those turns. If this becomes observable, add a follow-up plan for endpoint-aware association backfill.
- Supersession and contradiction are deliberately out of scope. A later "call me X instead" scenario should get its own plan.

## Definition Of Done

- The Ari/Lars/volition transcript produces durable memories for Ari, Lars, and a remembered-topic record containing a bounded volition source excerpt.
- Follow-up questions retrieve the correct memory records.
- Unrelated turns do not retrieve identity memories solely because they are important or recent.
- Irrelevant retrieved memories are not reinforced.
- Warm summaries are not persisted when the summarizer response truncates.
- The behavior is covered by deterministic tests and a documented QA experiment.
- Architecture docs reflect implemented behavior, and diary entries exist for code changes.
