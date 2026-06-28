# Testing Handoff: Realtime Volition Read-Only Tools

**Date:** 2026-06-28  
**Status:** Fix applied — live voice re-test pending  
**Related:** `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md`

---

## What Was Tested

Two live realtime sessions were run against `qsf.ps1 realtime` using the stable
`default` QSF session id. The experiment prompts from
`Experiment.RealtimeVolitionReadOnlyInspection.md` were spoken:

| Session | Call ID | Prompt |
|---|---|---|
| 1 | `rtc_u0_DvnlZabnp5PraQn1IWNMC` | "What are you currently focused on?" |
| 2 | `rtc_u2_DvnmZWPnGII3nTQ1N6q2M` | "What goals relate to helping me?" |

---

## What the Evidence Shows

### Sideband is healthy

Both sessions show:

- Sideband attached successfully (`engine.log`: `sideband attached to call ...`)
- `session.update` sent with all five registered tools (confirmed by source inspection below)
- Memory injection and `response.create` ran on the trusted turn path (latency observations
  recorded in `state/realtime/diagnostics/default.jsonl`)

Latency observations for session 2 (representative):

```
final_transcript_received_to_memory_injected: 1 ms
memory_injected_to_response_create_sent:      0 ms
response_create_sent_to_response_created:   171 ms
response_created_to_first_audio:            312 ms
final_transcript_received_to_first_audio:   485 ms
```

The `response_created_to_first_audio` at 312 ms confirms the model returned **audio
directly** — not a function-call-only response. If a tool had been called, the first
audio would come only after the tool loop completed, putting this figure well above 1 s.

### Tools are wired correctly

`default_tool_definitions()` in
`crates/qsf_realtime_server/src/realtime/tools.rs:114` returns all five tools:

```
search_memory
get_associations
inspect_session_state
inspect_volition_state      ← new
select_volition_goals       ← new
```

`BrowserSessionConfig` is constructed with these definitions at
`crates/qsf_realtime_server/src/state.rs:303`:

```rust
tools: crate::realtime::tools::default_tool_definitions(),
```

The sideband sends them to the model at startup via
`build_openai_realtime_conversation_session_update` with `tool_choice: Some("auto")`
(`crates/qsf_realtime_server/src/realtime/sideband.rs:218–233`).

### The tool execution path works

A June 2026 `engine.log` entry shows:

```
ignored continuation transcript for session `default` during ToolLoop: `Thank you.`
```

`ToolLoop` is the `TurnPhase` the sideband enters only after the model makes a
function call and before the tool result is returned. This proves the tool dispatch
path through the sideband is functional. The memory-search tool (`search_memory`)
triggered this state in that session.

### The model did not call either volition tool

Both experiment sessions show `tool_requests: []` and `tool_executions: []` in
`state/realtime/diagnostics/default.jsonl`. The engine log shows no ToolLoop state
for either call id. The model answered both prompts generically from training context.

---

## Root Cause

`DEFAULT_INSTRUCTIONS` at
`crates/qsf_realtime_server/src/state.rs:19` is:

```
"Speak briefly. Keep the browser UI informed, keep secrets server-side, and preserve the QSF trust boundary."
```

There is no guidance telling the model:
- that volition tools exist
- when to call them
- how to frame their output

Without this, the model answers open-ended questions like "what are you focused on?"
from its general training rather than calling `inspect_volition_state`.

---

## Required Fix

Update `DEFAULT_INSTRUCTIONS` in `crates/qsf_realtime_server/src/state.rs` to include
volition tool guidance. Suggested addition:

```rust
const DEFAULT_INSTRUCTIONS: &str = "\
Speak briefly. Keep the browser UI informed, keep secrets server-side, \
and preserve the QSF trust boundary. \
You have read-only access to your simulated internal volition state through tools. \
When asked about your current focus, goals, motivations, or internal state, \
call inspect_volition_state first. \
When asked which goals relate to a specific topic or how you can help with something, \
call select_volition_goals with the relevant query. \
Frame any volition tool result as simulated internal state — not a claim of real \
desire, consciousness, or subjective experience.\
";
```

After changing this constant, rebuild and restart the server. No other code changes
are needed — the tool wiring, registration, and execution path are all correct.

**Applied 2026-06-28:** `DEFAULT_INSTRUCTIONS` now includes the volition tool guidance
above. The crate builds clean (`cargo build -p qsf_realtime_server`) and passes
`cargo clippy --all-targets -- -D warnings`. The live voice re-test below has not yet
been run — it requires a human to speak the experiment prompts.

---

## How to Verify the Fix

1. **Rebuild the server:**
   ```powershell
   cargo build -p qsf_realtime_server
   ```

2. **Start the session:**
   ```powershell
   .\scripts\qsf.ps1 realtime
   ```

3. **Speak the experiment prompts:**
   - "What are you currently focused on?"
   - "What goals relate to helping me?"

4. **Check `engine.log` for ToolLoop:**
   The log should show something like:
   ```
   [WARN] ignored continuation transcript for session `default` during ToolLoop: ...
   ```
   or no such warning if the tool completed cleanly before the next turn.

5. **Check the diagnostic file for tool records:**
   ```powershell
   python3 -c "
   import json
   for l in open('state/realtime/diagnostics/default.jsonl', encoding='utf-8'):
       d = json.loads(l)
       if d.get('kind') == 'diagnostic_exchange_recorded':
           ex = d['exchange']
           tr = ex.get('tool_requests', [])
           te = ex.get('tool_executions', [])
           if tr or te:
               print('Exchange', ex['index'])
               for r in tr: print(' req:', r.get('tool_name'))
               for e in te: print(' exec:', e.get('tool_name'), e.get('status'))
   "
   ```
   Expected: entries for `inspect_volition_state` or `select_volition_goals`.

   Note: trusted sideband tool records appear in the session-state file
   (`state/session/session-state.json`), not in the browser relay diagnostic exchanges.
   Check both.

6. **Verify the trace fields** in `result_summary` for `select_volition_goals`:
   All fields listed in `Experiment.RealtimeVolitionReadOnlyInspection.md` §Automated
   Verification must be present and non-empty.

7. **Confirm the spoken answer** references specific goal names from the fixture and
   explicitly uses simulated-state language (not first-person desire claims).

---

## Key File Locations

| Purpose | Path |
|---|---|
| Session instructions | `crates/qsf_realtime_server/src/state.rs:19` |
| Tool definitions | `crates/qsf_realtime_server/src/realtime/tools.rs:114` |
| Volition tool implementations | `crates/qsf_realtime_server/src/realtime/volition_tools.rs` |
| Session.update builder | `crates/qsf_realtime_protocol/src/lib.rs:89` |
| Sideband tool dispatch | `crates/qsf_realtime_server/src/realtime/sideband.rs:1380` |
| Diagnostic output | `state/realtime/diagnostics/default.jsonl` |
| Engine log | `engine.log` (project root) |
| Session state | `state/session/session-state.json` |
| Experiment spec | `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md` |
