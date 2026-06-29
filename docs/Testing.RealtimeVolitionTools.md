# Testing Handoff: Realtime Volition Read-Only Tools

**Date:** 2026-06-28  
**Status:** Instruction fix applied but insufficient — model still does not call volition tools  
**Related:** `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md`

---

## Current Conclusion

Adding volition-tool guidance to `DEFAULT_INSTRUCTIONS` did **not** make the realtime
model call `inspect_volition_state` or `select_volition_goals`. The fixed binary was
confirmed in use for the latest run (timeline below), and both experiment prompts were
still answered without any tool call. The blocker is the model's decision not to call
the tools under `tool_choice: "auto"`, not a wiring or instruction-delivery defect.

---

## Run Log

### Run 1 — 2026-06-28, pre-fix

Two live sessions on the stable `default` QSF session id:

| Call ID | Prompt |
|---|---|
| `rtc_u0_DvnlZabnp5PraQn1IWNMC` | "What are you currently focused on?" |
| `rtc_u2_DvnmZWPnGII3nTQ1N6q2M` | "What goals relate to helping me?" |

Both showed `tool_requests: []` / `tool_executions: []` and no `ToolLoop` in `engine.log`.
At this point `DEFAULT_INSTRUCTIONS` did not mention the volition tools, so the missing
usage signal was the leading hypothesis.

### Fix applied — 2026-06-28

`DEFAULT_INSTRUCTIONS` in `crates/qsf_realtime_server/src/state.rs` was extended to tell
the model the read-only volition tools exist, when to call each, and to frame results as
simulated internal state. The crate builds clean and passes
`cargo clippy --all-targets -- -D warnings`.

### Run 2 — 2026-06-28, post-fix — **tool still not called**

Latest run, call `rtc_u2_DvpYM8lvYq0LhoaqveEGH`:

| Exchange | Prompt | `tool_requests` | `tool_executions` |
|---|---|---|---|
| 0 | "What are you currently focused on?" | `[]` | `[]` |
| 1 | "What goals relate to helping me?" | `[]` | `[]` |

- `engine.log`: server start `19:33:08`, sideband attached to the call at `19:33:58`,
  call invalidated `19:34:41` (`reason: stop`). **No `ToolLoop` turn phase** for this call.
- `state/session/session-state.json` untouched since Jun 8 → no trusted tool-execution
  record was written, consistent with no tool call.

**Timeline confirms the fixed binary was used** (this was easy to misread because the
two clocks differ):

| Event | Source clock | UTC |
|---|---|---|
| Fixed binary built (`target/debug/qsf_realtime_server.exe` mtime) | local CEST `20:59:16` | `18:59` |
| Server start for Run 2 (`engine.log`) | UTC `19:33:08` | `19:33` |
| Run 2 exchanges completed (`default.jsonl`) | UTC `19:34:41` | `19:34` |

`engine.log` and the diagnostics JSONL record timestamps in **UTC**; filesystem mtimes
are **local (CEST, UTC+2)**. Converting both to UTC: binary `18:59` < run `19:33`, so
Run 2 ran on the fixed binary. `scripts/qsf.ps1 realtime` launches via
`cargo run -p qsf_realtime_server` (line 979), which rebuilds before launching, so a
stale binary is not a plausible explanation either.

---

## Why This Is Not a Wiring Defect

- All five tools (`search_memory`, `get_associations`, `inspect_session_state`,
  `inspect_volition_state`, `select_volition_goals`) are returned by
  `default_tool_definitions()` (`crates/qsf_realtime_server/src/realtime/tools.rs:114`)
  and wired into `BrowserSessionConfig` (`crates/qsf_realtime_server/src/state.rs:303`).
- They are sent at startup via `session.update` with `tool_choice: Some("auto")`
  (`crates/qsf_realtime_server/src/realtime/sideband.rs:218–233`).
- `DEFAULT_INSTRUCTIONS` flows to `config.instructions`, which feeds both the
  `session.update` and every trusted `response.create`
  (`crates/qsf_realtime_server/src/realtime/injection.rs:95`,
  `crates/qsf_realtime_server/src/realtime/sideband.rs:1417–1422`).
- The trusted turn **can** enter `ToolLoop`: a June 13 session log shows
  `ignored continuation transcript ... during ToolLoop` for `search_memory` on the
  `default` session. The volition tools share the same dispatch path, so they are
  reachable when the model decides to call them.

The gap is the model's choice, not the plumbing.

---

## Open Question / What Couldn't Be Verified

The browser relay diagnostic exchanges have `output: null`, so the **spoken answer text
was not captured** in `state/realtime/diagnostics/default.jsonl`. We can confirm no tool
was called, but cannot judge the grounding or simulated-state-framing criteria from these
artifacts. If the model-visible output is needed, capture it from the realtime UI
transcript or extend diagnostics to persist trusted-turn output text.

---

## Suggested Next Steps (in scope for this experiment)

These stay within the experiment's scope (read-only inspection; context injection and
write paths remain out of scope):

1. **Strengthen the instruction into an explicit rule.** The current wording is
   advisory. Try an imperative, unconditional form, e.g. "Whenever the user asks about
   your focus, goals, motivations, or internal state, you MUST call
   `inspect_volition_state` before answering; never answer such questions from memory."
2. **Consider a scoped `tool_choice` nudge.** Leaving `tool_choice: "auto"` for general
   turns but forcing a volition tool when the transcript matches an
   introspection-style prompt would deterministically exercise the path. This is a
   behavioral change to the trusted turn and should be weighed against the experiment's
   "model decides" intent.
3. **Re-run the two prompts** after either change and confirm a `ToolLoop` phase in
   `engine.log` plus non-empty `tool_requests`/`tool_executions`.

> Note: feeding the volition snapshot directly into context instead of relying on a tool
> call would also work, but "Context injection before `response.create`" is explicitly
> **out of scope** for this experiment.

---

## How to Verify a Future Fix

1. **Rebuild and restart** (`qsf.ps1 realtime` does `cargo run`, which rebuilds):
   ```powershell
   .\scripts\qsf.ps1 realtime
   ```
2. **Speak the experiment prompts:**
   - "What are you currently focused on?"
   - "What goals relate to helping me?"
3. **Check `engine.log` for a `ToolLoop` phase** on the new call id (its absence means
   the tool was not called). Remember log timestamps are UTC.
4. **Check the diagnostic file for tool records:**
   ```powershell
   python -c "
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

   Note: trusted sideband tool records also appear in `state/session/session-state.json`
   (mtime should advance on a successful tool call). Check both.
5. **Verify the trace fields** in `result_summary` for `select_volition_goals` against
   `Experiment.RealtimeVolitionReadOnlyInspection.md` §Trace Completeness Contract.
6. **Confirm the spoken answer** references specific goal names from the fixture and uses
   simulated-state language. Capture this from the UI transcript — the relay diagnostic
   currently stores `output: null`.

---

## Key File Locations

| Purpose | Path |
|---|---|
| Session instructions | `crates/qsf_realtime_server/src/state.rs:19` |
| Tool definitions | `crates/qsf_realtime_server/src/realtime/tools.rs:114` |
| Volition tool implementations | `crates/qsf_realtime_server/src/realtime/volition_tools.rs` |
| Trusted instructions assembly | `crates/qsf_realtime_server/src/realtime/injection.rs:95` |
| Sideband tool dispatch | `crates/qsf_realtime_server/src/realtime/sideband.rs:1380` |
| Trusted `response.create` | `crates/qsf_realtime_server/src/realtime/sideband.rs:1417–1422` |
| Diagnostic output | `state/realtime/diagnostics/default.jsonl` |
| Engine log (UTC timestamps) | `engine.log` (project root) |
| Session state | `state/session/session-state.json` |
| Realtime launcher | `scripts/qsf.ps1:979` |
| Experiment spec | `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md` |
