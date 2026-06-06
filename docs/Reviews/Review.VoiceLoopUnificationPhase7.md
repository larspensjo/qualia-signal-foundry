# Review: Voice Loop Unification Phase 7

Status: Review (snapshot in time)

Reviewed: 2026-06-06 against the staged Phase 7 changes (sleep consumes voice
sessions) for `Plan.VoiceLoopUnification.md`.

Scope of the staged diff under review:

- `crates/qsf_app/src/session/sleep_records.rs` (new normalized sleep view)
- `crates/qsf_app/src/session/mod.rs` (re-exports)
- `crates/qsf_app/src/experiments/sleep_phase_session_summary.rs`
- `crates/qsf_app/src/sleep/proposers/safety_net_co_retrieval.rs`
- `crates/qsf_app/src/sleep/sleep_report.rs` (`diagnostic_notes`)
- Architecture/Experiment docs + EngineeringDiary entry

## Verification performed

- `cargo test -p qsf_app sleep --lib` — 45 passed, 0 failed.
- `cargo clippy -p qsf_app --all-targets -- -D warnings` — clean.

These reproduce the diary's claimed verification.

## Strengths

- The normalized `SleepRecord` view orders mixed turn/exchange content
  chronologically by `started_at`, with `completed_at` then `kind`/`source_index`
  tie-breakers, exactly as the plan's open-question 1 required (no reliance on the
  non-unique vector index). Regression coverage exercises both text-then-voice and
  voice-then-text orderings.
- The provider-preamble boundary is well defended on multiple layers:
  `provider_diagnostic_notes()` emits only *counts* (never preamble text), the
  promotable transcript never appends `provider_events`, and `diagnostic_notes` are
  routed only to the markdown artifact — `build_sleep_user_prompt`
  (`sleep/session_summary.rs`) consumes `session_text` + `review_notes` only, not
  `diagnostic_notes`. The boundary regression test asserts the preamble text is
  absent from the prompt and the promotable report fields.
- Interrupted / empty-output exchanges render coherent placeholders
  (`(no final transcript recorded)`, `(no completed response)`) and the path is
  covered by a no-panic regression test.
- `latest_sleep_record_completion` now derives `as_of` from the merged records, so a
  voice-only session no longer falls back to `now_utc` for the sleep timestamp — a
  real correctness improvement over the old `turns`-only max.

## Findings

### 1. Diagnostic note label uses chronological position, not `exchange.index` (Minor, observability)

In `session_sleep_input`, the transcript section is labelled with
`exchange.index`:

```rust
transcript.push_str(&format!("\nVoice exchange {}:\n", exchange.index));
```

but the matching diagnostic note is labelled with the merged-list enumerate
position `sleep_index`:

```rust
.map(|note| format!("Voice exchange {}: {note}", sleep_index))
```

These diverge in mixed sessions: realtime voice assigns
`exchange_index = turns.len() + exchanges.len()`, so a transcript "Voice exchange 2"
can carry a diagnostic labelled "Voice exchange 0". That undermines the plan's
stated goal that a sleep run over a voice session be traceable per record. Fix: use
`exchange.index` in the diagnostic label for consistency with the transcript.

### 2. `SleepRecord` accessor methods are largely dead code (Minor, DRY)

`SleepRecord` exposes `user_input_text`, `assistant_output_text`,
`retrieved_memory_block`, `recalled_items`, `interruption_records`,
`provider_events`, and `final_transcript`, but `session_sleep_input` re-matches on
`SleepRecord::Turn` / `SleepRecord::Exchange` and reads the underlying fields
directly, so those accessors are unused. (Clippy does not flag them because they are
`pub` on a `pub` type.) The plan asked for "one chronological representation instead
of duplicating the turns-vs-exchanges branch," and the consumer still branches.
Presentation legitimately differs between turns and voice exchanges, so *some*
branching is justified — but the unused accessors should either become the
consumption surface or be trimmed to what is actually used (`kind`, `source_index`,
`started_at`, `completed_at`, `retrieval_source_ids`, `provider_diagnostic_notes`).

### 3. Phase 7b coverage appears absent from this diff (Question / gap)

The staged work thoroughly covers Phase 7a. The plan's Phase 7b verification asked
for: (a) a routine voice memory candidate auto-promoting as an observation while a
decision/preference-like candidate lands as a reviewed draft, and (b) extending the
voice resume coverage to prove the next voice run resumes from `ConsolidatedBrief`.
`sleep/auto_promote.rs` is untouched and no voice-resume test was added. The
auto-promote logic is source-agnostic (it operates on the summarizer report), so it
likely works once voice content reaches the summarizer — but the plan explicitly
asked for the coverage. Confirm whether 7b is intentionally deferred (and record it
in the plan/diary) or still to land before the phase is closed.

### 4. Safety-net processed-range index space shifted (Risk, verify)

The proposer now bounds coverage and records `ProcessedRange.first/last_turn_index`
over the *chronological position in the merged view* (`0..sleep_records.len()-1`)
rather than turn-index space. This is safe for text-only sessions (position ==
`turn.index`) and for append-only voice (timestamps monotonic, so positions are
stable across runs), and `safety_net_skips_already_processed_ranges` still passes.
Worth one confirming check: that live co-retrieval records its processed ranges in
the *same* index space the sleep view now uses for voice exchanges, so a partially
live-processed mixed session does not get mis-counted as covered/uncovered at sleep
time.

### 5. Duplicated `review_notes` literal (Minor)

`session_sleep_input` builds the same three-line `review_notes` vec in both the
non-empty (early `return`) and empty fall-through paths. Extract to a helper to keep
it DRY.

## Assessment

Phase 7a is solid, verified, and the provider-preamble boundary — the riskiest part
of the plan — is correctly enforced and tested. None of the findings block landing
7a. Recommended before closing the phase: fix finding 1 (cheap, observability), and
resolve finding 3 (decide and record whether Phase 7b is in scope of this commit).
Findings 2, 4, 5 are cleanup/verification follow-ups.
