# Glossary

## Status

Candidate project glossary. This document explains vocabulary used across Qualia Signal
Foundry; it is not a new decision record and does not override code, the decision log,
or architecture implementation-status sections.

## Purpose

The project uses research vocabulary from several overlapping domains: runtime systems,
memory, realtime voice, tool use, context management, observability, and volition. Some
imported documents also use different names for concepts the project already has.

Use this glossary as a translation aid. When a term sounds broad, anthropomorphic, or
imported from another document, translate it into the project term here and then verify
current implementation status against code and architecture docs.

Core rule:

```text
External or informal wording -> project vocabulary -> verify status against code and architecture docs.
```

## Project And Documentation Terms

| Term | Meaning in this project | Notes |
|---|---|---|
| Qualia Signal Foundry / QSF | The experimental platform for simulating consciousness-like behavior. | The project studies structures and behavior; it does not claim real consciousness. |
| Simulator | The runtime system being built and studied. | Often used for the whole QSF runtime plus model/tool/memory surfaces. |
| Consciousness-like | Research vocabulary for observable structures such as memory, attention, salience, tools, and volition. | Not a claim of subjective experience. |
| Run | One execution of an experiment or app path that can produce artifacts, logs, reports, and state. | Run artifacts are evidence about that run only. |
| Session | A continuing interaction context, such as a text loop, voice loop, or realtime call. | Some state is session-local; some can be promoted to durable memory. |
| Experiment | A focused validation slice under `docs/Experiments/Experiment.*.md`. | Experiments gather evidence; they are not automatically project commitments. |
| Plan | A phased implementation document under `docs/Plans/Plan.*.md`. | Do not confuse with the volition brief's "multi-turn Plans" concept. |
| Design note | A focused design/reconciliation document under `docs/Plans/Design.*.md`. | Supports a plan; verify durable commitments against the decision log. |
| Decision log | `docs/DecisionLog.md`, the durable source of deliberate project commitments. | Summaries and postmortems usually belong elsewhere. |
| Architecture doc | A document under `docs/Architecture/` describing current or candidate system structure. | Read its Implementation Status section first. |
| Implementation Status | The section that says what is implemented, partial, or not yet implemented. | This section scopes the rest of an architecture doc. |
| Artifact | A persisted output used as evidence: logs, reports, state JSON, diagnostic JSONL, traces, or run files. | Artifacts are stronger evidence than plans but still need source/trust context. |
| Trace | Structured evidence explaining why something happened. | A trace should carry enough identifiers to link back to inputs, records, or artifacts. |

## Runtime And State Terms

| Term | Meaning in this project | Notes |
|---|---|---|
| Runtime loop | The event-driven loop that turns input into actions, reducer updates, model/tool calls, output, and traces. | Governed by the unidirectional flow rule. |
| Event | A structured record of something that happened. | Reducers consume events to update state. |
| Reducer | A pure function that transforms state from events. | Reducers should stay unit-testable and side-effect free. |
| State | The current structured runtime/session/system data. | State changes through reducers, not hidden side effects. |
| Action | A requested operation emitted by the runtime, often leading to a side effect. | Side effects feed results back as events. |
| Side effect | I/O, model calls, tool execution, filesystem writes, or provider calls. | Isolated outside reducers. |
| Render | The projection of state into output, UI, report, or model-visible context. | View derivation should live in selectors/view-models. |
| ModelRole | A named role for model use, with its own purpose and allowed tools. | Keeps model invocation explicit rather than ambient. |
| ModelClient | The provider-agnostic boundary for model calls. | Mock and provider-backed clients implement the same contract. |
| Live loop | The low-latency path for interactive text or voice behavior. | Heavy reflection and consolidation should stay outside this path. |
| Sleep / consolidation | Offline or session-end processing that summarizes, promotes memories, extracts open questions, or prepares future state. | Inspired by sleep, but implemented as inspectable data processing. |
| Continuity | The mechanism by which useful state survives across turns or sessions. | Can come from session state, memory, summaries, or future persisted volition state. |

## Memory And Context Terms

| Term | Meaning in this project | Notes |
|---|---|---|
| MemoryRecord | A versioned durable memory item. | Records are not raw transcripts by default; they carry schema and metadata. |
| Association | A weighted relationship between memories. | Used for associative retrieval and reinforcement. |
| MemoryStore | The durable store for memory records and associations. | Different loops can resolve to different state roots. |
| Retrieval | Selecting relevant memory records for a query or current turn. | Retrieval can omit candidates with explicit skip reasons. |
| Reinforcement | Updating memory/association strength from live use or sleep-side evidence. | Reinforcement is evidence-backed, not mystical reward. |
| Promotion | Moving a candidate observation or summary into durable memory. | Some paths require manual review; some routine candidates can be auto-promoted. |
| ContextFragment | A bounded piece of context that can be assembled into model-visible prompt context. | Lives in `qsf_context`, not `qsf_volition`. |
| ContextBudget | The token/space budget for context assembly. | Forces deliberate selection instead of appending everything. |
| ContextAssembly | The selected and omitted context fragments for a turn, with reasons. | Useful for explaining why the model saw some context and not other context. |
| Hot context | Recent turns still directly available in the live loop. | Exact thresholds are implementation-specific. |
| Warm summary | A compact summary replacing older live turns when the context budget ages them out. | Used for continuity without keeping all raw turns hot. |
| Open question | A tracked unresolved question or design uncertainty. | Can feed sleep, memory, and volition candidate goals. |

## Realtime Voice Terms

| Term | Meaning in this project | Notes |
|---|---|---|
| Realtime session | A live voice session mediated by `qsf_realtime_server` and provider realtime APIs. | Not the same as an offline experiment run. |
| Call id / provider id | Provider-side identifier for a realtime call or utterance. | Useful for correlating logs and diagnostics. |
| Exchange | One trusted user/model interaction record, often containing input transcript, output text, tool requests, and tool executions. | Trusted sideband exchanges are the source of truth for tool execution. |
| Sideband | The server-owned trusted realtime path that controls tools, instructions, and response creation. | Preferred source for tool records and model-visible output. |
| Trusted sideband diagnostics | Diagnostic records with `source: "sideband_trusted"` and `trust: "trusted"`. | Authoritative for trusted tool execution in realtime validation. |
| Browser relay diagnostics | Browser-relayed diagnostic records from the realtime UI path. | Useful for timing/call binding, but not authoritative for trusted tool execution. |
| Continuity state | Persisted session state such as `state/realtime/continuity/<session>/session-state.json`. | Durable trusted exchange records can live here. |
| `response.create` | Provider realtime request to produce a model response. | Context injection must happen before this call to influence the turn. |
| Final transcript | The trusted finalized transcription of user speech. | Partial transcripts are useful for UI but should be treated carefully as evidence. |

## Tool Terms

| Term | Meaning in this project | Notes |
|---|---|---|
| Tool | A controlled capability the simulator can call to inspect, calculate, retrieve, or observe something. | Early tools are primarily perception extensions. |
| Tool as perception | The design stance that tools extend observation rather than grant unconstrained agency. | Especially important for safety and traceability. |
| ToolRegistry | Runtime registry of available tools. | Dispatch goes through permissions and registry lookup. |
| ToolDefinition | Model-facing schema/name/description for a callable tool. | Definitions are what the model sees. |
| ToolRequest | Internal request record created from a model tool call. | Contains tool name, input, structured args, permission, and provenance. |
| ToolResult | Internal result record: output text, observation summary, category, and side-effect level. | Tool results can be fed back to the model or persisted as traces. |
| ToolPermission | The permission/side-effect boundary for a tool call. | Realtime tools are currently restricted to read-only. |
| Read-only tool | A tool that can inspect or compute but not mutate project/runtime state. | `inspect_session_state`, `inspect_volition_state`, and `select_volition_goals` are examples. |
| Allow-list | The set of tools a role or realtime session is permitted to call. | Unlisted tool calls are denied before execution. |

## Volition Terms

| Project term | Meaning in this project | Common external wording | Status |
|---|---|---|---|
| Volition | The inspectable mechanism by which persistent internal tensions select, preserve, and revisit meaningful discrepancies between current state and active concerns. Not biological desire. | motivation, autonomy, internal life | Built in `qsf_volition`; realtime influence still in progress |
| Tension | A durable pressure or concern, such as coherence, continuity, curiosity, or boundary preservation. Tensions back goals and set arbitration precedence. | drive, personality trait, value | Built |
| Goal | A concrete, inspectable state object derived from one or more tensions. It names a concern, carries evidence references, has a lifecycle status, and can propose bounded initiatives. | goal, desired state, persistent interest | Built |
| InitiativeProposal | A proposed bounded internal effect derived from a selected goal, such as reflection, context retrieval, experiment proposal, or surfacing an open thread. | intention, short-term conversational move | Built |
| InitiativeOutput | The structural result of executing a bounded initiative proposal. It records what the runtime would do internally; it is not an external action. | intention result, conversational action | Built |
| Mode | An explicit arbitration-bias setting: `Neutral`, `Focused`, or `Exploratory`. It changes ranking within the biasable band but cannot override protected tiers. | mood, personality mode, disposition | Built |
| VolitionFixture | The static seed data: tensions plus accepted fixture goals. It is the stable reference state used to create `VolitionState`. | personality configuration, drive set | Built |
| VolitionState | Durable-within-a-run dynamic state: tick, per-goal dynamic state, pending candidates, accepted candidates, and mode. | current internal state, motivational state | Built; per-session realtime state exists |
| GoalDynamicState | The runtime part of a goal: lifecycle status, salience, reinforcement count, evidence refs, cooldown, and last initiative output. | current goal state | Built |
| VolitionEvent | A pure reducer event that changes volition state, such as `GoalActivated`, `GoalBlocked`, `GoalSatisfied`, `GoalRetired`, or `ModeChanged`. | motivational update, lifecycle update | Built |
| GoalStatus | The lifecycle status of a goal: `Proposed`, `Accepted`, `Active`, `Blocked`, `Satisfied`, `Cooldown`, or `Retired`. | goal lifecycle | Built |
| GoalScope | Where a goal applies: `Input`, `Session`, or `Project`. | local/session/persistent goal | Built |
| EvidenceRef | A non-empty reference to evidence supporting a goal, progress update, satisfaction, or candidate acceptance. | justification, grounding, trace evidence | Built |
| Salience | Integer points that make a goal more or less prominent over ticks. It rises on activation/progress and decays over time. | importance, emotional intensity, unresolvedness | Built |
| GoalSelection | A context-neutral selected goal: goal, relevance score, matched terms, and proposed initiative. | chosen goal, selected intention | Built |
| RankedSelectionResult | The selector output grouping selected goals, omitted goals, cooldown-suppressed goals, and visible-blocked goals. | ranked goals, relevant goals | Built |
| OmittedGoal | A goal considered by selection but not selected, with a reason. | omitted candidate, non-selected goal | Built |
| Arbitration | Deterministic conflict resolution among selected goals. Lower effective tension tier wins; ties use priority and goal id. | choose what to pursue, conflict between goals | Built |
| Arbitration tier | The conflict-resolution priority on a tension. Lower number wins. Tier 1 is strongest. | importance, priority, safety level | Built |
| Protected tier floor | Tiers 1 through 3 are protected from mode bias. User intent and current task completion must not be displaced by curiosity/exploration. | user-control safety rule | Built in mode arbitration; protected goals are in realtime seed |
| AllowedEffect | The bounded internal effects an initiative may propose: `Reflect`, `RetrieveContext`, `ProposeExperiment`, or `SurfaceOpenThread`. | allowed action, intention type | Built |
| DetectedDelta | A recorded discrepancy between current input/state and a goal concern. It cites matched evidence and the goal concern summary. | motivational delta, world-model mismatch | Built in compact offline form |
| GoalCandidate | A proposed new goal awaiting explicit accept/reject before it can influence selection. | memory-created goal, emergent goal | Built offline |
| VolitionStateInspection | A compact inspection view grouping goals by status and summarizing recent initiative output. | introspection result, internal-state summary | Built |
| StableBaselineLayer | A constant session-start rendering of configured tension priors, default mode, project stance, and trust boundary. It is the prompt-facing form of "personality" without adding a mutable personality object. | personality layer, baseline stance | Designed next; not started |
| VolitionContextPacket | A compact dynamic turn packet planned for injection before the initial realtime `response.create` for a trusted user turn, containing selected goal/intention context and trace fields. It is one layer in the broader injection stack, not the whole stack. | ambient motivational context | Designed next; not started |
| OpportunitySignal | A planned deterministic signal that trusted user input created a goal-relevant opportunity, with a grounding span or goal/memory id. | notice opportunities | Designed next; not started |
| ShapingIntensity | A planned `None`/`Low`/`Medium`/`High` dial for how strongly volition may shape the next response. Protected tiers clamp intensity. | autonomy level, conversational control policy | Designed next; not started |
| Bounded initiative | A volition output that can shape reflection/context/conversation but cannot execute write-capable external effects. | autonomous action, self-directed behavior | Partly built offline; realtime live-loop use not started |
| Read-only volition tools | Realtime tools `inspect_volition_state` and `select_volition_goals`. They inspect state without mutation and persist trusted traces. | introspection tools | Built and live-validated |

## External Volition Brief Translation

| External brief term | Translate to project term | Notes |
|---|---|---|
| Personality | Tension set + tension priors + arbitration tiers + `Mode` | No separate personality object is planned. Stable disposition is the configured tension set and its declared weights. |
| Drives | `Tension` | A drive is not an executable behavior. It backs goals and arbitration. |
| Goals | `Goal` | Same broad word, but project goals are inspectable records with lifecycle, scope, evidence, and allowed effects. |
| Intentions | `InitiativeProposal` / `InitiativeOutput` | Local proposed conversational/internal moves. |
| Plans | Multi-turn initiative sequence | Deferred new scope. Do not confuse with `docs/Plans/Plan.*.md`. |
| Notice opportunities | `OpportunitySignal` / opportunity detection | Designed for realtime context injection; deterministic and grounded in input spans or goal/memory ids. |
| Choose what to pursue | Selection + arbitration + mode bias | Mostly built. Realtime influence comes through context injection next. |
| Maintain internal preferences | Tensions + salience + arbitration | Built as inspectable bias, not ungrounded desire. |
| Initiate topics | Bounded initiative + realtime context injection | Offline structure exists; live conversational initiation is later work. |
| Resist or redirect | Shaping intensity with protected-tier cap | Designed next; must be rare, trace-backed, and subordinate to protected tiers. |
| Unfinished business | `Blocked` goals, open-thread goals, persistence/continuity | Partly built. Cross-session persistence is an open ordering decision. |
| Conscious goals | Inspectable goals | Built for ordinary goal inspection. A formal visibility attribute is deferred. |
| Subconscious goals | Goal visibility/filtering attribute | Deferred. If added, it must not create untraceable hidden motives. |
| World model | Compact structured current state | Built only in narrow forms: current input, memories, traces, open questions, runtime state. Not a full simulated reality. |
| Desired state | Goal/tension concern summary | Represented through goals and their satisfaction conditions. |
| Delta | `DetectedDelta` or opportunity/context-injection trace | Built in compact offline form; planned for realtime influence. |
| Emotion | Evidence-derived functional signal | Deferred and gated. Must mean a trace-backed signal such as repeated blocked goals, not felt experience. |
| Curiosity | Usually `research-curiosity` tension or an active curiosity goal | Built as a tension/goal pattern. |
| Frustration | Repeatedly `Blocked` goal despite attempts | Deferred as a derived signal. |
| Satisfaction | `GoalSatisfied` with an `EvidenceRef` | Built as lifecycle state; emotion-like interpretation deferred. |
| Tension/conflict | Arbitration conflict among selected goals | Built. Note that `Tension` also means durable pressure, so context matters. |
| Memory creates goals | `propose_goal_candidates` | Built offline; accepted candidates can join selection. |
| User goals vs simulator goals | Goal provenance/ownership tag | Deferred. Current protected tiers already prioritize explicit user intent and current task completion. |
| Autonomy level | Shaping-intensity policy | Designed into context injection, not a free-form personality setting. |
| Idle-time behavior | Sleep/consolidation pass plus future volition re-ranking | Partly built outside volition; volition-specific idle behavior deferred. |
| External actions | Out of scope for current volition work | Future external actions require separate permission and approval design. |

## Phrases To Read Carefully

- "Want", "desire", and "motivation" should be read as shorthand for selected,
  trace-backed goals or tensions. They are not claims of subjective experience.
- "Emotion" means a possible functional signal derived from evidence. It is not a felt
  state.
- "Personality" means the stable tension configuration plus mode bias unless a future
  decision introduces another representation.
- "Autonomy" currently means bounded conversational shaping and internal initiative. It
  does not mean permission to take external-world actions.
- "Tool use" usually means perception or controlled inspection, not unconstrained agency.
- "Plan" in the imported volition brief means a possible multi-turn initiative sequence.
  `Plan.*.md` files in this repository are implementation-planning documents.

## Reading Path

1. Read [ProjectFrame/DocumentStatus.md](ProjectFrame/DocumentStatus.md) to understand
   document authority.
2. Read [Architecture/Architecture.Overview.md](Architecture/Architecture.Overview.md)
   for the current high-level system map.
3. Read focused architecture docs for the subsystem you are working on, starting with
   their Implementation Status sections.
4. Read plans and experiments for intent and evidence, not as proof that a behavior exists.
5. For the imported volition brief, read
   [Plans/Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md)
   and translate terms through the volition sections above.
