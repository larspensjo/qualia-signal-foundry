# Transcript CLI Implementation Plan

> **For agentic workers:** implement task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> Plans are ephemeral (`docs/ProjectFrame/ProjectWorkflow.md`): delete this file after the work
> lands and its durable content has moved into the README, the architecture note and the decision
> log. Never cite this plan's phase numbers from durable documents.

**Goal:** `qsf.ps1 transcript` emits a realtime session's conversation as JSONL, each turn joined to
the volition traces that decided which goals fired.

**Architecture:** The diagnostics ledger's schema moves out of the process that writes it into a new
`qsf_diagnostics` crate, so both the realtime server (writer) and `qsf_app` (readers) share one
definition. `qsf_app` gains a `transcript` command beside `goals`, built from pure functions:
records → runs → serialized lines. No I/O in the join logic.

**Tech stack:** Rust 2024 (workspace `rust-version = 1.85`), `clap` derive, `serde`/`serde_json`,
`time` with `serde-well-known`, PowerShell 7.6 launcher, Pester for launcher tests.

**Scope note:** this is a read-only reporting tool. Nothing here changes runtime behavior, so every
phase is ordinary engineering verification and none of it earns an experiment
(`docs/ProjectFrame/ProjectWorkflow.md`). `docs/Experiments/Experiment.GoalMatchingProbeSet.md` is
*separate* work that will use this tool as an instrument once it exists; it is not a gate on this
plan and this plan does not block on it.

`docs/Reviews/Review.TranscriptCli.md` is the external review this plan has been revised against; it
is ephemeral and goes when the plan does.

## Global Constraints

- Build with `cargo build`. On task completion run `cargo clippy --all-targets -- -D warnings` then
  `cargo fmt`.
- Entry points (`main.rs`, `mod.rs`, `lib.rs`) stay thin wrappers only.
- Modules are named after stable domain concepts, never plan phases.
- Reducers and view-derivation stay pure; keep the join logic free of file I/O.
- `use super::*;` is acceptable in tests; explicit imports preferred elsewhere.
- `.gitattributes` pins `eol=lf`. Every committed fixture must be LF-only.
- **The curated transcript record contains no floating point** — counts, tiers, strengths and
  thresholds are all integers. This is a property of the default output only. `--full` embeds traces
  verbatim, and `WorldConsultationTrace.candidates` carries `qsf_corpus::QueryCandidate` whose
  `score` field is `f64`, so a `--full` artifact can contain floats. That exemption is deliberate:
  `--full` exists to reproduce the ledger's own bytes for a turn, and re-encoding a score into
  fixed-point would make it something other than what was recorded. Task 3.1 pins the curated
  guarantee with a test; the README states the exemption. The repo-wide no-float rule in `Agents.md`
  governs artifacts whose hashes must re-derive — the frozen evaluation corpus — and neither
  transcript mode is one of those.
- No new dependency versions: reuse `[workspace.dependencies]` entries via `.workspace = true`.
- The default invocation must exercise the new code path; `-Full` is strictly additive.

## Out of Scope

- The realtime debug UI. It reads records over the websocket, not from the ledger, and nothing here
  changes what it shows.
- What gets written to the ledger. This work adds a reader; no record type gains, loses or renames a
  field, and `DiagnosticWriter` keeps its append-only open mode.
- `session-state.json`. The transcript is derived from the ledger alone, so it can cover runs whose
  session state has since been overwritten.
- The world-corpus trace beyond a curated summary. `--full` carries it verbatim for anyone who needs
  the rest.

## Trace Completeness Contract

Per `docs/ProjectFrame/ProjectWorkflow.md`, this plan reads traces to explain a behavioral chain, so
it declares:

- **Artifact boundary:** `state/realtime/diagnostics/<session-id>.jsonl`, append-only, written by
  `DiagnosticWriter`. The transcript command reads that file and nothing else. It never reads
  `session-state.json`, and never writes to the state directory.
- **Required fields per turn:** user text, assistant text, exchange status, completion time, the
  qualification threshold, every selected goal with its matched keywords and match strength, the
  arbitration winner, the initiative effect and whether it surfaced, and the formation outcome
  (candidate, contradictions, resolution). A turn missing any of these because the ledger has no
  such record renders that section as `null`, never as a silent omission.
- **Source integrity is part of the emitted artifact, not a console warning.** Every run's session
  record carries `source.complete`, the skipped-line count with each line's number and decoded
  `kind`, and orphan-trace counts; every turn carries `undecodable` naming the kinds the ledger held
  for it that this build could not read. Without this a `-Out` file or a redirect could not be told
  apart from a complete one by whoever reads it later, and a skipped volition line would be
  indistinguishable from a genuinely quiet turn. Warnings are still printed to stderr, but nothing
  depends on anyone having seen them.
- **Artifact-parsing verification:** a committed excerpt of a real ledger is parsed by a test
  (Task 3.4), not merely a synthesized fixture, and not merely "the command exited zero". A second
  test writes a deliberately partial ledger through the same path and asserts the emitted artifact
  admits it.

---

## Phase 1 — Extract the diagnostics schema into its own crate

The schema of a persisted artifact currently lives inside the writer. `qsf_app` already reads this
ledger and cannot import the types, so it parses `serde_json::Value` and does string-keyed field
surgery ([volition_continuity.rs:516](../../crates/qsf_app/src/experiments/volition_continuity.rs#L516)),
with a hand-written 1,200-character wire-format fixture in its tests. This phase makes one
definition and leaves the 18 files that reference `crate::diagnostics` untouched, by keeping
re-export facades where the types used to live.

### Task 1.1: Create `qsf_diagnostics` with the volition trace schema

**Files:**
- Create: `crates/qsf_diagnostics/Cargo.toml`
- Create: `crates/qsf_diagnostics/src/lib.rs`
- Create: `crates/qsf_diagnostics/src/volition_injection_trace.rs`
- Create: `crates/qsf_diagnostics/src/live_goal_formation_trace.rs`
- Create: `crates/qsf_diagnostics/src/initiative_trace.rs`
- Create: `crates/qsf_diagnostics/src/turn_phase.rs`
- Modify: `crates/qsf_realtime_server/Cargo.toml` (add the dependency)
- Modify: `crates/qsf_realtime_server/src/realtime/volition_injection.rs` (delete moved types, add re-export)
- Modify: `crates/qsf_realtime_server/src/realtime/volition_injection_text.rs` (delete `AmbientExposure`, add re-export)
- Modify: `crates/qsf_realtime_server/src/realtime/live_goal_formation.rs` (delete moved type, add re-export)
- Modify: `crates/qsf_realtime_server/src/realtime/volition_initiative.rs` (delete moved types, add re-export)
- Modify: `crates/qsf_realtime_server/src/realtime/turn_integrity.rs` (delete `TurnPhase`, add re-export)

**Interfaces:**
- Produces: crate `qsf_diagnostics` exporting `VolitionContextInjectionTrace`,
  `VolitionInjectionLayer`, `VolitionCandidateSummary`, `VolitionSelectedMatchDetail`,
  `VolitionSelectorSummary`, `VolitionModeBiasOutcome`, `VolitionArbitrationSummary`,
  `DeclinedCandidateInjectionRef`, `AmbientExposure`, `LiveGoalFormationTrace`,
  `RealtimeBoundedInitiativeTrace`, `RealtimeBoundedOrExternalOutput`, `TurnPhase` — all with the
  same field names, types, serde attributes and doc comments they have today.

- [ ] **Step 1: Create the manifest**

`crates/qsf_diagnostics/Cargo.toml`:

```toml
[package]
name = "qsf_diagnostics"
version = "0.1.0"
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[dependencies]
anyhow.workspace = true
qsf_corpus = { path = "../qsf_corpus" }
qsf_session = { path = "../qsf_session" }
qsf_volition = { path = "../qsf_volition" }
serde.workspace = true
serde_json.workspace = true
time.workspace = true
```

`crates/*` is already the workspace member glob, so no root `Cargo.toml` change is needed.

- [ ] **Step 2: Write the crate facade**

`crates/qsf_diagnostics/src/lib.rs` — thin wrapper only:

```rust
//! Schema of the realtime diagnostics ledger (`state/realtime/diagnostics/<session>.jsonl`).
//!
//! These types are the persisted wire format, separated from the runtime that emits them so
//! readers (the sleep phase, the transcript command) share one definition instead of
//! re-describing the format by hand.

mod initiative_trace;
pub use initiative_trace::*;

mod live_goal_formation_trace;
pub use live_goal_formation_trace::*;

mod turn_phase;
pub use turn_phase::*;

mod volition_injection_trace;
pub use volition_injection_trace::*;
```

- [ ] **Step 3: Move the volition injection schema types verbatim**

Cut these types from `crates/qsf_realtime_server/src/realtime/volition_injection.rs` and paste them
into `crates/qsf_diagnostics/src/volition_injection_trace.rs`, unchanged including doc comments and
`#[serde(default)]` attributes: `DeclinedCandidateInjectionRef`, `VolitionInjectionLayer`,
`VolitionCandidateSummary`, `VolitionSelectedMatchDetail`, `VolitionSelectorSummary`,
`VolitionModeBiasOutcome`, `VolitionArbitrationSummary`, `VolitionContextInjectionTrace`, and the
private `default_ambient_exposure` helper that `VolitionContextInjectionTrace` names in
`#[serde(default = "default_ambient_exposure")]`.

Cut `AmbientExposure` from `volition_injection_text.rs` into the same new file — the enum only; the
`compute_ambient_exposure` function stays where it is. Make `default_ambient_exposure` **`pub`** when
moving it; the next step explains why.

The new file's imports — note that `VolitionEvent` and `OpportunitySignal` are *not* imported,
because the moved fields spell them `Vec<qsf_volition::VolitionEvent>` and
`Vec<OpportunitySignal>` respectively. Check each moved field as you paste it and import exactly the
names the pasted code actually uses; an unused import fails
`cargo clippy --all-targets -- -D warnings`:

```rust
use serde::{Deserialize, Serialize};

use qsf_volition::{
    ActivationKeyword, DeclineReason, GoalVisibility, Mode, OpportunitySignal, ShapingIntensity,
    ShapingIntensityInputs,
};
```

`OpportunitySignal` is imported because `VolitionContextInjectionTrace` spells that field
`Vec<OpportunitySignal>` unqualified, while `events_applied` is fully qualified. Keep both spellings
as they are rather than normalizing them — this task is a move, and a spelling change here would hide
in the diff.

- [ ] **Step 4: Move the remaining trace types verbatim**

- `live_goal_formation_trace.rs` ← `LiveGoalFormationTrace` from `realtime/live_goal_formation.rs`.
  Imports `qsf_volition::{AdmissionResolution, Contradiction, DeclinedCandidate, VolitionEvent}`,
  `serde::{Deserialize, Serialize}`, `time::OffsetDateTime`.
- `initiative_trace.rs` ← `RealtimeBoundedOrExternalOutput` and `RealtimeBoundedInitiativeTrace`
  from `realtime/volition_initiative.rs`. Imports `qsf_volition::{AllowedEffect, InitiativeOutput,
  InitiativeProposal, VolitionStateInspection, VolitionSuppressionReason}`. Note
  `MAX_RENDERED_INITIATIVE_LINE_CHARS` is a rendering constant and stays behind.
- `turn_phase.rs` ← `TurnPhase` from `realtime/turn_integrity.rs`. `TranscriptDisposition` is
  `pub(crate)` runtime logic and stays behind.

- [ ] **Step 5: Add the re-export facades so no other file changes**

Add `qsf_diagnostics = { path = "../qsf_diagnostics" }` to
`crates/qsf_realtime_server/Cargo.toml` `[dependencies]`, alphabetically before `qsf_memory`.

**`realtime/volition_injection.rs` needs an import *replacement*, not just an addition.** Line 14-16
currently reads:

```rust
pub(crate) use crate::realtime::volition_injection_text::{
    AmbientExposure, compute_ambient_exposure,
};
```

Adding a second `pub use qsf_diagnostics::{AmbientExposure, ...}` alongside it is a duplicate binding
(E0252) and will not compile. Narrow the existing import to the function only, then add the
re-export:

```rust
pub(crate) use crate::realtime::volition_injection_text::compute_ambient_exposure;

pub use qsf_diagnostics::{
    AmbientExposure, DeclinedCandidateInjectionRef, VolitionArbitrationSummary,
    VolitionCandidateSummary, VolitionContextInjectionTrace, VolitionInjectionLayer,
    VolitionModeBiasOutcome, VolitionSelectedMatchDetail, VolitionSelectorSummary,
};
```

**`default_ambient_exposure` has two users, and only one of them moves.**
`VolitionTurnPacketSummary` stays in `volition_injection.rs` and also carries
`#[serde(default = "default_ambient_exposure")]`. Moving the function with the trace would break it,
and duplicating the function would create two sources of truth for one default. Instead, make the
moved function `pub` in `qsf_diagnostics` and point the staying struct's attribute at it by path:

```rust
    #[serde(default = "qsf_diagnostics::default_ambient_exposure")]
    pub ambient_exposure: AmbientExposure,
```

serde accepts any path in `default = "…"`, so this needs no local wrapper.

In `realtime/volition_injection_text.rs` add `pub use qsf_diagnostics::AmbientExposure;` and delete
the enum definition, leaving `compute_ambient_exposure` in place. In
`realtime/live_goal_formation.rs` add `pub use qsf_diagnostics::LiveGoalFormationTrace;`, in
`realtime/volition_initiative.rs` add
`pub use qsf_diagnostics::{RealtimeBoundedInitiativeTrace, RealtimeBoundedOrExternalOutput};`, and
in `realtime/turn_integrity.rs` add `pub use qsf_diagnostics::TurnPhase;`.

Note for `volition_initiative.rs`: the moved `RealtimeBoundedInitiativeTrace` spells one field
`qsf_volition::InitiativeProposal` fully qualified, so `initiative_trace.rs` must **not** import
`InitiativeProposal`. Its import list is `qsf_volition::{AllowedEffect, InitiativeOutput,
VolitionStateInspection, VolitionSuppressionReason}` only.

- [ ] **Step 5a: Compile the facade before going further**

```
cargo check -p qsf_realtime_server
```

Expected: clean. Run this *immediately* after the facade edit, before the tests in Step 6 — a
duplicate binding or an unused import surfaces here in seconds, whereas discovering it after the
whole extraction makes the cause ambiguous. If it fails on an unused import, delete the import; do
not "fix" it by changing a moved field's spelling.

- [ ] **Step 6: Write the round-trip test**

Append to `crates/qsf_diagnostics/src/volition_injection_trace.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_match_detail_round_trips_through_json() {
        let detail = VolitionSelectedMatchDetail {
            goal_id: "grow-the-library".to_string(),
            matched_keywords: vec![
                ActivationKeyword::normal("remember"),
                ActivationKeyword::weak("earlier"),
            ],
            match_strength: 5,
            visibility: GoalVisibility::Conscious,
        };

        let json = serde_json::to_string(&detail).expect("serialize");
        let parsed: VolitionSelectedMatchDetail = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed, detail);
    }

    #[test]
    fn selected_match_detail_defaults_visibility_for_older_traces() {
        let parsed: VolitionSelectedMatchDetail = serde_json::from_str(
            r#"{"goal_id":"g","matched_keywords":[],"match_strength":0}"#,
        )
        .expect("deserialize without visibility");

        assert_eq!(parsed.visibility, GoalVisibility::Conscious);
    }
}
```

- [ ] **Step 7: Verify**

```
cargo build
cargo test -p qsf_diagnostics
cargo test -p qsf_realtime_server
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expected: `qsf_realtime_server`'s suite passes with no test edits — this phase is a pure move, and
any failure means a definition changed while being moved.

- [ ] **Step 8: Commit**

```bash
git add crates/qsf_diagnostics crates/qsf_realtime_server
git commit -m "Diagnostics: Extract the volition trace schema into qsf_diagnostics"
```

### Task 1.2: Move the world-consultation schema types

These are `pub(crate)` today and must become `pub`. That widening is correct: they are the persisted
wire format of an external-effect boundary, not crate internals.

**Files:**
- Create: `crates/qsf_diagnostics/src/world_consultation_trace.rs`
- Modify: `crates/qsf_diagnostics/src/lib.rs`
- Modify: `crates/qsf_realtime_server/src/realtime/world_consultation.rs`

**Interfaces:**
- Consumes: the `qsf_diagnostics` crate from Task 1.1.
- Produces: `WorldConsultationTrace`, `WorldConsultationCandidate`, `SurfacedWorldFact`,
  `CorpusMarkerMetadata`, `WorldEffectBoundary`, `TopicTermMajorityThreshold`,
  `WorldInjectionPoint`, `CandidateEligibility` — every field `pub`.

- [ ] **Step 1: Move the types**

Cut from `realtime/world_consultation.rs` into `crates/qsf_diagnostics/src/world_consultation_trace.rs`,
rewriting `pub(crate)` to `pub` on each type and each field, and changing nothing else — no field
renames, no serde attribute changes: `WorldInjectionPoint`, `CandidateEligibility`,
`WorldConsultationCandidate`, `SurfacedWorldFact`, `CorpusMarkerMetadata`, `WorldEffectBoundary`,
`TopicTermMajorityThreshold`, `WorldConsultationTrace`.

`WorldQueryOrigin` and `WorldConsultationTrigger` are **not** reachable from the trace; leave them
in `world_consultation.rs` as `pub(crate)`.

Imports for the new file:

```rust
use serde::{Deserialize, Serialize};

use qsf_corpus::QueryCandidate;
use qsf_volition::{InitiativeOutput, WorldQueryTerm};
```

`WorldConsultationCandidate` keeps its `#[serde(flatten)] pub candidate: QueryCandidate`, which is
why the crate depends on `qsf_corpus`.

- [ ] **Step 2: Register the module**

Add to `crates/qsf_diagnostics/src/lib.rs`, keeping the modules alphabetical:

```rust
mod world_consultation_trace;
pub use world_consultation_trace::*;
```

- [ ] **Step 3: Add the re-export facade**

In `realtime/world_consultation.rs`, replace the deleted definitions with:

```rust
pub use qsf_diagnostics::{
    CandidateEligibility, CorpusMarkerMetadata, SurfacedWorldFact, TopicTermMajorityThreshold,
    WorldConsultationCandidate, WorldConsultationTrace, WorldEffectBoundary, WorldInjectionPoint,
};
```

- [ ] **Step 4: Verify**

```
cargo build
cargo test -p qsf_realtime_server
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expected: passes with no test edits. The existing test at
`crates/qsf_realtime_server/src/realtime/world_consultation.rs:999` already writes and re-parses a
`WorldConsultationPerformed` record, so it covers this move directly.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_diagnostics crates/qsf_realtime_server
git commit -m "Diagnostics: Move the world-consultation trace schema into qsf_diagnostics"
```

### Task 1.3: Move the record enum and the writer

**Files:**
- Create: `crates/qsf_diagnostics/src/record.rs`
- Create: `crates/qsf_diagnostics/src/writer.rs`
- Modify: `crates/qsf_diagnostics/src/lib.rs`
- Modify: `crates/qsf_realtime_server/src/diagnostics.rs` (becomes a facade)
- Modify: `docs/DecisionLog.md`
- Modify: `docs/Architecture/Architecture.RealtimeSessionServer.md`

**Interfaces:**
- Produces: `qsf_diagnostics::{DiagnosticRecord, DiagnosticTrust, DiagnosticWriter}`.
  `DiagnosticRecord` keeps `#[serde(tag = "kind", rename_all = "snake_case")]` and every existing
  variant name and field. `DiagnosticWriter::create(path) -> anyhow::Result<Self>` and
  `DiagnosticWriter::write(&DiagnosticRecord) -> anyhow::Result<()>` keep their signatures.

- [ ] **Step 1: Move `DiagnosticRecord` and `DiagnosticTrust` into `record.rs`**

Move verbatim from `crates/qsf_realtime_server/src/diagnostics.rs`. Imports:

```rust
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use qsf_session::Exchange;

use crate::{
    LiveGoalFormationTrace, RealtimeBoundedInitiativeTrace, TurnPhase,
    VolitionContextInjectionTrace, WorldConsultationTrace,
};
```

- [ ] **Step 2: Move `DiagnosticWriter` into `writer.rs`**

Move verbatim, including the `Mutex<BufWriter<File>>` field and the
`OpenOptions::new().create(true).append(true)` open mode — append-only is a property of the
artifact and must not change.

- [ ] **Step 3: Register both modules**

```rust
mod record;
pub use record::*;

mod writer;
pub use writer::*;
```

- [ ] **Step 3a: Add the minimal record envelope**

Both readers need to inspect a line without committing to parsing every variant: the sleep reader so
an unrelated or unknown record cannot abort it, and the transcript loader so a line it cannot decode
can still be attributed to a run and a turn. One helper serves both, in `record.rs`:

```rust
/// The fields every reader can rely on without deserializing a whole record. Used to dispatch on
/// `kind` before committing to a variant, and to attribute an undecodable line to a run and turn.
/// Every field is optional: this must succeed on any syntactically valid JSON object, including
/// records written by a build that this one does not know.
#[derive(Clone, Debug, Deserialize)]
pub struct RecordEnvelope {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub qsf_session_id: Option<String>,
    #[serde(default)]
    pub exchange_index: Option<usize>,
}

/// The `kind` tag of `DiagnosticRecord::RealtimeBoundedInitiative`, as serde writes it. Named so
/// readers dispatching on the tag cannot drift from the enum's `rename_all = "snake_case"`.
pub const REALTIME_BOUNDED_INITIATIVE_KIND: &str = "realtime_bounded_initiative";

/// Decodes only the envelope. `None` when the line is not a JSON object at all.
pub fn decode_envelope(line: &str) -> Option<RecordEnvelope> {
    serde_json::from_str::<RecordEnvelope>(line).ok()
}
```

Test it in the same module:

```rust
    #[test]
    fn the_envelope_decodes_a_record_kind_this_build_does_not_know() {
        let envelope = decode_envelope(
            r#"{"kind":"from_a_future_build","qsf_session_id":"s","exchange_index":4,"extra":{}}"#,
        )
        .expect("an unknown kind still decodes as an envelope");

        assert_eq!(envelope.kind.as_deref(), Some("from_a_future_build"));
        assert_eq!(envelope.qsf_session_id.as_deref(), Some("s"));
        assert_eq!(envelope.exchange_index, Some(4));
    }

    #[test]
    fn the_initiative_kind_constant_matches_what_serde_writes() {
        let record = DiagnosticRecord::LiveGoalFormationSkipped {
            qsf_session_id: "s".to_string(),
            exchange_index: 0,
            recorded_at: OffsetDateTime::UNIX_EPOCH,
            reason: "guard".to_string(),
        };
        // Guards the tag-naming convention the constant depends on.
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(json.contains(r#""kind":"live_goal_formation_skipped""#));
        assert_eq!(
            REALTIME_BOUNDED_INITIATIVE_KIND,
            "realtime_bounded_initiative"
        );
    }

    #[test]
    fn a_non_object_line_has_no_envelope() {
        assert!(decode_envelope("not json").is_none());
        assert!(decode_envelope("[1,2,3]").is_none());
    }
```

- [ ] **Step 4: Reduce `qsf_realtime_server/src/diagnostics.rs` to a facade**

Replace the whole file with:

```rust
//! Facade over the persisted diagnostics schema, which lives in `qsf_diagnostics` so readers
//! outside this crate share one definition. Kept as a module path because the server's write
//! sites refer to `crate::diagnostics::*`.

pub use qsf_diagnostics::{DiagnosticRecord, DiagnosticTrust, DiagnosticWriter};
```

None of the 18 files that reference `crate::diagnostics` need editing. Any test that lived in the
old `diagnostics.rs` moves with the type it exercises.

- [ ] **Step 5: Write the variant round-trip test**

Append to `crates/qsf_diagnostics/src/record.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_allocated_round_trips_with_its_kind_tag() {
        let record = DiagnosticRecord::SessionAllocated {
            qsf_session_id: "default".to_string(),
            at: OffsetDateTime::UNIX_EPOCH,
        };

        let json = serde_json::to_string(&record).expect("serialize");
        assert!(json.contains(r#""kind":"session_allocated""#));

        let parsed: DiagnosticRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, record);
    }

    #[test]
    fn unknown_kind_is_rejected_rather_than_silently_accepted() {
        let result = serde_json::from_str::<DiagnosticRecord>(r#"{"kind":"not_a_record"}"#);
        assert!(result.is_err(), "unknown record kinds must fail to parse");
    }
}
```

`DiagnosticRecord` cannot derive `PartialEq`: `WorldConsultationTrace` and the `QueryCandidate` it
flattens do not implement it, and adding `PartialEq` to types in other crates is out of scope here.
So the first test asserts on the round-tripped JSON rather than on the value:

```rust
        let parsed: DiagnosticRecord = serde_json::from_str(&json).expect("deserialize");
        let reserialized = serde_json::to_string(&parsed).expect("reserialize");
        assert_eq!(reserialized, json);
```

Replace the `assert_eq!(parsed, record);` line above with those two lines.

- [ ] **Step 6: Record the decision**

Add to `docs/DecisionLog.md`, following the file's existing entry format, dated 2026-07-27:

> **The persisted diagnostics schema owns its own crate.** `DiagnosticRecord`, `DiagnosticWriter`
> and the trace types they carry moved from `qsf_realtime_server` into `qsf_diagnostics`. The
> ledger has readers outside the process that writes it — the sleep phase's initiative outcomes and
> the transcript command — and while the schema lived inside the writer each reader re-described the
> wire format by hand, one of them through untyped `serde_json::Value` field surgery. A persisted
> format with more than one consumer is a contract, and a contract belongs in a crate whose purpose
> is that contract.

In `docs/Architecture/Architecture.RealtimeSessionServer.md`, in the section that describes the
diagnostics ledger, name `qsf_diagnostics` as the owner of the record schema and note that
`qsf_realtime_server::diagnostics` is a re-export facade.

- [ ] **Step 7: Verify**

```
cargo build
cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt
```

- [ ] **Step 8: Commit**

```bash
git add crates/qsf_diagnostics crates/qsf_realtime_server docs/DecisionLog.md docs/Architecture/Architecture.RealtimeSessionServer.md
git commit -m "Diagnostics: Move the record enum and writer into qsf_diagnostics"
```

---

## Phase 2 — Retire the untyped ledger reader

This is the payoff for Phase 1. Skipping it would leave a hand-maintained mirror of a format we now
have one definition for.

### Task 2.1: Parse `DiagnosticRecord` in the sleep-phase reader

**Files:**
- Modify: `crates/qsf_app/Cargo.toml`
- Modify: `crates/qsf_app/src/experiments/volition_continuity.rs:509-560` (`load_initiative_outcomes`)
- Modify: `crates/qsf_app/src/experiments/volition_continuity.rs:793-812` (the fixture helper)

**Interfaces:**
- Consumes: `qsf_diagnostics::{DiagnosticRecord, RealtimeBoundedInitiativeTrace}`.
- Produces: `load_initiative_outcomes(path: &Path) -> anyhow::Result<Vec<VolitionTurnOutcome>>` —
  same signature, same behavior, typed internals.

- [ ] **Step 1: Add the dependency**

In `crates/qsf_app/Cargo.toml`, add `qsf_diagnostics = { path = "../qsf_diagnostics" }` to
`[dependencies]`, alphabetically before `qsf_memory`.

- [ ] **Step 2: Rewrite the fixture helper to serialize a real record**

Replace `minimal_initiative_jsonl_line` and its 1,200-character format string with construction of
the real type. The old comment explaining that the type "cannot be imported here" is now false and
must be deleted.

```rust
    /// Serializes a real `DiagnosticRecord::RealtimeBoundedInitiative` so the test exercises the
    /// same wire format the realtime server writes.
    fn minimal_initiative_jsonl_line(
        suppression_reason: Option<VolitionSuppressionReason>,
        surfaced: bool,
        artifact_or_record_reference: &str,
        recorded_at: OffsetDateTime,
    ) -> String {
        let output = InitiativeOutput::ReflectionRequested {
            proposed_question: "What next?".to_string(),
        };
        let inspection = build_state_inspection(
            &VolitionState::from_fixture(&realtime_seed_fixture()),
            &realtime_seed_fixture(),
        );
        let record = DiagnosticRecord::RealtimeBoundedInitiative {
            qsf_session_id: "session-1".to_string(),
            exchange_index: 3,
            recorded_at,
            trace: RealtimeBoundedInitiativeTrace {
                qsf_session_id: "session-1".to_string(),
                exchange_index: 3,
                winning_goal_id: "serve-the-present-person".to_string(),
                initiative_proposal: InitiativeProposal {
                    goal_id: "serve-the-present-person".to_string(),
                    goal_title: "Serve".to_string(),
                    effect: AllowedEffect::Reflect,
                    rationale: "test".to_string(),
                    matched_terms: vec![],
                    scope: GoalScope::Input,
                },
                allowed_effect: AllowedEffect::Reflect,
                initiative_output: output.clone(),
                bounded_or_external_output: RealtimeBoundedOrExternalOutput {
                    initiative_output: output,
                    external_effect_executed: false,
                },
                surfaced,
                suppression_reason,
                rendered_line_present: surfaced,
                context_retrieval_hint_terms: None,
                hint_consumed_by_next_memory_injection: false,
                rationale: "test".to_string(),
                state_snapshot_before: inspection.clone(),
                state_snapshot_after: inspection,
                response_create_event_ref: "ref-1".to_string(),
                artifact_or_record_reference: artifact_or_record_reference.to_string(),
            },
        };
        serde_json::to_string(&record).expect("serialize initiative record")
    }
```

Update the call sites in that test module to pass the typed `suppression_reason` and an
`OffsetDateTime` instead of strings.

The test module needs these imports added:

```rust
use time::OffsetDateTime;

use qsf_diagnostics::{DiagnosticRecord, RealtimeBoundedInitiativeTrace, RealtimeBoundedOrExternalOutput};
use qsf_volition::{
    AllowedEffect, GoalScope, InitiativeOutput, InitiativeProposal, VolitionState,
    VolitionSuppressionReason, build_state_inspection, realtime_seed_fixture,
};
```

`build_state_inspection(&VolitionState, &VolitionFixture) -> VolitionStateInspection` and
`InitiativeOutput::ReflectionRequested { proposed_question }` are both confirmed against
`qsf_volition`; the variant name matches the `"kind":"reflection_requested"` tag the old hand-written
fixture used.

- [ ] **Step 3: Run the tests to confirm they still pass against the serialized form**

```
cargo test -p qsf_app volition_continuity
```

Expected: PASS. This proves the hand-written string and the real serializer agreed, before anything
about the reader changes.

- [ ] **Step 4: Rewrite the reader with envelope-first dispatch**

The reader must keep the tolerance the old untyped version had. The diagnostics ledger is
append-only and outlives builds — the live `state/realtime/diagnostics/default.jsonl` already holds
eleven `session_allocated` records spanning weeks — so an unrelated record kind that this build
cannot parse must not abort a sleep update whose initiative records are all valid. Deserializing
every line into `DiagnosticRecord` before filtering would do exactly that.

So dispatch on the envelope first and pay for full deserialization only on the lines that matter:

```rust
/// Parse `VolitionTurnOutcome` records from a diagnostics JSONL file.
///
/// Dispatches on `RecordEnvelope::kind` and deserializes only initiative records. The ledger is
/// append-only across builds, so a line of some other kind that this build cannot parse — an
/// unknown kind, or a known kind in an older shape — must not fail the read: none of it is input to
/// sleep. A malformed *initiative* record is a different matter and still fails, with its line
/// number, because that one is input.
///
/// `recorded_at` lives at the record level rather than inside the trace, and the trace names its
/// reference field `artifact_or_record_reference`, so this function maps the two shapes explicitly.
pub(crate) fn load_initiative_outcomes(path: &Path) -> anyhow::Result<Vec<VolitionTurnOutcome>> {
    if !path.exists() {
        return Ok(vec![]);
    }

    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read diagnostics JSONL `{}`", path.display()))?;
    let mut outcomes = Vec::new();
    for (line_index, line) in contents.lines().enumerate() {
        let Some(envelope) = decode_envelope(line) else {
            continue;
        };
        if envelope.kind.as_deref() != Some(REALTIME_BOUNDED_INITIATIVE_KIND) {
            continue;
        }
        let record: DiagnosticRecord = serde_json::from_str(line).with_context(|| {
            format!(
                "failed to parse initiative record on line {} of `{}`",
                line_index + 1,
                path.display()
            )
        })?;
        let DiagnosticRecord::RealtimeBoundedInitiative {
            recorded_at, trace, ..
        } = record
        else {
            anyhow::bail!(
                "line {} of `{}` carries kind `{REALTIME_BOUNDED_INITIATIVE_KIND}` but did not \
                 deserialize as that variant",
                line_index + 1,
                path.display()
            );
        };
        outcomes.push(VolitionTurnOutcome {
            qsf_session_id: trace.qsf_session_id,
            exchange_index: trace.exchange_index,
            recorded_at: recorded_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            response_create_event_ref: trace.response_create_event_ref,
            winning_goal_id: trace.winning_goal_id,
            initiative_output: trace.initiative_output,
            surfaced: trace.surfaced,
            suppression_reason: trace.suppression_reason,
            rendered_line_present: trace.rendered_line_present,
            artifact_reference: trace.artifact_or_record_reference,
        });
    }
    Ok(outcomes)
}
```

Add `use qsf_diagnostics::{DiagnosticRecord, REALTIME_BOUNDED_INITIATIVE_KIND, decode_envelope};` to
the module's imports.

- [ ] **Step 5: Add the compatibility regression tests**

These pin the tolerance the envelope dispatch exists to preserve. Without them a later refactor back
to whole-record parsing would look harmless.

```rust
    #[test]
    fn unrelated_and_unknown_record_kinds_do_not_prevent_reading_initiative_outcomes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let initiative = minimal_initiative_jsonl_line(
            None,
            true,
            "artifact-1",
            OffsetDateTime::UNIX_EPOCH,
        );
        std::fs::write(
            &path,
            format!(
                // A kind from a future build, a known kind in an older shape (missing fields this
                // build requires), and then a valid initiative record.
                "{{\"kind\":\"from_a_future_build\",\"anything\":true}}\n\
                 {{\"kind\":\"live_goal_formation_skipped\",\"qsf_session_id\":\"s\"}}\n\
                 {initiative}\n"
            ),
        )
        .unwrap();

        let outcomes = load_initiative_outcomes(&path).expect("unrelated kinds must not abort");

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].artifact_reference, "artifact-1");
    }

    #[test]
    fn a_malformed_initiative_record_still_fails_with_its_line_number() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        std::fs::write(
            &path,
            "{\"kind\":\"from_a_future_build\"}\n\
             {\"kind\":\"realtime_bounded_initiative\",\"trace\":{\"nope\":true}}\n",
        )
        .unwrap();

        let error = load_initiative_outcomes(&path).expect_err("a bad initiative record must fail");

        assert!(
            error.to_string().contains("line 2"),
            "error must name the failing line: {error}"
        );
    }
```

- [ ] **Step 6: Run the tests**

```
cargo test -p qsf_app volition_continuity
cargo clippy --all-targets -- -D warnings
cargo fmt
```

- [ ] **Step 7: Commit**

```bash
git add crates/qsf_app
git commit -m "Volition continuity: Read initiative outcomes through the typed diagnostics record"
```

---

## Phase 3 — The transcript model, join and command

### Task 3.1: The serialized view types

**Files:**
- Create: `crates/qsf_app/src/transcript/mod.rs`
- Create: `crates/qsf_app/src/transcript/model.rs`
- Modify: `crates/qsf_app/src/lib.rs` (register the module)

**Interfaces:**
- Produces: every type below, all `Serialize`-only view models.

- [ ] **Step 1: Write the thin module wrapper**

`crates/qsf_app/src/transcript/mod.rs`:

```rust
mod join;
pub use join::*;

mod ledger;
pub use ledger::*;

mod model;
pub use model::*;

mod render;
pub use render::*;
```

Create `join.rs`, `ledger.rs` and `render.rs` as empty files for now so the module compiles; they
are filled in by Tasks 3.2–3.4. In `crates/qsf_app/src/lib.rs`, add `pub mod transcript;` between
`pub mod tools;` and `pub mod volition;` — the file's module list is alphabetical. It must be `pub`:
Task 3.4 adds an integration test that imports `qsf_app::transcript`.

- [ ] **Step 2: Write the view model**

`crates/qsf_app/src/transcript/model.rs`:

```rust
use serde::Serialize;

use qsf_diagnostics::{
    LiveGoalFormationTrace, RealtimeBoundedInitiativeTrace, VolitionContextInjectionTrace,
    WorldConsultationTrace,
};
use qsf_session::Exchange;
use qsf_volition::{
    ActivationKeyword, AdmissionResolution, AllowedEffect, Contradiction, DeclinedCandidate,
    GoalVisibility, InitiativeOutput, KeywordWeightClass, Mode, VolitionSuppressionReason,
    WorldQueryTerm,
};

/// One emitted JSONL line. The `kind` tag makes the stream self-describing, so a reader can tell a
/// run header from a turn without positional assumptions.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptLine {
    Session(SessionLine),
    Turn(TurnLine),
}

#[derive(Debug, Serialize)]
pub struct SessionLine {
    pub session_id: String,
    pub ledger: String,
    /// 1-based position of this run within the append-only ledger.
    pub run_index: usize,
    pub run_started_at: Option<String>,
    pub turn_count: usize,
    /// Whether this run was read completely. Part of the serialized contract, not just a console
    /// warning: a saved artifact must carry its own provenance, because whoever reads the file later
    /// did not see the invocation's stderr.
    pub source: SourceIntegrity,
}

#[derive(Debug, Default, Serialize)]
pub struct SourceIntegrity {
    /// `true` when no line of this run was skipped and no trace was orphaned.
    pub complete: bool,
    pub skipped_line_count: usize,
    pub skipped_lines: Vec<SkippedLineView>,
    pub orphans: OrphanCounts,
}

/// A ledger line this build could not decode, located well enough to go back to the source.
#[derive(Debug, Serialize)]
pub struct SkippedLineView {
    pub line_number: usize,
    /// The record's `kind`, when the envelope decoded. `null` when the line was not a JSON object.
    pub kind: Option<String>,
    /// The exchange index the line belonged to, when the envelope decoded. This is what lets a
    /// specific turn be marked incomplete rather than silently losing a section.
    pub exchange_index: Option<usize>,
    pub error: String,
}

/// Traces whose `exchange_index` matched no trusted exchange in the run.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct OrphanCounts {
    pub injection: usize,
    pub formation: usize,
    pub initiative: usize,
    pub world: usize,
    pub turn_context: usize,
}

impl OrphanCounts {
    pub fn total(&self) -> usize {
        self.injection + self.formation + self.initiative + self.world + self.turn_context
    }
}

/// Optional sections are always present as `null` when the ledger has no such record for the turn,
/// so keys are stable for downstream tooling and an absent trace is visible rather than implied.
#[derive(Debug, Serialize)]
pub struct TurnLine {
    pub turn: usize,
    pub at: Option<String>,
    pub user: String,
    pub assistant: Option<String>,
    pub status: String,
    pub volition: Option<VolitionView>,
    pub initiative: Option<InitiativeView>,
    pub formation: Option<FormationView>,
    pub world: Option<WorldView>,
    /// Record kinds that the ledger holds for this turn but this build could not decode. This is
    /// what separates "the ledger never recorded a volition trace for this turn" (`volition: null`,
    /// `undecodable: []`) from "it did, and we could not read it" (`volition: null`,
    /// `undecodable: ["volition_context_injected"]`). Without it a skipped line and a genuinely
    /// quiet turn are indistinguishable in the artifact.
    pub undecodable: Vec<String>,
    /// Present only under `--full`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traces: Option<TraceBundle>,
}

#[derive(Debug, Serialize)]
pub struct VolitionView {
    pub threshold: u32,
    pub mode: Option<Mode>,
    pub winner: Option<WinnerView>,
    /// Selected goals whose match strength reached `threshold`.
    pub fired: Vec<MatchView>,
    /// Selected goals that matched but stayed under `threshold`.
    pub below_threshold: Vec<MatchView>,
    pub omitted_count: usize,
    pub suppressed_cooldown_count: usize,
    pub blocked_count: usize,
    pub subconscious_selected_count: usize,
}

#[derive(Debug, Serialize)]
pub struct WinnerView {
    pub goal: String,
    pub title: String,
    pub effective_tier: u8,
    pub biased_tier: u8,
    pub losers: usize,
}

#[derive(Debug, Serialize)]
pub struct MatchView {
    pub goal: String,
    pub strength: u32,
    /// Rendered as `term:weight_class`, the one place this view compresses rather than nests,
    /// because this list is what a reader actually reads.
    pub keywords: Vec<String>,
    pub visibility: GoalVisibility,
    /// Populated from the trace's below-threshold candidate summary when present.
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InitiativeView {
    pub goal: String,
    pub effect: AllowedEffect,
    pub surfaced: bool,
    pub suppression: Option<VolitionSuppressionReason>,
    pub rendered_line_present: bool,
    pub output: InitiativeOutput,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationStatus {
    Performed,
    Failed,
    Skipped,
}

#[derive(Debug, Serialize)]
pub struct FormationView {
    pub status: FormationStatus,
    pub candidate_id: Option<String>,
    pub candidate_title: Option<String>,
    pub contradictions: Vec<Contradiction>,
    pub resolution: Option<AdmissionResolution>,
    pub declined: Option<DeclinedCandidate>,
    /// The error text for `Failed`, the guard reason for `Skipped`, `None` for `Performed`.
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorldView {
    pub serving_goal: String,
    pub serving_goal_title: String,
    /// Carried whole rather than flattened to strings: `WorldQueryTerm` is two fields and its
    /// `source` says whether the term came from goal activation or an explicit current topic.
    pub query_terms: Vec<WorldQueryTerm>,
    pub surfaced_facts: Vec<SurfacedFactView>,
    pub injection_reason: String,
    pub injected_chars: usize,
}

/// The framed text of a surfaced fact is deliberately excluded: it is large and it is already in
/// the ledger. `--full` carries the whole trace for anyone who needs it.
#[derive(Debug, Serialize)]
pub struct SurfacedFactView {
    pub title: String,
    pub url: String,
    pub source_domain: String,
    pub trust_tier: String,
}

#[derive(Debug, Serialize)]
pub struct TraceBundle {
    pub injection: Option<VolitionContextInjectionTrace>,
    pub formation: Option<LiveGoalFormationTrace>,
    pub initiative: Option<RealtimeBoundedInitiativeTrace>,
    pub world: Option<WorldConsultationTrace>,
    pub turn_context: Option<TurnContextView>,
    pub exchange: Exchange,
}

#[derive(Debug, Serialize)]
pub struct TurnContextView {
    pub request_hash: String,
    pub messages: Vec<serde_json::Value>,
}

/// Renders one activation keyword as `term:weight_class`.
pub fn render_keyword(keyword: &ActivationKeyword) -> String {
    let class = match keyword.weight_class {
        KeywordWeightClass::Weak => "weak",
        KeywordWeightClass::Normal => "normal",
        KeywordWeightClass::Strong => "strong",
    };
    format!("{}:{}", keyword.term, class)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_rendering_pairs_term_with_weight_class() {
        assert_eq!(
            render_keyword(&ActivationKeyword::strong("evidence")),
            "evidence:strong"
        );
        assert_eq!(render_keyword(&ActivationKeyword::weak("i")), "i:weak");
    }

    #[test]
    fn absent_sections_serialize_as_null_so_keys_stay_stable() {
        let line = TranscriptLine::Turn(TurnLine {
            turn: 0,
            at: None,
            user: "hello".to_string(),
            assistant: None,
            status: "completed".to_string(),
            volition: None,
            initiative: None,
            formation: None,
            world: None,
            undecodable: vec![],
            traces: None,
        });

        let json = serde_json::to_string(&line).expect("serialize");

        assert!(json.contains(r#""kind":"turn""#));
        assert!(json.contains(r#""volition":null"#));
        assert!(json.contains(r#""formation":null"#));
        assert!(
            !json.contains("traces"),
            "traces must be omitted entirely outside --full"
        );
    }

    #[test]
    fn the_curated_view_serializes_no_floating_point() {
        // Pins the integer-only guarantee for the default output. `--full` is exempt by design; see
        // Global Constraints.
        let line = TranscriptLine::Turn(TurnLine {
            turn: 0,
            at: None,
            user: "hello".to_string(),
            assistant: None,
            status: "completed".to_string(),
            volition: Some(VolitionView {
                threshold: 4,
                mode: Some(Mode::Neutral),
                winner: Some(WinnerView {
                    goal: "g".to_string(),
                    title: "G".to_string(),
                    effective_tier: 1,
                    biased_tier: 1,
                    losers: 2,
                }),
                fired: vec![MatchView {
                    goal: "g".to_string(),
                    strength: 9,
                    keywords: vec!["remember:normal".to_string()],
                    visibility: GoalVisibility::Conscious,
                    reason: None,
                }],
                below_threshold: vec![],
                omitted_count: 3,
                suppressed_cooldown_count: 0,
                blocked_count: 0,
                subconscious_selected_count: 0,
            }),
            initiative: None,
            formation: None,
            world: None,
            undecodable: vec![],
            traces: None,
        });

        let value = serde_json::to_value(&line).expect("serialize");
        assert!(
            !contains_float(&value),
            "the curated view must not emit floating point: {value}"
        );
    }

    fn contains_float(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Number(number) => number.is_f64(),
            serde_json::Value::Array(items) => items.iter().any(contains_float),
            serde_json::Value::Object(fields) => fields.values().any(contains_float),
            _ => false,
        }
    }

    #[test]
    fn a_complete_run_says_so_in_the_serialized_header() {
        let line = TranscriptLine::Session(SessionLine {
            session_id: "s".to_string(),
            ledger: "ledger.jsonl".to_string(),
            run_index: 1,
            run_started_at: None,
            turn_count: 1,
            source: SourceIntegrity {
                complete: true,
                skipped_line_count: 0,
                skipped_lines: vec![],
                orphans: OrphanCounts::default(),
            },
        });

        let json = serde_json::to_string(&line).expect("serialize");

        assert!(json.contains(r#""complete":true"#));
        assert!(json.contains(r#""skipped_line_count":0"#));
    }
}
```

- [ ] **Step 3: Verify**

```
cargo test -p qsf_app transcript::model
cargo clippy --all-targets -- -D warnings
cargo fmt
```

- [ ] **Step 4: Commit**

```bash
git add crates/qsf_app
git commit -m "Transcript: Add the JSONL view model for realtime session turns"
```

### Task 3.2: The pure join

**Files:**
- Modify: `crates/qsf_app/src/transcript/join.rs`

**Interfaces:**
- Consumes: `TranscriptLine`, `TurnLine`, `SessionLine`, `SourceIntegrity`, `SkippedLineView`,
  `OrphanCounts`, `VolitionView`, `MatchView`, `InitiativeView`, `FormationView`, `WorldView`,
  `TraceBundle`, `render_keyword` from Task 3.1.
- Produces:
  - `pub enum LedgerEntry { Record(Box<DiagnosticRecord>), Skipped(SkippedLineView) }` — file order
    preserved, so a line this build could not decode is attributed to the run it fell inside
  - `pub struct TranscriptRun { pub header: SessionLine, pub turns: Vec<TurnLine> }` — integrity now
    lives in `header.source`, not beside it, so it cannot be computed and then left out of the output
  - `pub fn runs_from_entries(entries: Vec<LedgerEntry>, ledger: &str, full: bool) -> Vec<TranscriptRun>`

`OrphanCounts` is defined in `model.rs` (Task 3.1), not here: it is part of the serialized shape, and
`SessionLine` refers to it. `join.rs` computes it.

- [ ] **Step 1: Write the failing tests**

Append to `crates/qsf_app/src/transcript/join.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use qsf_diagnostics::{
        AmbientExposure, DiagnosticTrust, VolitionSelectedMatchDetail, VolitionSelectorSummary,
    };
    use qsf_session::{Exchange, ExchangeInput, ExchangeOutput, ExchangeStatus};
    use qsf_volition::{GoalVisibility, ShapingIntensity};

    use super::*;

    /// Test shim: the production entry point takes `LedgerEntry` values so an undecodable line keeps
    /// its place in file order. Most of these tests are about joining records, so they wrap plain
    /// records and keep the older, narrower call shape.
    fn runs_from_records(
        records: Vec<DiagnosticRecord>,
        ledger: &str,
        full: bool,
    ) -> Vec<TranscriptRun> {
        let entries = records
            .into_iter()
            .map(|record| LedgerEntry::Record(Box::new(record)))
            .collect();
        runs_from_entries(entries, ledger, full)
    }

    fn skipped(line_number: usize, kind: Option<&str>, exchange_index: Option<usize>) -> LedgerEntry {
        LedgerEntry::Skipped(SkippedLineView {
            line_number,
            kind: kind.map(str::to_string),
            exchange_index,
            error: "unknown variant".to_string(),
        })
    }

    fn trusted_exchange(index: usize, user: &str, assistant: &str) -> DiagnosticRecord {
        let started = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        DiagnosticRecord::DiagnosticExchangeRecorded {
            qsf_session_id: "s".to_string(),
            source: "sideband".to_string(),
            trust: DiagnosticTrust::Trusted,
            recorded_at: OffsetDateTime::UNIX_EPOCH,
            exchange: Exchange {
                index,
                started_at: started,
                completed_at: Some(started + Duration::from_secs(2)),
                input: ExchangeInput::Voice {
                    final_transcript: user.to_string(),
                    utterances: vec![],
                },
                output: Some(ExchangeOutput {
                    response_id: None,
                    text: assistant.to_string(),
                    produced_at: started + Duration::from_secs(2),
                    provider_name: None,
                    target: None,
                    audio_marker: None,
                }),
                context_assembly: None,
                retrieved_memory_block: String::new(),
                recalled_items: vec![],
                model: None,
                interruptions: vec![],
                provider_events: vec![],
                tool_requests: vec![],
                tool_executions: vec![],
                status: ExchangeStatus::Completed,
            },
        }
    }

    fn untrusted_exchange(index: usize, user: &str) -> DiagnosticRecord {
        let DiagnosticRecord::DiagnosticExchangeRecorded { exchange, .. } =
            trusted_exchange(index, user, "")
        else {
            unreachable!("constructor returns an exchange record")
        };
        DiagnosticRecord::DiagnosticExchangeRecorded {
            qsf_session_id: "s".to_string(),
            source: "browser_relay".to_string(),
            trust: DiagnosticTrust::Untrusted,
            recorded_at: OffsetDateTime::UNIX_EPOCH,
            exchange,
        }
    }

    fn allocated(session: &str) -> DiagnosticRecord {
        DiagnosticRecord::SessionAllocated {
            qsf_session_id: session.to_string(),
            at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn untrusted_relay_records_never_become_turns() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                untrusted_exchange(0, "relay observation"),
                trusted_exchange(0, "hello", "hi"),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].turns.len(), 1);
        assert_eq!(runs[0].turns[0].user, "hello");
        assert_eq!(runs[0].turns[0].assistant.as_deref(), Some("hi"));
    }

    #[test]
    fn each_session_allocation_starts_a_new_run() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(0, "first run", "a"),
                allocated("s"),
                trusted_exchange(0, "second run", "b"),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].header.run_index, 1);
        assert_eq!(runs[1].header.run_index, 2);
        assert_eq!(runs[1].turns[0].user, "second run");
        assert_eq!(runs[0].header.turn_count, 1);
    }

    #[test]
    fn a_trace_with_no_matching_exchange_is_counted_as_an_orphan() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                DiagnosticRecord::LiveGoalFormationSkipped {
                    qsf_session_id: "s".to_string(),
                    exchange_index: 7,
                    recorded_at: OffsetDateTime::UNIX_EPOCH,
                    reason: "guard".to_string(),
                },
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs[0].turns.len(), 0);
        assert_eq!(runs[0].header.source.orphans.formation, 1);
        assert!(
            !runs[0].header.source.complete,
            "an orphaned trace makes the run incomplete"
        );
    }

    #[test]
    fn an_undecodable_line_marks_its_turn_rather_than_vanishing() {
        // The distinction that matters: `volition: null` with `undecodable` naming the kind means
        // the ledger had a trace we could not read, not that the turn was quiet.
        let runs = runs_from_entries(
            vec![
                LedgerEntry::Record(Box::new(allocated("s"))),
                LedgerEntry::Record(Box::new(trusted_exchange(0, "hello", "hi"))),
                skipped(7, Some("volition_context_injected"), Some(0)),
            ],
            "ledger.jsonl",
            false,
        );

        let turn = &runs[0].turns[0];
        assert!(turn.volition.is_none());
        assert_eq!(turn.undecodable, vec!["volition_context_injected"]);

        let source = &runs[0].header.source;
        assert!(!source.complete);
        assert_eq!(source.skipped_line_count, 1);
        assert_eq!(source.skipped_lines[0].line_number, 7);
    }

    #[test]
    fn an_undecodable_line_naming_no_turn_still_reaches_the_run_header() {
        let runs = runs_from_entries(
            vec![
                LedgerEntry::Record(Box::new(allocated("s"))),
                skipped(3, None, None),
                LedgerEntry::Record(Box::new(trusted_exchange(0, "hello", "hi"))),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs[0].turns[0].undecodable, Vec::<String>::new());
        assert_eq!(runs[0].header.source.skipped_line_count, 1);
        assert!(!runs[0].header.source.complete);
    }

    #[test]
    fn an_undecodable_line_before_the_first_run_joins_that_run_instead_of_making_one() {
        let runs = runs_from_entries(
            vec![
                skipped(1, Some("from_a_future_build"), None),
                LedgerEntry::Record(Box::new(allocated("s"))),
                LedgerEntry::Record(Box::new(trusted_exchange(0, "hello", "hi"))),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs.len(), 1, "no phantom run for a leading skipped line");
        assert_eq!(runs[0].header.source.skipped_line_count, 1);
    }

    #[test]
    fn a_fully_read_run_is_marked_complete() {
        let runs = runs_from_records(
            vec![allocated("s"), trusted_exchange(0, "hello", "hi")],
            "ledger.jsonl",
            false,
        );

        assert!(runs[0].header.source.complete);
        assert_eq!(runs[0].header.source.skipped_line_count, 0);
        assert_eq!(runs[0].header.source.orphans.total(), 0);
    }

    #[test]
    fn turns_are_ordered_by_exchange_index() {
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(2, "third", "c"),
                trusted_exchange(0, "first", "a"),
                trusted_exchange(1, "second", "b"),
            ],
            "ledger.jsonl",
            false,
        );

        let users: Vec<&str> = runs[0].turns.iter().map(|t| t.user.as_str()).collect();
        assert_eq!(users, vec!["first", "second", "third"]);
    }

    #[test]
    fn records_before_any_allocation_form_a_run_without_a_start_time() {
        let runs = runs_from_records(
            vec![trusted_exchange(0, "orphaned run", "a")],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].header.run_started_at, None);
        assert_eq!(runs[0].turns.len(), 1);
    }

    #[test]
    fn a_repeated_exchange_index_keeps_the_last_record() {
        // The relay can record the same index more than once; the later record is the settled one.
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(0, "first attempt", "partial"),
                trusted_exchange(0, "first attempt", "final answer"),
            ],
            "ledger.jsonl",
            false,
        );

        assert_eq!(runs[0].turns.len(), 1);
        assert_eq!(runs[0].turns[0].assistant.as_deref(), Some("final answer"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```
cargo test -p qsf_app transcript::join
```

Expected: FAIL to compile — `runs_from_entries` and `LedgerEntry` do not exist.

- [ ] **Step 3: Implement the join**

Write above the test module in `join.rs`:

```rust
use std::collections::BTreeMap;
use std::time::SystemTime;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use qsf_diagnostics::{
    DiagnosticRecord, DiagnosticTrust, LiveGoalFormationTrace, RealtimeBoundedInitiativeTrace,
    VolitionContextInjectionTrace, WorldConsultationTrace,
};
use qsf_session::{Exchange, ExchangeInput};

use crate::transcript::model::{
    FormationStatus, FormationView, InitiativeView, MatchView, OrphanCounts, SessionLine,
    SkippedLineView, SourceIntegrity, SurfacedFactView, TraceBundle, TurnContextView, TurnLine,
    VolitionView, WinnerView, WorldView, render_keyword,
};

/// One realtime run: a `session_allocated` record and everything appended before the next one.
/// Source integrity lives inside `header.source` rather than beside it, so it cannot be computed
/// and then omitted from the emitted artifact.
#[derive(Debug)]
pub struct TranscriptRun {
    pub header: SessionLine,
    pub turns: Vec<TurnLine>,
}

/// One ledger line, in file order. Skipped lines stay in the sequence so they can be attributed to
/// the run — and where the envelope decoded an exchange index, to the turn — they belong to.
#[derive(Debug)]
pub enum LedgerEntry {
    Record(Box<DiagnosticRecord>),
    Skipped(SkippedLineView),
}

/// Everything gathered for one exchange index before it becomes a `TurnLine`.
#[derive(Default)]
struct TurnParts {
    exchange: Option<Exchange>,
    injection: Option<VolitionContextInjectionTrace>,
    formation: Option<(FormationStatus, Option<LiveGoalFormationTrace>, Option<String>)>,
    initiative: Option<RealtimeBoundedInitiativeTrace>,
    world: Option<WorldConsultationTrace>,
    turn_context: Option<TurnContextView>,
    /// Kinds present for this index that could not be decoded.
    undecodable: Vec<String>,
}

struct RunParts {
    session_id: String,
    started_at: Option<OffsetDateTime>,
    turns: BTreeMap<usize, TurnParts>,
    /// Lines skipped inside this run, including those that named no exchange index.
    skipped: Vec<SkippedLineView>,
}

impl RunParts {
    fn new(session_id: String, started_at: Option<OffsetDateTime>) -> Self {
        Self {
            session_id,
            started_at,
            turns: BTreeMap::new(),
            skipped: Vec::new(),
        }
    }
}

pub fn runs_from_entries(
    entries: Vec<LedgerEntry>,
    ledger: &str,
    full: bool,
) -> Vec<TranscriptRun> {
    let mut runs: Vec<RunParts> = Vec::new();
    // Skipped lines seen before any run opened. Attached to the first run that does, rather than
    // inventing a phantom run for them.
    let mut pending_skipped: Vec<SkippedLineView> = Vec::new();

    for entry in entries {
        let record = match entry {
            LedgerEntry::Record(record) => *record,
            LedgerEntry::Skipped(skipped) => {
                match runs.last_mut() {
                    Some(run) => attribute_skipped(run, skipped),
                    None => pending_skipped.push(skipped),
                }
                continue;
            }
        };

        if let DiagnosticRecord::SessionAllocated { qsf_session_id, at } = &record {
            open_run(
                &mut runs,
                &mut pending_skipped,
                qsf_session_id.clone(),
                Some(*at),
            );
            continue;
        }

        if runs.is_empty() {
            open_run(
                &mut runs,
                &mut pending_skipped,
                session_id_of(&record),
                None,
            );
        }
        let run = runs.last_mut().expect("a run is open");

        match record {
            DiagnosticRecord::DiagnosticExchangeRecorded { trust, exchange, .. } => {
                if trust == DiagnosticTrust::Trusted {
                    run.turns.entry(exchange.index).or_default().exchange = Some(exchange);
                }
            }
            DiagnosticRecord::VolitionContextInjected {
                exchange_index,
                trace,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().injection = Some(trace);
            }
            DiagnosticRecord::RealtimeBoundedInitiative {
                exchange_index,
                trace,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().initiative = Some(trace);
            }
            DiagnosticRecord::WorldConsultationPerformed {
                exchange_index,
                trace,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().world = Some(trace);
            }
            DiagnosticRecord::LiveGoalFormationPerformed {
                exchange_index,
                trace,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().formation =
                    Some((FormationStatus::Performed, Some(trace), None));
            }
            DiagnosticRecord::LiveGoalFormationFailed {
                exchange_index,
                error,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().formation =
                    Some((FormationStatus::Failed, None, Some(error)));
            }
            DiagnosticRecord::LiveGoalFormationSkipped {
                exchange_index,
                reason,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().formation =
                    Some((FormationStatus::Skipped, None, Some(reason)));
            }
            DiagnosticRecord::TurnContextCaptured {
                exchange_index,
                request_hash,
                messages,
                ..
            } => {
                run.turns.entry(exchange_index).or_default().turn_context =
                    Some(TurnContextView {
                        request_hash,
                        messages,
                    });
            }
            _ => {}
        }
    }

    // A ledger consisting only of undecodable lines still owes the caller an artifact that says so.
    if runs.is_empty() && !pending_skipped.is_empty() {
        open_run(&mut runs, &mut pending_skipped, String::new(), None);
    }

    runs.into_iter()
        .enumerate()
        .map(|(index, run)| build_run(index + 1, run, ledger, full))
        .collect()
}

/// Records an undecodable line against a run, and against a specific turn when its envelope named
/// an exchange index and a kind.
fn attribute_skipped(run: &mut RunParts, skipped: SkippedLineView) {
    if let (Some(index), Some(kind)) = (skipped.exchange_index, skipped.kind.clone()) {
        run.turns.entry(index).or_default().undecodable.push(kind);
    }
    run.skipped.push(skipped);
}

fn open_run(
    runs: &mut Vec<RunParts>,
    pending_skipped: &mut Vec<SkippedLineView>,
    session_id: String,
    started_at: Option<OffsetDateTime>,
) {
    runs.push(RunParts::new(session_id, started_at));
    let run = runs.last_mut().expect("just pushed");
    for skipped in pending_skipped.drain(..) {
        attribute_skipped(run, skipped);
    }
}

fn session_id_of(record: &DiagnosticRecord) -> String {
    match record {
        DiagnosticRecord::DiagnosticExchangeRecorded { qsf_session_id, .. }
        | DiagnosticRecord::VolitionContextInjected { qsf_session_id, .. }
        | DiagnosticRecord::RealtimeBoundedInitiative { qsf_session_id, .. }
        | DiagnosticRecord::WorldConsultationPerformed { qsf_session_id, .. }
        | DiagnosticRecord::LiveGoalFormationPerformed { qsf_session_id, .. }
        | DiagnosticRecord::LiveGoalFormationFailed { qsf_session_id, .. }
        | DiagnosticRecord::LiveGoalFormationSkipped { qsf_session_id, .. }
        | DiagnosticRecord::TurnContextCaptured { qsf_session_id, .. } => qsf_session_id.clone(),
        _ => String::new(),
    }
}

fn build_run(run_index: usize, run: RunParts, ledger: &str, full: bool) -> TranscriptRun {
    let mut orphans = OrphanCounts::default();
    let mut turns = Vec::new();

    for (index, parts) in run.turns {
        let Some(exchange) = parts.exchange else {
            if parts.injection.is_some() {
                orphans.injection += 1;
            }
            if parts.formation.is_some() {
                orphans.formation += 1;
            }
            if parts.initiative.is_some() {
                orphans.initiative += 1;
            }
            if parts.world.is_some() {
                orphans.world += 1;
            }
            if parts.turn_context.is_some() {
                orphans.turn_context += 1;
            }
            continue;
        };
        turns.push(build_turn(index, exchange, parts, full));
    }

    let complete = run.skipped.is_empty() && orphans.total() == 0;
    TranscriptRun {
        header: SessionLine {
            session_id: run.session_id,
            ledger: ledger.to_string(),
            run_index,
            run_started_at: run.started_at.and_then(|at| at.format(&Rfc3339).ok()),
            turn_count: turns.len(),
            source: SourceIntegrity {
                complete,
                skipped_line_count: run.skipped.len(),
                skipped_lines: run.skipped,
                orphans,
            },
        },
        turns,
    }
}

fn build_turn(index: usize, exchange: Exchange, parts: TurnParts, full: bool) -> TurnLine {
    let user = match &exchange.input {
        ExchangeInput::Text { text } => text.clone(),
        ExchangeInput::Voice {
            final_transcript, ..
        } => final_transcript.clone(),
    };
    let assistant = exchange.output.as_ref().map(|output| output.text.clone());
    let at = exchange.completed_at.and_then(rfc3339_from_system_time);
    let status = serde_json::to_value(exchange.status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());

    let volition = parts.injection.as_ref().map(volition_view);
    let initiative = parts.initiative.as_ref().map(initiative_view);
    let formation = parts
        .formation
        .as_ref()
        .map(|(status, trace, detail)| formation_view(*status, trace.as_ref(), detail.as_deref()));
    let world = parts.world.as_ref().map(world_view);
    let undecodable = parts.undecodable.clone();

    let traces = full.then(|| TraceBundle {
        injection: parts.injection,
        formation: parts.formation.and_then(|(_, trace, _)| trace),
        initiative: parts.initiative,
        world: parts.world,
        turn_context: parts.turn_context,
        exchange,
    });

    TurnLine {
        turn: index,
        at,
        user,
        assistant,
        status,
        volition,
        initiative,
        formation,
        world,
        undecodable,
        traces,
    }
}

fn rfc3339_from_system_time(value: SystemTime) -> Option<String> {
    OffsetDateTime::from(value).format(&Rfc3339).ok()
}

/// Splits the selector's matched goals at the qualification threshold, so `fired` means "could
/// win arbitration" and `below_threshold` means "matched but stayed quiet". A trace written before
/// `qualification_threshold` existed deserializes it as 0, in which case every match counts as
/// fired — the honest reading, since the threshold that applied is not recorded.
fn volition_view(trace: &VolitionContextInjectionTrace) -> VolitionView {
    let threshold = trace.qualification_threshold;
    let mut fired = Vec::new();
    let mut below_threshold = Vec::new();

    for detail in &trace.selector_output.selected_match_details {
        let reason = trace
            .below_threshold_candidates
            .iter()
            .find(|candidate| candidate.goal_id == detail.goal_id)
            .map(|candidate| candidate.reason.clone());
        let view = MatchView {
            goal: detail.goal_id.clone(),
            strength: detail.match_strength,
            keywords: detail.matched_keywords.iter().map(render_keyword).collect(),
            visibility: detail.visibility,
            reason,
        };
        if detail.match_strength >= threshold {
            fired.push(view);
        } else {
            below_threshold.push(view);
        }
    }

    VolitionView {
        threshold,
        mode: trace.arbitration_result.as_ref().map(|result| result.mode),
        winner: trace.arbitration_result.as_ref().map(|result| WinnerView {
            goal: result.winner_goal_id.clone(),
            title: result.winner_goal_title.clone(),
            effective_tier: result.winner_effective_tier,
            biased_tier: result.winner_biased_tier,
            losers: result.loser_count,
        }),
        fired,
        below_threshold,
        omitted_count: trace.selector_output.omitted_count,
        suppressed_cooldown_count: trace.selector_output.suppressed_cooldown_count,
        blocked_count: trace.selector_output.visible_blocked_count,
        subconscious_selected_count: trace.subconscious_selected_count,
    }
}

fn initiative_view(trace: &RealtimeBoundedInitiativeTrace) -> InitiativeView {
    InitiativeView {
        goal: trace.winning_goal_id.clone(),
        effect: trace.allowed_effect,
        surfaced: trace.surfaced,
        suppression: trace.suppression_reason,
        rendered_line_present: trace.rendered_line_present,
        output: trace.initiative_output.clone(),
    }
}

fn formation_view(
    status: FormationStatus,
    trace: Option<&LiveGoalFormationTrace>,
    detail: Option<&str>,
) -> FormationView {
    FormationView {
        status,
        candidate_id: trace.and_then(|t| t.proposed_candidate_id.clone()),
        candidate_title: trace.and_then(|t| t.proposed_candidate_title.clone()),
        contradictions: trace.map(|t| t.contradictions.clone()).unwrap_or_default(),
        resolution: trace.and_then(|t| t.resolution.clone()),
        declined: trace.and_then(|t| t.declined_candidate.clone()),
        detail: detail.map(str::to_string),
    }
}

fn world_view(trace: &WorldConsultationTrace) -> WorldView {
    WorldView {
        serving_goal: trace.serving_goal_id.clone(),
        serving_goal_title: trace.serving_goal_title.clone(),
        query_terms: trace.query_terms.clone(),
        surfaced_facts: trace
            .surfaced_facts
            .iter()
            .map(|fact| SurfacedFactView {
                title: fact.title.clone(),
                url: fact.url.clone(),
                source_domain: fact.source_domain.clone(),
                trust_tier: fact.trust_tier.clone(),
            })
            .collect(),
        injection_reason: trace.injection_reason.clone(),
        injected_chars: trace.injected_text.chars().count(),
    }
}
```

Types used above, confirmed against `qsf_volition` so no guessing is needed while implementing:
`Mode`, `AllowedEffect` and `VolitionSuppressionReason` are `Copy`, so they are read by value.
`AdmissionResolution` and `DeclinedCandidate` are not `Copy`, so they are cloned.
`WorldQueryTerm` is `{ term: String, source: WorldQueryTermSource }` and is cloned whole.

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p qsf_app transcript::join
```

Expected: all five tests PASS.

- [ ] **Step 5: Add the threshold-split test**

```rust
    /// Builds an injection trace whose only meaningful fields are the qualification threshold and
    /// the selected match details. Every other field is populated explicitly with an inert value
    /// rather than via `..Default::default()`: if a field is later added to the trace, this helper
    /// stops compiling, which is the signal we want.
    fn injection_trace_with(
        threshold: u32,
        goals: Vec<(&str, u32)>,
    ) -> VolitionContextInjectionTrace {
        let selected_match_details = goals
            .into_iter()
            .map(|(goal_id, match_strength)| VolitionSelectedMatchDetail {
                goal_id: goal_id.to_string(),
                matched_keywords: vec![],
                match_strength,
                visibility: GoalVisibility::Conscious,
            })
            .collect();

        VolitionContextInjectionTrace {
            qsf_session_id: "s".to_string(),
            exchange_index: 0,
            injected_layers: vec![],
            stable_baseline_hash: String::new(),
            input_transcript_ref: String::new(),
            volition_tick_before: 0,
            events_applied: vec![],
            opportunity_signals: vec![],
            selector_output: VolitionSelectorSummary {
                selected_goal_ids: vec![],
                selected_goal_titles: vec![],
                selected_goal_summaries: vec![],
                selected_count: 0,
                omitted_count: 0,
                suppressed_cooldown_count: 0,
                visible_blocked_count: 0,
                selected_match_details,
            },
            omitted_or_suppressed_candidates: vec![],
            qualification_threshold: threshold,
            below_threshold_candidates: vec![],
            arbitration_result: None,
            mode_bias_outcomes: vec![],
            protected_tier_active: false,
            shaping_intensity: ShapingIntensity::None,
            shaping_intensity_inputs: None,
            context_packet_hash: String::new(),
            context_packet_token_estimate: 0,
            response_create_event_ref: String::new(),
            declined_candidates_injected: vec![],
            winner_visibility: None,
            ambient_exposure: AmbientExposure::Ordinary,
            subconscious_selected_count: 0,
        }
    }

    #[test]
    fn matches_split_on_the_qualification_threshold() {
        // Strength 9 qualifies at threshold 4, strength 3 does not, and that split is what the
        // transcript reports as fired versus quiet.
        let trace = injection_trace_with(
            4,
            vec![("grow-the-library", 9), ("serve-the-present-person", 3)],
        );
        let runs = runs_from_records(
            vec![
                allocated("s"),
                trusted_exchange(0, "do you remember", "yes"),
                DiagnosticRecord::VolitionContextInjected {
                    qsf_session_id: "s".to_string(),
                    exchange_index: 0,
                    recorded_at: OffsetDateTime::UNIX_EPOCH,
                    trace,
                },
            ],
            "ledger.jsonl",
            false,
        );

        let volition = runs[0].turns[0]
            .volition
            .as_ref()
            .expect("injection trace present");
        assert_eq!(volition.threshold, 4);
        assert_eq!(volition.fired.len(), 1);
        assert_eq!(volition.fired[0].goal, "grow-the-library");
        assert_eq!(volition.below_threshold.len(), 1);
        assert_eq!(volition.below_threshold[0].goal, "serve-the-present-person");
    }
```

Write the `injection_trace_with(threshold, goals)` helper in the same test module. It constructs a
`VolitionContextInjectionTrace` with every field populated with an inert value — empty strings,
empty vectors, `None` for the optional summaries — except `qualification_threshold` and
`selector_output.selected_match_details`, which carry the arguments. Populating every field by hand
is deliberate: if a field is added to the trace later, this helper stops compiling, which is the
signal we want.

- [ ] **Step 6: Verify and commit**

```
cargo test -p qsf_app transcript
cargo clippy --all-targets -- -D warnings
cargo fmt
```

```bash
git add crates/qsf_app
git commit -m "Transcript: Join ledger records into per-run turns"
```

### Task 3.3: Ledger loading and rendering

**Files:**
- Modify: `crates/qsf_app/src/transcript/ledger.rs`
- Modify: `crates/qsf_app/src/transcript/render.rs`

**Interfaces:**
- Produces:
  - `pub fn resolve_ledger_path(state_dir: &Path, session: Option<&str>) -> anyhow::Result<PathBuf>`
  - `pub fn load_ledger(path: &Path) -> anyhow::Result<Vec<LedgerEntry>>` — one entry per non-blank
    line, in file order, each either a decoded record or a located `SkippedLineView`
  - `pub fn render_runs(runs: Vec<TranscriptRun>, pretty: bool) -> anyhow::Result<String>` — takes
    ownership so each `SessionLine` and `TurnLine` moves into the tagged `TranscriptLine` without a
    clone

- [ ] **Step 1: Write the failing ledger tests**

In `ledger.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn an_explicit_session_selects_its_own_ledger() {
        let dir = tempfile::tempdir().expect("tempdir");
        let diagnostics = dir.path().join("diagnostics");
        fs::create_dir_all(&diagnostics).expect("create");
        fs::write(diagnostics.join("default.jsonl"), "").expect("write");
        fs::write(diagnostics.join("run-2.jsonl"), "").expect("write");

        let resolved = resolve_ledger_path(dir.path(), Some("run-2")).expect("resolve");

        assert_eq!(resolved, diagnostics.join("run-2.jsonl"));
    }

    #[test]
    fn an_absent_session_is_an_error_naming_the_expected_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("diagnostics")).expect("create");

        let error = resolve_ledger_path(dir.path(), Some("missing"))
            .expect_err("absent ledger must fail");

        assert!(error.to_string().contains("missing.jsonl"));
    }

    #[test]
    fn unparseable_lines_are_skipped_and_located_rather_than_aborting() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ledger.jsonl");
        fs::write(
            &path,
            "{\"kind\":\"session_allocated\",\"qsf_session_id\":\"s\",\"at\":\"1970-01-01T00:00:00Z\"}\n\
             {\"kind\":\"from_a_future_build\",\"qsf_session_id\":\"s\",\"exchange_index\":2}\n",
        )
        .expect("write");

        let entries = load_ledger(&path).expect("load");

        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0], LedgerEntry::Record(_)));
        let LedgerEntry::Skipped(skipped) = &entries[1] else {
            panic!("second line must be skipped");
        };
        assert_eq!(skipped.line_number, 2);
        assert_eq!(skipped.kind.as_deref(), Some("from_a_future_build"));
        assert_eq!(
            skipped.exchange_index,
            Some(2),
            "the envelope's exchange index must survive so the turn can be marked incomplete"
        );
    }

    #[test]
    fn a_line_that_is_not_json_is_skipped_with_no_kind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ledger.jsonl");
        fs::write(&path, "this is not json\n").expect("write");

        let entries = load_ledger(&path).expect("load");

        let LedgerEntry::Skipped(skipped) = &entries[0] else {
            panic!("must be skipped");
        };
        assert_eq!(skipped.kind, None);
        assert_eq!(skipped.exchange_index, None);
    }

    #[test]
    fn the_newest_ledger_wins_automatic_selection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let diagnostics = dir.path().join("diagnostics");
        fs::create_dir_all(&diagnostics).expect("create");
        let older = diagnostics.join("older.jsonl");
        let newer = diagnostics.join("newer.jsonl");
        fs::write(&older, "").expect("write");
        fs::write(&newer, "").expect("write");
        set_modified(&older, 1_000);
        set_modified(&newer, 2_000);

        let resolved = resolve_ledger_path(dir.path(), None).expect("resolve");

        assert_eq!(resolved, newer);
    }

    #[test]
    fn equal_timestamps_are_broken_by_file_name_not_directory_order() {
        // `read_dir` order is unspecified, so without an explicit tie-break the default invocation
        // could show a different conversation on different runs or filesystems.
        let dir = tempfile::tempdir().expect("tempdir");
        let diagnostics = dir.path().join("diagnostics");
        fs::create_dir_all(&diagnostics).expect("create");
        for name in ["aaa.jsonl", "zzz.jsonl", "mmm.jsonl"] {
            let path = diagnostics.join(name);
            fs::write(&path, "").expect("write");
            set_modified(&path, 5_000);
        }

        let resolved = resolve_ledger_path(dir.path(), None).expect("resolve");

        assert_eq!(
            resolved,
            diagnostics.join("zzz.jsonl"),
            "the greatest file name wins a timestamp tie"
        );
    }

    #[test]
    fn non_jsonl_files_are_ignored_by_automatic_selection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let diagnostics = dir.path().join("diagnostics");
        fs::create_dir_all(&diagnostics).expect("create");
        let ledger = diagnostics.join("default.jsonl");
        fs::write(&ledger, "").expect("write");
        set_modified(&ledger, 1_000);
        let note = diagnostics.join("notes.txt");
        fs::write(&note, "").expect("write");
        set_modified(&note, 9_000);

        let resolved = resolve_ledger_path(dir.path(), None).expect("resolve");

        assert_eq!(resolved, ledger);
    }

    /// Sets a file's modification time to a fixed offset from the Unix epoch so tie-break behavior
    /// is testable rather than dependent on write-order timing. Uses `File::set_modified`, stable
    /// since Rust 1.75, so this needs no new dependency.
    fn set_modified(path: &std::path::Path, epoch_secs: u64) {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open for mtime");
        file.set_modified(
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(epoch_secs),
        )
        .expect("set mtime");
    }
}
```

- [ ] **Step 2: Run to verify failure**

```
cargo test -p qsf_app transcript::ledger
```

Expected: FAIL to compile — the functions do not exist.

- [ ] **Step 3: Implement the loader**

```rust
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;

use qsf_diagnostics::{DiagnosticRecord, decode_envelope};

use crate::transcript::join::LedgerEntry;
use crate::transcript::model::SkippedLineView;

/// Resolves which ledger to read. An explicit session names its file directly; otherwise the newest
/// `*.jsonl` wins, so a `-RandomSessionId` run is found without looking up its UUID. This mirrors
/// how `goals` auto-selects a continuity session
/// (`crate::goal_detail_loading::resolve_session`) over a different directory.
///
/// Ties on modification time break on file name, descending. `read_dir` order is unspecified, so
/// without an explicit tie-break two ledgers sharing a timestamp could resolve differently between
/// runs or filesystems — and since this is the default invocation, that would silently show a
/// different conversation than the last one.
pub fn resolve_ledger_path(state_dir: &Path, session: Option<&str>) -> anyhow::Result<PathBuf> {
    let diagnostics_dir = state_dir.join("diagnostics");
    if let Some(session_id) = session {
        let path = diagnostics_dir.join(format!("{session_id}.jsonl"));
        if !path.exists() {
            anyhow::bail!("no diagnostics ledger for session `{session_id}` at `{}`", path.display());
        }
        return Ok(path);
    }

    // Collect, then pick by an explicit total order. Sorting the candidates rather than tracking a
    // running maximum makes the tie-break obvious and keeps `read_dir` order out of the result.
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let entries = fs::read_dir(&diagnostics_dir).with_context(|| {
        format!(
            "failed to read diagnostics directory `{}`",
            diagnostics_dir.display()
        )
    })?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        if !entry.metadata()?.is_file() {
            continue;
        }
        candidates.push((entry.metadata()?.modified()?, path));
    }

    candidates.sort_by(|(left_time, left_path), (right_time, right_path)| {
        left_time
            .cmp(right_time)
            .then_with(|| left_path.file_name().cmp(&right_path.file_name()))
    });

    candidates.pop().map(|(_, path)| path).ok_or_else(|| {
        anyhow::anyhow!(
            "no diagnostics ledger found under `{}`",
            diagnostics_dir.display()
        )
    })
}

/// Reads every non-blank line into a `LedgerEntry`, preserving file order.
///
/// A line this build cannot deserialize does not abort the read: the ledger is append-only and
/// outlives builds, so old runs may hold record shapes this build no longer knows. Instead the line
/// is recorded as skipped, located by line number and — when the envelope decodes — by kind and
/// exchange index, so the emitted artifact can say which turn lost which section.
pub fn load_ledger(path: &Path) -> anyhow::Result<Vec<LedgerEntry>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read diagnostics ledger `{}`", path.display()))?;
    let mut entries = Vec::new();

    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<DiagnosticRecord>(line) {
            Ok(record) => entries.push(LedgerEntry::Record(Box::new(record))),
            Err(error) => {
                let envelope = decode_envelope(line);
                entries.push(LedgerEntry::Skipped(SkippedLineView {
                    line_number: index + 1,
                    kind: envelope.as_ref().and_then(|e| e.kind.clone()),
                    exchange_index: envelope.as_ref().and_then(|e| e.exchange_index),
                    error: error.to_string(),
                }));
            }
        }
    }

    Ok(entries)
}
```

- [ ] **Step 4: Write the failing render test**

In `render.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::transcript::model::{SessionLine, SourceIntegrity};

    use super::*;

    fn turn(index: usize, user: &str) -> TurnLine {
        TurnLine {
            turn: index,
            at: None,
            user: user.to_string(),
            assistant: Some("reply".to_string()),
            status: "completed".to_string(),
            volition: None,
            initiative: None,
            formation: None,
            world: None,
            undecodable: vec![],
            traces: None,
        }
    }

    fn run_with_two_turns() -> TranscriptRun {
        TranscriptRun {
            header: SessionLine {
                session_id: "s".to_string(),
                ledger: "ledger.jsonl".to_string(),
                run_index: 1,
                run_started_at: None,
                turn_count: 2,
                source: SourceIntegrity {
                    complete: true,
                    skipped_line_count: 0,
                    skipped_lines: vec![],
                    orphans: Default::default(),
                },
            },
            turns: vec![turn(0, "first"), turn(1, "second")],
        }
    }

    #[test]
    fn compact_output_is_one_line_per_record() {
        let rendered = render_runs(vec![run_with_two_turns()], false).expect("render");

        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 3, "one session header plus two turns");
        for line in &lines {
            serde_json::from_str::<serde_json::Value>(line).expect("each line is valid JSON");
        }
    }

    #[test]
    fn pretty_output_parses_to_the_same_values_as_compact() {
        let compact = render_runs(vec![run_with_two_turns()], false).expect("compact");
        let pretty = render_runs(vec![run_with_two_turns()], true).expect("pretty");

        let compact_values: Vec<serde_json::Value> = compact
            .lines()
            .map(|line| serde_json::from_str(line).expect("compact line"))
            .collect();
        let pretty_values: Vec<serde_json::Value> =
            serde_json::Deserializer::from_str(&pretty)
                .into_iter::<serde_json::Value>()
                .map(|value| value.expect("pretty value"))
                .collect();

        assert_eq!(compact_values, pretty_values);
    }
}
```

Write `run_with_two_turns()` in the test module, constructing a `TranscriptRun` with a `SessionLine`
and two `TurnLine` values with `volition: None`.

The unused `TurnLine` import above is needed by the test module's helper, not by `render_runs`
itself; if clippy flags it, move it into the `#[cfg(test)]` module.

- [ ] **Step 5: Run to verify failure, then implement**

```rust
use crate::transcript::join::TranscriptRun;
use crate::transcript::model::{TranscriptLine, TurnLine};

/// Serializes runs as JSONL: one session header line per run, then one line per turn. `pretty`
/// indents each record, which is no longer strict JSONL but stays a valid concatenated JSON
/// stream, so the two forms parse to identical values.
pub fn render_runs(runs: Vec<TranscriptRun>, pretty: bool) -> anyhow::Result<String> {
    let mut out = String::new();
    for run in runs {
        push_line(&mut out, &TranscriptLine::Session(run.header), pretty)?;
        for turn in run.turns {
            push_line(&mut out, &TranscriptLine::Turn(turn), pretty)?;
        }
    }
    Ok(out)
}

fn push_line(out: &mut String, line: &TranscriptLine, pretty: bool) -> anyhow::Result<()> {
    let rendered = if pretty {
        serde_json::to_string_pretty(line)?
    } else {
        serde_json::to_string(line)?
    };
    out.push_str(&rendered);
    out.push('\n');
    Ok(())
}
```

- [ ] **Step 6: Verify and commit**

```
cargo test -p qsf_app transcript
cargo clippy --all-targets -- -D warnings
cargo fmt
```

```bash
git add crates/qsf_app
git commit -m "Transcript: Resolve and load the diagnostics ledger, render runs as JSONL"
```

### Task 3.4: The `transcript` subcommand and the real-ledger fixture

**Files:**
- Modify: `crates/qsf_app/src/cli.rs:56-65` (add the variant after `Goals`) and `:131-149` (dispatch)
- Create: `crates/qsf_app/tests/fixtures/realtime-diagnostics-sample.jsonl`
- Create: `crates/qsf_app/tests/transcript_ledger.rs`

**Interfaces:**
- Consumes: `resolve_ledger_path`, `load_ledger`, `runs_from_entries`, `render_runs`.
- Produces: `qsf_app transcript [--state-dir PATH] [--session ID] [--all] [--pretty] [--full] [--out PATH]`.

- [ ] **Step 1: Build the fixture from the real ledger**

Take the last run of the live ledger and keep only its first three trusted turns with their traces.
Run this from the repository root, then inspect the result:

```bash
python - <<'PY'
import json
lines = open('state/realtime/diagnostics/default.jsonl', encoding='utf-8').read().splitlines()
start = max(i for i, l in enumerate(lines) if '"kind":"session_allocated"' in l)
keep_kinds = {
    'session_allocated', 'diagnostic_exchange_recorded', 'volition_context_injected',
    'live_goal_formation_performed', 'realtime_bounded_initiative', 'turn_context_captured',
}
out = []
for line in lines[start:]:
    record = json.loads(line)
    if record['kind'] not in keep_kinds:
        continue
    index = record.get('exchange_index', record.get('exchange', {}).get('index'))
    if index is not None and index > 2:
        continue
    if record['kind'] == 'diagnostic_exchange_recorded' and record['trust'] != 'trusted':
        continue
    out.append(json.dumps(record, separators=(',', ':'), ensure_ascii=False))
open('crates/qsf_app/tests/fixtures/realtime-diagnostics-sample.jsonl', 'w',
     encoding='utf-8', newline='\n').write('\n'.join(out) + '\n')
print(len(out), 'records')
PY
```

`newline='\n'` matters: `.gitattributes` pins `eol=lf`, and a CRLF fixture would not reproduce on
another platform. Read the file before committing it — it contains real conversation text, so
confirm it holds nothing you would not put in the repository, and trim or replace transcripts if it
does.

- [ ] **Step 2: Write the fixture test**

`crates/qsf_app/tests/transcript_ledger.rs`:

```rust
use std::path::PathBuf;

use qsf_app::transcript::{LedgerEntry, load_ledger, render_runs, runs_from_entries};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/realtime-diagnostics-sample.jsonl")
}

#[test]
fn a_real_ledger_excerpt_parses_with_no_skipped_lines() {
    let entries = load_ledger(&fixture_path()).expect("load the committed ledger excerpt");

    let skipped: Vec<&LedgerEntry> = entries
        .iter()
        .filter(|entry| matches!(entry, LedgerEntry::Skipped(_)))
        .collect();
    assert!(
        skipped.is_empty(),
        "every line of a real ledger must parse: {skipped:?}"
    );
    assert!(!entries.is_empty());
}

#[test]
fn a_real_ledger_excerpt_produces_turns_with_both_sides_and_a_threshold() {
    let entries = load_ledger(&fixture_path()).expect("load");
    let runs = runs_from_entries(entries, "fixture.jsonl", false);

    assert_eq!(runs.len(), 1, "the excerpt holds exactly one run");
    let run = &runs[0];
    assert!(
        run.header.source.complete,
        "a real excerpt must read completely: {:?}",
        run.header.source
    );
    assert!(!run.turns.is_empty());

    let first = &run.turns[0];
    assert!(!first.user.is_empty(), "a trusted turn carries user text");
    assert!(first.assistant.is_some(), "a trusted turn carries a response");
    assert!(first.undecodable.is_empty());

    let volition = first
        .volition
        .as_ref()
        .expect("a trusted turn carries an injection trace");
    assert!(volition.threshold > 0, "the qualification threshold is recorded");
}

#[test]
fn compact_rendering_of_a_real_ledger_is_valid_jsonl() {
    let entries = load_ledger(&fixture_path()).expect("load");
    let runs = runs_from_entries(entries, "fixture.jsonl", false);
    let rendered = render_runs(runs, false).expect("render");

    for line in rendered.lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("each emitted line is valid JSON");
    }
}

/// The artifact must carry its own provenance: a reader who has only the file, and never saw the
/// invocation's stderr, has to be able to tell that a line was skipped.
#[test]
fn a_partially_read_ledger_says_so_in_the_written_artifact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("partial.jsonl");
    let fixture = std::fs::read_to_string(fixture_path()).expect("read fixture");
    std::fs::write(
        &path,
        format!(
            "{fixture}{}\n",
            r#"{"kind":"from_a_future_build","qsf_session_id":"default","exchange_index":0}"#
        ),
    )
    .expect("write");

    let runs = runs_from_entries(
        load_ledger(&path).expect("load"),
        path.display().to_string().as_str(),
        false,
    );
    let rendered = render_runs(runs, false).expect("render");

    let session: serde_json::Value =
        serde_json::from_str(rendered.lines().next().expect("a session line")).expect("parse");
    assert_eq!(session["source"]["complete"], serde_json::json!(false));
    assert_eq!(session["source"]["skipped_line_count"], serde_json::json!(1));
    assert_eq!(
        session["source"]["skipped_lines"][0]["kind"],
        serde_json::json!("from_a_future_build")
    );

    let turn: serde_json::Value =
        serde_json::from_str(rendered.lines().nth(1).expect("a turn line")).expect("parse");
    assert_eq!(
        turn["undecodable"],
        serde_json::json!(["from_a_future_build"]),
        "the turn the undecodable line belonged to must be marked"
    );
}
```

`tempfile` is already in `qsf_app`'s `[dependencies]`, so the last test needs no manifest change.

This is an integration test, so `transcript` must be reachable from outside the crate: confirm
`crates/qsf_app/src/lib.rs` exposes `pub mod transcript;`. Add `serde_json` to `[dev-dependencies]`
of `crates/qsf_app/Cargo.toml` if the integration target cannot see the normal dependency.

- [ ] **Step 3: Run to verify the tests fail for the right reason**

```
cargo test -p qsf_app --test transcript_ledger
```

Expected: FAIL because `render_runs` and friends are not yet public, or PASS if Task 3.3 already
exposed them. A failure mentioning unresolved imports is correct at this point; a parse failure on
the fixture is a real defect in the loader and must be fixed rather than worked around.

- [ ] **Step 4: Add the CLI variant**

In `crates/qsf_app/src/cli.rs`, after the `Goals` variant:

```rust
    /// Print a realtime session's turns joined to their volition traces, as JSONL.
    Transcript {
        /// Realtime state directory containing the diagnostics ledger.
        #[arg(long, default_value = "state/realtime", value_name = "PATH")]
        state_dir: PathBuf,

        /// Explicit session id. Bypasses automatic ledger selection.
        #[arg(long, value_name = "ID")]
        session: Option<String>,

        /// Emit every run in the ledger, not just the most recent one.
        #[arg(long)]
        all: bool,

        /// Indent each record. Readable, but no longer one record per line.
        #[arg(long)]
        pretty: bool,

        /// Attach the verbatim traces and the full exchange to each turn.
        #[arg(long)]
        full: bool,

        /// Write to this file instead of stdout.
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
```

- [ ] **Step 5: Add the dispatch arm**

After the `Goals` arm in `run()`:

```rust
        Some(Command::Transcript {
            state_dir,
            session,
            all,
            pretty,
            full,
            out,
        }) => {
            let path = crate::transcript::resolve_ledger_path(&state_dir, session.as_deref())?;
            let entries = crate::transcript::load_ledger(&path)?;

            let ledger_label = path.display().to_string();
            let mut runs = crate::transcript::runs_from_entries(entries, &ledger_label, full);
            if !all {
                let last = runs.pop();
                runs = last.into_iter().collect();
            }

            // Warnings duplicate what `header.source` already carries in the artifact. They exist so
            // an interactive run notices; the artifact does not depend on anyone having read them.
            for run in &runs {
                let source = &run.header.source;
                if source.complete {
                    continue;
                }
                eprintln!(
                    "warning: run {} was read incompletely: {} skipped line(s), {} orphaned \
                     trace(s). See `source` in the emitted session record.",
                    run.header.run_index,
                    source.skipped_line_count,
                    source.orphans.total()
                );
                for skipped in &source.skipped_lines {
                    eprintln!(
                        "  line {}: kind {:?}: {}",
                        skipped.line_number, skipped.kind, skipped.error
                    );
                }
            }

            let rendered = crate::transcript::render_runs(runs, pretty)?;
            match out {
                Some(destination) => {
                    std::fs::write(&destination, rendered).with_context(|| {
                        format!("failed to write transcript to `{}`", destination.display())
                    })?;
                    eprintln!("wrote {}", destination.display());
                }
                None => print!("{rendered}"),
            }
            Ok(())
        }
```

Add `use anyhow::Context;` to `cli.rs` if it is not already imported. Warnings go to stderr on
purpose: stdout must stay pure JSONL so `qsf.ps1 transcript > turns.jsonl` produces a clean
artifact.

- [ ] **Step 6: Add the CLI parsing tests**

In the `cli.rs` test module:

```rust
    #[test]
    fn transcript_command_defaults_to_realtime_state_and_compact_output() {
        let cli = Cli::try_parse_from(["qsf_app", "transcript"]).unwrap();

        let Some(super::Command::Transcript {
            state_dir,
            session,
            all,
            pretty,
            full,
            out,
        }) = cli.command
        else {
            panic!("expected transcript command");
        };
        assert_eq!(state_dir, std::path::PathBuf::from("state/realtime"));
        assert_eq!(session, None);
        assert!(!all);
        assert!(!pretty);
        assert!(!full);
        assert_eq!(out, None);
    }

    #[test]
    fn transcript_command_parses_every_flag() {
        let cli = Cli::try_parse_from([
            "qsf_app",
            "transcript",
            "--session",
            "run-123",
            "--all",
            "--pretty",
            "--full",
            "--out",
            "turns.jsonl",
        ])
        .unwrap();

        let Some(super::Command::Transcript {
            session,
            all,
            pretty,
            full,
            out,
            ..
        }) = cli.command
        else {
            panic!("expected transcript command");
        };
        assert_eq!(session.as_deref(), Some("run-123"));
        assert!(all);
        assert!(pretty);
        assert!(full);
        assert_eq!(out, Some(std::path::PathBuf::from("turns.jsonl")));
    }
```

- [ ] **Step 7: Verify**

```
cargo test -p qsf_app
cargo run -p qsf_app -- transcript --session default | Select-Object -First 3
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expected: the third command prints a `session` header line followed by turn lines from the real
ledger. **Human verification recommended here:** read a few turns and confirm the goal-matching
numbers match what the debug UI showed for that session.

- [ ] **Step 8: Commit**

```bash
git add crates/qsf_app
git commit -m "Transcript: Add the qsf_app transcript subcommand"
```

---

## Phase 4 — Launcher, completion and documentation

### Task 4.1: `qsf.ps1 transcript`

**Files:**
- Modify: `scripts/qsf.ps1` (param block, `Invoke-Transcript`, dispatch, help)
- Modify: `scripts/qsf.Tests.ps1`
- Modify: `README.md`

**Interfaces:**
- Produces: `Get-TranscriptArguments` (pure, returns the cargo argument array) and
  `Invoke-Transcript` (calls `Invoke-LoggedCommand` with it). The split exists so the argument
  mapping is testable without running cargo, matching how the existing launcher tests probe
  `Get-SleepEnvironmentDelta`.

- [ ] **Step 1: Add the switches to the param block**

In `scripts/qsf.ps1`, add to `param(...)` after `[switch]$RandomSessionId`:

```powershell
    [switch]$All,
    [switch]$Pretty,
    [switch]$Full,
    [string]$Out = "",
```

`-StateDir` already exists and defaults to `state/realtime`.

- [ ] **Step 2: Write the failing Pester tests**

Add to `scripts/qsf.Tests.ps1`:

```powershell
Describe "qsf.ps1 transcript launcher" {
    BeforeAll {
        $script:QsfSkipAutoRun = $true
        . $script:LauncherScript -Command "help"
    }

    It "defaults to the realtime state directory and compact output" {
        $arguments = Get-TranscriptArguments

        $arguments | Should -Contain "transcript"
        $arguments | Should -Contain "--state-dir"
        $arguments | Should -Contain "state/realtime"
        $arguments | Should -Not -Contain "--pretty"
        $arguments | Should -Not -Contain "--all"
        $arguments | Should -Not -Contain "--full"
    }

    It "passes an explicit session id through as --session" {
        $script:QsfSkipAutoRun = $true
        . $script:LauncherScript -Command "transcript" -Subject "run-123"

        $arguments = Get-TranscriptArguments

        $arguments | Should -Contain "--session"
        $arguments | Should -Contain "run-123"
    }

    It "maps each switch to its cargo flag" {
        $script:QsfSkipAutoRun = $true
        . $script:LauncherScript -Command "transcript" -All -Pretty -Full -Out "turns.jsonl"

        $arguments = Get-TranscriptArguments

        $arguments | Should -Contain "--all"
        $arguments | Should -Contain "--pretty"
        $arguments | Should -Contain "--full"
        $arguments | Should -Contain "--out"
        $arguments | Should -Contain "turns.jsonl"
    }
}
```

- [ ] **Step 3: Run to verify failure**

```
pwsh -NoProfile -Command "Invoke-Pester -Path scripts/qsf.Tests.ps1 -Output Detailed"
```

Expected: FAIL — `Get-TranscriptArguments` is not defined.

- [ ] **Step 4: Implement the launcher functions**

Add next to `Invoke-Goals` in `scripts/qsf.ps1`:

```powershell
function Get-TranscriptArguments {
    $arguments = @(
        "run",
        "-p",
        "qsf_app",
        "--",
        "transcript",
        "--state-dir",
        $StateDir
    )
    if (-not [string]::IsNullOrWhiteSpace($Subject)) {
        $arguments += @("--session", $Subject)
    }
    if ($All) {
        $arguments += "--all"
    }
    if ($Pretty) {
        $arguments += "--pretty"
    }
    if ($Full) {
        $arguments += "--full"
    }
    if (-not [string]::IsNullOrWhiteSpace($Out)) {
        $arguments += @("--out", $Out)
    }
    return $arguments
}

function Invoke-Transcript {
    Invoke-LoggedCommand -Executable "cargo" -Arguments (Get-TranscriptArguments)
}
```

Add the dispatch arm after `"goals" { Invoke-Goals }`:

```powershell
        "transcript" {
            Invoke-Transcript
        }
```

- [ ] **Step 5: Update the help text**

In `Show-Help`, after the `goals` usage line:

```
  .\scripts\qsf.ps1 transcript [<session-id>] [-StateDir <path>] [-All] [-Pretty] [-Full] [-Out <path>]
```

And in the Defaults section, after the `Goals:` line:

```
  Transcript:      prints the newest run in state/realtime/diagnostics as JSONL, one line per turn with its
                   volition traces; an optional session id bypasses ledger auto-selection; -All emits every run
```

- [ ] **Step 6: Run the tests**

```
pwsh -NoProfile -Command "Invoke-Pester -Path scripts/qsf.Tests.ps1 -Output Detailed"
```

Expected: PASS.

- [ ] **Step 7: Document it in the README**

Add `transcript` to the README's command list, next to `goals`, with a one-line description and one
worked example showing redirection to a file:

```powershell
.\scripts\qsf.ps1 transcript > turns.jsonl
```

Note in the README that `Write-Host` banners and cargo's build output go to other streams, so `>`
captures only the JSONL — and that `-Out` writes the file directly if you would rather not rely on
that.

Two contract details also belong in the README, because they are what a reader of a saved artifact
needs to know:

- Every session record carries `source`, so a file can be checked for completeness on its own:
  `source.complete` is `false` when any line was skipped or any trace orphaned, and each turn's
  `undecodable` names kinds the ledger held for it that this build could not read.
- The default output contains no floating-point values; `--full` embeds traces verbatim and can,
  because `qsf_corpus::QueryCandidate.score` is `f64`. Do not claim all transcript artifacts are
  float-free.

- [ ] **Step 8: Verify the redirection claim by hand**

**Human verification required.** Run:

```powershell
.\scripts\qsf.ps1 transcript > turns.jsonl
Get-Content turns.jsonl -TotalCount 1
```

Confirm the first line is the `session` record and not a launcher banner. If a banner leaks into the
file, change the README to recommend `-Out` and say plainly that `>` is not clean.

- [ ] **Step 9: Commit**

```bash
git add scripts/qsf.ps1 scripts/qsf.Tests.ps1 README.md
git commit -m "Launcher: Add qsf.ps1 transcript"
```

### Task 4.2: Tab completion

**Files:**
- Modify: `scripts/qsf-completion.ps1`
- Modify: `scripts/qsf-completion.Tests.ps1`

- [ ] **Step 1: Write the failing completion tests**

Add to the `Describe "qsf.ps1 argument completion"` block in `scripts/qsf-completion.Tests.ps1`. The
harness's `Complete-QsfInput` helper and the `$script:QsfCompletionProjectRoot` override are already
defined by the file's `BeforeAll`.

```powershell
    It "completes the transcript command" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 tr"

        $completions | Should -Contain "transcript"
    }

    Context "with a hermetic project root that has diagnostics ledgers" {
        BeforeEach {
            $script:OriginalCompletionRoot = $script:QsfCompletionProjectRoot
            $script:QsfCompletionProjectRoot = "$TestDrive"
            New-Item -ItemType Directory -Force (Join-Path $TestDrive "state/realtime/diagnostics") | Out-Null
            Set-Content -LiteralPath (Join-Path $TestDrive "state/realtime/diagnostics/default.jsonl") -Value ""
            Set-Content -LiteralPath (Join-Path $TestDrive "state/realtime/diagnostics/run-abc.jsonl") -Value ""
        }

        AfterEach {
            $script:QsfCompletionProjectRoot = $script:OriginalCompletionRoot
            Remove-Item -LiteralPath (Join-Path $TestDrive "state") -Recurse -Force -ErrorAction SilentlyContinue
        }

        It "completes ledger session ids for transcript" {
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 transcript "

            $completions | Should -Contain "default"
            $completions | Should -Contain "run-abc"
        }

        It "strips the jsonl extension from completed session ids" {
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 transcript "

            $completions | Should -Not -Contain "default.jsonl"
        }

        It "stops completing session ids once one is supplied" {
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 transcript default "

            $completions | Should -Not -Contain "run-abc"
        }
    }
```

- [ ] **Step 2: Run to verify failure**

```
pwsh -NoProfile -Command "Invoke-Pester -Path scripts/qsf-completion.Tests.ps1 -Output Detailed"
```

Expected: the first test FAILS because `transcript` is not in the command list, and the session-id
tests FAIL because nothing enumerates ledgers.

- [ ] **Step 3: Implement**

Add `"transcript",` to `$script:QsfCompletionCommands` immediately after `"goals",`.

Add the ledger enumerator next to `Get-QsfCompletionContinuitySessionIds`, which it deliberately
parallels — same shape, different directory and a stripped extension:

```powershell
function Get-QsfCompletionLedgerSessionIds {
    param(
        [string]$StateDir = "state/realtime"
    )

    $ids = [System.Collections.Generic.List[string]]::new()
    $diagnosticsRoot = Join-Path $script:QsfCompletionProjectRoot (Join-Path $StateDir "diagnostics")
    if (Test-Path -LiteralPath $diagnosticsRoot -PathType Container) {
        Get-ChildItem -LiteralPath $diagnosticsRoot -File -Filter "*.jsonl" -ErrorAction SilentlyContinue |
            ForEach-Object { $ids.Add($_.BaseName) }
    }

    return @($ids | Sort-Object -Unique)
}
```

**Teach the shared classifier which switches take values.** `Get-QsfCompletionGoalsContext` currently
advances two tokens past anything beginning with `-`, which is right for `-StateDir <path>` and wrong
for the valueless switches `transcript` introduces: `transcript -All <TAB>` would treat the next token
as `-All`'s value, so a positional session id that follows a switch is miscounted. Add an explicit
value-taking set instead of assuming every flag takes one:

```powershell
    # Flags that consume a following value. Everything else is a valueless switch, so the classifier
    # must advance one token past it or a positional that follows gets swallowed as its value.
    $valueFlags = @("-StateDir")
    $stateDir = "state/realtime"
    $positionalCount = 0
    $index = 1
    while ($index -lt $Arguments.Count) {
        $token = $Arguments[$index]
        if ($token -like "-*") {
            if ($valueFlags -contains $token) {
                if ($token -eq "-StateDir" -and ($index + 1) -lt $Arguments.Count) {
                    $value = $Arguments[$index + 1]
                    if (-not [string]::IsNullOrWhiteSpace($value)) {
                        $stateDir = $value
                    }
                }
                $index += 2
            }
            else {
                $index += 1
            }
        }
        else {
            $positionalCount++
            $index++
        }
    }
```

This is a behavior fix for `goals` too — `goals -Something <TAB>` previously miscounted the same way
— and the existing `goals` completion tests must keep passing, which is the regression gate.

Add these tests alongside the ones from Step 1, inside the hermetic-ledger `Context`:

```powershell
        It "completes session ids after a valueless switch" {
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 transcript -Pretty "

            $completions | Should -Contain "default"
            $completions | Should -Contain "run-abc"
        }

        It "stops completing session ids when one already follows a valueless switch" {
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 transcript -Full default "

            $completions | Should -Not -Contain "run-abc"
        }

        It "still honours an explicit state dir alongside switches" {
            New-Item -ItemType Directory -Force (Join-Path $TestDrive "state/other/diagnostics") | Out-Null
            Set-Content -LiteralPath (Join-Path $TestDrive "state/other/diagnostics/elsewhere.jsonl") -Value ""

            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 transcript -All -StateDir state/other "

            $completions | Should -Contain "elsewhere"
            $completions | Should -Not -Contain "default"
        }
```

Then add the dispatch branch after the `goals` branch in the completer, reusing the now-fixed
classifier:

```powershell
        if ($nativeContext.Arguments.Count -ge 1 -and $nativeContext.Arguments[0] -eq "transcript") {
            $transcriptContext = Get-QsfCompletionGoalsContext -Arguments $nativeContext.Arguments
            if ($transcriptContext.PositionalCount -eq 0) {
                Select-QsfCompletionMatches -Values (Get-QsfCompletionLedgerSessionIds -StateDir $transcriptContext.StateDir) -WordToComplete $wordToComplete
            }
            return
        }
```

Update `Get-QsfCompletionGoalsContext`'s leading comment so it no longer claims to be about `goals`
alone: it classifies `<command> [<session-id>] [-StateDir <path>] [switches]` for both `goals` and
`transcript`, and it distinguishes value-taking flags from valueless switches.

- [ ] **Step 4: Verify and commit**

```
pwsh -NoProfile -Command "Invoke-Pester -Path scripts/qsf-completion.Tests.ps1 -Output Detailed"
```

```bash
git add scripts/qsf-completion.ps1 scripts/qsf-completion.Tests.ps1
git commit -m "Launcher: Complete the transcript command and its session ids"
```

---

## Phase 5 — Acceptance on a live session

### Task 5.1: Emit a transcript for a real conversation and read it

**Human testing required.** This is acceptance of the *tool*: does it produce a readable, complete,
correct artifact from a session driven through the launcher. It makes no claim about what the volition
matcher should have done — that question belongs to
`docs/Experiments/Experiment.GoalMatchingProbeSet.md`, which is separate work.

- [ ] **Step 1: Hold a short live conversation**

```powershell
.\scripts\qsf.ps1 realtime -RandomSessionId
```

Any conversation of four or more turns will do. Include at least one turn you expect to activate a
goal strongly (mention *AI* or *jobs*) and one you expect to activate nothing, so the artifact
exercises both a populated `volition` section and a `null` one. Then stop the session.

- [ ] **Step 2: Emit the artifact both ways**

```powershell
.\scripts\qsf.ps1 transcript -Out turns.jsonl
.\scripts\qsf.ps1 transcript -Pretty
```

- [ ] **Step 3: Check the artifact is complete and self-describing**

In `turns.jsonl`: the first line is a `session` record whose `source.complete` is `true`; there is one
`turn` line per trusted turn; no `turn` line has a non-empty `undecodable`. If `complete` is `false`,
diagnose the skipped lines it names before reading anything else — a partial artifact is not evidence
about anything.

- [ ] **Step 4: Check it against the debug UI**

For two or three turns, compare `user`, `assistant`, and the goals listed in `volition.fired` against
what the realtime debug UI showed for the same turns. This is a cross-check of the join, not of
volition: the two views read the same records, so they must agree.

- [ ] **Step 5: Decide whether the curated view is sufficient**

If a field you wanted is missing, add it to `VolitionView` or `MatchView` rather than reaching for
`-Full`; `-Full` exists for hashes and event refs, not for routine reading.

- [ ] **Step 6: Delete this plan**

Plans and reviews are ephemeral. Confirm the durable content landed in `README.md`,
`docs/Architecture/Architecture.RealtimeSessionServer.md` and `docs/DecisionLog.md`, then delete
`docs/Plans/Plan.TranscriptCli.md` and `docs/Reviews/Review.TranscriptCli.md`.

`docs/Experiments/Experiment.GoalMatchingProbeSet.md` is not part of this cleanup — it is a separate
piece of work with its own lifecycle, and it stays whether or not it has been run.

---

## Verification summary per phase

| Phase | Automated gate | Human gate |
|---|---|---|
| 1 | `cargo check -p qsf_realtime_server` after the facade edit, then `cargo test --workspace` passes with **no test edits** — a pure move | none |
| 2 | `cargo test -p qsf_app volition_continuity`, including the unknown/incompatible-kind tolerance tests | none |
| 3 | `cargo test -p qsf_app`, including the real-ledger fixture test and the partial-artifact integrity test | read a few turns; compare against the debug UI |
| 4 | `Invoke-Pester` on both launcher test files; the existing `goals` completion tests are the regression gate for the classifier fix | confirm `>` redirection yields clean JSONL |
| 5 | none | live session emits a `complete` artifact that agrees with the debug UI |

## Open questions

- **Does the ledger's oldest run still parse?** The committed fixture comes from the newest run.
  Older runs in `default.jsonl` date from early July and may predate trace fields. The loader skips
  and reports unparseable lines rather than aborting, so `-All` degrades rather than fails — but if
  many lines skip, that is worth knowing before relying on `-All`. Answer it cheaply during Task 3.4
  by running the command with `--all` and reading the stderr warnings.
- **Should `qsf.ps1 goals` and `qsf.ps1 transcript` agree on "the current session"?** They resolve
  independently: `goals` picks a continuity session directory, `transcript` picks the newest ledger
  file. For a normal run these name the same session, but after a `-RandomSessionId` run followed by
  a sleep update they could diverge. Left as is deliberately — sharing a resolver would couple two
  commands to one directory layout — but worth revisiting if it ever surprises anyone in practice.
