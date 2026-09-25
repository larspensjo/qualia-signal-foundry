# Qualia Signal Foundry

**Qualia Signal Foundry** is an experimental platform for exploring simulations of consciousness-like behavior.

The project investigates how a software system can model presence, continuity, memory, perception, reflection, and interaction over time. The goal is not to build a productivity assistant, but to create a research playground for experimenting with artificial agents that feel more continuous, situated, and internally coherent.

Human minds are an important source of inspiration here, but not a ceiling. The
simulation may borrow from human cognition while also exploring super-human
traits such as exact temporal awareness, broader memory access, faster
reflection, richer self-observation, or multiple cognitive roles working in
parallel. Those capabilities should be explicit and observable rather than
hidden shortcuts.

This project is currently in an early research and prototyping phase.

The long-term target interaction is realtime voice conversation: a live,
interruptible spoken mode where QSF-owned memory, context, perception tools,
observability, and continuity sit behind the voice surface. Today's named
experiments and launcher commands are scaffolding for reaching that mode.

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
- non-human or super-human cognitive traits when they clarify the simulation

Many design questions are still open. The repository should be treated as a working lab, not a finished framework.

## Current Status

This is a work in progress.

Expect:

- incomplete features
- changing architecture
- experimental code
- evolving documentation
- research notes mixed with implementation plans

The early focus is on building enough infrastructure to run small experiments and
learn from them while moving toward the realtime conversation mode.

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
- **Realtime voice session preview** via `qsf_realtime_server` plus its dedicated
  browser UI, using server-side SDP rendezvous and diagnostic-only browser relay
  events.
- **Realtime voice session**, a **text-owned voice loop**, and a peer **voice-loop**
  surface that share the `state/session/` continuity root, retrieve memory before
  context assembly, and route any provider-requested tool calls through the QSF tool
  boundary instead of executing them directly.
- **Sleep-phase session summary**, **reviewed memory draft**, and **accept reviewed
  memory** — the pipeline that reads persisted text turns and voice exchanges,
  promotes routine memory candidates into the shared store, writes a consolidated
  brief for the next run, and keeps decision-like candidates in manually reviewed
  draft artifacts.
- **Associative memory toy model**, **context budget retrieval test**, and
  **tool-as-perception calculator** as smaller focused experiments.

Implemented infrastructure includes a pure-reducer runtime loop, an event/trace log
contract, a `ModelRole` + `ModelClient` boundary with mock and OpenAI adapters, a
tool registry with role-level allow-listing enforced at model dispatch, a versioned
memory record schema, and association-weighted memory retrieval.
Text and voice interaction now share one continuity universe by default, and a sleep
pass over either modality feeds the same memory store and consolidated-brief resume
path.

Not yet implemented (documented as concepts, plans, or ideas):

- the first-class, human-verified browser-based realtime voice conversation mode
- a first-class launcher command for that realtime mode
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

The app side of the launcher is experiment-centric (`app -Experiment ...`) and remains
the harness for regression and fixture-backed experiments. The realtime voice
conversation path is now a first-class launcher command, `realtime`, which starts the
`qsf_realtime_server` API and its browser UI together (see below).

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

Before launching `qsf_app`, the launcher clears known and ambient non-secret `QSF_*`
variables from the child process, then applies launcher defaults, explicit flags, and
the selected profile. API keys and other secret-like variables remain inherited and are
checked only when a profile requires them. The effective environment changes are printed
before running Cargo. Checked-in profiles do not contain secrets. `-Profile` remains
accepted as a compatibility alias, but new examples use `-LaunchProfile`:

```powershell
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -LaunchProfile mock
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -LaunchProfile openai-text
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -SessionMemorySource file -SessionMemoryFile docs/Experiments/Fixtures/session-memory.empty.json
.\scripts\qsf.ps1 app -Experiment text-owned-voice-loop -LaunchProfile file-memory -VoiceMemoryFile docs/Experiments/Fixtures/voice-memory.example.json
.\scripts\qsf.ps1 app -Experiment voice-loop
```

`openai-text` and `openai-transcription-mic` require `OPENAI_API_KEY`; the launcher
checks this before starting the experiment and does not print secret-like values.

When a command needs an API key, the launcher injects it from PowerShell SecretStore: it
relaunches itself with the same arguments through `Invoke-WithSecretMap` (from the
PowerShell profile's SecretLaunch module). The key then exists only in that relaunched
launcher and the processes it starts, never in your shell. This needs an interactive
terminal, because SecretStore may prompt for its password.

- SecretStore entry `OpenAIProductionKey` becomes `OPENAI_API_KEY` for `realtime`,
  `probe`, `sleep` with the `openai` provider, and OpenAI-backed `app` profiles. An
  `OPENAI_API_KEY` already set in the shell is used as-is.
- SecretStore entry `TypesafeAiApiKey` (the application key) becomes `TYPESAFE_API_KEY`
  for `bench`, always. A persistent `TYPESAFE_API_KEY` in the shell belongs to agent
  plugins and is never used: the launcher removes it from the relaunch before injecting
  the application key.

For `multi-turn-text-loop`, the launcher passes an empty session-memory source by
default; the loop still resumes from `state/session/memory-store.json` when that
store exists, with a read-only fallback to `state/text-loop/memory-store.json` for
legacy continuity. Use `-DemoMemory` or `-LaunchProfile demo-memory` to opt into the
deterministic Phase 4 fixture, or use `-SessionMemorySource file -SessionMemoryFile
<path>` for a specific JSON fixture. Raw Cargo runs still use the experiment's
in-code default.

Check local prerequisites without starting Cargo, Vite, or the API server:

```powershell
.\scripts\qsf.ps1 doctor
.\scripts\qsf.ps1 doctor -LaunchProfile openai-text
.\scripts\qsf.ps1 doctor -Workbench
```

`doctor` reports PowerShell, Cargo, Rust, Node/npm, UI dependencies (browser and
realtime), the default memory store, ports `3939` and `3940`, and whether
`OPENAI_API_KEY` and `TYPESAFE_API_KEY` are present or can be injected from SecretStore
(`TYPESAFE_API_KEY` is always injected, whatever the shell holds),
without printing their values. General checks warn about
optional UI or OpenAI prerequisites; `-Workbench` turns workbench requirements into
failures.

Start the memory browser workbench with the default store, API host, and API port:

```powershell
.\scripts\qsf.ps1 browser
```

The browser command starts both the Rust API and the Vite UI, resolves the first
free UI port at or above `5173`, and opens that UI automatically. The API defaults
are `state/realtime/continuity/default/memory-store.json`, `127.0.0.1`, and `3939`.
Run `.\scripts\qsf.ps1 realtime` followed by `.\scripts\qsf.ps1 sleep` to create
the default store, or use the tracked sample store instead:

```powershell
.\scripts\qsf.ps1 browser -Store crates/qsf_browser_server/tests/fixtures/small-store.json -BindHost 127.0.0.1 -Port 3939
.\scripts\qsf.ps1 browser crates/qsf_browser_server/tests/fixtures/small-store.json
```

Start the Vite UI from `crates/qsf_browser_server/ui`:

```powershell
.\scripts\qsf.ps1 ui
```

The `ui` command takes an optional target; `ui realtime` starts the Vite UI from
`crates/qsf_realtime_server/ui` instead (used by the `realtime` command below):

```powershell
.\scripts\qsf.ps1 ui realtime
```

If UI dependencies are missing, run:

```powershell
cd crates/qsf_browser_server/ui
npm install
```

To start the API and UI together through the workbench alias:

```powershell
.\scripts\qsf.ps1 workbench
.\scripts\qsf.ps1 workbench crates/qsf_browser_server/tests/fixtures/small-store.json
```

To stop the workbench, press Ctrl+C in the launcher terminal. The launcher opens the
chosen Vite UI URL automatically once both the UI and API are reachable; port `3939`
is the default backend API port and its root page is informational.

Start a live realtime voice conversation — the realtime server and its browser UI
together — in one command:

```powershell
.\scripts\qsf.ps1 realtime
.\scripts\qsf.ps1 realtime -RandomSessionId
.\scripts\qsf.ps1 realtime -StateDir state/realtime-isolated
```

`realtime` verifies `OPENAI_API_KEY` is present (the server requires it and the value
is never printed), checks the realtime UI dependencies, starts the Vite UI in a
separate PowerShell window, opens your browser to `http://localhost:5174` once the UI
is reachable, and runs `qsf_realtime_server` in the current terminal so its logs are
visible. Press Ctrl+C in that terminal to stop; the launcher then closes the UI
window. The realtime server and UI ports are fixed at `3940` and `5174` because the
Vite dev proxy is pinned to the server, so `realtime` does not take `-Port`/`-BindHost`.
By default it uses the stable `default` QSF session id (reusing local memory and
continuity); pass `-RandomSessionId` to allocate a fresh session id per run.
`-StateDir` selects the server's state directory (default `state/realtime`), which
is what lets a session be captured in isolation from earlier runs — the diagnostics
ledger is opened in append mode, so sessions sharing a state directory share a file.

Run a scripted conversation headlessly, with no browser and no microphone:

```powershell
.\scripts\qsf.ps1 probe
.\scripts\qsf.ps1 probe -PhraseSet smoke
.\scripts\qsf.ps1 probe -PhraseSet smoke -ColdStart -TurnDelayMs 500
```

`probe` runs a checked-in phrase script end to end against the live OpenAI Realtime
API and leaves a realtime-shaped artifact tree, so an analysis corpus can be produced
on demand instead of by holding a voice conversation. **It spends real money**: the
default `designed` phrase set is a twelve-turn live run, including audio-modality
output tokens. Use `-PhraseSet smoke` for the short set. It verifies `OPENAI_API_KEY`
is present (never printing it), pins `QSF_MODEL_PROVIDER=openai` while clearing the
other non-secret `QSF_*` values, and writes an isolated run directory
`state/probe/<run-id>` — the launcher refuses an existing directory rather than
reusing one. Every run ends with a terminal `run-manifest.json` carrying its verdict;
a failed run exits non-zero. Pass `-ColdStart` to skip the warm-start seed state and
`-WorldCorpusPath` to point world consultation at a local corpus.

A probe run directory is a normal continuity state directory, so the inspection
commands read it unchanged:

```powershell
.\scripts\qsf.ps1 transcript -StateDir state/probe/<run-id> -Full
.\scripts\qsf.ps1 goals -StateDir state/probe/<run-id>
.\scripts\qsf.ps1 sleep -StateDir state/probe/<run-id> -NoBackup
```

After a realtime session ends, run a first-class sleep/consolidation update over
the realtime state:

```powershell
.\scripts\qsf.ps1 sleep
.\scripts\qsf.ps1 sleep -Provider mock
.\scripts\qsf.ps1 sleep -StateDir state/realtime -Provider openai
```

`sleep` defaults to `state/realtime` and the `openai` provider, matching the
normal realtime workflow. It verifies `OPENAI_API_KEY` for OpenAI-backed runs,
then calls `qsf_app sleep` to produce reviewable sleep artifacts, update the
consolidated brief and memory store, and mark the consumed session in the
continuity manifest. It backs the state directory up first; `-NoBackup` skips that
backup, which is the documented form for a probe follow-on, since a unique run-id
leaf per run would otherwise accumulate backups that crowd the `restore` listing.
The direct Cargo form is:

```powershell
cargo run -p qsf_app -- sleep --state-dir state/realtime --provider openai
```

Inspect the full persisted volition goal detail from the newest continuity session:

```powershell
.\scripts\qsf.ps1 goals
.\scripts\qsf.ps1 goals > goals.jsonl
.\scripts\qsf.ps1 goals -Pretty
.\scripts\qsf.ps1 goals -Out goals.jsonl
.\scripts\qsf.ps1 goals | ConvertFrom-Json  | Where-Object kind -eq 'goal'
```

`goals` emits a self-describing JSONL session header followed by one full-detail goal record per
line. Use `-Pretty` for the existing human-readable console view, or `-Out <path>` to write either
output mode directly to a file. Pass an explicit session id to bypass continuity-session
auto-selection.

Read the newest realtime diagnostics run as a transcript joined to its volition traces:

```powershell
.\scripts\qsf.ps1 transcript
.\scripts\qsf.ps1 transcript > turns.jsonl
```

`transcript` emits a session record followed by one JSONL turn per trusted exchange. The
launcher uses `Write-Host` for banners, and Cargo's build output uses other streams, so `>`
captures only the JSONL. Use `-Out turns.jsonl` to have the command write the file directly
instead. Pass an explicit session id to bypass ledger auto-selection, or `-All` to emit every run.
Every session record carries `source`: `source.complete` is `false` when a line was skipped or a
trace was orphaned, and each turn's `undecodable` lists record kinds this build could not read.
Runs recorded before 2026-07-19 contain world-consultation records this build cannot decode, so
those runs report `source.complete: false`.
The default curated output contains no floating-point values; `--full` embeds traces verbatim and
can contain floating point because `qsf_corpus::QueryCandidate.score` is `f64`.

#### Launcher troubleshooting

- **Blocked port:** `doctor` reports whether `127.0.0.1:3939` and `127.0.0.1:3940`
  appear occupied. For the browser server, stop the existing process or launch with
  another port, for example `.\scripts\qsf.ps1 browser -Port 3950`. The realtime
  server's port is fixed at `3940`; free it before running `realtime`.
- **Missing API key:** OpenAI-backed profiles and the `realtime` and `probe` commands
  require `OPENAI_API_KEY`. When it is not set, the launcher injects it from
  SecretStore, which needs your PowerShell profile loaded and an interactive terminal;
  otherwise it stops and says which is missing. The launcher never prints the value. The default `sleep` command is also
  OpenAI-backed; use `.\scripts\qsf.ps1 sleep -Provider mock` for a deterministic
  local smoke run.
- **Probe state directory already exists:** `probe` refuses to write into an existing
  directory so two runs cannot be conflated in one appended diagnostics ledger. Pass a
  different `-StateDir`, or let the launcher mint a fresh `state/probe/<run-id>`.
- **Missing UI dependencies:** If `ui` or `workbench` reports missing dependencies,
  run `cd crates/qsf_browser_server/ui; npm install`. For `realtime` (or `ui
  realtime`), run `cd crates/qsf_realtime_server/ui; npm install`.
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
persisted memory store through `qsf_browser_server` and the Vite UI. The launcher
starts both processes, opens the Vite UI, and chooses a free UI port when the
preferred `5173` is occupied.

Launcher path:

```powershell
.\scripts\qsf.ps1 browser
.\scripts\qsf.ps1 ui
.\scripts\qsf.ps1 workbench
```

Raw fallback/reference commands:

```powershell
# Shell 1: API server on 127.0.0.1:3939
cargo run -p qsf_browser_server -- --store state/realtime/continuity/default/memory-store.json --host 127.0.0.1 --port 3939

# Shell 2: Vite UI
cd crates/qsf_browser_server/ui
npm install
npm run dev -- --port 5173 --strictPort
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

### Relevance-judge bench

`qsf_semantics` has a direct Cargo entry point for measuring the pair-scoring backend
selected by `QSF_RELEVANCE_JUDGE_BACKEND`. Backend selection is explicit; having an API
key in the environment does not select the hosted service. The default is the deterministic
`fixture` backend, whose results are synthetic and are not relevance evidence.

The launcher runs it against the hosted System One service with the pinned model
`jev-1.13.0`, always using the application key from SecretStore entry `TypesafeAiApiKey`
as `TYPESAFE_API_KEY`, never the agent key in the shell:

```powershell
.\scripts\qsf.ps1 bench -DryRun
.\scripts\qsf.ps1 bench -NetworkDescription "home fibre, wired"
.\scripts\qsf.ps1 bench -LocalOverheadMs 40
```

`bench` pins the backend, base URL and model and clears every other non-secret `QSF_*`
variable, so ambient retry or concurrency overrides never reach a measurement.

To run it directly, pin the versioned model id and supply the application key (not the
agent key). The endpoint is
`POST https://api.typesafe.ai/v1/systemone`:

```powershell
$env:QSF_RELEVANCE_JUDGE_BACKEND = "remote_http"
$env:QSF_RELEVANCE_JUDGE_BASE_URL = "https://api.typesafe.ai"
$env:QSF_RELEVANCE_JUDGE_MODEL = "jev-1.13.0"
$env:TYPESAFE_API_KEY = "<application key>"

cargo run -p qsf_semantics -- bench --dry-run
cargo run -p qsf_semantics -- bench --network-description "wired office network"
```

The remote config (including `TYPESAFE_API_KEY`) is validated for a dry run, but
`--dry-run` sends no requests. Before any measurement, the bench prints each shaping and
candidate-count cell, nominal requests, worst-case requests including retries, and a cost
estimate. The default cap is 400 physical requests; change it with
`--max-total-requests`. A nominal-plan refusal writes `bench-plan.json` and sends nothing.
The backend also stops a run cleanly when its next send would exceed the cap and records
the stop in `bench-report.json`. The first request warms the connection and is discarded.
The default candidate counts are 18, 100, and 500, with 40 measured shared-state turns
per cell and 10 measured per-candidate turns at the smallest count. Cells run from the
smallest to the largest store within each shaping, with shared-state cells first. A cell
stops after three consecutive failed invocations, records the stop and last failure, and
the bench continues with the next cell. The preflight plan retains all configured
repetitions and its full request and cost estimate. Larger per-candidate
cells are labeled derived-not-measured using per-attempt latency and concurrency waves.
The p95 is withheld below 20 successful samples, and p99 below 100. Failed invocations
never enter end-to-end percentiles or a deadline verdict. The preflight token estimate
uses the actual serialized hosted request body with a bytes-per-token heuristic; the final
measured cost uses observed provider token counts only.

Rate feasibility uses a configurable target of 10 spoken turns per minute and two judge
workloads per turn, memory and goal, sharing the documented 1,200 requests per minute.
At 100 candidates the per-candidate shape would need 2,000 requests per minute and is
rate-limit-infeasible; its maximum sustainable pace is 6 turns per minute. The shared
state shape is measured at 500 candidates. The plan flags heuristic token-limit risks;
the report also compares provider-reported per-request input tokens with the documented
cap. Vendor limits and their read date are stored as report inputs.

Reports and the preflight plan are written under `runs/<run-id>/`; nothing is copied into
`evaluation/reports/` automatically. After reviewing a run, an operator may deliberately
freeze its report, for example:

```powershell
$RunId = "<reviewed-run-id>"
Copy-Item "runs/$RunId/bench-report.json" "evaluation/reports/relevance-judge-bench.$RunId.json"
```

The report records the pinned and resolved model ids, endpoint, wording version,
machine/network description, commit and dirty state, date, request timeout, concurrency,
retry policy, and per-shaping derived injection deadline at the largest store size,
alongside the largest measured store that fits the fixed limit for each shaping and
overall (with its p95-derived deadline and sample count),
and the stated local-overhead input. Local overhead defaults to 0 ms as a lower-bound
assumption; supply the measured candidate/context assembly and send overhead with
`--local-overhead-ms`. The deadline calculation always uses
`MAX_ADDED_TIME_TO_FIRST_AUDIO_MS = 300` and does not raise that limit. Cost comes only
from observed usage and the checked-in price table at
`crates/qsf_semantics/prices/price-table.v1.json`; an unpriced model reports tokens without
cost. `jev-latest` is not priced or selected by this command's documented operating point.

Only the `bench` subcommand is implemented from the planned `score` / `bench` / `verify`
binary surface. One-shot `score` and read-only `verify` remain unspecified follow-ups.

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
docs/DecisionLog.md       deliberate commitments
```

The documentation should help both a project manager and a researcher understand:

- what the project is trying to explore
- what is already decided
- what is still open
- which experiments should be run next
- why earlier decisions were made

The git commit log records implementation chronology. `DecisionLog.md` is reserved
for durable commitments — architecture rules, scope boundaries, and reusable
conventions — and is the source of truth for what the project has agreed to do
going forward.

## Design Philosophy

The project should remain open-ended while still being executable.

A useful rule of thumb:

> Capture ideas early, but do not promote them to architecture too quickly.

Research notes, concept documents, experiment logs, and decision records should remain separate so that the project can evolve without prematurely locking down the design.
