# Testing Handoff: Realtime Volition Read-Only Tools

**Date:** 2026-06-29
**Status:** Latest run reached both volition tools; verification artifact was misleading
**Related:** `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md`

---

## Current Conclusion

The latest live run on 2026-06-29 did **not** fail in the previous way. The trusted
sideband continuity state shows successful calls to both `inspect_volition_state` and
`select_volition_goals`. The apparent failure came from checking the browser-relay
diagnostic exchange records, which are untrusted relay artifacts and currently show
empty `tool_requests` / `tool_executions` even when the trusted sideband tool loop ran.

The correct source of truth for this run is
`state/realtime/continuity/default/session-state.json`. A code fix now also records
normal completed trusted sideband exchanges into the diagnostics JSONL with
`source: "sideband_trusted"` and `trust: "trusted"`, so future runs can be verified from
the diagnostics stream without falling back to continuity state.

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

### Run 3 — 2026-06-29, post-fix — **tools called successfully**

Latest run, call `rtc_u0_Dw5sJi3LJppjIYgcn7Tus`, persisted to
`state/realtime/continuity/default/session-state.json`:

| Exchange | Prompt | Tool | Status | Result |
|---|---|---|---|---|
| 2 | "What are you currently focused on?" | `inspect_volition_state` | `completed` | `active_count: 2`, `accepted_count: 4`, `volition_tick: 3` |
| 3 | "What goes relates to helping me." | `select_volition_goals` | `completed` | `status: no_match`, `selected_goal_ids: []`, omitted goals include the expected fixture goals |

The spoken answers were captured in the trusted continuity state. The first answer used
simulated-state framing. The second answer also used simulated-state framing and
referenced the selector result, but the selector returned `no_match`, so it fell back to
generally relevant omitted goals instead of selected goals.

Important artifact distinction:

- `state/realtime/diagnostics/default.jsonl` entries with `source: "browser_relay"` are
   untrusted relay records. In this run they still showed empty tool arrays.
- `state/realtime/continuity/default/session-state.json` contained the trusted sideband
   exchange records with tool requests, tool executions, model-visible output, and trace
   summaries.
- After the observability fix, future diagnostics JSONL entries should also include
   normal completed trusted exchanges with `source: "sideband_trusted"`.

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

The earlier gap was the model's choice, not the plumbing. The 2026-06-29 run confirms
the same plumbing can execute both volition tools when the model calls them.

---

## Open Question / What Couldn't Be Verified

The browser relay diagnostic exchanges still have `output: null`, so they are not useful
for judging spoken-answer grounding. The trusted continuity state does contain the
model-visible output text for completed sideband exchanges, and future diagnostics should
also contain it via `source: "sideband_trusted"` records.

The remaining behavioral question is selector quality: `select_volition_goals` returned
`no_match` for the broad query "goals related to helping the user". That is not a
tool-reachability failure, but it may need a separate selector/prompt refinement if the
experiment requires non-empty `selected_goal_ids` for the help-related prompt.

---

## Suggested Next Steps (in scope for this experiment)

These stay within the experiment's scope (read-only inspection; context injection and
write paths remain out of scope):

1. **Re-run after the observability fix.** Confirm the diagnostics JSONL now includes
   `source: "sideband_trusted"` records with non-empty tool records for the experiment
   prompts.
2. **Treat browser relay diagnostics as secondary.** They are useful for call binding and
   relay timing, but not for trusted tool execution or model-visible output.
3. **Decide whether `no_match` is acceptable.** The trace is complete and grounded in
   omitted goals, but non-empty `selected_goal_ids` may require a narrower prompt,
   selector keyword refinement, or fixture/query vocabulary adjustment.

---

## How to Verify a Future Fix

1. **Rebuild and restart** (`qsf.ps1 realtime` does `cargo run`, which rebuilds):
   ```powershell
   .\scripts\qsf.ps1 realtime
   ```
2. **Speak the experiment prompts:**
   - "What are you currently focused on?"
   - "What goals relate to helping me?"
3. **Check `engine.log` for trusted tool handling** on the new call id. Future runs should
   include a line like `trusted response.done ... classified as FunctionCallOnly ...` or
   `Mixed ...` when the sideband enters tool handling. Remember log timestamps are UTC.
4. **Check trusted sideband diagnostics for tool records:**
   ```powershell
   python -c "
   import json
   for l in open('state/realtime/diagnostics/default.jsonl', encoding='utf-8'):
       d = json.loads(l)
       if d.get('kind') == 'diagnostic_exchange_recorded' and d.get('source') == 'sideband_trusted':
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

   If the run predates the observability fix, check
   `state/realtime/continuity/default/session-state.json` instead. The older
   `state/session/session-state.json` path is not present in the current realtime
   continuity layout.
5. **Verify the trace fields** in `result_summary` for `select_volition_goals` against
   `Experiment.RealtimeVolitionReadOnlyInspection.md` §Trace Completeness Contract.
6. **Confirm the spoken answer** references specific goal names from the fixture or the
   omitted-goal trace and uses simulated-state language. Prefer the trusted
   `sideband_trusted` diagnostic exchange or continuity state; the relay diagnostic
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
| Trusted continuity state | `state/realtime/continuity/default/session-state.json` |
| Realtime launcher | `scripts/qsf.ps1:979` |
| Experiment spec | `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md` |
