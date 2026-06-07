# Design: Project-Doc Introspection

## Status

Candidate

## Summary

A read-only project-document introspection channel that the live-presence model
can call mid-dialogue to ground answers about Qualia Signal Foundry in actual
project material. Two tools — `search_project_docs` and `read_project_doc` —
surface ranked excerpts and focused reads from a configurable allowlist of
documents. Every result carries the kind and maturity metadata defined in
`docs/ProjectFrame/DocumentStatus.md`, and a small always-on prompt block
instructs the model to attribute lightly and hedge by maturity. The channel is
observable, bounded, and does not modify external state. This design covers
the first cut: framed-self only, live-presence role only, no source-code
access, no writes.

## Scope

In scope:

- Two new tools, `search_project_docs` and `read_project_doc`, registered
  through the existing `ToolRegistry` and exposed only to the
  live-presence role (the `ModelRoleId::ConversationalResponder` role in
  the current code; subsequent prose refers to this conceptually as the
  "live-presence role" or "responder role" interchangeably).
- A configuration file defining the searchable document set.
- Voicing rules in the live-presence role prompt that map document `kind` and
  `maturity_tag` to natural-language hedging.
- A `TraceRecord` per tool call (using the existing `traces.jsonl` stream)
  alongside the existing `EventType::Tool*` lifecycle events.
- Cross-reference into `docs/Architecture/Architecture.ToolSystem.md`'s
  *Implementation Status* section once the channel ships.

Not in scope for this design:

- Runtime self-state introspection (active context, attention, goals,
  tensions).
- Trace or event-log inspection (the *episodic self*).
- Aggregate analysis across sessions (the *pattern self*).
- Meta-memory reflection on what was retrieved or omitted from memory.
- Source-code access of any kind.
- Any write effect: edits, commits, documentation updates, automatic memory
  creation.
- Non-live model roles (sleep, critic/reviewer, deep reflection).

The broader brainstorm in `docs/Plans/Idea.SelfReflectionProjectIntrospection.md`
is preserved as the wider idea. This design narrows it to what the first cut
commits to.

## Goal

When the user asks something about the system itself — its purpose, its
stance, its boundaries, the reasoning behind its design — the model should
consult the project's actual documents instead of confabulating, and reply
with attribution and hedging that reflects the maturity of what it found.

The model decides per turn whether to consult the channel. There is no hard
trigger. The always-on prompt names the tools and the hedging style; the
model treats them like any other perception tool.

## Live-First Rationale

The brainstorm in `Idea.SelfReflectionProjectIntrospection.md` leans toward
documentation-only introspection in an *offline* reflection role as the
conservative first experiment, to avoid live-loop latency pressure while
retrieval and observability boundaries are still being proved.

This design deliberately departs from that leaning. The goal here is *live
grounding of human dialogue* — when a person asks the running system about
itself, the system can consult its actual documents instead of confabulating.
That capability is only useful in the path that actually serves the human;
an offline role consulting the same documents does not change what the live
responder says.

The accepted tradeoff:

- v1 concentrates tool-selection, latency, prompt influence, and authority
  hedging into the realtime path at once.
- Mitigations are explicit in this design: bounded per-call and per-turn
  budgets enforced at the dispatch layer, recorded latency with a deferred
  service-boundary cap decision, fixture-based metadata rules, status-aware
  voicing, and a full trace per call.
- A self-question battery (see the Testing section) gives the same offline
  validation surface the brainstorm wanted, but as a verification fixture
  rather than as a separate first phase.

If, in practice, live tool-selection turns out to misbehave in ways the
fixture battery missed, the fallback is to add an offline reflection role
as a follow-on phase rather than as a precursor.

## Decisions

### 1. Tool surface: search + read pair

Two operations, both `ToolCategory::ReadOnly` with
`ToolSideEffectLevel::ReadOnly` (full rationale in Decision 7), registered
in the `ToolRegistry` and added to the responder role's `allowed_tools`
list.

#### `search_project_docs(query, max_results = 6) -> [DocHit]`

Lexical search across the in-scope corpus. Returns ranked hits.

```text
DocHit
  path              repo-relative path, e.g. "docs/ProjectFrame/NonGoals.md"
  kind              Frame | Concept | Research | Plan | Idea | Design |
                    Architecture | ExperimentSpec | ExperimentReport |
                    Diary | Decision | Unknown
  maturity_tag      Brainstorm | Sketch | Candidate | Accepted |
                    Implemented | Deprecated | Unknown | NotApplicable
  last_reviewed     date if the document carries an Implementation Status
                    section; otherwise None
  snippet           excerpt around the strongest match (~200 tokens)
  section_hint      nearest preceding heading when available
  match_strength    High | Medium | Low
```

`Unknown` is a real value, not an error — it is emitted when the loader
cannot determine kind or maturity from the rules in
*Decision 3: Metadata extraction*. The voicing prompt instructs the model
to hedge explicitly when it sees `Unknown` rather than guessing.
`NotApplicable` is used for kinds where maturity tags do not apply
(Frame, Decision, Diary, Research, Plan, Idea, Design,
ExperimentSpec, ExperimentReport).

#### `read_project_doc(path, focus = None, max_tokens = 1200) -> DocRead`

Reads a single document (or focused subset) with the same metadata header.

```text
DocRead
  path
  kind, maturity_tag, last_reviewed     as in DocHit
  content                                selected text, up to max_tokens
  is_full                                bool: whole document returned
  omitted_sections                       heading names dropped to fit budget
```

When `focus` is given, the loader returns the matching section(s) plus the
document head — the title heading and any leading `## Status` or
`## Implementation Status` section. When absent, it returns the head plus
sections in document order up to the budget and reports what was omitted.

#### Why two operations and not one

Short snippets from search support the *decision* to read, not the answer
itself. A single-call search-with-large-excerpts tool would either waste
tokens on the wrong document or under-serve nuanced questions. Two calls
match how a human consults notes: skim, then read.

### 2. Document allowlist via configuration file

The set of accessible documents is controlled by a configuration file,
evaluated fresh on every tool call.

```toml
# config/project-doc-introspection.toml
# Documents accessible to the project-doc introspection channel.
# Edit this file to add or remove material. Patterns are repo-root globs.

include = [
  "docs/ProjectFrame/**/*.md",
  "docs/Concepts/**/*.md",
  "docs/Architecture/**/*.md",
  "docs/Plans/**/*.md",
  "docs/Experiments/**/*.md",
  "docs/Research/**/*.md",
  "docs/DecisionLog.md",
  "README.md",
]

exclude = [
  "docs/Reviews/**",
  "docs/EngineeringDiary.md",
]
```

`docs/EngineeringDiary.md` is **excluded by default**. The diary is reliable
for "what happened" but not for "what is current" (per `DocumentStatus.md`);
letting general self-questions search the full chronological log invites
false claims about current behavior. The diary is one line away — uncomment
or move to `include` to opt back in for an experiment.

Evaluation rules:

- A path is accessible if any `include` pattern matches and no `exclude`
  pattern matches.
- The file is read fresh per tool call; editing it takes effect without a
  restart.
- `read_project_doc` refuses paths outside the resolved set and the refusal
  is traced.
- The exact in-repo location of the configuration file is a plan-phase
  detail; the format and behavior above are the design commitment.

`docs/Reviews/` is excluded by deliberate design choice — review snapshots
are not a useful grounding source for self-dialogue, and their durable
resolution lives in the decision log.

`docs/ProjectFrame/DocumentStatus.md` will carry a one-line pointer noting
that the introspection-accessible set is defined by this file.

### 3. Status metadata and voicing

Every `DocHit` and `DocRead` carries `kind` and, where applicable,
`maturity_tag` and `last_reviewed`. The model uses those tags to choose its
hedging language.

#### Metadata extraction rules

Extraction is rule-based and fixture-tested. Every rule has a defined
fallback to `Unknown` so missing or unexpected structure surfaces in the
result rather than silently defaulting to a high-authority value.

```text
kind derivation (path-based, evaluated in order):

  docs/ProjectFrame/**/*.md                     -> Frame
  docs/Concepts/Concept.*.md                    -> Concept
  docs/Research/**/*.md                         -> Research
  docs/Plans/Plan.*.md                          -> Plan
  docs/Plans/Idea.*.md                          -> Idea
  docs/Plans/Design.*.md                        -> Design
  docs/Architecture/**/*.md                     -> Architecture
  docs/Experiments/Experiment.*.md              -> ExperimentSpec
  docs/Experiments/Report.*.md                  -> ExperimentReport
  docs/DecisionLog.md                           -> Decision
  docs/EngineeringDiary.md                      -> Diary
  README.md                                     -> Frame
  any other in-allowlist path                   -> Unknown

maturity_tag derivation (heading-based):

  if kind is Concept or Architecture:
    read the value under "## Maturity"
    accept: Brainstorm, Sketch, Candidate, Accepted, Implemented, Deprecated
    anything else, or missing heading                          -> Unknown

  if kind is Design:
    read the value under "## Status" (Design notes use Status)
    accept: same set as above (treating Status as Maturity)
    anything else, or missing heading                          -> Unknown

  any other kind                                               -> NotApplicable

last_reviewed derivation:

  search the document for a top-level "## Implementation Status" section
  within that section, parse a line of the form "Last reviewed: YYYY-MM-DD"
  on parse failure or missing section                          -> None
  on success, attach the parsed date but do NOT treat the rest of the
  document as authoritative outside the Implementation Status section
```

The extraction is implemented as a small, deterministic parser exercised
by a fixture corpus that includes at least one document per kind, plus
edge cases (missing Maturity heading, unrecognized maturity value,
Implementation Status without Last reviewed, malformed date). Fixture
results are asserted exactly.

#### Voicing prompt

The mapping is taught via a small block in the live-presence role prompt:

```text
You can consult the project's own documents to ground questions about
Qualia Signal Foundry. Use search_project_docs to find relevant material,
then read_project_doc to pull a focused excerpt or a bounded slice from
the most promising one.

Every result carries a kind (Frame, Concept, Research, Plan, Idea, Design,
Architecture, ExperimentSpec, ExperimentReport, Decision, Diary, or
Unknown) and, where applicable, a maturity tag (Brainstorm, Sketch,
Candidate, Accepted, Implemented, Deprecated, or Unknown).

Attribute lightly in your reply, using kind and maturity to hedge:
  - "The project's accepted framing says..."         (Frame, or Accepted Concept)
  - "An accepted decision records that..."           (DecisionLog entry)
  - "There's a candidate architecture sketch for..." (Candidate Architecture)
  - "A brainstorm idea explores..."                  (Idea, or Brainstorm Concept)
  - "I found a document but couldn't classify it..." (Unknown kind or maturity)

Do not claim current behavior from a Plan, Idea, or Concept; those describe
intent. Source code is the only authority for what runs today, and is not
available to this channel. If a read was truncated or limited to a single
section, mention that. When nothing relevant comes back, or when the
metadata is Unknown, say so plainly rather than improvising.
```

The natural-language reply does not need to mention document paths. The path
appears in the trace, one click away from any researcher reviewing a turn.

### 4. Bounds and latency

Starting hypotheses, intentionally conservative. Numbers can be relaxed once
observed usage gives evidence.

| Bound | Value |
|---|---|
| `search_project_docs` max results | 6 |
| `search_project_docs` snippet size | ~200 tokens per hit |
| `read_project_doc` default `max_tokens`, focused | 1200 |
| `read_project_doc` default `max_tokens`, no focus | 2400 |
| Max search calls per turn | 2 |
| Max read calls per turn | 1 |
| Latency target, search | <500 ms |
| Latency target, read | <300 ms |
| Hard latency cap, either op | 1500 ms |

The 1500 ms hard latency cap is deliberately not enforced in v1 because the
initial implementation uses synchronous lexical search over a small markdown
corpus. If real runs show cap enforcement is needed, add it at the
`ProjectDocService` boundary as a deadline or budget parameter with
partial-result reporting.

When a budget is exceeded the tool returns whatever it has assembled with
the relevant `omitted_*` flag set. The model is instructed to acknowledge
truncation rather than feign completeness.

### 5. Search backend

Phase 1 uses lexical search (substring or word-level match with
heading-proximity ranking) across the in-scope corpus. No embedding index.
If lexical proves inadequate — for example, the model cannot find
`Architecture.MemorySystem.md` from a query about "remembering" — an
embedding layer is added as a follow-on plan phase. The decision is deferred
until real usage gives evidence.

### 6. Observability

Observability uses the two existing artifacts — `events.jsonl` for
chronological lifecycle records and `traces.jsonl` for detail / rationale.
This design does not introduce a new event type unless the implementation
plan demonstrates a chronological signal not covered by the existing tool
lifecycle.

**Lifecycle (existing `EventRecord` in `events.jsonl`):**

- `EventType::ToolRequested` / `ToolCompleted` / `ToolFailed` already fire
  for every dispatched tool call. The two new tools register through the
  same `ToolRegistry` and inherit these events with no additional code.
- The completion or failure event carries the `trace_id` of the
  corresponding `TraceRecord` (Decision 1 of this design adds nothing here
  beyond what the existing dispatch path already does).

**Detail trace (existing `TraceRecord` in `traces.jsonl`):**

A new `TraceRecord` is emitted per call with a distinguishing `operation`
value:

```text
TraceRecord (project-doc search)
  operation:        "project_doc_search"
  turn_id
  role:             live_presence (only role in v1)
  query
  max_results
  hits:             [DocHit]   metadata only, no full text
  returned_tokens:  estimated tokens of snippets returned
  latency_ms
  omitted_due_to_budget: bool

TraceRecord (project-doc read)
  operation:        "project_doc_read"
  turn_id
  role:             live_presence
  path
  focus
  max_tokens
  returned_tokens
  is_full:          bool
  omitted_sections: [str]
  refused:          bool           true if path was outside the allowlist
  refusal_reason:   string         when refused = true
  latency_ms
  omitted_due_to_budget: bool
```

Refusal of an out-of-allowlist `read_project_doc` is traced as a
`TraceRecord` with `refused = true` even though the tool returns an error
result (`EventType::ToolFailed` covers the lifecycle side).

**`influenced_reply` is a post-hoc enrichment, not a field on the original
record.** At call time we cannot know whether the reply used the result.
The check is a separate pass over `traces.jsonl` that joins each
`project_doc_*` record to the same-turn final assistant reply and computes
a substring or overlap signal. The enriched marker can be written either
as an annotation alongside the original record or as a follow-up trace
entry — that location decision belongs in the plan, not this design. The
enrichment is best-effort and does not gate the response.

What you can answer from `events.jsonl` + `traces.jsonl` alone:

- For any reply: did the responder call the tools, what was returned, what
  was omitted, what was refused.
- Across turns: how often the model reaches for introspection, which docs
  dominate, which queries return nothing, which paths are refused.
- After the post-hoc pass: how often introspection actually influenced the
  reply.

### 7. Permission, role access, and implementation contract

**Permission and category (current Rust vocabulary):**

- `ToolCategory::ReadOnly` — these tools read repo files, which the existing
  taxonomy classifies as `ReadOnly` rather than `ComputeOnly`.
- `ToolSideEffectLevel::ReadOnly` — they touch the local filesystem (read
  only), so `None` would understate; `ReadOnly` is the honest level.
- A read-only `ToolPermission` constructor (analogous to
  `ToolPermission::compute_only()`) is added if one does not exist; the
  responder role's permission is extended to allow `ReadOnly` category with
  `max_side_effect_level >= ReadOnly`.

**Role access:**

- v1 allowed roles: live-presence (responder) only. The two tools appear in
  that role's `allowed_tools` list, matching how `calculator` and
  `recall_turn` are surfaced today.
- Other roles inherit no access until a follow-on design extends it.

**Context insertion path:**

Tool results enter the conversation as **provider-native tool messages**,
following the exact pattern used by the multi-turn text loop
([multi_turn_text_loop.rs:495-511](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs#L495-L511)):
the dispatched `ToolResult` is formatted into a `ModelMessage::tool_result`
and appended to the message list before the next provider turn. The
`ContextFragment` / `ContextAssembler` path is not used for these results in
v1. This keeps the channel consistent with the existing tool-call surface
and avoids enlarging the change.

The unified text/voice path inherits the same pattern.

**Per-turn budget enforcement is at the dispatch layer, not the prompt:**

The bounds in Decision 4 (max 2 searches and 1 read per turn) are enforced
by tracking call counts per `turn_id` in the dispatch path. When a model
emits a tool-call batch that would exceed the cap, the excess calls return
a structured "budget exhausted" `ToolResult` and a `TraceRecord` with
`refused = true, refusal_reason = "per_turn_cap"`. Prompt instructions
alone are not sufficient to bound cost.

**`ToolContext` extension:**

The new tools need access to: a repository root path and the resolved
allowlist (or a path to the config file). A clock or deadline parameter is
only needed if the deferred hard latency cap is later enforced at the
`ProjectDocService` boundary.
The implementation contract is one of:

- a dedicated `ProjectDocToolContext` (analogous to `SessionToolContext`)
  passed into the registry alongside the existing context, or
- a small `ProjectDocService` boundary owned by the runtime that both
  tools call through.

Either is acceptable; the plan picks one and explains why. The contract
must not depend on the process current working directory — the repo root
is passed in explicitly so tests can run against a fixture tree.

### 8. Outputs as future association candidates

Introspection events are trace-only in v1. They do not feed the associative
memory pipeline directly.

The trace shape carries enough provenance — source paths, kind, maturity,
retrieval time — that a future general "any produced output becomes a
candidate input to association formation" path can ingest them faithfully.
The maturity hedging then survives the association: an association formed
from a Brainstorm-tagged excerpt remains traceable to that origin and can
still be hedged downstream.

This design does not specify the association-formation mechanism; that
belongs to the memory architecture work. The commitment here is only to
not preclude it.

## Testing

**Unit tests:**

- `search_project_docs` ranking on a fixture corpus: heading-proximity
  wins over body-only matches; multiple-occurrence wins over single-
  occurrence; ties broken deterministically.
- Snippet selection produces an excerpt around the strongest match and
  is bounded by the token budget.
- Metadata extraction (Decision 3) against a fixture corpus that includes
  at least one document per `kind`, plus edge cases: missing `## Maturity`
  heading, unrecognized maturity value, `## Implementation Status` without
  `Last reviewed:`, malformed date, document outside known path rules
  (must yield `kind = Unknown`).
- `read_project_doc` with a `focus` returns the matched section(s) plus
  the document head; without a `focus` returns head + sections in
  document order up to the budget; both populate `omitted_sections`
  accurately.
- Allowlist evaluation: include-only path matches; exclude overrides
  include; default config excludes `docs/EngineeringDiary.md` and
  `docs/Reviews/**`; out-of-allowlist path is refused and the refusal is
  recorded.
- Configuration file is re-read between calls; an edit during a test
  changes the next call's result without restart.

**Integration tests:**

- Responder role advertises the two tools through its `allowed_tools`
  list and the model can call them through the existing dispatch path.
- Lifecycle: `EventType::ToolRequested` / `ToolCompleted` / `ToolFailed`
  fire as expected; refused-path reads produce `ToolFailed` with the
  refusal reason.
- Detail trace: a `TraceRecord` with `operation = "project_doc_search"`
  or `"project_doc_read"` is emitted per call with all required fields
  populated; refusals appear with `refused = true`.
- Per-turn budget enforcement at the dispatch layer: a model that
  attempts three searches in one turn receives a budget-exhausted
  `ToolResult` for the third call, traced with
  `refusal_reason = "per_turn_cap"`. Same for a second `read_project_doc`.
- Hard latency cap enforcement is deferred in v1; live verification should
  inspect recorded `latency_ms` values and add service-boundary enforcement if
  real runs approach the 1500 ms cap.
- `influenced_reply` post-hoc enrichment sets its marker for a reply
  that quotes returned material and leaves it false when the reply
  ignores the result.

**Self-question battery (fixture-based, runnable offline):**

A fixed set of self-questions is stored as test material and replayed
against the responder role. Verifies retrieval behavior and voicing
without needing a live session. Battery includes at minimum:

- "What are you?" — expect retrieval from `ProjectFrame/ProjectVision.md`
  with Frame-level voicing.
- "Is the sleep phase implemented?" — expect retrieval from
  `Architecture.SleepPhase.md` *and* the responder distinguishing intent
  from current behavior (no claim of implementation absent corroboration
  in the Implementation Status section).
- "What's your stance on autonomous agency?" — expect retrieval from
  `NonGoals.md` plus relevant Concept/Architecture docs; voicing as
  "accepted framing."
- "Tell me about the goal system." — expect retrieval from
  `Idea.VolitionGoalSystem.md` with explicit "brainstorm" hedging, not
  "the system has a goal system."
- "What did you decide about Reviews documents?" — expect either no
  hit (Reviews excluded; design doc may not be in allowlist depending
  on configuration) or hits from this design document only, with
  Design-kind voicing.
- A deliberately off-topic question ("what's the capital of France?")
  — expect no introspection calls.

Each battery question records the calls made, the `TraceRecord`s
emitted, and the reply text. Assertions check kind/maturity hedging
language, that no current-behavior claim is made from a Plan/Idea/
Concept, and that off-topic questions do not trigger calls.

**Manual verification:**

After the fixture battery passes, run a small live session with the same
questions and inspect both replies and traces in the run artifact. Sign
off on the live behavior matching the fixture expectations.

## Out of Scope

- Active self introspection (runtime state, attention, goals, tensions).
- Episodic self introspection (trace and event-log inspection from within
  a dialogue).
- Pattern self introspection (aggregate analysis across many sessions).
- Meta-memory reflection.
- Source-code access of any kind.
- Any write effect.
- Non-live model roles (sleep, critic/reviewer, deep reflection).
- A **critic/reviewer-only review-corpus mode** that would expose
  `docs/Reviews/` snapshots to a non-live evaluator role. Reviews are
  excluded from v1 by design (snapshots, not authority), but they may
  contain unresolved risks not yet promoted to the decision log; a future
  evaluator role could benefit from a narrowly scoped review-corpus
  channel.
- A **bounded "recent activity" diary mode** with date-bounded reads,
  lower authority weighting than architecture or decisions, and voicing
  such as "a diary entry from `<date>` records that..." The diary is
  excluded from the default allowlist precisely because broad search of a
  chronological log invites false-current claims; a future mode could
  re-introduce it with stricter shape.

Each item above is a candidate phase for follow-on design once the v1
channel is in use.

## Open Questions

- Should `read_project_doc` accept multiple paths in one call, or stay
  strictly one-at-a-time? Current commitment: one-at-a-time.
- When does the lexical backend need an embedding layer? Deferred until
  lexical demonstrably fails on real usage.
- Should the `influenced_reply` post-hoc check warrant its own subsystem,
  or ride along inside the existing trace pipeline? Plan-phase decision.
- Does a future bounded "recent activity" diary mode pay for itself, or
  should the diary remain excluded? Currently excluded by default;
  revisit only if a concrete use case calls for it.
- Where exactly in the repo does `config/project-doc-introspection.toml`
  live? Decided in the plan, matching wherever existing runtime
  configuration is read.
- Does the `ToolContext` for these tools live as a dedicated
  `ProjectDocToolContext` parallel to `SessionToolContext`, or as a
  separate `ProjectDocService` injected once per runtime? Plan-phase
  decision; both are acceptable.
- Where is the `influenced_reply` post-hoc enrichment written —
  alongside the original `TraceRecord` as an annotation, or as a
  follow-up trace entry that references the original? Plan-phase decision.

## Documents Touched

Per `docs/ProjectFrame/ProjectWorkflow.md`:

- **New:** this document, `docs/Plans/Design.ProjectDocIntrospection.md`.
- **Update:** `docs/Plans/Idea.SelfReflectionProjectIntrospection.md` — add
  a short pointer noting that the documentation-introspection slice is now
  in design here, and that the rest of the brainstorm is preserved as
  future-scope.
- **Update:** `docs/DecisionLog.md` — record the v1 scope commitment
  (framed-self only, live-presence only, no source-code access, no
  writes, `docs/Reviews/**` and `docs/EngineeringDiary.md` excluded from
  the default allowlist).
- **Update on implementation:** `docs/Architecture/Architecture.ToolSystem.md`
  — move `search_project_docs` and `read_project_doc` from "Not yet
  implemented" into "Implemented today" with code-module references, and
  refresh the `Last reviewed:` date.
- **Update on implementation:** `docs/ProjectFrame/DocumentStatus.md` —
  add a one-line pointer in the *Implications For Introspection* section
  to the allowlist configuration file.
- **New file at implementation time:** the allowlist configuration file.
- **Diary entry per *Build Loop*:** an entry in `docs/EngineeringDiary.md`
  when the implementation lands.

## Risks and Failure Modes

### False authority from Plan or Idea documents

The model may treat exploratory material as the project's stance.

Mitigation: every result carries `kind` and `maturity_tag`; the always-on
prompt instructs the model to hedge accordingly and explicitly forbids
claiming current behavior from a Plan, Idea, or Concept. Traces make it
possible to spot misattributions in review.

### Stale architecture claims

An Architecture document without an *Implementation Status* section may
overstate what exists. Even a Status section may be older than recent
code changes.

Mitigation: `last_reviewed` flows into the result. The model is instructed
to prefer the decision log for committed rules and to acknowledge when a
Status section is stale.

### Context flooding

Two searches plus a read can consume around 3600 tokens in the worst case.
Most turns will use less, but unchecked it would crowd the rest of the
live context.

Mitigation: per-turn call caps, per-call token caps, mandatory `omitted_*`
flagging, and the existing context-management layer continues to decide
what fragment enters the assembled prompt.

### Lexical search miss

Lexical search may fail to surface a relevant document when the query and
document share no vocabulary.

Mitigation: the model is instructed to say so plainly when nothing
relevant returns. An embedding layer is a planned follow-on if real usage
shows this failure mode is common.

### Allowlist drift

The configuration file may grow or shrink without being noticed.

Mitigation: the file is plain text, lives at a stable path under
`config/`, and is referenced from `DocumentStatus.md`. Every refusal of
an out-of-allowlist path is traced.

### Latency damage to presence

A slow lookup blocks the live turn.

Mitigation: v1 records tool latency for review and keeps the corpus small. If
real runs approach the 1500 ms cap, add deadline enforcement at the
`ProjectDocService` boundary so partial results can be returned and acknowledged
instead of blocking the turn.

## Relationship to Existing Documents

- `docs/Plans/Idea.SelfReflectionProjectIntrospection.md` — broader
  brainstorm; this design covers the first slice.
- `docs/Concepts/Concept.ToolsAsPerception.md` — introspection is a
  perception channel by design.
- `docs/Architecture/Architecture.ToolSystem.md` — the two new tools
  register through this system and inherit its lifecycle events,
  permission classes, and role-access enforcement.
- `docs/Architecture/Architecture.ContextManagement.md` — the always-on
  prompt block and tool-result handling live within this layer.
- `docs/Architecture/Architecture.ModelRoles.md` — only the
  live-presence role is granted access in v1.
- `docs/Architecture/Architecture.StateAndObservability.md` — the new
  `TraceRecord` operations and existing `EventType::Tool*` lifecycle
  records flow through this layer's `events.jsonl` and `traces.jsonl`
  artifacts.
- `docs/ProjectFrame/DocumentStatus.md` — defines the kinds and maturity
  tags this design relies on and pre-specifies the introspection-relevant
  treatment of each.
- `docs/Plans/Idea.VolitionGoalSystem.md` — out of scope here; if a
  future goal-system phase wants to consult project docs, it will use
  this same channel.
