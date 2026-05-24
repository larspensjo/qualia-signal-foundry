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
  mock model by default and with the live OpenAI Chat Completions API when explicitly
  selected through configuration.
- **Streaming transcription** of microphone or WAV input via the OpenAI realtime
  transcription adapter.
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
- PowerShell 7.6 (`pwsh`) in Windows Terminal or an equivalent terminal

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

### PowerShell launcher

On Windows, the documented happy path for common local launches is the repository
launcher. It is a thin wrapper over Cargo and npm: it prints the underlying command
and any child-process environment changes before execution.

```powershell
pwsh -NoProfile -File .\scripts\qsf.ps1 help
.\scripts\qsf.ps1 help
.\scripts\qsf.ps1 list experiments
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -DemoMemory
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -LaunchProfile mock
.\scripts\qsf.ps1 doctor
```

The launcher requires PowerShell 7.6 or newer.

Argument completion is opt-in per shell session. Dot-source the completion script
before using tab completion for launcher commands, profiles, experiment names, browser
store paths, and bind hosts:

```powershell
. .\scripts\qsf-completion.ps1
```

List the checked-in launch profiles:

```powershell
.\scripts\qsf.ps1 list profiles
```

Profiles apply environment variables only to the launched child process and print the
effective environment changes before running Cargo. Checked-in profiles do not contain
secrets. `-Profile` remains accepted as a compatibility alias, but new examples use
`-LaunchProfile`:

```powershell
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -LaunchProfile mock
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -LaunchProfile openai-text
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -SessionMemorySource file -SessionMemoryFile docs/Experiments/Fixtures/session-memory.empty.json
.\scripts\qsf.ps1 app -Experiment text-owned-voice-loop -LaunchProfile file-memory -VoiceMemoryFile docs/Experiments/Fixtures/voice-memory.example.json
```

`openai-text` and `openai-transcription-mic` require `OPENAI_API_KEY` to already exist
in the shell environment; the launcher checks this before starting the experiment and
does not print secret-like values.

For `multi-turn-text-loop`, the launcher passes an empty session-memory source by
default; the loop still resumes from `state/text-loop/memory-store.json` when that
store exists. Use `-DemoMemory` or `-LaunchProfile demo-memory` to opt into the
deterministic Phase 4 fixture, or use `-SessionMemorySource file -SessionMemoryFile
<path>` for a specific JSON fixture. Raw Cargo runs still use the experiment's
in-code default.

Check local prerequisites without starting Cargo, Vite, or the API server:

```powershell
.\scripts\qsf.ps1 doctor
.\scripts\qsf.ps1 doctor -LaunchProfile openai-text
.\scripts\qsf.ps1 doctor -Workbench
```

`doctor` reports PowerShell, Cargo, Rust, Node/npm, UI dependencies, the default
memory store, port `3939`, and whether `OPENAI_API_KEY` is present without printing
its value. General checks warn about optional UI or OpenAI prerequisites; `-Workbench`
turns workbench requirements into failures.

Start the memory browser API with the default store, host, and port:

```powershell
.\scripts\qsf.ps1 browser
```

The browser defaults are `state/text-loop/memory-store.json`, `127.0.0.1`, and
`3939`. To use the tracked sample store instead:

```powershell
.\scripts\qsf.ps1 browser -Store crates/qsf_browser_server/tests/fixtures/small-store.json -BindHost 127.0.0.1 -Port 3939
.\scripts\qsf.ps1 browser crates/qsf_browser_server/tests/fixtures/small-store.json
```

Start the Vite UI from `crates/qsf_browser_server/ui`:

```powershell
.\scripts\qsf.ps1 ui
```

If UI dependencies are missing, run:

```powershell
cd crates/qsf_browser_server/ui
npm install
```

To start the API in the current terminal and the UI in a separate PowerShell window:

```powershell
.\scripts\qsf.ps1 workbench
.\scripts\qsf.ps1 workbench crates/qsf_browser_server/tests/fixtures/small-store.json
```

To stop the workbench, press Ctrl+C in the API terminal. The launcher prints the Vite
UI process ID and attempts to close that process when the API exits.
Open the workbench at `http://localhost:5173/`; port `3939` is the backend API and
its root page only points to the Vite UI and `/api/health`.

#### Launcher troubleshooting

- **Blocked port:** `doctor` reports whether `127.0.0.1:3939` appears occupied. Stop
  the existing process or launch with another port, for example
  `.\scripts\qsf.ps1 browser -Port 3940`.
- **Missing API key:** OpenAI-backed profiles require `OPENAI_API_KEY` in the current
  shell before launch. The launcher checks presence but never prints the value.
- **Missing UI dependencies:** If `ui` or `workbench` reports missing dependencies,
  run `cd crates/qsf_browser_server/ui; npm install`.
- **Execution policy:** If the script is blocked by local PowerShell policy, use the
  one-shot bypass form:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts\qsf.ps1 help
```

- **Stale completion:** Completion currently reads checked-in profiles and static
  experiment names from `scripts/qsf-completion.ps1`. If those change, dot-source the
  completion script again in the current shell.

Raw Cargo and npm commands still work and remain useful when debugging.

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

### Memory Association Browser

The Memory Association Browser is a read-only local workbench for inspecting a
persisted memory store through `qsf_browser_server` and the Vite UI.

Launcher path:

```powershell
.\scripts\qsf.ps1 browser
.\scripts\qsf.ps1 ui
.\scripts\qsf.ps1 workbench
```

Raw fallback/reference commands:

```powershell
# Shell 1: API server on 127.0.0.1:3939
cargo run -p qsf_browser_server -- --store state/text-loop/memory-store.json --host 127.0.0.1 --port 3939

# Shell 2: Vite UI
cd crates/qsf_browser_server/ui
npm install
npm run dev
```

The tracked sample store is useful before a local continuity store exists:

```powershell
.\scripts\qsf.ps1 browser -Store crates/qsf_browser_server/tests/fixtures/small-store.json
.\scripts\qsf.ps1 workbench crates/qsf_browser_server/tests/fixtures/small-store.json
```

### OpenAI-backed providers

OpenAI-backed providers require an explicit provider selection through environment
variables; possessing an API key alone does not switch the runtime away from the
deterministic mock path.

```powershell
$env:OPENAI_API_KEY = "<key>"
$env:QSF_MODEL_PROVIDER = "openai"
cargo run -p qsf_app -- experiment multi-turn-text-loop
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
