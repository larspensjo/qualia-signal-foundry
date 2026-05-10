# Engineering Diary

Chronological "what happened" log: every submitted code change, plus research findings,
planning notes, surprises, and open questions encountered during work. This is Stage 1 of
the project workflow; entries may later be promoted to concept notes, research questions,
experiments, or decisions.

How to use:
- Add one entry per logical change. A logical change can span several related commits.
- Every code change submitted must be reflected by some diary entry. Non-code activities
  (research, planning, observations, things tried that did not pan out) also belong here.
- Decisions and commitments belong in `DecisionLog.md`, not here.
- Keep entries short and reference concrete artifacts.
- New entries go to the end of the file.
- If a change implements a prior decision, note it in the Refs line.
- Don't reference planning documents. Entries shall stand on their own, even after plans are archived.

Entry template (only the topic line and summary are mandatory; add other sections when
they apply):

## YYYY-MM-DD - <topic>

<one or two sentence summary>

What changed:
- <bullet>

Observed:
- <bullet>

Open question:
- <bullet>

Refs: <files, commits>; implements: <decision title> (if applicable)

## 2026-05-09 - Workspace skeleton and placeholder app

Phase 1 of the Framework MVP landed: a buildable Cargo workspace pairing the existing
`engine_logging` crate with a thin new `qsf_app` application crate.

What changed:
- Cargo workspace set up with `engine_logging` and `qsf_app` as members.
- `qsf_app` gained a basic CLI, placeholder experiment registration, and
  `engine_logging` integration.
- `.gitignore` extended to cover generated run and log outputs.

Refs: Cargo.toml, Cargo.lock, crates/qsf_app, crates/engine_logging,
docs/Plans/Plan.FrameworkMVP.md

## 2026-05-09 - Event log and trace MVP

Phase 2 of the Framework MVP landed: placeholder experiments now produce separate
per-run artifacts for developer logs, chronological events, explanatory traces, and a
Markdown report.

What changed:
- `RunContext` introduced to own the per-run output directory and the JSONL writers
  for event and trace logs.
- `engine_logging` initialization redirected to `runs/<run-id>/engine.log` per run,
  keeping it as the developer/operator logging layer.
- Per-run Markdown report artifact generated alongside the JSONL streams.

Refs: crates/qsf_app/src/runtime/run_context.rs,
crates/qsf_app/src/observability/event_log.rs,
crates/qsf_app/src/observability/trace.rs,
crates/qsf_app/src/reports/markdown_report.rs

## 2026-05-09 - Diary and decision-log conventions clarified

Reworked the contracts of `EngineeringDiary.md` and `DecisionLog.md` so they no longer
overlap. Diary is now an activity log + observations (every code change plus non-code
work). Decision log is deliberate commitments only.

What changed:
- Diary header rewritten; entry template reshaped around topic / What changed / Observed
  / Open question / Refs.
- Decision log header rewritten; `Type:` and `Implementation | Bug Fix` removed from the
  template since every entry is a decision by construction.
- `ProjectWorkflow.md` Stage 1, Stage 9, and the Document Responsibilities one-liners
  updated to match.

Refs: docs/EngineeringDiary.md, docs/DecisionLog.md,
docs/ProjectFrame/ProjectWorkflow.md;
implements: 2026-05-09 - Diary and decision-log document contracts

## 2026-05-09 - Experiment runner MVP

Named experiments now dispatch through a
first-class runner abstraction instead of a single placeholder function.

What changed:
- Added an `Experiment` trait, explicit registry, run summary, and placeholder
  experiment implementation.
- Moved CLI experiment execution through the runner and kept output artifacts under
  per-run directories.
- Made report sections data-driven so future experiments can provide their own
  observations, failure modes, follow-up questions, and decision candidates.

Refs: crates/qsf_app/src/experiments,
crates/qsf_app/src/runtime/run_context.rs,
crates/qsf_app/src/reports/markdown_report.rs

## 2026-05-10 - Transcript-first realtime speech planning

Accepted a transcript-first path for incorporating OpenAI realtime speech models:
streaming transcription before full speech-to-speech voice sessions.

What changed:
- Added `Experiment.StreamingTranscriptionMVP` as the first real audio provider
  experiment.
- Updated framework, audio architecture, realtime presence, audio research, and backlog
  docs to route realtime speech through QSF events.
- Verified the OpenAI realtime model IDs against current OpenAI API documentation and
  tightened the plan after review.
- Recorded the durable rule that realtime providers are side-effect adapters, not owners
  of runtime state or memory/tool decisions.

Refs: docs/Plans/Plan.FrameworkMVP.md,
docs/Experiments/Experiment.StreamingTranscriptionMVP.md,
docs/Architecture/Architecture.AudioLoop.md,
docs/Concepts/Concept.RealtimePresence.md,
docs/Research/ResearchQuestions.Audio.md,
docs/Experiments/Experiment.Backlog.md,
docs/DecisionLog.md;
implements: 2026-05-10 - Transcript-first realtime speech integration
