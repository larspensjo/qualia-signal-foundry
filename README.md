# Qualia Signal Foundry

**Qualia Signal Foundry** is an experimental platform for exploring simulations of consciousness-like behavior.

The project investigates how a software system can model presence, continuity, memory, perception, reflection, and interaction over time. The goal is not to build a productivity assistant, but to create a research playground for experimenting with artificial agents that feel more continuous, situated, and internally coherent.

This project is currently in an early research and prototyping phase.

## Goals

The project explores ideas such as:

- real-time audio interaction
- short-term and long-term memory
- associative memory
- memory decay and reinforcement
- sleep-like consolidation phases
- tool use as a form of perception
- context-budgeted cognition
- multiple AI model roles
- simulated continuity of identity

Many design questions are still open. The repository should be treated as a working lab, not a finished framework.

## Current Status

This is a work in progress.

Expect:

- incomplete features
- changing architecture
- experimental code
- evolving documentation
- research notes mixed with implementation plans

The early focus is on building enough infrastructure to run small experiments and learn from them.

### What works today

The runtime is organized as a registry of named experiments. Each run produces its own
directory of artifacts (event log, trace log, engine log, markdown report) under `runs/`.

Currently implemented experiment paths include:

- **Multi-turn text loop** with hot active turns, warm summaries, and a `recall_turn`
  tool that can fetch verbatim text from summarized turns — works with a deterministic
  mock model by default and with the live OpenAI Chat Completions API when the
  `openai` feature is enabled.
- **Streaming transcription** of microphone or WAV input via the OpenAI realtime
  transcription adapter (feature-gated).
- **Realtime voice session** and a **text-owned voice loop** that retrieves memory
  before context assembly and routes any provider-requested tool calls through the
  QSF tool boundary instead of executing them directly.
- **Sleep-phase session summary**, **reviewed memory draft**, and **accept reviewed
  memory** — the pipeline that turns a session summary into a manually reviewed
  file-backed memory source.
- **Associative memory toy model**, **context budget retrieval test**, and
  **tool-as-perception calculator** as smaller focused experiments.

Implemented infrastructure includes a pure-reducer runtime loop, an event/trace log
contract, a `ModelRole` + `ModelClient` boundary with mock and OpenAI adapters, a
tool registry with role-level allow-listing enforced at model dispatch, a versioned
memory record schema, and association-weighted memory retrieval.

Not yet implemented (documented as concepts, plans, or ideas):

- session persistence and cross-session continuity
- a volition or goal system
- self-reflection through project-document introspection
- attention/salience as a first-class signal
- a live activation dashboard

## Requirements

Recommended development environment:

- Rust
- Git
- Visual Studio Code or another Rust-capable editor
- A terminal such as PowerShell, Windows Terminal, or equivalent

Check the Rust installation with:

```powershell
rustc --version
cargo --version
```

If Rust is not installed, install it from:

```text
https://rustup.rs/
```

## Setup

Clone the repository:

```powershell
git clone https://github.com/<owner>/<repo>.git
cd <repo>
```

Build the project:

```powershell
cargo build
```

Run tests:

```powershell
cargo test
```

List the experiments available in this build:

```powershell
cargo run -p qsf_app -- list-experiments
```

Run a named experiment (replace `<name>` with one of the kebab-case ids printed
above, for example `multi-turn-text-loop`):

```powershell
cargo run -p qsf_app -- experiment <name>
```

Each run writes its artifacts into a fresh directory under `runs/`.

### Optional `openai` feature

Audio providers and the live OpenAI-backed model client are gated behind the
`openai` Cargo feature. Enabling them also requires an explicit provider selection
through environment variables; possessing an API key alone does not switch the
runtime away from the deterministic mock path.

```powershell
$env:OPENAI_API_KEY = "<key>"
$env:QSF_MODEL_PROVIDER = "openai"
cargo run -p qsf_app --features openai -- experiment multi-turn-text-loop
```

Per-experiment configuration variables (warm-summary thresholds, memory sources,
transcript providers, and so on) are documented in the corresponding plan and
experiment notes under `docs/`.

At this stage, the exact executable behavior may change frequently as the project evolves.

## Repository Structure

The repository is a Cargo workspace with documentation alongside the crates:

```text
crates/
  engine_logging/   shared logging helpers redirected per run
  qsf_app/          experiment runner, runtime, memory, models, tools, audio

docs/
  ProjectFrame/     vision, non-goals, workflow
  Concepts/         brainstorm-stage ideas
  Architecture/     candidate architecture sketches
  Plans/            in-flight plans and ideas
  Experiments/      experiment specs and reports
  Research/         research notes and references
  Reviews/          plan and code reviews
  EngineeringDiary.md   chronological log of every change and observation
  DecisionLog.md        durable record of deliberate commitments

runs/   per-run output artifacts (gitignored)
```

Tests live next to the code they cover, either as inline `#[cfg(test)]` modules or
extracted under each crate's source tree. There is no top-level `tests/` or
`examples/` directory yet.

The documentation is part of the experiment. Some documents describe stable background ideas, while others track open questions, working assumptions, design sketches, and research decisions.

## Documentation

Important documentation areas include:

```text
docs/ProjectFrame/        framing and non-goals
docs/Concepts/            speculative ideas
docs/Architecture/        candidate architecture sketches
docs/Plans/               in-flight plans and brainstorm ideas
docs/Experiments/         experiment specs and reports
docs/Research/            research notes
docs/Reviews/             plan and code review notes
docs/EngineeringDiary.md  chronological "what happened" log
docs/DecisionLog.md       deliberate commitments
```

The documentation should help both a project manager and a researcher understand:

- what the project is trying to explore
- what is already decided
- what is still open
- which experiments should be run next
- why earlier decisions were made

`EngineeringDiary.md` records every code change and notable observation in
chronological order. `DecisionLog.md` is reserved for durable commitments —
architecture rules, scope boundaries, and reusable conventions — and is the source
of truth for what the project has agreed to do going forward.

## Design Philosophy

The project should remain open-ended while still being executable.

A useful rule of thumb:

> Capture ideas early, but do not promote them to architecture too quickly.

Research notes, concept documents, experiment logs, and decision records should remain separate so that the project can evolve without prematurely locking down the design.
