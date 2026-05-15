# Plan: Reviewed Memory Promotion

## Status

Draft

## Purpose

Create a reviewable bridge from sleep-phase outputs to file-backed voice memory.

The goal is not automatic memory mutation. The goal is to turn sleep-report memory
candidates into a draft `MemoryFixture` file that can be inspected, edited, accepted,
and then explicitly used by the text-owned voice loop.

## Context

The text-owned voice loop can now load memory from an explicit source:

```powershell
$env:QSF_VOICE_MEMORY_SOURCE="file"
$env:QSF_VOICE_MEMORY_FILE="path\to\voice-memory.json"
```

The repeatable fixture at `docs/Experiments/Fixtures/voice-memory.example.json`
proves the file-backed interface. The next question is how records get created,
reviewed, and accepted into that file.

Sleep-phase outputs already produce provisional `memory_candidates`, `open_questions`,
`decision_candidates`, and `future_context_hints`. Those candidates should remain
reviewable and must not silently become durable memory.

## Principles

- Sleep output remains provisional.
- Durable memory changes require explicit review.
- Voice-loop memory source remains explicit through `QSF_VOICE_MEMORY_SOURCE=file`.
- Generated memory records must preserve source references.
- First pass creates records only; associations can come later.
- Generated artifacts should be inspectable without reading event logs.
- The live voice loop must not run sleep conversion implicitly.

## Stage 1: Draft Conversion And Minimal Review Artifact

Input:

```text
runs/<sleep-run>/sleep-report.json
```

Output location:

```text
runs/<conversion-run>/reviewed-memory-draft.json
runs/<conversion-run>/reviewed-memory-draft.md
```

The conversion should be a distinct experiment or command that references the source
sleep run. It should not write new artifacts into the original sleep run directory.
Keeping the conversion in its own run directory makes the review boundary visible and
avoids coupling sleep summarization to memory promotion.

Behavior:

- Read `SleepReport.memory_candidates`.
- Convert each candidate into a `MemoryRecord`.
- Write a `MemoryFixture` JSON with `records` and empty `associations`.
- Write a minimal Markdown review companion.
- Preserve `source_reference` when present, and provide a deterministic fallback when
  missing.
- Default missing importance to a low provisional value.
- Use current memory and association schema versions.
- Signal draft status through the filename and Markdown checklist.

Required record mapping:

| MemoryRecord field | Source |
|---|---|
| `schema_version` | current `MEMORY_RECORD_SCHEMA_VERSION` |
| `id` | deterministic `memory.sleep.<source-sleep-run-id>.<index>` |
| `kind` | `observation` by default |
| `title` | deterministic title derived from the summary |
| `summary` | sleep memory candidate summary |
| `tags` | simple generated tags or empty list |
| `created_at` | conversion time or source run timestamp |
| `importance` | candidate importance or default `0.3` |
| `reinforcement_count` | `0` |
| `source_reference` | candidate source reference, or fallback `sleep-run:<source-sleep-run-id>#memory_candidates[<index>]` |
| `estimated_tokens` | deterministic heuristic from summary length |

Deterministic derivation rules:

- `source-sleep-run-id`: the source sleep run directory name, sanitized for memory ids.
- `index`: zero-padded candidate index, starting at `001`.
- `title`: first sentence of `summary`, trimmed at a word boundary to at most 64
  characters. If empty after trimming, use `Sleep memory candidate <index>`.
- `estimated_tokens`: `max(1, ceil(summary.chars().count() / 4))`.
- `importance`: clamp candidate importance to `0.0..=1.0`; use `0.3` when absent.
- `source_reference`: always non-empty because `MemoryRecord.source_reference` is
  required.

Verification:

- Unit test conversion from one structured sleep memory candidate.
- Unit test conversion from string-only memory candidate.
- Ensure schema versions are current.
- Ensure empty memory candidates still produce a valid empty fixture.
- Ensure repeated conversion of the same sleep report produces the same ids.
- Ensure the Markdown review artifact includes the source sleep run id and candidate
  indexes.

## Stage 2: Expanded Review Artifact

Expand the generated Markdown companion after draft conversion is stable.

Include:

- source sleep report path
- source sleep run id
- review policy: provisional until manually accepted
- each candidate memory record
- importance
- source reference
- generated tags
- review checklist
- command for testing with `QSF_VOICE_MEMORY_FILE`

Suggested checklist:

```text
- Is this memory grounded in the source report?
- Is the summary compact and reusable?
- Is the source reference specific enough?
- Should the kind remain observation?
- Should tags be added or removed?
- Should this candidate be rejected?
```

Verification:

- Run sleep-phase experiment.
- Generate draft files.
- Confirm a human can inspect records without reading JSON directly.

## Stage 3: File-Backed Voice Test

Use reviewed draft as voice memory:

```powershell
$env:QSF_VOICE_MEMORY_SOURCE="file"
$env:QSF_VOICE_MEMORY_FILE="runs\<conversion-run>\reviewed-memory-draft.json"
cargo run -p qsf_app --features openai -- experiment text-owned-voice-loop
```

Success criteria:

- diagnostics show `Memory source: file`
- diagnostics show expected memory record count
- selected memory comes from the reviewed draft
- answer reflects selected memory
- exact speech handoff remains `true`
- raw audio logged remains `false`

## Stage 4: Association Drafts

Add optional association suggestions after record conversion is stable.

Rules:

- associations remain draft
- association reason is required
- association endpoints must exist in the draft or accepted fixture
- no silent reinforcement
- keep graph small and inspectable

Possible inputs:

- explicit association suggestions from sleep reports
- shared tags among accepted candidates
- human-authored association notes

Verification:

- Generated associations pass existing schema validation.
- Association endpoints are validated against the draft or accepted fixture records.
- Omitted or weak association candidates are visible in the review artifact.
- Voice retrieval traces expose association paths when they influence selection.

## Stage 5: Acceptance Workflow

Define how a reviewed draft becomes durable voice memory.

Initial conservative option:

```text
docs/Experiments/Fixtures/voice-memory.reviewed.json
```

Acceptance is manual file edit/copy, not automatic mutation.

Possible workflow:

1. Generate `reviewed-memory-draft.json`.
2. Inspect `reviewed-memory-draft.md`.
3. Edit or reject candidate records.
4. Copy accepted records into `voice-memory.reviewed.json`.
5. Run the voice loop with `QSF_VOICE_MEMORY_FILE` pointing at the reviewed file.

Rejected candidates do not need a durable rejected-record log in the first pass. If a
candidate is not copied into the accepted file, rejection is implicit. A rejected-item
ledger can be added later if repeated proposals become noisy.

## Relationship To Idea Documents

### Self-Reflection Through Project Introspection

Reviewed memory promotion is a conservative project-introspection path:

- sleep-phase output is treated as an artifact to inspect
- conversion is explicit and observable
- accepted memory remains a reviewed file-backed source
- live voice turns only retrieve from the selected source

This keeps project introspection read-only and reviewable before it becomes active
memory.

### Volition And Goal System

This plan does not implement goals or volition.

It prepares a future path where sleep or reflection can propose memory or goal
candidates, but durable adoption still requires explicit review. That preserves the
boundary between internal initiative and uncontrolled external agency.

## Non-Goals

- no automatic durable memory writes
- no hidden background sleep process
- no live-loop sleep conversion
- no goal-system integration yet
- no autonomous document edits
- no accepted decision promotion
- no production memory database

## Open Questions

- Should draft conversion be a new experiment, a CLI subcommand, or both?
- Should tag extraction be deterministic keyword matching or model-assisted?
- Should accepted memory files live under `docs/Experiments/Fixtures/` or a dedicated
  `memory/` directory?
- Should reviewed memory retain a link to the exact sleep run artifact?
- Should future context hints become memory candidates, separate context hints, or
  neither?

## First Implementation Slice

Implement draft conversion plus a minimal Markdown review artifact:

```text
runs/<sleep-run>/sleep-report.json
  -> runs/<conversion-run>/reviewed-memory-draft.json
  -> runs/<conversion-run>/reviewed-memory-draft.md
```

Keep associations empty, use deterministic ids, add unit tests, and do not connect the
draft automatically to the voice loop.
