# Multi-Turn Text Loop Refactor Plan

## Goal

Reduce `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` from a large mixed
experiment/runtime module into a thin experiment entry point plus focused session,
memory, tool, console, and report modules. Preserve existing behavior while making
the text and voice loops depend on shared runtime modules instead of one experiment
importing implementation details from another.

## Open Questions

- Should shared text/voice session behavior live directly under `crate::session`, or
  should it use a narrower module such as `crate::session::text_runtime`?
- Should the large existing inline tests stay as unit tests split across modules, or
  should some end-to-end loop tests move to integration tests?
- Should report rendering remain experiment-specific, or become a reusable session
  report helper if voice/text reports continue converging?

## Phase 1 - Extract Tests

Goal: Move the inline test module out of `multi_turn_text_loop.rs` into an adjacent
test file without changing behavior.

Work:
- Add `crates/qsf_app/src/experiments/multi_turn_text_loop/tests.rs`.
- Replace the inline `#[cfg(test)] mod tests` body with a module declaration.
- Use explicit imports in the extracted test file.

Verify:
- `cargo test -p qsf_app multi_turn_text_loop`
- `cargo fmt`

## Phase 2 - Move Shared Live Memory Runtime

Goal: Stop `text_owned_voice_loop.rs` from importing shared behavior from the text
experiment module.

Work:
- Move live memory reinforcement and capture helpers into a stable shared module,
  likely under `crate::session` or `crate::memory`.
- Update both text and voice-owned loops to call the shared module.
- Keep public surface narrow: expose only the functions needed by both loops.

Verify:
- `cargo test -p qsf_app live_loop`
- `cargo test -p qsf_app text_owned_voice_loop`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`

## Phase 3 - Move Warm Ageing And Token-Budget Ageing

Goal: Separate session ageing policy and side effects from the text experiment.

Work:
- Move warm-threshold ageing, summarization retry, token-budget drop planning,
  cross-turn co-retrieval persistence, and session-end flush behavior into a shared
  session ageing module.
- Keep reducers pure; ageing side effects should still feed back through
  `SessionEvent`.
- Preserve default thresholds so the existing ageing paths remain exercised.

Verify:
- `cargo test -p qsf_app warm`
- `cargo test -p qsf_app token_budget`
- `cargo test -p qsf_app session_end_flush`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`

## Phase 4 - Split Text-Loop-Local Concerns

Goal: Make `multi_turn_text_loop.rs` read as orchestration instead of a collection of
unrelated utilities.

Work:
- Move console rendering helpers into a text-loop console module.
- Move report generation into a report module.
- Move environment/config parsing into a config module if it remains text-loop-only.
- Move responder tool-call handling into a tool/runtime module if it is still only
  used by the text loop.

Verify:
- `cargo test -p qsf_app multi_turn_text_loop`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`

## Phase 5 - Simplify Turn Orchestration

Goal: Reduce `run_one_turn` into a readable pipeline with small result structs for
context assembly, responder execution, memory updates, and ageing.

Work:
- Extract context retrieval and prompt assembly into a focused function.
- Extract bounded responder tool-loop execution into a focused function.
- Extract post-response memory/session updates into a focused function.
- Keep state transitions explicit through actions/events.

Verify:
- `cargo test -p qsf_app multi_turn_text_loop`
- `cargo test -p qsf_app text_owned_voice_loop`
- Manual smoke test of the text loop with a short session and `:quit`.
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`

## Documentation Updates

- `docs/EngineeringDiary.md`: add one short entry per implemented phase, because each
  phase is a logical code change.
- `docs/Architecture/`: update or add a session-runtime architecture note if the
  shared text/voice runtime boundary becomes durable.
- `docs/DecisionLog.md`: update only if implementation makes a durable architectural
  commitment, such as "shared live memory runtime belongs under `session`".

