# Experiment: Realtime Volition Inspection UI

## Status

Running

## Question

Does a lightweight, read-only volition panel in the realtime browser UI make the
live session easier to explain without interrupting the spoken interaction?

## Context

Realtime volition already computes a compact state inspection and a per-turn
decision summary in the server. This experiment surfaces that existing data over
the realtime events socket and renders it in the browser as a collapsible panel
next to the turn-context inspector.

The panel is latest-only, preserving the most recent trusted turn's volition
snapshot after Stop so diagnostics remain visible, and clearing only when a new
session is allocated.

## Scope

- Backend capture type for the volition snapshot plus optional decision summary.
- Per-session `watch` channel and events-socket forwarding for `kind: "volition_state"`.
- UI parser, reducer field, pure selector, and collapsible panel rendering.
- No new tool, no new mutation path, and no provider payload persistence.

## Trace Completeness Contract

The `volition_state` capture must contain:

- `qsf_session_id`
- `exchange_index`
- `captured_at` as an RFC3339 string
- `response_create_event_ref`
- `inspection` with the compact state snapshot:
  - `mode`
  - `tick`
  - goal groups
  - pending and accepted candidate counts
  - last initiative summaries
- `decision: Option<VolitionTurnDecisionSummary>`
  - present on selection turns
  - absent on no-selection turns

When present, the decision summary must carry:

- `winner_goal_id`
- `winner_goal_title`
- `winner_effective_tier`
- `winner_biased_tier`
- `protected_tier_active`
- compact `mode_bias_outcomes`
- `selected_goal_ids`
- `omitted_or_suppressed_goal_ids`
- `shaping_intensity`
- `last_initiative_output_kind`
- `last_initiative_surfaced`
- `last_initiative_suppression_reason`
- `last_initiative_rendered_line_present`

## Artifact Boundary

- Events socket: live browser-facing `volition_state` messages.
- Diagnostics JSONL: `VolitionContextInjected` and `RealtimeBoundedInitiative`
  remain the authoritative causal chain for selection turns.
- The capture itself is live-only and carries no provider payloads or instruction
  text.

## Verification

Automated verification should cover:

- Capture construction for a selection turn and a no-selection turn.
- RFC3339 capture serialization.
- No-secret and no-provider-payload serialization checks.
- Per-session watch-channel late-subscriber behavior.
- Events-socket forwarding of the `volition_state` message.
- UI parser acceptance and rejection cases.
- UI reducer stale-session guard, preserve-on-stop behavior, and session reset.
- UI selector output for:
  - a decision-present capture with protected-tier winner fields,
  - a decision-null capture with an explicit no-decision marker,
  - the no-capture fallback state.

Human verification should confirm that:

- The panel updates on every trusted turn.
- No-selection turns show the state snapshot plus the no-decision marker.
- Selection turns show the winner tiers, protected status, selected and omitted
  goal ids, and the last initiative outcome.
- The panel remains visible after Stop.

## Results

Pending. The code path and browser surface are implemented; the live run still
needs operator confirmation.
