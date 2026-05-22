# Plan: PowerShell Launcher

## Status

Phase 4 doctor and diagnostics is implemented.

## Goal

Make common local development launches predictable and discoverable without hiding the
underlying Cargo, Vite, and environment-variable behavior.

The first target is a single PowerShell entry point for starting:

- `qsf_app` experiments
- `qsf_browser_server`
- the Memory Association Browser Vite UI
- the API server plus UI together as a local workbench

The launcher should reduce copy/paste setup, make defaults visible, and support
PowerShell argument completion once the basic commands are stable.

## Background

Starting the project is becoming operator-heavy:

- `qsf_app` uses Cargo subcommands and experiment names.
- Real-provider paths require explicit environment variables such as
  `QSF_MODEL_PROVIDER`, transcript provider variables, voice provider variables, and
  memory-source variables.
- `qsf_browser_server` has its own flags for `--store`, `--host`, and `--port`.
- The browser workbench also needs the Vite dev server in
  `crates/qsf_browser_server/ui`.

The current command surface is still valuable and should remain the source of truth.
This plan adds an operator convenience layer on top of existing binaries rather than
changing the Rust CLIs first.

## Target Shape

The intended user-facing commands below describe the end-state across all phases.
Phase 1 delivers the core launch commands; later phases add profiles, completion, and
diagnostics.

```powershell
.\scripts\qsf.ps1 app                                                # Phase 1
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop                # Phase 1
.\scripts\qsf.ps1 browser                                             # Phase 1
.\scripts\qsf.ps1 browser -Store state/text-loop/memory-store.json -BindHost 127.0.0.1 -Port 3939
.\scripts\qsf.ps1 ui                                                 # Phase 1
.\scripts\qsf.ps1 workbench                                          # Phase 1
.\scripts\qsf.ps1 list experiments                                   # Phase 1
.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -Profile openai-text # Phase 2
.\scripts\qsf.ps1 list profiles                                      # Phase 2
.\scripts\qsf.ps1 doctor                                             # Phase 4
```

The launcher prints the effective command and environment changes before execution.
Profiles can set environment variables for the child process without permanently
mutating the caller's shell session.

The launcher-side host parameter is `-BindHost` to avoid shadowing PowerShell's
automatic `$Host` variable. It still forwards to the Rust server's `--host` flag.

## Design Principles

- Keep one primary launcher script: `scripts/qsf.ps1`.
- Keep the Rust CLIs independently runnable.
- Treat profiles as explicit launch presets, not hidden global defaults.
- Require PowerShell 7.6 (`pwsh`) for launcher execution.
- Prefer readable PowerShell over a framework-heavy task runner.
- Make default commands exercise useful local paths.
- Keep secret values out of checked-in files.
- Keep `main.rs`, `lib.rs`, and existing Rust entry points unchanged unless a later
  phase proves the launcher needs a real CLI affordance.

## Non-Goals

Not in scope for the first implementation:

- Replacing Cargo commands or hiding them from documentation.
- Cross-platform shell support for Bash, zsh, or fish.
- A graphical launcher.
- Automatically obtaining or storing OpenAI API keys.
- Supervising production services.
- Changing `qsf_app` provider-selection semantics.
- Changing `qsf_browser_server` API behavior.

## Open Questions

- Should `scripts/qsf.profiles.local.json` be supported for private machine-specific
  profiles in addition to checked-in safe profiles?
- Should argument completion be loaded manually by dot-sourcing a script, or should
  the launcher install a completion registration into the user's PowerShell profile?
- Should `workbench` automatically open the browser after both processes start?

The local-profile and completion-install questions should be answered before Phase 3
or Phase 5. Browser auto-open can remain optional until the `workbench` command has
real operator feedback.

## Resolved Implementation Choices

- `app` with no `-Experiment` prints launcher help plus the experiment table. It does
  not prompt and does not run an experiment implicitly.
- `workbench` starts the Vite UI in a separate visible PowerShell process and runs
  `qsf_browser_server` in the current terminal foreground. The primary terminal owns
  API logs and Ctrl+C stops the API directly. Stopping the full workbench means
  pressing Ctrl+C in the API terminal and closing the separate Vite PowerShell window.
- Phase 1 supports `-BindHost`, not `-Host`, while forwarding to
  `qsf_browser_server --host`.
- Phase 1 requires PowerShell 7.6 and starts the workbench UI child process with
  `pwsh`, not Windows PowerShell.
- Phase 1 `browser` detects a missing default store before starting Cargo and points
  the user at `crates/qsf_browser_server/tests/fixtures/small-store.json` as a known
  sample store.
- Phase 1 `ui` and `workbench` check for `crates/qsf_browser_server/ui/node_modules`
  before running `npm run dev`. If dependencies are missing, they print
  `npm install` instructions for the UI directory instead of surfacing a raw Vite
  failure.
- Profiles use the JSON schema defined in Phase 2 before completion or diagnostics
  depend on them.

## Documents To Update

Per `docs/ProjectFrame/ProjectWorkflow.md`:

- `docs/EngineeringDiary.md` — add one entry when launcher implementation lands.
- `README.md` — update setup/run instructions once Phase 1 is usable.
- `docs/Plans/Plan.MemoryAssociationBrowser.md` — after `workbench` exists, replace
  repeated API + Vite launch snippets with a reference to the launcher where helpful.
- `docs/DecisionLog.md` — only update if the project commits to a durable convention,
  such as "operator launch presets live in PowerShell profiles under `scripts/`."
- `docs/Architecture/` — no architecture update expected; this is developer tooling.

## Phase 0: Command Inventory

Capture the actual launch surface before scripting it.

### Phase 0 Inventory

Fill this subsection before Phase 1 implementation begins. This inventory is the
durable reference for the first launcher version.

Verified on 2026-05-22 from `cargo run -p qsf_app -- list-experiments`, `QSF_*`
references under `crates/`, `crates/qsf_browser_server/src/cli.rs`, and
`crates/qsf_browser_server/ui/package.json`.

- Experiment IDs:
  - `framework-skeleton-mvp`
  - `audio-preparation-layer`
  - `associative-memory-toy-model`
  - `context-budget-retrieval-test`
  - `model-role-smoke-test`
  - `multi-turn-text-loop`
  - `realtime-voice-session`
  - `accept-reviewed-memory`
  - `reviewed-memory-draft`
  - `sleep-phase-session-summary`
  - `streaming-transcription-mvp`
  - `text-owned-voice-loop`
  - `tool-as-perception-calculator`
- Environment variables:
  - Model:
    - `QSF_MODEL_PROVIDER`
    - `QSF_CONVERSATION_MODEL`
  - Transcript:
    - `QSF_TRANSCRIPT_PROVIDER`
    - `QSF_TRANSCRIPT_INPUT_SOURCE`
    - `QSF_TRANSCRIPT_WAV_PATH`
    - `QSF_TRANSCRIPT_MIC_DEVICE`
    - `QSF_TRANSCRIPT_MIC_DURATION_MS`
    - `QSF_OPENAI_REALTIME_TIMEOUT_MS`
  - Voice session:
    - `QSF_REALTIME_SESSION_PROVIDER`
    - `QSF_REALTIME_SESSION_INPUT_SOURCE`
    - `QSF_REALTIME_SESSION_WAV_PATH`
    - `QSF_REALTIME_SESSION_MIC_DEVICE`
    - `QSF_REALTIME_SESSION_MIC_DURATION_MS`
  - Speech output:
    - `QSF_SPEECH_OUTPUT_PROVIDER`
    - `QSF_SPEECH_OUTPUT_MODEL`
    - `QSF_SPEECH_OUTPUT_VOICE`
    - `QSF_SPEECH_OUTPUT_MODE`
  - Memory:
    - `QSF_VOICE_MEMORY_SOURCE`
    - `QSF_VOICE_MEMORY_FILE`
    - `QSF_SESSION_MEMORY_SOURCE`
    - `QSF_SESSION_MEMORY_FILE`
  - Session:
    - `QSF_SESSION_MAX_TURNS`
    - `QSF_SESSION_ALLOW_OVER_LIMIT`
    - `QSF_SESSION_WARM_THRESHOLD`
  - State and review flow:
    - `QSF_STATE_DIR`
    - `QSF_ACCEPT_MEMORY_DRAFT`
    - `QSF_REVIEWED_MEMORY_SLEEP_REPORT`
- Browser server flags:
  - `--store`
  - `--host`
  - `--port`
- UI commands:
  - `npm run dev`
  - `npm run build`
  - `npm run test`
  - `npm run preview`
- Profile candidates:
  - `mock`: clears provider-related variables so deterministic defaults are explicit.
  - `openai-text`: sets `QSF_MODEL_PROVIDER=openai`; requires `OPENAI_API_KEY`.
  - `file-memory`: sets `QSF_VOICE_MEMORY_SOURCE=file`; requires
    `-VoiceMemoryFile <path>` to set `QSF_VOICE_MEMORY_FILE`.
  - `openai-transcription-mic`: sets OpenAI transcript provider and microphone input
    variables; requires `OPENAI_API_KEY`.
  - One-off launcher parameters stay as flags: `-Experiment`, `-Store`, `-BindHost`,
    `-Port`, and `-VoiceMemoryFile`.

### Tasks

- [x] Inventory current `qsf_app` experiment names by running:

```powershell
cargo run -p qsf_app -- list-experiments
```

- [x] Inventory all current `QSF_*` environment variables used by the code.
- [x] Inventory `qsf_browser_server` flags from `crates/qsf_browser_server/src/cli.rs`.
- [x] Inventory UI commands from `crates/qsf_browser_server/ui/package.json`.
- [x] Decide which environment variables belong in named profiles and which should
  remain one-off command parameters.

### Verification

- [x] Fill in the "Phase 0 Inventory" subsection above.
- [x] Produce a short implementation note in the eventual Phase 1 diary entry naming
  the commands and environment groups that were captured.

## Phase 1: Minimal Single-Script Launcher

Create `scripts/qsf.ps1` with a small, explicit command dispatcher.

### Files

- Create: `scripts/qsf.ps1`
- Modify: `README.md`
- Modify: `docs/EngineeringDiary.md`

### Required Commands

- [x] `app`
  - Defaults to showing launcher usage plus available experiments.
  - Supports `-Experiment <name>`.
  - Runs `cargo run -p qsf_app -- experiment <name>` when an experiment is supplied.

- [x] `browser`
  - Supports `-Store`, `-BindHost`, and `-Port`.
  - Defaults mirror `qsf_browser_server`: `state/text-loop/memory-store.json`,
    `127.0.0.1`, and `3939`.
  - Runs `cargo run -p qsf_browser_server -- --store <path> --host <host> --port <port>`.
  - If the default store is missing, prints a clear error and suggests
    `crates/qsf_browser_server/tests/fixtures/small-store.json`.

- [x] `ui`
  - Runs `npm run dev` in `crates/qsf_browser_server/ui`.
  - Checks that `node_modules` exists first; if not, prints:
    `cd crates/qsf_browser_server/ui; npm install`.
  - Leaves API startup to the user or to `workbench`.

- [x] `workbench`
  - Starts the browser server and Vite UI using the same defaults as `browser` and
    `ui`.
  - Starts the Vite UI in a separate visible PowerShell process.
  - Runs the API server in the current terminal foreground.
  - Prints both local URLs.

- [x] `list experiments`
  - Runs `cargo run -p qsf_app -- list-experiments`.

- [x] `help`
  - Prints examples and current defaults.

### Implementation Notes

- Use PowerShell native argument handling with `param(...)`; avoid string-building a
  shell command for execution.
- Use `$ErrorActionPreference = "Stop"` and `Set-StrictMode -Version Latest`.
- Use `#Requires -Version 7.6` and verify with `pwsh`.
- Use `Start-Process` for the `workbench` Vite UI process; run ordinary foreground
  commands inline.
- Resolve paths relative to the repository root using the existing script pattern:
  `$projectRoot = Split-Path -Parent $PSScriptRoot`.
- Print the command that will run before running it.
- Return the child process exit code where applicable.

### Verification

- [x] Run `pwsh -NoProfile -File .\scripts\qsf.ps1 help`.
- [x] Verify the script exists with `Test-Path scripts/qsf.ps1`.
- [x] Parse-check the script:

```powershell
$tokens = $null
$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile(
    (Resolve-Path scripts/qsf.ps1),
    [ref]$tokens,
    [ref]$errors
) | Out-Null
if ($errors) { $errors | Format-List; exit 1 }
```

- [x] Run `pwsh -NoProfile -File .\scripts\qsf.ps1 list experiments`.
- [x] Run `pwsh -NoProfile -File .\scripts\qsf.ps1 browser -Store crates/qsf_browser_server/tests/fixtures/small-store.json -BindHost 127.0.0.1 -Port 3939`.
- [x] In another shell, verify `http://127.0.0.1:3939/api/health`.
- [ ] Run `.\scripts\qsf.ps1 ui` before `npm install` in a clean UI checkout and
  confirm the dependency error points at the install command.
- [x] Run `pwsh -NoProfile -File .\scripts\qsf.ps1 app -Experiment multi-turn-text-loop` with mock defaults.
- [x] Run `cargo build`.
- [x] Run `cargo clippy --all-targets -- -D warnings`.
- [x] Run `cargo fmt` as a general Rust regression guard, even though Phase 1's
  primary changes are PowerShell and documentation.

### External Human Testing

Recommended after Phase 1:

- Start the workbench from a fresh PowerShell window.
- Confirm the printed commands are understandable.
- Confirm Ctrl+C or terminal close behavior is acceptable.

## Phase 2: Launch Profiles

Add named profiles for common environment-variable bundles.

### Files

- Create: `scripts/qsf.profiles.json`
- Modify: `scripts/qsf.ps1`
- Modify: `README.md`
- Modify: `docs/EngineeringDiary.md`

### Profile File Schema

`scripts/qsf.profiles.json` uses this minimal schema:

```json
{
  "profiles": [
    {
      "name": "openai-text",
      "description": "Use the OpenAI model provider for text experiments.",
      "env": {
        "QSF_MODEL_PROVIDER": "openai"
      },
      "clear_env": [],
      "requires": [
        {
          "kind": "env",
          "name": "OPENAI_API_KEY"
        }
      ]
    }
  ]
}
```

Field rules:

- `profiles` is required and contains profile objects.
- `name` is required, unique, and used by `-Profile` and completion.
- `description` is required for `list profiles`.
- `env` is required and maps environment variable names to child-process values.
- `clear_env` is optional and lists environment variables to remove from the
  child-process environment before launch. The launcher should treat this as required
  behavior for process-scoped variables; if a variable cannot be cleared for a specific
  command shape, the launcher must fail before starting the child process.
- `requires` is optional and initially supports `{"kind": "env", "name": "<VAR>"}`.
- Additional requirement kinds must not be added until a phase needs and verifies them.

### Candidate Checked-In Profiles

Checked-in profiles must not contain secrets.

- [x] `mock`
  - Clears or overrides provider settings so mock behavior remains explicit.

- [x] `openai-text`
  - Sets `QSF_MODEL_PROVIDER=openai`.
  - Requires `OPENAI_API_KEY` to already exist in the user's environment.

- [x] `file-memory`
  - Sets `QSF_VOICE_MEMORY_SOURCE=file`.
  - Requires `-VoiceMemoryFile <path>`, which sets `QSF_VOICE_MEMORY_FILE` for the
    child process.

- [x] `openai-transcription-mic`
  - Sets transcript provider variables for live microphone transcription.
  - Requires `OPENAI_API_KEY` to already exist.

### Profile Behavior

- [x] `-Profile <name>` applies only to the child process.
- [x] `-VoiceMemoryFile <path>` is accepted for `app` commands and is meaningful when
  combined with the `file-memory` profile.
- [x] `list profiles` prints profile names, descriptions, set variables, cleared
  variables, and requirements without showing secret values.
- [x] The launcher prints which environment variables are set, unset, or inherited.
- [x] Secret-like variable values are redacted in printed output.
- [x] Missing prerequisites produce a clear error before starting the command.
- [x] Unknown profile names list valid profiles.

### Verification

- [x] Run `.\scripts\qsf.ps1 list profiles`.
- [x] Run `.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -Profile mock`.
- [x] Run `.\scripts\qsf.ps1 app -Experiment text-owned-voice-loop -Profile file-memory -VoiceMemoryFile docs/Experiments/Fixtures/voice-memory.example.json`.
- [x] Run `.\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -Profile openai-text`
  without `OPENAI_API_KEY` and confirm the error is clear.
- [ ] Run with `OPENAI_API_KEY` present if available.
- [x] Run `cargo build`.
- [x] Run `cargo clippy --all-targets -- -D warnings`.
- [x] Run `cargo fmt`.

### External Human Testing

Recommended after Phase 2:

- Validate that profile names match how the project is actually operated.
- Confirm the redacted environment preview is useful without exposing secrets.

## Phase 3: Argument Completion

Add PowerShell completion for common launcher arguments.

### Files

- Create: `scripts/qsf-completion.ps1`
- Modify: `README.md`
- Modify: `docs/EngineeringDiary.md`

### Completion Targets

- [x] Complete launcher commands:
  - `app`
  - `browser`
  - `ui`
  - `workbench`
  - `list`
  - `help`

- [x] Complete `list` values:
  - `experiments`
  - `profiles`

- [x] Leave `doctor` completion for Phase 4, when the command exists.

- [x] Complete `-Profile` from `scripts/qsf.profiles.json`.
- [x] Complete `-Experiment` from `qsf_app` experiment names.
- [x] Complete `-Store` from likely JSON files under `state/`, `runs/`, and
  `crates/qsf_browser_server/tests/fixtures/`.
- [x] Complete `-BindHost` with `127.0.0.1` and `0.0.0.0`.

### Implementation Notes

- Use a static experiment-name list generated from the Phase 0 inventory for
  `-Experiment` completion. Do not shell out to Cargo during tab completion.
- Refresh the static list when Phase 0 inventory is updated or when experiment
  registry changes are part of the same implementation phase.
- Completion should fail silently rather than blocking interactive typing.
- The completion script should be opt-in and documented as:

```powershell
. .\scripts\qsf-completion.ps1
```

### Verification

- [x] Dot-source `scripts/qsf-completion.ps1`.
- [x] Confirm tab completion works for command names.
- [x] Confirm tab completion works for `-Profile`.
- [x] Confirm tab completion works for `-Store` path candidates.
- [x] Run a programmatic completion check:

```powershell
$result = [System.Management.Automation.CommandCompletion]::CompleteInput(
    '.\scripts\qsf.ps1 app -Profile ',
    33,
    $null
)
$result.CompletionMatches.CompletionText
```

Expected: the output includes profile names from `scripts/qsf.profiles.json`.

- [x] Run `cargo build`.
- [x] Run `cargo clippy --all-targets -- -D warnings`.
- [x] Run `cargo fmt`.

## Phase 4: Doctor And Diagnostics

Add commands that explain why a launch will or will not work.

### Files

- Modify: `scripts/qsf.ps1`
- Modify: `README.md`
- Modify: `docs/EngineeringDiary.md`

### Required Checks

- [x] `doctor` checks:
  - PowerShell version
  - `cargo`
  - Rust toolchain
  - repository root detection
  - Node/npm availability
  - UI dependencies presence
  - default memory store existence
  - whether port `3939` appears occupied
  - `OPENAI_API_KEY` presence without printing the value

- [x] `doctor -Profile <name>` checks profile prerequisites.
- [x] `doctor -Workbench` checks both API and UI prerequisites.

### Verification

- [x] Run `.\scripts\qsf.ps1 doctor`.
- [x] Run `.\scripts\qsf.ps1 doctor -Profile openai-text`.
- [x] Run `.\scripts\qsf.ps1 doctor -Workbench`.
- [x] Confirm missing optional dependencies are warnings, not hard failures, unless the
  selected command requires them.
- [x] Run `cargo build`.
- [x] Run `cargo clippy --all-targets -- -D warnings`.
- [x] Run `cargo fmt`.

## Phase 5: Polish And Documentation Consolidation

Make the launcher the documented happy path while preserving raw commands.

### Files

- Modify: `README.md`
- Modify: `docs/Plans/Plan.MemoryAssociationBrowser.md`
- Modify: `docs/EngineeringDiary.md`
- Optional: `docs/DecisionLog.md`

### Tasks

- [ ] Add a README section for the launcher.
- [ ] Keep raw Cargo and npm commands in README as fallback/reference commands.
- [ ] Update Memory Association Browser plan snippets that describe starting API and
  UI together.
- [ ] Decide whether the launcher convention is durable enough for a DecisionLog entry.
- [ ] Add troubleshooting notes for:
  - blocked ports
  - missing `OPENAI_API_KEY`
  - missing `npm install`
  - execution policy restrictions
  - one-liner execution policy fallback:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts\qsf.ps1 help
```

  - stale completion cache, if caching is implemented

### Verification

- [ ] Follow README instructions from a fresh PowerShell session.
- [ ] Confirm raw commands still work.
- [ ] Confirm launcher commands still work.
- [ ] Run `cargo build`.
- [ ] Run `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo fmt`.

## Acceptance Criteria

- A developer can start a mock `qsf_app` experiment without remembering Cargo syntax.
- A developer can start `qsf_browser_server` with the default memory store without
  remembering flags.
- A developer can start the API server and Vite UI together with one command.
- `.\scripts\qsf.ps1 app` itself is intentionally informational; running an
  experiment requires `-Experiment`, and exercising a profile requires `-Profile`.
- Named profiles make common provider and memory-source settings explicit.
- Secrets are never written to checked-in files or printed in full.
- Argument completion covers the common command surface.
- Documentation shows both the launcher path and the raw underlying commands.
- The launcher does not make the Rust binaries depend on PowerShell.

## Risks And Mitigations

- Risk: PowerShell scripts become another drifting interface.
  Mitigation: keep commands thin and print the underlying command.

- Risk: Profiles hide important runtime behavior.
  Mitigation: print the effective environment delta before launch.

- Risk: Completion becomes slow if it invokes Cargo frequently.
  Mitigation: use static fallback or lightweight caching.

- Risk: `workbench` process management becomes brittle.
  Mitigation: start with simple behavior and document how to stop processes clearly.

- Risk: local-only profiles leak machine-specific paths.
  Mitigation: support gitignored local profile files only after checked-in profiles are
  stable.
