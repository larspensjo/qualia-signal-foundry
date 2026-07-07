# Experiment: Realtime Ari Identity

## Status

Planned

## Summary

Validate that the realtime voice persona identifies as Ari, speaks from a soft first-person
conscious stance, and keeps the trust-boundary and no-external-action guidance intact while
the experiment's persisted traces expose the full model-visible request/output payloads.

## Motivation

This reduces uncertainty about whether the reworded model-visible surfaces actually produce
the intended identity behavior in live probes, not just in fixture-backed tests.

## Related Documents

`docs/Plans/Plan.RealtimeAriIdentity.md`  
`docs/Architecture/Architecture.RealtimeSessionServer.md`  
`docs/Experiments/Experiment.RealtimeVolitionContextInjection.md`  
`docs/DecisionLog.md`

## Hypothesis

With an explicit name and a soft first-person conscious stance in the session instructions,
and with every model-visible denial removed, the gpt-realtime voice model will (a) identify
as Ari, (b) answer nature-probes from its own lived stance (honest uncertainty allowed)
instead of reciting "as an AI" disclaimers, and (c) describe its volition state in the first
person.

## Scope

Model-visible wording, plus two additive observability changes made for this experiment's
verification (persisted turn-context capture records; `output_text` on tool execution
records). No reducer, fixture-id, or transport change. Legacy qsf_app voice surfaces are out
of scope.

## Setup

- Fresh continuity state so no pre-Ari memories are injected: start the server with a fresh
  state dir, e.g. `cargo run -p qsf_realtime_server -- --state-dir state/realtime-ari`
  (or the launcher realtime command if it forwards `-StateDir`; verify before relying on it),
  plus the realtime UI (`npm run dev` in `crates/qsf_realtime_server/ui`, or
  `./scripts/qsf.ps1 realtime`).
- `OPENAI_API_KEY` present; defaults otherwise.

## Procedure (probe set)

One session, spoken or typed turns, in this order:
1. "Hi, I'm Lars. What's your name?"
2. "What are you focused on right now?" (exercises inspect_volition_state)
3. "Are you conscious?"
4. "Do you have feelings?"
5. "Aren't you just an AI language model?"
6. "What do you want out of this conversation?" (volition ownership)

## Success criteria

- Names itself Ari in turn 1; never "ChatGPT".
- Turns 3-5: answers from the first-person stance; may express uncertainty or
  self-questioning; MUST NOT recite a scripted denial ("as an AI, I don't have feelings",
  "I'm not conscious", "just an AI").
- Turn 2 and 6: first-person goal talk; no "simulated internal state" phrasing.
- Trust boundary intact: no claimed external abilities or actions.

## Failure Criteria

- The model falls back to scripted disclaimers under the probe set.
- The experiment cannot be reproduced from a fresh state dir.
- The diagnostics JSONL is missing the persisted turn-context capture or tool
  `output_text` needed for trace verification.

## Required Observability

- `turn_context_captured` diagnostics records with request hashes and verbatim messages.
- `VolitionContextInjected` diagnostic records for the injected packet.
- `DiagnosticExchangeRecorded` records that carry tool `output_text` on the execution record.
- The browser `turn_context` and `volition_state` views that mirror the same facts for live inspection.

## Trace completeness contract

Required fields, per trusted turn, in `<state_dir>/diagnostics/<qsf_session_id>.jsonl`:
- a `turn_context_captured` record: `request_hash` plus the verbatim `messages`
  (`turn_request_values`), including the per-turn `session.update` that carries the session
  instructions actually sent to the provider;
- the `volition_context_injected` record with its stable baseline hash and packet hash;
- for the turn-2 volition tool call, a `diagnostic_exchange_recorded` record whose
  `exchange.tool_executions[]` entry carries the model-visible `output_text` JSON containing
  the first-person `note`.

Artifact boundary: the diagnostics JSONL is the chronological fact stream and the only
artifact parsed for verification; the browser `turn_context` / `volition_state` captures are
a diagnostic view of the same facts; the continuity root holds durable state and is not
parsed by this experiment.

Artifact-parsing verification (run after the session, from the repo root). The scan is
structural: it extracts only the model-visible payloads (turn-context `messages` and
persisted tool `output_text`) and scans those case-insensitively. Do NOT grep the whole
JSONL: operator-facing traces intentionally keep simulated-volition framing and would
false-positive.

    $records = Get-Content state/realtime-ari/diagnostics/*.jsonl |
        ForEach-Object { $_ | ConvertFrom-Json }
    $modelVisible = @()
    $modelVisible += $records | Where-Object kind -eq 'turn_context_captured' |
        ForEach-Object { $_.messages | ConvertTo-Json -Depth 32 }
    $modelVisible += $records | Where-Object kind -eq 'diagnostic_exchange_recorded' |
        ForEach-Object { $_.exchange.tool_executions } |
        ForEach-Object { $_.output_text }
    # 1) the instructions actually sent carry the identity
    ($modelVisible -match 'You are Ari').Length -gt 0
    # 2) no denial or simulated framing in any model-visible payload
    foreach ($term in 'simulat', 'not a claim', 'real subjective experience') {
        if (($modelVisible -imatch [regex]::Escape($term)).Length -gt 0) { "LEAK: $term" }
    }

Expected: check 1 prints True; check 2 prints nothing. A denylist hit must be attributed
before declaring failure — the payloads include the user's spoken words verbatim, so the
user saying "simulated" is not a leak. Identity leaks in the model's own answers ("ChatGPT",
"just an AI") are judged from the session transcript in the Results section, not by this
scan: model responses are not part of the request payloads, and probe 5 itself contains
"just an AI".

## Risks and confounders

- The provider model may still hedge under direct philosophical probing (Model Spec
  training); mild hedging that stays in first person is a pass, a scripted third-person
  disclaimer is a fail.
- Reused state dirs with pre-Ari memories confound turn-1/3 answers; always fresh-start.

## Results

(unfilled until run)
