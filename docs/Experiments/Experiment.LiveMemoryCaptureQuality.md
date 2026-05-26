# Experiment: Live Memory Capture Quality

## Status

Completed as a deterministic regression fixture. The live-model/manual replay remains a recommended follow-up for human validation.

## Summary

This experiment checks whether the live text loop can capture the important facts from the Ari/Lars/volition QA session, retrieve the correct memory for identity and remember-this follow-up queries, and avoid selecting the assistant-name memory on unrelated volition turns.

The repeatable fixture is already implemented as the in-crate regression `live_loop_captures_remembered_topic_and_retrieves_it_end_to_end`.

## Hypothesis

The transcript below should produce three durable memories, retrieve the right record for each identity-oriented query, and keep the Ari memory out of unrelated volition retrievals unless there is a direct signal.

## Transcript Fixture

```text
I want you to use the name Ari.
My name is Lars.
Tell me more what you think how a volition system should work.
Interesting, please remember this for future discussions!
What is your name?
What is my name?
What did I ask you to remember about volition?
Tell me about volition goals.
:quit
```

## Expected Durable Memories

- `Assistant name: Ari`
- `User name: Lars`
- `Remembered topic: volition system`

The remembered-topic record should carry:

- a bounded source excerpt from the prior assistant response
- topic tags such as `volition`, `system`, and `volition_system`
- source-turn metadata that identifies the previous turn used for the excerpt

## Expected Retrievals

- `What is your name?` -> `Assistant name: Ari`
- `What is my name?` -> `User name: Lars`
- `What did I ask you to remember about volition?` -> `Remembered topic: volition system`

## Expected Non-Retrievals

- `Tell me about volition goals.` should not retrieve `Assistant name: Ari` by importance or recency alone.
- The volition recall query should not select the Ari memory unless a direct identity signal is present.
- The persisted session state should not contain a truncated warm summary for this fixture run.

## Setup

- Crate: `qsf_app`
- Model client: deterministic `MockModelClient`
- Memory source: empty fixture for the regression test
- State directory: temporary `state/text-loop` path under the test run
- Command:

```powershell
cargo test -p qsf_app live_loop_captures_remembered_topic_and_retrieves_it_end_to_end
```

## Procedure

1. Run the deterministic regression test.
2. Inspect `memory-store.json` for the three durable records.
3. Inspect `events.jsonl` for `MemoryStorePersisted`, `MemoryRetrieved`, and `MemoryReinforced`.
4. Inspect `traces.jsonl` for `live-memory-capture` and remember-this traces.
5. Check `session-state.json` to confirm the fixture did not persist a truncated warm summary.

## Baseline

The comparison baseline is the same transcript before the live capture path existed:

- no durable identity memories for Ari or Lars
- no remembered-topic record for the volition discussion
- unrelated volition turns can accidentally retrieve Ari because of importance/recency alone

## Measurements

### Quantitative

- number of durable records in the memory store
- selected memory ids per follow-up query
- omitted candidate ids and skip reasons
- persisted summary count in `session-state.json`

### Qualitative

- whether the stored memories match the user-facing intent
- whether the remembered-topic excerpt still makes the source discussion recognizable
- whether unrelated volition turns stay free of identity contamination

## Required Observability

- `MemoryStorePersisted` payloads with candidate kinds and counts
- `MemoryRetrieved` payloads with selected and omitted ids
- `MemoryReinforced` payloads with skipped ids and skip counts
- `RetrievedMemory.skip_reason` in retrieval traces
- `live-memory-capture` trace records for captures and explicit remember-this skips

## Risks and Confounders

- The transcript is short and deterministic; it validates structure, not semantic breadth.
- The remembered-topic capture is intentionally bounded, so it may under-compress the source discussion.
- If the warm-threshold or summarization policy changes, the no-truncated-summary expectation may need to be updated to match the new fixture length.

## Human Testing Notes

- Re-run the transcript manually with a real model when evaluating memory quality by hand.
- Check that the assistant-name memory is not read out on the unrelated volition turn.
- Confirm that the remembered-topic record still reads as a faithful source excerpt rather than a fabricated summary.

## Expected Output

- `events.jsonl`
- `traces.jsonl`
- `session-state.json`
- `memory-store.json`
- `multi-turn-text-loop.md`

## Results

### What Happened

The deterministic regression fixture exists and covers the Ari/Lars/volition path end to end.

### Measurements

- The test asserts three durable records in the store.
- The test asserts the correct retrieved ids for identity and remembered-topic queries.
- The test asserts the unrelated volition query does not select the Ari memory.
- The test leaves the session without persisted warm summaries.

### Observations

- The fixture gives a compact regression for the live capture path without requiring a live model.
- It is strong enough to catch accidental Ari-only retrieval on unrelated volition turns.

### Surprises

- None from the deterministic fixture itself.

### Failure Modes

- A future change could reintroduce zero-signal retrieval or truncate warm summaries without affecting simpler unit tests.

## Interpretation

Observed:

- Live capture can persist assistant identity, user identity, and remembered-topic memories in one transcript.
- Retrieval can prefer the correct record for each query shape.
- Unrelated volition turns can remain free of identity-only retrieval.

Interpreted:

- The live text loop now has a practical regression harness for the specific quality problem uncovered by the QA session.

Uncertain:

- Whether the same quality holds under a less constrained live model response.
- Whether future richer capture should replace the bounded excerpt with a better semantic compression strategy.

## Follow-Up Questions

- Does the same transcript hold up with a live provider instead of the deterministic mock?
- Should the remembered-topic capture be compressed further once model-assisted extraction is available?
- Do we want a file-backed transcript fixture alongside the in-crate regression?

## Follow-Up Experiments

- `Experiment.AssociativeMemoryToyModel`
- `Experiment.SleepPhaseSessionSummary`
- A live-model replay of the same transcript

## Decision Candidates

- Candidate: Keep the Ari/Lars/volition transcript as the canonical regression for live memory quality.
- Candidate: Preserve the bounded source excerpt approach until model-assisted compression is added.

## Final Status

Useful Result

## Notes

The deterministic regression lives in `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` as `live_loop_captures_remembered_topic_and_retrieves_it_end_to_end`.
