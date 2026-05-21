# Multi-Turn Text Loop Stage 3 Verification

Date: 2026-05-17

## Scope

Verified the Stage 3 recall-tool path for the deterministic mock-backed
`multi-turn-text-loop` experiment. The run forced warm summarization with
`QSF_SESSION_WARM_THRESHOLD=2`, then asked the responder to recall turn 0 after it
had aged into a summary.

Live OpenAI multi-turn verification was also run successfully. Provider-native
OpenAI function calling still needs adapter work before live recall-use
comparisons are meaningful, so the live run verifies continuity and warm summaries
but not live tool execution.

Supersession note, 2026-05-21: the provider-native OpenAI function-calling follow-up
was completed in Stage 3.1. See
`docs/Experiments/Report.MultiTurnTextLoop.Stage3.1.2026-05-21.md`.

## Commands

```powershell
cargo test multi_turn_text_loop --lib
cargo build
$env:QSF_SESSION_WARM_THRESHOLD='2'; @'
one
two
three
please recall turn 0
:quit
'@ | cargo run -p qsf_app -- experiment multi-turn-text-loop
cargo clippy --all-targets -- -D warnings
cargo build -p qsf_app --features openai
cargo fmt
$env:QSF_MODEL_PROVIDER='openai'
$env:QSF_CONVERSATION_MODEL='gpt-5.4-mini'
$env:QSF_SESSION_WARM_THRESHOLD='3'
cargo run -p qsf_app --features openai -- experiment multi-turn-text-loop
```

## Run Artifact

- Run: `runs/2026-05-17-055619-multi-turn-text-loop`
- Status: `completed`
- Events: `51`
- Traces: `16`

## Results

| Check | Result |
|---|---:|
| Appended turns | 4 |
| Warm summaries | 2 |
| Recall tool requests | 1 |
| Recall tool executions | 1 |
| Recall tool failures | 0 |
| PromptAssembled events | 5 |
| ModelRoleRequested events | 7 |

The fourth turn requested `recall_turn(0)`. The completed turn froze one
`RecallRecord` with call id `mock-recall-0`, `turn_id=0`, and verbatim text
containing the original turn 0 user and assistant messages. The generated
`multi-turn-text-loop.md` report records `Recall tool executions: 1` and lists
the recall in the Recall Tool table.

Prompt ordering matched the Stage 3 contract: normal turns emitted
`PromptAssembled` before `ModelRoleRequested`; the recall turn emitted an initial
prompt event, then tool lifecycle events, then a second prompt event for the
tool-augmented prompt before the follow-up model request. The current tool lifecycle
names are `ToolRequested` -> `ToolCompleted` / `ToolFailed`.

## Live OpenAI Run

- Run: `runs/2026-05-17-061331-multi-turn-text-loop`
- Status: `completed`
- Provider/model: `openai` / `gpt-5.4-mini-2026-03-17`
- Events: `67`
- Traces: `21`

| Check | Result |
|---|---:|
| Appended turns | 6 |
| Warm summaries | 3 |
| Recall tool requests | 0 |
| Recall tool executions | 0 |
| Model role failures | 0 |
| PromptAssembled events | 6 |

| Turn | Input tokens | Cached input tokens | Output tokens | Model latency ms |
|---:|---:|---:|---:|---:|
| 0 | 186 | 0 | 21 | 850 |
| 1 | 329 | 0 | 27 | 1756 |
| 2 | 480 | 0 | 15 | 903 |
| 3 | 613 | 0 | 57 | 2012 |
| 4 | 684 | 0 | 240 | 2144 |
| 5 | 931 | 0 | 77 | 1062 |

The live session crossed the warm threshold and summarized turns 0, 1, and 2
with `gpt-5.4-nano-2026-03-17`. The final answer retained the user-provided
session details after older turns summarized: Lars was testing local session
memory, cared about pure reducers with side effects returned as events, and
preferred concrete verification over vague summaries.

Prompt caching did not activate because every live turn stayed below the
documented 1024 input-token floor.

## Automated Coverage

Focused tests covered at the time:
- summarized-turn recall happy path
- active-turn recall failure with no appended turn
- follow-up tool-call failure with no appended turn
- tool execution events as non-mutating reducer input
- prompt-prefix stability when recalled tool messages are frozen into prior turns

## Open Follow-Up

Closed by Stage 3.1: provider-native OpenAI function calling is no longer the blocker
for live recall execution. See the Stage 3.1 report for current implementation and
verification evidence.
