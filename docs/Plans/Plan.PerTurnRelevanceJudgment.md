# Plan: Per-turn relevance judgment for memory and goal look-up

Status: In progress — pair-scoring foundation and deterministic memory retrieval landed
Maturity: Candidate
Area: Memory retrieval / Volition (goal activation) / Realtime live path / Evaluation infrastructure

## Why this plan exists

Live memory look-up today is exact-token lexical matching plus one-hop association
(`crates/qsf_memory/src/retrieval.rs`), and live goal activation is exact-token keyword matching
(`crates/qsf_volition/src/selection.rs`). Both are fast and both are wrong in the same way: a
stored text that is unmistakably *about* what the person just said, but shares no words with it,
activates nothing at all. The measured goal-side figure is 33.6% recall under conditions chosen to
flatter the scorer (`docs/Plans/Idea.SemanticMatchingCorpus.md`). Nothing measures the memory side
at all.

A class of language-based decision models has appeared that answers typed questions about a state
in one forward pass, with probabilities, in hundreds of milliseconds and at negligible cost. That
makes a *per-turn* judgment plausible for the first time: after every external input, ask which
stored memories and which goals are about what was just said, and get the relevant ones into the
assistant's context before it answers.

**Done** looks like: in a live (and headless scripted) conversation, a memory that is thematically
relevant but lexically disjoint from the user's utterance is retrieved and injected in the same
turn when the judge meets the deadline, or on the next turn when it does not; the same for goal
relevance; every judgment, deadline miss and fallback is visible in traces; and an offline scoring
set lets any new model be compared against the lexical baseline and against the current judge
within an hour.

The models involved are days old and expected to change, so this plan deliberately builds a
**swappable seam** and an **offline scoring set** first, and puts an **operator-judged stop/go
gate with pre-registered reference numbers** between the evidence and any live behavior change.

### Naming and ephemerality

This document owns its phase numbers. Durable artifacts — crates, modules, trace record kinds,
task contracts, experiment specs, decision-log entries — name stable behaviors
(`relevance_judgment`, `memory_relevance`, `goal_relevance`, `judge_admission`,
`selection_eligibility`, `deadline_outcome`, `carry_over`) and never cite a phase number
(`Agents.md`; `docs/ProjectFrame/ProjectWorkflow.md`).

---

## Verified against the source (2026-09-20, `main` @ a90a6cb)

Every claim below was re-checked in the code. They are load-bearing: several phases and several
design commitments exist only because of them.

1. **The memory relevance gate has a sibling that runs *before* it.** `retrieve_memories`
   (`crates/qsf_memory/src/retrieval.rs:77-165`) omits candidates in three ordered steps:
   `SUPERSEDED_WORLD_OBSERVATION_SKIP_REASON` first, then `RELEVANCE_GATE_SKIP_REASON`
   (`is_relevant_for_strategy`, line 218), then `RETRIEVAL_LIMIT_SKIP_REASON` after the limit is
   full. A judge verdict is admissible at the **second** step only: a superseded world observation
   must never be resurrected by a judge, and the limit cut is a budget decision, not a relevance
   decision.
2. **`retrieve_memories` is not pure: it reads the clock.** Line 89 calls
   `OffsetDateTime::now_utc()` and feeds it to `compute_recency_decay`, which affects the score and
   therefore the ordering and the limit cut. A frozen store with fixed timestamps produces
   different results as wall-clock time passes, so a frozen-metric regression check would drift
   without any code change.
3. **The token ledger is not keyed by `ModelRoleId`.** `TokenUsageSnapshot::record/declare`
   (`crates/qsf_realtime_server/src/realtime/token_usage.rs:59-95`) key on
   `(role: &str, model_id: &str)` with role string constants. `ModelRoleId`
   (`crates/qsf_models/src/model_role.rs:9`) is a closed enum for *chat-shaped* model role
   selection and is unrelated to the ledger key. A new role constant is needed, not an enum
   widening. This corrects the design brief's B6 phrasing.
4. **`retrieval_source_ids()` reads the context assembly, not the memory records.**
   `SleepRecord::retrieval_source_ids` (`crates/qsf_session/src/sleep_records.rs:100-109`) returns
   `ContextAssembly::retrieved_memory_ids()`, which filters `selected` fragments by
   `ContextSourceKind::Memory` (`crates/qsf_context/src/lib.rs:76-83`).
5. **Reinforcement does *not* read the context assembly.**
   `apply_live_memory_reinforcement` (`crates/qsf_app/src/session/live_memory.rs:16-48`) consumes
   `RetrievalResult.selected` directly and also reports relevance-gate and over-limit skipped ids
   from `RetrievalResult.omitted`. Any eligibility rule must therefore live on `RetrievedMemory`
   itself, not only on `ContextFragment`.
6. **Context assembly re-sorts and can undo retrieval's ordering.** `assemble_context`
   (`crates/qsf_context/src/lib.rs:85-135`) sorts by `(source_priority desc, score desc,
   estimated_tokens asc, fragment_id asc)` and then fills the fragment/token budget. Any ordering
   decision made inside retrieval survives only if it is expressed in the fragment score that
   assembly sorts on, or if assembly is given an explicit ordering input.
7. **The realtime path records no recalled items.** `inject_trusted_turn_context_and_response`
   applies `LiveSessionEvent::MemoryContextRecorded { .., recalled_items: vec![], .. }`
   (`sideband_turn_injection.rs:133-142`), so the durable per-turn selection record *is* the
   context assembly.
8. **`result.omitted` is discarded on the live path.** `sideband_turn_injection.rs:76-90` keeps
   only `result.selected` and drops the omission list and every skip reason.
9. **The sideband receive loop awaits everything inline.** `connect_and_run_once`
   (`crates/qsf_realtime_server/src/realtime/sideband_connection.rs:150-222`) is one `select!` over
   `stop_rx`, `command_rx` and `stream.next()`, and it `await`s `handle_provider_event(...)` — which
   calls `inject_trusted_turn_context_and_response` — inline. `SidebandRuntimeState`
   (`realtime/sideband.rs:68-94`) is a `&mut` local to that loop. **Consequence:** awaiting a
   network judge call inside injection stalls stop handling, typed-turn commands, interruptions and
   every subsequent provider event, and no other task can write the runtime state. The judge must
   be an isolated effect whose results re-enter through the loop's `SidebandCommand` channel
   (`sideband.rs:63-65`, today a single-variant enum).
10. **The goal side has two gates, and `match_strength` is a closed quantity.**
    `select_goals_ranked` (`crates/qsf_volition/src/selection.rs:197-205`) drops any goal with no
    matched keyword; then `arbitrate_with_mode` (`crates/qsf_volition/src/arbitration.rs:285-288`)
    **independently** partitions on `selection.match_strength >= threshold`. Admitting a goal only
    at selection therefore leaves it below the arbitration threshold and unable to win.
    `match_strength` is a `u32` feeding both `compute_relevance`
    (`RELEVANCE_PER_STRENGTH_POINT = 25.0`) and that partition, so a probability cannot be folded
    into it without redefining the 2026-07-04 decision.
11. **A winning initiative mutates persisted volition state.**
    `sideband_turn_injection.rs:316-324` builds `VolitionEvent::InitiativeExecuted` and calls
    `guard.volition.apply_events(...)`; promotion is the server's writer of `volition-state.json`.
    Marking a trace does not prevent this — the reducer must be told.
12. **The store is re-read and re-parsed synchronously inside an async fn every turn.**
    `retrieve_session_memories` (`crates/qsf_realtime_server/src/realtime/memory_store.rs:12-27`)
    calls `MemoryStore::load_or_empty` then `retrieve_memories` with no `spawn_blocking`.
13. **Injection budget confirmed.** `DEFAULT_INJECTION_FRAGMENT_LIMIT = 4`,
    `DEFAULT_INJECTION_TOKEN_LIMIT = 600` (`realtime/sideband.rs:18-19`), used both as the
    retrieval `limit` and as the `ContextBudget` — one number doing two jobs.
14. **Session-state schema tolerates additive fields.** `SESSION_STATE_SCHEMA_VERSION = 2` and the
    loader rejects only *newer* versions (`crates/qsf_session/src/persistence.rs:55-60`).
15. **The realtime launcher has no per-invocation override path.**
    `Get-RealtimeEnvironmentDelta` (`scripts/qsf.ps1:373-393`) takes no parameters, reads
    script-scope values (`$WorldCorpusPath`), pins `QSF_MODEL_PROVIDER=openai`, and clears every
    other managed non-secret `QSF_*`. `Invoke-Realtime` calls `Test-RequiredSecret -Name
    "OPENAI_API_KEY"` unconditionally (`qsf.ps1:1238`). Adding managed variables alone would make
    them unsettable from the launcher. `Test-SecretLikeName` (`qsf.ps1:144-151`) already matches
    `KEY`, so `TYPESAFE_API_KEY` is auto-excluded from clearing and redacted in printed deltas.
16. **The evaluation track is further along than its plan header says.**
    `docs/Plans/Plan.SemanticEvaluationFoundation.md` reads "Proposed — not started", but
    `crates/qsf_semantic_eval`, `crates/qsf_semantic_datagen` and the `evaluation/` tree all exist.
    `run_baseline` and `PairResult.goal_ref` (`crates/qsf_semantic_eval/src/runner.rs:19-58`) are
    hard-wired to the volition scorer and to goal naming.

---

## Settled inputs (decided; not re-opened here)

**From the design brief.** Swappable judge seam with the model identity in every trace; an offline
scoring set on invented data able to score any backend including today's lexical retrieval; the
hosted System One model as the primary live judge called over HTTP from the Rust server with its
usage in the token ledger; same-turn verdicts under a deadline, with a late verdict used on the
next turn rather than discarded; the bridging line as its own later phase; lexical retrieval as
floor and fallback with every fallback recorded; both memories and goals judged as two separate
question sets with two separate scores; a local challenger later through a helper process with
in-process inference as the written end state; verification through the headless scripted
conversation probe; the sleep phase otherwise unchanged. Plus the blindspot outcomes: a
pre-registered stop/go gate that halts rather than scraps; judge-influenced selections marked and
excluded from durable structure; one shared scoring set for both task families built here; memories
err generous and goals stay conservative; the judge runs after the final transcript first, with
early-start judging as its own following phase.

**U1 — the gate is an operator judgment against pre-registered reference numbers.** The reference
bar is accepted as a start: ≥ 15 percentage points more of the truly relevant memories than lexical
retrieval; the advantage does not shrink as the store grows (~20 → 500+ records); admitted
candidates ≤ ~3× the injection allowance; measured latency fits the added-silence limit. The gate
is **not** an automatic pass/fail: the gate report presents three factors side by side — how long it
takes, what it actually costs (measured cost per turn and per conversation from real usage), and how
good the results are — and the operator makes the go/halt call with the pre-registered numbers as
the reference line. Tuning (question wording, thresholds, request shaping, combination policy) is
expected and happens on the **development split only**; the sealed split is opened after tuning and
remains the one logged honest check. The operator may revise the reference numbers at any time
**before** the first sealed-split look, never after; each revision is recorded with its reason. A
halt still halts-not-scraps.

**U2 — the acceptable added time to first audio is 300 ms, fixed now.** It is set independently of
any judge measurement, is expected to be revised, and every revision is recorded with its reason.
The injection deadline may be derived from measurement but must never exceed what the 300 ms budget
allows. An optional operator listening check using the fixture backend with synthetic delays
(0 / 150 / 300 / 500 ms) is the tool for revising the number. When goals are added, the *combined*
memory-and-goal workload is verified against the same limit and the same shared rate budget.

## Design commitments this plan adds

These become project commitments only when the corresponding Decision Log entries land.

**C1. One canonical pair-scoring contract, two traits, one record schema.** The judge seam **is**
the pair-scoring surface of `docs/Plans/Plan.SharedSemanticInfrastructure.md`, reconciled here
rather than merely cross-referenced:

```text
PairScoreRequest { task, utterance_text, utterance_content_hash,
                   candidates: [{ candidate_id, candidate_text, candidate_content_hash,
                                  candidate_kind }],
                   options: { question_wording_version, shaping, score_kind_expected } }

PairScore { candidate_id, candidate_content_hash, score_basis_points,
            score_kind: Probability | Similarity, abstained, abstain_reason }

PairScorer          (sync)   score_pairs(PairScoreRequest) -> Traced<Vec<PairScore>>
PairScoringService  (async)  score_pairs(PairScoreRequest, Deadline)
                                 -> Traced<Vec<PairScore>>
BlockingPairScorerService — the adapter that presents any sync `PairScorer` as the async service
                            via `spawn_blocking`; the in-process end state needs no new abstraction.
```

`RelevanceJudge` is **not** a third trait: it is `PairScoringService` used with a relevance task and
the admission types owned by the domain adapter. One `SemanticTraceRecord` schema serves every
backend, with a `backend_kind` discriminator and optional, per-backend-declared payload fields
(`embedding_score` / `candidate_embedding_source` only for local encoder backends; `usage` /
`attempt_count` / `retry_reasons` only for service backends). One `SemanticFailure` enum carries
both local variants (`AssetsMissing | HashMismatch | LoadFailed | TokenizationFailed |
InferenceFailed | IncompatibleHead`) and service variants (`Unauthorized | InvalidRequest |
RateLimited | Overloaded | Timeout | Transport | Decode | BackendUnavailable | Cancelled`),
documented by which backends can produce which. `Plan.SharedSemanticInfrastructure.md` is updated to
consume this contract, not merely to reference it.

**C2. Caller-owned fields wrap the inference record; they are never inputs to it.** Session id,
exchange index, deadline outcome, admission, selection, eligibility, fallback and carry-over are
fields of the **caller's** `relevance_judgment` records, which reference the inference record by
`invocation_id`. The inference record never requires a live-session field, so offline scoring runs
produce the same record type with no synthetic session data.

**C3. The seam speaks candidates, never memories or goals.** The judge API knows nothing about
`MemoryRecord` or `Goal`. Domain crates own the adapters that render a record or a goal into one
canonical `candidate_text`, pinned by `candidate_content_hash`.

**C4. Retrieval becomes pure by taking its evaluation time explicitly.** `retrieve_memories` stops
reading the clock (Verified item 2). Its arguments become a `RetrievalRequest` struct — records,
associations, query, strategy, limit, `evaluation_time`, and optional `judge_verdicts` — so future
signals cause no further signature churn across `qsf_app`, `qsf_realtime_server::realtime::tools`
and the text loop. Live callers pass the clock value; the evaluation runner passes the time frozen
with the dataset. This is what makes the frozen baseline and the regression gate reproducible.

**C5. Verdicts are a fourth admission path, and ordering is affected too.** A verdict at or above
the admission threshold admits a candidate in `is_relevant_for_strategy`, alongside keyword, tag /
association, and the profile-identity allowance — preserving the 2026-05-25 rule and adding an
auditable signal with its own skip reason (`judge verdict below admission threshold`). The
superseded-world-observation omission keeps running first. Admission alone is insufficient: a
lexically disjoint memory scores near zero, so it would be admitted and then lost at the limit cut.
Two combination policies are therefore implemented behind one named policy enum and chosen on the
development split: **(a) a bounded additive judge term** in the retrieval total (`RetrievalScore`
gains a `judge` component so the total stays one explainable sum); only verdicts at or above the
admission threshold earn the bonus, while below-threshold verdicts, abstentions, and missing verdicts
contribute zero. **(b) reserved slots**: lexical order fills `limit − reserved` slots, then the
highest-verdict judge-admitted candidates not already selected take the reserved slots; unused
reserved slots return to lexical order. Under this policy the score total stays lexical and the
verdict basis points remain visible separately. Because `assemble_context` re-sorts by fragment score (Verified item 6), policy (a)
survives assembly naturally through the existing score mapping, while policy (b) requires an
explicit ordering input to assembly; both are verified **through the whole path**
retrieval → assembly → injected set, never at retrieval alone.
`qsf_semantics` keeps the remote HTTP implementation and its reqwest, Tokio, and futures
dependencies behind a default-on `remote-http` feature; `qsf_memory` opts out and consumes only
the lean score and trace contracts. The hosted judge's `noul` probability is both judgment and
confidence in one number; vendor wire types retain `deny_unknown_fields`.

**C6. Eligibility for durable structure is decided by a lexical-only counterfactual, not by
admission basis.** A selected memory is ineligible for association building and reinforcement only
when the lexical-only selection would not have picked it. A `lexical_and_judge` match remains
associable when the lexical-only selection picked it; a weak lexical match promoted across the limit
by judge ordering or a reserved slot is ineligible. Admission basis cannot answer "would this have been
selected anyway", so pure retrieval computes the **lexical-only selection** (same records, same
evaluation time, same limit, no verdicts) alongside the judged one, and a candidate is
`associable` only if it appears in that lexical-only selection. `RetrievedMemory` carries
`selection_eligibility` and `RetrievalResult` carries `lexical_only_selected_ids`, so
`apply_live_memory_reinforcement` — which consumes `RetrievalResult.selected` (Verified item 5) —
can filter without consulting the context assembly. `ContextFragment` carries both
`admission_basis` (`lexical | judge | lexical_and_judge`, for explanation) and `associable` (for the
rule); `ContextAssembly::associable_retrieval_source_ids()` returns only the latter, and sleep's
co-retrieval proposer uses it. The counterfactual is pure and cheap: no extra model call. Switching
this off is a deliberate, separately gated later step, named here and not built.

**C7. The judge is an isolated effect; its results re-enter as actions.** The sideband receive loop
must never await a network judge call (Verified item 9). Injection splits into a planning step and a
completion step with a pending-injection slot on `SidebandRuntimeState`; the judge runs as a spawned
effect and its outcome returns through the loop's command channel. The lifecycle, its identities and
its two distinct timers are specified under "Judge lifecycle" below. State transitions live in a
pure reducer module with unit tests; the effect layer only performs I/O.

**C8. Error direction is per subsystem.** Memory admission errs generous (recall-oriented: "keep out
the clearly unrelated"), so the injection allowance — not the judge — becomes the binding
constraint, and raising it is an explicit, measured step. Goal admission stays conservative
(DecisionLog 2026-07-26). The two families carry different operating points over the same scoring
set, which is why they keep separate scores (DecisionLog 2026-07-30).

**C9. Provider selection is explicit; the default backend is a *fixture* backend that costs
nothing.** `QSF_RELEVANCE_JUDGE_BACKEND` selects the backend by name and never by the presence of a
key (DecisionLog 2026-05-11). Its in-code default is `fixture`: a deterministic implementation whose
probabilities are derived from content hashes or supplied by a verdict table, with configurable
synthetic latency and failures. It exercises the entire new path end to end — candidate assembly,
lifecycle, admission, eligibility, ledger, traces — **and its verdicts are explicitly not relevance
evidence**. It is named `fixture` everywhere, logs a prominent startup line when a live session uses
it, and stamps `backend_kind: fixture` into every record so no artifact can be mistaken for
evidence. The paid remote backend is pinned explicitly by the launcher profiles that run live
sessions, exactly as `realtime` already pins `QSF_MODEL_PROVIDER=openai` (DecisionLog 2026-07-03).
This is the resolution of "defaults must exercise the new path" against "never select a provider by
the presence of a key" for a paid per-turn call, following the 2026-07-02/03
mock-that-still-runs-end-to-end precedent.

**C10. Two budgets, fixed in opposite ways.** `MAX_ADDED_TIME_TO_FIRST_AUDIO_MS = 300` (U2) is fixed
before any measurement and is the falsifier. The **injection deadline** is derived from measurement
but is constrained by it: injection deadline + candidate assembly + context assembly + send must fit
inside 300 ms added at p95. The injection deadline is distinct from the **request timeout** (C12).
Both live in `qsf_semantics` as single sources of truth, following the
`WORLD_CONSULT_INLINE_BUDGET_MS` relocation precedent, and are overridable through configuration.
The hosted retry count, initial and maximum backoff, and maximum concurrency defaults also live in
`budgets.rs` and have explicit `QSF_RELEVANCE_JUDGE_*` overrides; malformed overrides are typed
configuration errors rather than silent fallback.

**C11. The pinned model version and the question wording are part of the operating point.**
`jev-latest` is a moving alias and the vendor documents that probability thresholds do not transfer
between question wordings. Configuration pins an explicit version id; every record carries
`model_identity` and `question_wording_version`; changing either invalidates the admission threshold
and requires a re-run on the development split.

**C12. A missed injection deadline must not cancel the request.** The injection deadline decides
when the turn stops waiting; the request timeout decides when the HTTP call is abandoned. The
request timeout is strictly greater, so a verdict that arrives late still completes and becomes the
carry-over candidate. Cancelling the future at the injection deadline would destroy the very verdict
the carry-over decision promises, and is explicitly forbidden.

**C13. Probabilities are integers everywhere.** Verdicts are carried as integer basis points
(0–10000) in every artifact and every record: the evaluation artifacts forbid floating point so
hashes stay stable and re-derivation is exact, and one representation avoids two rounding stories.

---

## Judge lifecycle (the async design C7 requires)

**Identity.** Every judge call has an `invocation_id` and an invocation identity of
`(qsf_session_id, attachment_epoch, exchange_index, input_revision, invocation_seq)`. `input_revision`
increments when the input text for the same exchange changes (a partial transcript refreshed, a
final transcript replacing partials). Any completion whose identity does not match the live state is
**stale**: it is never applied and is recorded with `completion_disposition: stale_rejected`.

**Actions** (new `SidebandCommand` variants, delivered into the existing `select!` loop so the loop
keeps servicing stop, typed turns, interruptions and provider events while a judgment is in flight):

```text
RelevanceJudgmentStarted    { invocation_id, identity, candidate_count, deadline_ms }
RelevanceVerdictReady       { invocation_id, identity, verdicts, latency, usage }
RelevanceJudgmentFailed     { invocation_id, identity, failure_reason, latency }
InjectionDeadlineElapsed    { invocation_id, identity }        // a spawned timer, not a blocking sleep
RelevanceJudgmentCancelled  { invocation_id, identity, reason }
```

**Pure reducer.** `relevance_judgment::apply(state, action) -> (state, effects)` decides: complete
the pending injection now, keep waiting, park a late verdict for carry-over, reject a stale result,
or drop. `PendingInjection` lives on `SidebandRuntimeState` with the prepared packet, the identity,
and the timer handle. No I/O in the reducer; unit-tested without a network or a session.

**Completion paths for one turn.** Verdict before the injection deadline → judged injection. Deadline
first → lexical injection, `fallback_executed` recorded, the request continues (C12) and its later
completion is parked as a carry-over candidate. Failure → lexical injection with the typed reason.
Cancellation (interruption, session stop, attachment termination, superseding input revision) →
record and drop.

**Precedence.** A current-turn verdict always beats a carried verdict; a carried verdict is used only
when no current verdict arrived in time. Both are recorded, and a used carried verdict names the
exchange it came from.

**Shutdown drain.** The stop path awaits in-flight judge effects with a bounded drain before the
sideband task exits, so no diagnostic record can be appended after a probe finalizer has parsed the
ledger. This composes with the probe branch's live-goal-formation drain barrier: the finalizer waits
on both barriers before it stops and joins the sideband.

---

## Repository placement

- **`crates/qsf_semantics`** (new, lean) — `pair_scoring` (the C1 contract and both traits),
  `trace` (`Traced<T>`, `SemanticTraceRecord`, `SemanticFailure`, and the three lifecycle record
  types), `backends/fixture.rs`, `backends/remote_http.rs`, `config.rs`, `budgets.rs` (C10),
  `bench.rs`; a binary with
  `score` (one-shot), `bench`, and read-only `verify` subcommands. Depends only on
  `engine_logging` plus third-party crates; a dependency-boundary test forbids `qsf_app`,
  `qsf_realtime_server`, `qsf_volition`, `qsf_memory`, `qsf_semantic_eval`.
- **`crates/qsf_memory`** — `RetrievalRequest` with `evaluation_time`, the judge admission path,
  the `judge` score component, both combination policies, the lexical-only counterfactual and
  `selection_eligibility`.
- **`crates/qsf_context`** — `ContextFragment.admission_basis` and `.associable`,
  `ContextAssembly::associable_retrieval_source_ids()`, and the explicit ordering input the
  reserved-slot policy needs.
- **`crates/qsf_diagnostics`** — persistence, writer integration, and diagnostic-ledger wiring for
  the lifecycle record types defined by `qsf_semantics` (2026-07-27, narrowed 2026-09-22).
- **`crates/qsf_realtime_server`** — `realtime/relevance_judgment.rs` (pure reducer),
  candidate assembly, the effect, the pending-injection slot, the carry-over slot, the ledger role.
- **`crates/qsf_volition`** — judge qualification at selection **and** at the arbitration partition,
  plus the narrow durable-state path.
- **`crates/qsf_semantic_eval`** — generalized `candidate_ref` runner and a scorer abstraction over
  production code.
- **`crates/qsf_semantic_datagen`** — generation, labeling, review and freeze for the new set.
- **`evaluation/contracts/SemanticRelevance.TaskContract.md`** (new, versioned) with two
  task-family annexes; **`evaluation/frozen/semantic-relevance/`**;
  **`evaluation/reports/`**; per-run artifacts under `runs/<run-id>/`.

---

## Trace completeness contract — the `relevance_judgment` chain

Required because the whole claim of this work is "every judgment, deadline miss and fallback is
visible" (`Agents.md`; `ProjectWorkflow.md`). Durable documents refer to this contract by name. It
covers: input → candidates judged → verdicts → selected and omitted with skip reasons → deadline
outcome → injected context → carry-over consumption.

A single record per turn cannot represent the planned lifecycle: early judgments, a final refresh
and a late completion all happen around one turn, and a record written at the injection deadline
cannot yet know a late result's fate. The contract is therefore an **append-only, correlated
three-record lifecycle**.

```text
relevance_judgment_requested
  schema_version, task (memory_relevance | goal_relevance), backend_kind,
  invocation_id, identity { qsf_session_id, attachment_epoch, exchange_index, input_revision,
                            invocation_seq },
  request_hash, utterance_content_hash, input_kind (partial_transcript | final_transcript),
  transcript_fraction, context_spec (version + what it included), retrieval_query_hash,
  volition_hint_consumed,
  candidate_count, candidate_selection (all_records | prefiltered{name,params}),
  store_record_count, per candidate { candidate_id, candidate_content_hash, candidate_kind },
  shaping, question_wording_version,
  model_identity { backend, model_id, identity_kind (pinned_remote_version |
                   local_artifact_digest | fixture), identity_value },
  injection_deadline_ms, request_timeout_ms

relevance_judgment_completed
  invocation_id, identity,
  per candidate { probability_basis_points, judged, unjudged_reason },
  latency_micros, attempt_count, retry_reasons, usage_raw, usage_parsed,
  failure_reason (typed; present exactly when the call failed),
  completion_disposition (used_same_turn | late_carried_over | late_dropped | stale_rejected |
                          cancelled | superseded_by_revision)

relevance_selection_recorded          (exactly one per injected turn)
  exchange_index, request_hash,
  consulted_invocations [{ invocation_id, role: current | carried,
                           carried_from_exchange_index, carry_over_age_turns }],
  deadline_outcome (in_time | late_carried_over | late_dropped | failed | not_attempted),
  fallback_executed, fallback_reason,
  per candidate { admitted, admission_basis, associable, skip_reason },
  lexical_only_selected_ids,
  judge_admission_threshold_in_force (numeric basis points), combination_policy_in_force,
  injection_budget_in_force { fragments, tokens } (both numeric), injected_fragment_ids,
  used_estimated_tokens, omitted_by_budget,
  evaluation_time,
  goal_family_extra { qualification_threshold_in_force (numeric),
                      judge_qualified_goal_ids, arbitration_winner_changed_by_judge,
                      durable_mutation_scope }
```

**Artifact boundary.**

```text
state/<root>/diagnostics/<session>.jsonl
  Chronological facts plus the three lifecycle records above, plus memory_selection_recorded for
  turns with no judge at all, plus latency observations:
  final_transcript_received_to_relevance_verdict, relevance_verdict_to_memory_injected, and the
  existing final_transcript_received_to_first_audio (the U2 falsifier metric).

state/<root>/continuity/<session>/session-state.json
  Durable promoted selection: exchange context_assembly carrying admission_basis and associable per
  fragment. This is the surface sleep reads and the one C6 protects.

runs/<run-id>/relevance-judgment.jsonl
  Offline per-pair result records: the same verdict, threshold, admission and eligibility fields
  plus gold label, dataset version, split, store size, frozen evaluation_time, sampling weight, and
  scorer source (crate + fn path). Authoritative for every number in the gate report.

runs/<run-id>/metrics.json
  Derived structured metrics, including measured cost per turn and per conversation. The regression
  gate compares structured fields, never rendered prose.

evaluation/reports/ + evaluation/frozen/semantic-relevance/sealed-look-ledger.jsonl
  Frozen human-readable reports derived from the structured artifacts, and the append-only record of
  every sealed-split consultation (date, model identity, wording version, shaping, combination
  policy, code commit, metrics hash) and of every pre-look revision of the reference numbers with
  its reason.

state/probe/<run-id>/run-manifest.json
  Probe provenance: judge model identity in model_ids, per-turn look-up counts and look-up latency,
  deadline outcomes, drain outcome, and the secret-scan result covering the judge key.
```

**Artifact-parsing verification** — tests parse *generated* artifacts and cover the whole lifecycle,
not field presence on a single happy path:

- one judged turn: every required field of all three records, with numeric thresholds and numeric
  budgets;
- a deliberately failed call: `failure_reason` on a fully formed `completed` record, and
  `fallback_executed` with its reason on the selection record;
- **multiple judgments in one turn** (early start plus final refresh): each `requested` has exactly
  one `completed`, and the selection record names which invocation it used;
- **out-of-order completion**: an earlier invocation completing after a later one is recorded with
  `superseded_by_revision` and does not change the selection;
- **stale completion**: a verdict for a previous exchange or a previous attachment epoch is recorded
  `stale_rejected` and changes nothing;
- **cancellation**: interruption and session stop each produce a `cancelled` completion;
- **actual persisted provenance**: the test reads `session-state.json` and asserts the
  `admission_basis` and `associable` values that were really written, and that an ineligible
  selection is absent from `associable_retrieval_source_ids()`.

No trace criterion is marked complete until generated artifacts satisfy it.

---

## The operator-judged stop/go gate

The gate sits between the evidence phases and every live phase. It is an **operator judgment
against pre-registered reference numbers** (U1), not an automatic pass/fail.

**The gate report presents three factors side by side**, each derived from the structured artifacts:

- **(a) How long it takes** — measured judge latency for the chosen shaping at each store size, and
  the projected added time to first audio against the fixed 300 ms limit.
- **(b) What it actually costs** — measured cost per turn and per conversation from *real* usage
  (tokens observed, not estimated from a price sheet alone), at each store size and shaping, with
  the turns-per-conversation assumption stated.
- **(c) How good the results are** — recall, precision, admitted-candidates-per-utterance and the
  miss classification, per task family and per store size.

**Pre-registered reference numbers** (U1; committed to git before the first sealed-split look, each
later revision recorded with its reason and its date, and no revision permitted after that look):

- **R1 — recall advantage.** `judge_recall − lexical_recall ≥ 15` percentage points on the
  fully-labeled evaluation subset, at each method's chosen operating point.
- **R2 — the advantage does not shrink with store size.** The margin at the largest invented store
  (≥ 500 records) is at least the margin at the smallest (≈ 20 records, today's live store).
- **R3 — the generous cut-off is still a filter.** Mean admitted candidates per utterance ≤ ~3× the
  injection allowance in force.
- **R4 — ranking is the bottleneck at all.** Over the **utterance-level** population (below), the
  share of utterances that *had* a relevant memory in the store but did not get it injected is at
  least 60% of the utterances that had any relevant memory; the complementary share of utterances
  with nothing relevant is reported alongside. If most lexical failures are "nothing relevant
  existed", better ranking cannot help.
- **R5 — latency fits the independent limit.** Measured p95 judge latency for the chosen shaping at
  the largest store size, plus the fixed serial overhead, fits inside
  `MAX_ADDED_TIME_TO_FIRST_AUDIO_MS = 300` (U2), and requests per turn fit the provider's rate
  limits. This is tested against the independently fixed limit, never against a budget derived from
  the same measurement.
- **R6 — cost is acceptable.** Measured cost per turn and per conversation, presented for the
  operator's judgment; no numeric bar is pre-registered because no prior number exists, so this
  factor is explicitly judged rather than thresholded.

**Population and classification (making R4 testable).** The pair pool is sparse and stratified, so
it cannot by itself establish whole-store recall, precision, or the absence of a relevant memory.
The scoring set therefore contains **two parts**: a broad sparse stratified pool carrying **sampling
weights**, used for development and tuning; and a **fully labeled evaluation subset** — fewer
utterances, but every (utterance × every record in its store) pair labeled — over which whole-store
claims are valid. Utterances with *no* relevant memory are deliberately included in that subset.
Every utterance receives exactly one outcome class per method, from an exhaustive set:

```text
no_relevant_memory_exists
relevant_present_but_gate_rejected
relevant_present_but_outranked_within_limit
relevant_present_but_cut_by_limit_or_budget
relevant_present_and_injected
```

A test asserts exhaustiveness and mutual exclusivity for every utterance and every method. Gate
numbers come from the fully labeled subset; any figure quoted from the sparse pool is weighted and
labeled as such.

**Tuning discipline.** Question wording, admission thresholds, request shaping and the combination
policy are tuned on the development split only. The sealed split is opened once per candidate model,
after tuning, and every look appends to the look ledger. A gate evaluated on the development split
is not a gate.

**If the operator halts, the project halts — it is not scrapped.** What stays: the judge seam and
both backends, the pure admission and combination policies, the selection recording and eligibility
rule, the trace contract, the latency/cost report, the generalized evaluation runner, and the frozen
scoring set with its contracts, splits and look ledger. What is not built: every phase after the
gate. A Decision Log entry records the halt and the evidence, and `docs/Handoff.md` is updated. The
work **re-opens** when a new or improved judge passes the same reference bar on the same sealed
split, consuming one logged look.

---

## Experiments

Each phase that probes a consciousness-simulation mechanism is validated by an `Experiment.*.md`
scaffold (`ProjectWorkflow.md`); plumbing, measurement harnesses and backend swaps get code, tests
and a commit instead (DecisionLog 2026-07-04). Named here, written when their phase starts:

- `docs/Experiments/Experiment.MemoryRelevanceJudgmentVersusLexicalFloor.md`
- `docs/Experiments/Experiment.ShadowMemoryRelevanceJudgment.md`
- `docs/Experiments/Experiment.SameTurnJudgedMemoryInjection.md`
- `docs/Experiments/Experiment.JudgedMemoryInjectionAllowance.md`
- `docs/Experiments/Experiment.LateRelevanceVerdictCarryOver.md`
- `docs/Experiments/Experiment.EarlyStartRelevanceJudgment.md`
- `docs/Experiments/Experiment.ShadowGoalRelevanceJudgment.md`
- `docs/Experiments/Experiment.JudgedGoalActivation.md`
- `docs/Experiments/Experiment.BridgingLineUnderLateVerdict.md`

All are added to `docs/Experiments/Experiment.Backlog.md` when named.

---

# Part A — Foundations (gate-independent; all offline, no paid calls except Phase 4)

## Phase 1 — The pair-scoring contract, the lifecycle records, and both backends

**Status: Landed (2026-09-22).** `crates/qsf_semantics` now provides the isolated pair-scoring
contract, fixture and hosted HTTP backends, lifecycle records, timing budgets, generated-artifact
coverage, and the dependency boundary. It intentionally has no diagnostics or live-path wiring.

**Work**

- Create `crates/qsf_semantics` per C1: `PairScoreRequest` / `PairScore`, the sync `PairScorer` and
  async `PairScoringService` traits, `BlockingPairScorerService`, `Traced<T>`,
  `SemanticTraceRecord` with `backend_kind` and per-backend optional payloads, the single
  `SemanticFailure` enum, `budgets.rs` with `MAX_ADDED_TIME_TO_FIRST_AUDIO_MS = 300` (U2) and the
  injection-deadline and request-timeout constants (C10, C12).
- The three lifecycle record types (`requested`, `completed`, `selection_recorded`) with
  `invocation_id` and identity, defined here so both the offline runner and the live path emit the
  same shapes; caller-owned fields stay out of the inference record (C2).
- `FixtureRelevanceJudge` per C9: deterministic verdicts from content hashes or a verdict table,
  configurable synthetic latency and failures, `backend_kind: fixture` stamped into every record,
  and a prominent startup log line whenever a live session selects it.
- `RemoteRelevanceJudge`: configurable base URL, bearer auth, pinned model id, typed error mapping,
  bounded backoff for rate-limited and overloaded responses, bounded request concurrency against a
  configured rate budget, **separate** injection-deadline and request-timeout handling, raw `usage`
  preserved verbatim.
- `config.rs`: explicit backend selection by name; loud typed error when a selected backend's
  prerequisites are missing.
- Update `docs/Plans/Plan.SharedSemanticInfrastructure.md` to consume this contract (its
  `PairScorer`, `Traced<T>`, record and failure definitions become references to this crate).

**Verification (automated)** — from `C:\Users\larsp\src\qualia-signal-foundry`:
`cargo build`; `cargo test -p qsf_semantics` covering fixture determinism, the adapter presenting a
sync scorer as the async service, typed error mapping against a local HTTP stub (401/422/429/529),
backoff, injection-deadline expiry **without** cancelling the underlying request (C12), request
timeout producing `Timeout` inside a fully formed record, and `deny_unknown_fields` round-trips of
all three lifecycle records; the dependency-boundary test; the artifact-parsing test over generated
`semantic-trace.jsonl` including a failed call; `cargo clippy --all-targets -- -D warnings`;
`cargo fmt`.

**Experiment scaffold**: none. **Human testing**: none. **Cost**: none.

---

## Phase 2 — Deterministic evaluation time, judge admission, and both combination policies

Pure code only — no live wiring and no judge calls from the server. This is the production
functionality the gate's comparison needs (review issue 3), so it lands **before** the gate.

**Work**

- **Landed in `1a1c645`:** `RetrievalRequest` per C4, including `evaluation_time`;
  `retrieve_memories` no longer reads the clock; every caller (`qsf_app` text loop,
  `realtime/tools.rs`, `realtime/memory_store.rs`) passes a clock value.
- **Landed:** memory-keyed judge verdicts and their qsf_semantics identity adapter; the
  `is_relevant_for_strategy` admission path (C5) with its distinct below-threshold skip reason,
  after superseded-world-observation omission.
- **Landed:** `RetrievalScore.judge`; bounded-additive and reserved-slot combination policies
  behind one request enum; bounded-additive is the documented provisional default and the policy
  in force is recorded.
- **Landed:** the lexical-only counterfactual and `RetrievedMemory.selection_eligibility` /
  `RetrievalResult.lexical_only_selected_ids` (C6).
- **Landed:** `ContextFragment.admission_basis` and `.associable` (serde-defaulted to `lexical` /
  `true`), plus the explicit ordering input used by reserved-slot assembly (Verified item 6).
- **Not landed in this slice:** generalize `qsf_semantic_eval`: `PairResult.goal_ref` →
  `candidate_ref` + `task_family`; a scorer
  abstraction admitting production lexical memory retrieval through its public API, the production
  goal scorer, and any `PairScoringService` backend. Existing goal-relevance artifacts either keep
  working through a compatibility path or are deliberately re-versioned; the choice is recorded.

**Verification (automated)**: the landed retrieval/context slice passes `cargo build` and
`cargo test -p qsf_memory -p qsf_context -p qsf_semantics -p qsf_session -p qsf_app
-p qsf_realtime_server`; determinism, admission, compatibility, eligibility, and whole-path ordering
are covered through public retrieval and assembly contracts. The `qsf_semantic_eval` fidelity test
remains with its later scorer-generalization work; clippy; fmt.

**Experiment scaffold**: none (pure production code, outcome not in doubt).
**Human testing**: none. **Cost**: none.

---

## Phase 3 — Selection recording, provenance, and the durable-structure exclusion

Gate-independent and independently valuable: today the live path discards every omission and every
skip reason (Verified item 8). It also installs the C6 protection *before* any judge-influenced
selection can exist.

**Work**

- Load the session memory store once per turn, off the async executor (`spawn_blocking`), and pass
  the loaded contents to retrieval and later to candidate assembly, so the store is never parsed
  twice per turn nor parsed on the async thread (Verified item 12). Behavior-preserving.
- Keep `RetrievalResult.omitted` on the live path and emit `memory_selection_recorded` per turn:
  selected and omitted candidates with scores, matched terms, association paths, skip reasons,
  strategy, numeric limit in force, `evaluation_time`, and retrieval latency. (Once the judge is
  live this record is subsumed by `relevance_selection_recorded`; both share field names.)
- Count both lexical relevance-gate skips and judge-verdict-below-threshold skips in the
  relevance-skipped selection record; the current live counter sees only the lexical reason.
- **Single-time-per-turn follow-up:** when adding `evaluation_time` to the selection record, capture
  one turn-owned wall-clock value and thread it through all per-turn work that currently reads the
  clock separately.
- Persist `admission_basis` and `associable` through `ContextFragment` into `session-state.json`.
- `ContextAssembly::associable_retrieval_source_ids()`; sleep's co-retrieval proposer switches to
  it; `apply_live_memory_reinforcement` filters on `RetrievedMemory.selection_eligibility`
  (Verified item 5). `retrieved_memory_ids()` keeps its display meaning.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_memory -p qsf_context -p qsf_session
-p qsf_app -p qsf_realtime_server`; a test that the per-turn store load happens once and off the
async executor; a parse test over a *generated* diagnostics ledger asserting the selection-record
fields; exclusion tests proving a judge-influenced selection becomes neither an association nor a
reinforcement, exercised through both consumers (co-retrieval via the context assembly, reinforcement
via `RetrievalResult.selected`); a session-state round-trip test proving an old file without the new
fields loads and defaults to `lexical` / `true`; clippy; fmt. If the diagnostics UI renders the new
record, `npm run check` then `npm run fmt` from `crates/qsf_realtime_server/ui`.

**Experiment scaffold**: none. **Human testing (recommended)**: one live or probe session, then read
the ledger and confirm every omitted memory has a skip reason and the selection record is legible.

---

## Phase 4 — Latency, request shaping and cost measured against the fixed 300 ms limit

The first paid step and the cheapest. It measures how long judging takes and what it costs; the
*acceptable* added silence is already fixed at 300 ms (U2) and is not derived from these numbers.

**Work**

- `bench` measures, against the real endpoint, per shaping variant and at candidate counts spanning
  today's live store (≈ 18) to the largest store size in scope (≥ 500): end-to-end p50/p95/p99,
  per-attempt latency, retry incidence, tokens per request, **measured cost per turn and per
  conversation**, and requests per turn against the documented rate limits.
- Record the measurement environment: pinned model id, endpoint, wording version, machine and
  network profile, code commit, date.
- Derive the injection deadline **subject to** the 300 ms budget (C10): deadline + candidate
  assembly + context assembly + send ≤ 300 ms added at p95. If no shaping fits, that is a finding
  for the gate, not a reason to raise the limit silently.
- Freeze the report into `evaluation/reports/`, naming explicitly which shapes are infeasible at
  which store size (the rate-limit wall for one-request-per-candidate at large candidate counts is
  expected to bite).

**Verification (automated)**: `cargo build`; `cargo test -p qsf_semantics` (aggregation arithmetic,
percentile computation, cost derivation from observed usage, report serialization); a test that the
derived injection deadline can never exceed what `MAX_ADDED_TIME_TO_FIRST_AUDIO_MS` allows; clippy;
fmt.

**Human testing (required — the phase's product is a measurement).** Operator runs
`cargo run -p qsf_semantics -- bench …` with `TYPESAFE_API_KEY` set. Cost: a few hundred small
requests. Evidence: the frozen latency/cost report, plus a note on any behavior the vendor
documentation did not predict (the model facts are days old and expected to drift).

**Optional human testing (the tool for revising U2's limit).** A listening check with the fixture
backend at synthetic delays of 0 / 150 / 300 / 500 ms added before the response, to judge by ear
what added silence is tolerable. Any revision of `MAX_ADDED_TIME_TO_FIRST_AUDIO_MS` is recorded with
its reason and its date.

---

# Part B — Evidence and the operator-judged gate

## Phase 5 — The shared relevance scoring set

One scoring set, two task families, invented data only. The existing goal-relevance v1 labels are
**not** gold for this work (bound to a superseded scope and an earlier threshold); they survive as
evidence and a sampling prior and are not deleted.

**Work**

- **`evaluation/contracts/SemanticRelevance.TaskContract.md`** (versioned): the pair unit
  `(utterance, stored_text)` with a gold "is the stored text about the utterance" label; the label
  space including `Ambiguous` as a first-class gold value; the prediction contract per scorer (the
  lexical baselines never abstain; the judge emits a probability); the frozen `evaluation_time` and
  complete store/association inputs as part of the dataset identity (C4); the metric package
  (recall, precision, admitted-candidates-per-utterance, recall at k, per-store-size breakdowns,
  measured cost); the utterance outcome classification and the two-part population design; and two
  family annexes — `memory_relevance` and `goal_relevance` — carrying family-specific rubric, error
  direction (C8), operating-point rule and cost sketch. `evaluation/contracts/GoalRelevance.TaskContract.md`
  is **not** retired; it gains a pointer to the new family annex and a note that merge-or-retire is
  decided when the panel campaign is run or abandoned.
- **Invented content**: memory stores at several sizes (≈ 20, ≈ 100, ≈ 500+ records) with realistic
  kinds, tags, provenance, trust tiers and **relative** ages materialized against the frozen
  evaluation time; an invented goal roster resembling self-authored production goals plus the
  permanent seed goals; utterances including lexically disjoint but thematically relevant cases,
  hard negatives, distractor-rich stores, negation, quoted speech, degraded-ASR shapes, and
  deliberately **utterances with nothing relevant in the store**.
- **Two-part population** (the R4 requirement): a broad sparse stratified pool carrying explicit
  **sampling weights** for development and tuning, and a **fully labeled evaluation subset** where
  every utterance is labeled against every record in its store, over which whole-store recall,
  precision and "nothing relevant existed" are valid.
- **Labeling**: gold drafted by two different LLMs labeling blind; operator review of every
  disagreement (DecisionLog 2026-07-21); anchoring is the central threat, so no model answer is ever
  pre-filled in front of a reviewer and rubric examples are authored fresh; no floating point in
  persisted artifacts; `deny_unknown_fields` types with tests parsing the real generated files;
  `.gitattributes` LF discipline for every hash computed from disk.
- **Split**: development and **sealed** parts, split so no paraphrase cluster, source utterance or
  invented store spans both; the fully labeled subset is split the same way. The sealed part is
  consulted once per candidate model and each look is appended to the look ledger.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_semantic_eval -p qsf_semantic_datagen`
covering schema round-trip and off-version rejection, split integrity, slice-coverage floors per
family, completeness of the fully labeled subset (every utterance × every store record present),
sampling weights present and normalized on the sparse pool, the exhaustive-and-exclusive utterance
classification, the look-ledger append-only invariant, and the frozen `evaluation_time` present and
used; parse tests over the real generated frozen files; clippy; fmt.

**Experiment scaffold**: none (dataset construction is engineering).

**Human testing (required).** Operator review of every labeler disagreement, plus a blind
re-annotation pass on a shuffled sample of the hard slices. Evidence: per-slice agreement figures,
counts per slice and store size, and a note on categories that were hard to adjudicate.
**Cost**: two labeling passes over a sparse pool plus the fully labeled subset; budgeted and reported
by the datagen pricing machinery (DecisionLog 2026-07-21).

---

## Phase 6 — Development-split tuning, the sealed look, and the operator-judged gate

**Experiment**: `Experiment.MemoryRelevanceJudgmentVersusLexicalFloor.md`.

**Work**

- Commit the reference numbers R1–R6 and the two combination-policy alternatives **before** any
  sealed-split run; the git ordering is the evidence that the reference line preceded the numbers.
  Any pre-look revision is committed with its reason and date.
- **Development split — tune freely**: sweep the admission threshold, choose the shaping variant,
  choose the combination policy (C5) verified through the whole retrieval → assembly path, and fix
  the question wording. Every choice and its evidence is recorded.
- **Sealed split — one run, one logged look**: produce `relevance-judgment.jsonl`, `metrics.json`
  and the gate report; freeze the report into `evaluation/reports/`.
- The gate report presents latency, measured cost per turn and per conversation, and quality **side
  by side**, with R1–R6 shown as the reference line and the utterance-outcome classification
  breaking down where the lexical pipeline loses.
- Add the regression gate: a test re-runs the baselines at the frozen `evaluation_time` and compares
  structured `metrics.json` fields against the frozen figures within a stated tolerance, proven to
  have teeth by an injected perturbation.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_semantic_eval`; the gate figures
reproduce from the frozen per-pair artifact (no independently recomputed tables); the regression gate
passes on `main` and fails on a perturbation; the artifact-parsing test over the generated per-pair
JSONL; clippy; fmt.

**Human testing (required — this is the decision point for the whole plan).** The operator reads the
gate report, confirms the worst false negatives and false positives are scorer errors rather than
label errors, weighs latency, cost and quality together, and records the go/halt call with reasons
against each reference number.

---

# Part C — Memory in the live path (conditional on a go)

## Phase 7 — The judge as an isolated effect: actions, deadline, cancellation, drain

**Experiment**: `Experiment.ShadowMemoryRelevanceJudgment.md`.

The judge runs on every trusted turn and changes nothing. Its purpose is the async lifecycle, in-situ
latency evidence, and an honest look at what it would have admitted on real conversation.

**Work**

- Implement the full lifecycle of C7 / "Judge lifecycle": the pure
  `realtime/relevance_judgment.rs` reducer, the new `SidebandCommand` variants, the spawned effect,
  the spawned deadline timer, `PendingInjection` on `SidebandRuntimeState`, identity-based stale
  rejection, cancellation on interruption / stop / attachment termination / input-revision change,
  and the bounded shutdown drain.
- **Drive-to-completion constraint (C12 follow-up):** the wiring must spawn every
  `PairScoringService::score_pairs` future and race the injection deadline against its task handle.
  It must never race and drop the bare future, because doing so cancels the request and destroys the
  late verdict needed for carry-over.
- Injection splits into a planning step (candidate assembly and effect start) and a completion step
  (context assembly, `session.update`, `conversation.item.create`, `response.create`). In shadow
  mode the completion step runs immediately on the lexical result and the verdict only records what
  it *would* have admitted, so shadow mode adds no time to first audio by construction.
- `QSF_RELEVANCE_JUDGE_MODE` defaults to `shadow` in this phase (the default exercises this phase's
  new path).
- Candidate assembly renders store records to `candidate_text` through a named adapter, pinned by
  `candidate_content_hash`; verify returned pair-score hashes against those assembled candidates
  before adapting verdicts (the pure verdict adapter does not own candidate assembly).
- Ledger: `RELEVANCE_JUDGE_ROLE`, a declared zero-count row at session start carrying the pinned
  model id (DecisionLog 2026-07-29), usage recorded per call, raw `usage` logged at the boundary
  through the existing shared helper (DecisionLog 2026-07-30).
- **Launcher** (review issue 10): new script parameters `-RelevanceJudge`, `-RelevanceJudgeMode`,
  `-RelevanceJudgeDeadlineMs` on `realtime` and on `probe`, consumed by
  `Get-RealtimeEnvironmentDelta` exactly as `$WorldCorpusPath` already is; the corresponding
  `QSF_RELEVANCE_JUDGE_*` names added to the managed-environment delta; a shared resolution helper
  computes the *effective* backend and mode, and `Test-RequiredSecret -Name "TYPESAFE_API_KEY"` runs
  **only** when that resolution actually calls the remote backend. Precedence: explicit flag >
  profile default > server default. The live profiles default the backend to the remote one; the
  documented rollback `-RelevanceJudgeMode off` is reachable from the launcher.
- **Probe extension (depends on `feature/headless-conversation` landing)**: per-turn record of which
  memories were looked up and would have been admitted, look-up latency, a `model_ids` slot for the
  judge model identity, the trace-contract parser extended to the three lifecycle records, the
  finalizer waiting on the judge drain barrier as well as the formation barrier, and the secret scan
  extended to the judge key. That branch also changes attachment, shutdown and promotion behavior, so
  the drain design is reconciled with its fail-closed attachment policy and its formation barrier
  rather than bolted alongside. Until it lands, verification falls back to a manual live session plus
  diagnostics inspection, and each branch-dependent step is tracked as an explicit, named follow-up
  in this phase's status (not silently deferred).

**Verification (automated)**: `cargo build`; `cargo test -p qsf_realtime_server`; reducer unit tests
for every lifecycle transition (in-time, deadline-first, failure, stale, superseded, cancelled);
**a loop-responsiveness test** — with the fixture backend held at a long synthetic latency, a stop
command and a provider event are both still processed while a judgment is in flight; a test that the
injection deadline does not cancel the request (C12); a drain test — no diagnostic record is appended
after the stop path completes; ledger tests; the lifecycle artifact-parsing tests listed in the trace
contract; launcher tests in `scripts/qsf.Tests.ps1` for the new parameters, their precedence, the
managed delta, and the conditional secret check; clippy; fmt; UI obligations if the token-meter card
needs a note for the new row.

**Human testing (required, paid).** One live conversation (or one probe run once the branch lands)
with turns containing a lexically disjoint but thematically relevant memory. Evidence: in-situ judge
latency against the Phase 4 numbers, the would-have-admitted sets, the ledger row with real usage,
and confirmation that no selection changed and no turn stalled.

---

## Phase 8 — Same-turn judged admission with the injection deadline and a recorded fallback

**Experiment**: `Experiment.SameTurnJudgedMemoryInjection.md`.

The judge now decides. It sits on the critical path to first audio (`create_response: false`; QSF
owns `response.create`; injection is serial before it), judging **after the final transcript**, and
the added silence is measured against the independently fixed 300 ms limit.

**Work**

- `QSF_RELEVANCE_JUDGE_MODE` default flips to `live`.
- The completion step waits for `RelevanceVerdictReady` or `InjectionDeadlineElapsed`, whichever
  comes first, and then injects: in time → verdicts enter `RetrievalRequest.judge_verdicts` with the
  chosen combination policy; late / failed / unavailable → the lexical result stands,
  `fallback_executed` is recorded with its reason, and the still-running request's eventual verdict
  is parked (consumed in Phase 10; until then recorded `late_dropped`).
- Every judge-influenced fragment is ineligible for durable structure by the C6 counterfactual, not
  by its admission basis.
- **The falsifier** is `MAX_ADDED_TIME_TO_FIRST_AUDIO_MS = 300` (U2), fixed before this phase and
  independent of the judge measurement: p95 added time over the pre-change baseline of
  `final_transcript_received_to_first_audio` (current envelope ~600–850 ms per
  `Experiment.WorldConsultation.md`) must stay within it. Exceeding it means judging must move early
  (Phase 11) or this phase fails; it does not mean quietly accepting the latency, and it does not
  mean revising the limit to fit the measurement.
- "Judge only when lexical retrieval is weak" is rejected by decision and is not implemented.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_realtime_server -p qsf_memory`;
deadline tests using the fixture backend's synthetic latency (in-time admission changes the
selection; late leaves the lexical selection byte-identical and records the fallback; a typed failure
does the same and logs with session and operation context); a test that the whole path
retrieval → assembly yields the expected injected set under each combination policy; the lifecycle
artifact tests extended to the live selection record; clippy; fmt.

**Human testing (required, paid).** A live conversation with the designed lexically disjoint turns.
Evidence: measured added time to first audio versus the 300 ms limit, whether the intended memory was
injected in the same turn, the deadline-outcome distribution, and a subjective note on the added
silence.

---

## Phase 9 — The injection allowance raised and measured

**Experiment**: `Experiment.JudgedMemoryInjectionAllowance.md`.

With a generous admission rule the binding constraint moves to the budget: today one constant pair
serves as both the retrieval limit and the context budget (Verified item 13).

**Work**

- Separate the two uses into named constants with one source of truth each.
- Raise the injection allowance; the raised values are the **defaults**. Exact values come from this
  phase's measurement, with the sweep points recorded before the runs.
- Measure per setting: prompt tokens and cost per turn, time to first audio against the 300 ms
  limit, the count of injected fragments actually referenced in the reply, and answer quality.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_realtime_server -p qsf_context`;
tests that the two constants are independently honored and that `omitted_by_budget` is recorded with
the numeric budget in force; clippy; fmt.

**Human testing (required, paid; listening recommended).** Live runs at the sweep points. Evidence:
token/cost and latency deltas, and an operator judgment of whether the larger context made answers
better, worse (diluted, rambling), or indistinguishable.

---

## Phase 10 — Late verdicts carried over to the next turn

**Experiment**: `Experiment.LateRelevanceVerdictCarryOver.md`.

**Work**

- A carry-over slot on `SidebandRuntimeState` holding the late verdict with the input identity it was
  computed for, its age in turns, and an expiry rule. The slot is fed by the C12 request that
  outlived its injection deadline.
- The next turn consumes it as additional per-candidate data; a current-turn verdict always takes
  precedence (lifecycle precedence rule); the carried verdict is marked in the trace and remains
  ineligible for durable structure — it belongs to a different utterance.
- Expiry and drop reasons are recorded, never silent.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_realtime_server`; reducer tests for
emit / park / consume / precedence / expire / drop; a test that a carried verdict never reaches
`associable_retrieval_source_ids()` nor reinforcement; the lifecycle artifact test asserting the
producing turn's `late_carried_over` disposition and the consuming turn's back-reference; clippy;
fmt.

**Human testing (recommended, paid).** A live session where at least one turn misses the deadline;
confirm from the artifacts that the verdict was consumed on the following turn and that the
conversation did not feel disjointed.

---

## Phase 11 — Early-start judging on partial transcripts

**Experiment**: `Experiment.EarlyStartRelevanceJudgment.md`.

Moves the judge off the serial critical path by starting it while the person is still speaking.
Required if Phase 8's falsifier was breached; valuable regardless.

**Work**

- Start judging on partial transcription deltas (or on speech start) and refresh on the final
  transcript. Each start is a new invocation with an incremented `input_revision`; a superseded
  invocation's completion is recorded `superseded_by_revision` and never applied.
- Bounded number of early judgments per turn; the extra calls appear in the ledger and in the
  cost-per-turn figure.
- The record's `transcript_fraction` marks how much of the utterance a verdict saw.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_realtime_server`; tests for
supersession ordering, out-of-order completion, the bounded early-call count, and the trace marking
of partial-input verdicts; clippy; fmt.

**Human testing (required, paid).** Live comparison of added time to first audio against Phase 8 and
the 300 ms limit, plus a check that early verdicts on truncated utterances do not systematically
admit the wrong memories.

---

# Part D — Goals (conditional on a go)

Memories first, then goals; each independently verifiable. Goal admission stays conservative (C8),
and the two subsystems keep separate scores: shared machinery, never a shared salience number.

## Phase 12 — Goal relevance judged silently

**Experiment**: `Experiment.ShadowGoalRelevanceJudgment.md`.

**Work**

- A goal-side candidate adapter rendering each goal's description into one canonical
  `candidate_text`, pinned by content hash; a second question set with its own wording version,
  issued as its own request.
- Shadow mode records what would have been admitted at selection **and** what would have qualified
  at the arbitration partition, next to what actually happened, including the arbitration
  counterfactual: would the winner have changed?
- The goal family's operating point comes from the `goal_relevance` annex, not from the memory
  family.
- **Combined-workload check (U2):** the memory and goal judgments for one turn are measured
  *together* against the same 300 ms added-silence limit and the same shared request-rate budget,
  because a turn has one critical path, not one per task family.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_realtime_server -p qsf_volition`;
tests that shadow mode changes no selection, no arbitration and no durable volition state, that the
goal record carries both thresholds numerically, and that combined per-turn request counts are
recorded; clippy; fmt.

**Human testing (recommended, paid).** A live session; operator review of the would-have-qualified
sets, and of the combined added latency against the limit.

---

## Phase 13 — Goal relevance live: judge qualification at arbitration, with a stated durable-state boundary

**Experiment**: `Experiment.JudgedGoalActivation.md`.

**Work**

- **Both gates.** `GoalSelection` gains an optional `judge_qualification { probability, threshold }`.
  `select_goals_ranked` admits a judge-qualified goal that matched no keyword (recording the
  admission basis), and `arbitrate_with_mode`'s partition predicate becomes
  `match_strength >= threshold || judge_qualified` (Verified item 10). `match_strength` arithmetic,
  `compute_relevance`, status and cooldown exclusions, tier ordering and the below-threshold record
  shape are all unchanged; a judge-qualified selection records its basis, and a goal that is neither
  lexically qualified nor judge-qualified keeps today's reason text.
- **Durable-state boundary.** Executing an initiative applies `VolitionEvent::InitiativeExecuted`
  into volition state, which promotion persists (Verified item 11). Excluding judge-qualified winners
  from *all* durable mutation would mean they cannot execute an initiative at all, which would make
  live goal judging pointless — the whole purpose is that the goal shapes this turn. The narrowest
  defensible boundary is therefore proposed here, and the residual is an open question, not a silent
  resolution:
  - **Permitted**: executing the initiative for the current turn; the resulting `InitiativeExecuted`
    carrying an explicit `qualified_by: judge` marker so the reducer can apply the narrow path; the
    turn's context shaping and its traces.
  - **Not permitted**: contributing to cross-turn accumulators that compound — salience
    accumulation, satisfaction or progress credit, and goal-status transitions driven solely by a
    judge qualification.
  - The marker is what enforces this; marking only the trace would enforce nothing.
  - `durable_mutation_scope` is recorded on the selection record so an artifact reader can see which
    boundary was in force.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_volition -p qsf_realtime_server`;
tests that a lexically disjoint but clearly relevant goal can both be selected and **qualify at
arbitration**; that a below-threshold verdict can do neither; that `match_strength`,
`compute_relevance`, cooldown/status exclusions and tier ordering are unchanged; that a
judge-qualified winner's `InitiativeExecuted` carries the marker and does **not** move the excluded
accumulators, asserted against the persisted `volition-state.json`; clippy; fmt.

**Human testing (required, paid).** Live session with turns that are about a goal without using its
keywords. Evidence: whether the right goal qualified and won, whether anything wrong qualified, the
arbitration outcome versus the shadow counterfactual, and the combined added latency.

---

# Part E — Conversation quality

## Phase 14 — The bridging line under a late verdict

**Experiment**: `Experiment.BridgingLineUnderLateVerdict.md`.

When the person asked something that wants an answer and the verdict is late, the assistant may take
an intermediary turn ("Thinking about X — let me see…") to buy time for a proper answer instead of
immediately saying something of less value.

**Work**

- Detection of "the utterance wants an answer" is deliberately open (Open Questions); the phase
  implements one named, pure, testable detector and records its decision in the trace, so a wrong
  detection is visible rather than mysterious.
- The bridging line is rendered by a pure function from state, rate-limited by the existing anti-nag
  cadence thinking, and recorded as its own turn kind so transcripts stay honest.
- The follow-through is mandatory: a bridging line not followed by the improved answer is a
  regression, and a test asserts the pairing.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_realtime_server`; detector unit tests
over question-shaped and non-question-shaped fixtures; renderer tests; a test that no bridging line
is emitted when the verdict is in time; clippy; fmt.

**Human testing (required — listening test).** A live spoken conversation. Evidence: does the
bridging line feel natural or stalling; does the follow-up arrive quickly enough to justify it; and
the operator's verdict versus Phase 8 behavior.

---

# Part F — Backend evolution

## Phase 15 — Local challenger through a helper process

**Work**

- A Python helper process holding an open-weights model on the GPU, started by the launcher, spoken
  to over a local connection, implementing the C1 contract; a `HelperProcessPairScoringService`
  backend behind the same trait.
- Health, startup and shutdown are loud and observable; the server never blocks readiness on the
  helper; a missing helper is a typed failure with the lexical floor intact.
- Admitted only by evidence: scored on the development split, then one logged sealed-split look,
  compared against the lexical baseline and the incumbent judge on the same metrics, latency and
  cost factors.

**Verification (automated)**: `cargo build`; `cargo test -p qsf_semantics -p qsf_realtime_server`
(one backend-contract test suite exercised against every backend; failure and unavailability paths);
launcher tests for starting and stopping the helper; clippy; fmt.

**Experiment scaffold**: none — a backend swap measured on an existing scoring set is engineering
plus measurement (DecisionLog 2026-07-04).

**Human testing (required).** Operator runs the helper on this machine and records local latency,
VRAM footprint, cold-start time, and the comparison report.

---

## Phase 16 — In-process judging in the Rust server (the written end state)

**Trigger condition, stated now**: a model has stayed stable (unchanged identity across a stated
period and at least one re-measurement), has won on the scoring set against both the lexical baseline
and the incumbent judge on quality, latency and cost together, and its measured in-process latency
fits the 300 ms budget with margin.

**Work**

- An in-process backend built on the local inference runtime that
  `Plan.SharedSemanticInfrastructure.md` contributes to `qsf_semantics`, presented through
  `BlockingPairScorerService` (C1) so no new abstraction appears. The model artifact digest becomes
  the `identity_value`.
- The helper process is retired only after the in-process backend has passed the same reference bar
  on the same sealed split.

**Verification (automated)**: `cargo build`; workspace `cargo test`; the shared backend-contract
suite; clippy; fmt.

**Human testing (required).** Live session on the in-process backend; evidence: latency, added time
to first audio, and the comparison report.

---

## Metrics and thresholds

Measured and frozen: judge latency by shaping and candidate count, and measured cost per turn and per
conversation (Phase 4); recall, precision, admitted-candidates-per-utterance, recall at k and the
utterance-outcome classification per task family and per store size (Phase 6); added time to first
audio (Phases 8, 11, 13, 16); tokens and cost per turn at each injection allowance (Phase 9);
deadline-outcome distribution across live turns.

Fixed before the evidence: `MAX_ADDED_TIME_TO_FIRST_AUDIO_MS = 300` (U2, revisable only with a
recorded reason); the reference numbers R1–R6 (U1, revisable only before the first sealed look, each
revision recorded). Derived from measurement: the injection deadline (constrained by the 300 ms
budget) and the final injection allowance.

## Safety and fallback behavior

No invented verdict: a failure yields a typed error inside a fully formed record, and the lexical
result is the behavior of record. Every fallback is attested by the caller, because the judge crate
can only prove its own failure (silent fallback is an incident class in this repo). The 2026-05-25
zero-signal rule survives: the judge is an additional admissible signal with an auditable threshold,
not a bypass. Judge-influenced selections — decided by the lexical-only counterfactual, not by
admission basis — never reach associations, reinforcement, or the excluded volition accumulators.
The receive loop stays responsive while a judgment is in flight, so interruptions and stop requests
are never delayed by a network call. The realtime server stays read-only on the memory store. Real
conversation turns and stored texts are sent to a third-party endpoint under the remote backend — an
accepted decision — and the secret is handled through the launcher's existing secret discipline,
required only when the effective configuration actually calls that endpoint, and covered by the
probe's secret scan. The fixture backend is never presented as relevance evidence.

## Dependencies and branch coordination

- **`feature/headless-conversation`** has landed on `main`; its headless scripted conversation probe
  and decision-log entries dated 2026-09-21 are available to the later probe extension. The judge
  drain barrier, cancellation rules, and attachment-epoch identity still need reconciliation with
  the landed fail-closed attachment policy and live-goal-formation drain barrier — one finalization
  order, not two — but no branch wait or interim manual-verification fallback applies. The seeded
  memory-store fixture remains a stable control: judged runs get a new phrase set or expectation
  block, never a change to the existing seed.
- **`feature/sleep-world-study`** (active; own worktree). Its substantive changes are the staged
  persistence path and sleep update orchestration; the co-retrieval proposer itself is unchanged on
  that branch. Coordination: verify the C6 exclusion **through the staged write path** after that
  branch integrates, reconcile any `MemoryRecord` field additions before `RetrievalRequest` lands,
  and follow its inline-budget relocation precedent when placing the judge budget constants (C10).
  Collision risk is otherwise low: this work only reads memory.
- **External**: the hosted judge endpoint and its key; the vendor's model facts are days old, so
  Phase 4 re-verifies them against live behavior before anything is built on them.
- **Ledger extraction** proposed by `Plan.SemanticEvaluationFoundation.md` is **not** done here; the
  judge runs inside the realtime server and records into the existing ledger, and offline runs record
  usage into their own artifacts. If that extraction lands first, the judge row moves with it.

## Relationship to related plans and ideas

- **`Plan.SharedSemanticInfrastructure.md`** — shares the crate **and the contract**. This plan
  creates `qsf_semantics` and defines the canonical pair-scoring contract, the unified trace record
  and the single failure enum (C1); that plan's phases add the local inference runtime, asset
  manifests, normalizer, n-gram features, head execution and calibration inside the same crate and
  behind the same traits, and that document is **updated to consume this contract** rather than to
  define a parallel one. Its commitments on record-in-the-result, the canonical artifact digest and
  loud observability are adopted; `identity_kind` covers pinned remote versions as well as artifact
  digests. Its failure-floor gate and this plan's gate are independent. Its Open Question 4 ("where
  shadow-mode wiring lands") is answered here for the relevance behaviors.
- **`Research/TechBrief.QSF_Local_Semantic_Classification.md`** — planning input only; this plan is a
  remote-first instantiation of its hybrid semantic-retrieval direction with the local end state
  written down (Phase 16).
- **`Idea.SemanticMatchingCorpus.md`** — this plan builds the generic "is this text about that text"
  corpus at reduced scope, inherits its constraints wholesale, and settles its open questions 1
  (division and look budget), 2 (a family with per-task annexes), 3 (stratified with explicit
  weights) and, partially, 5 (the gate is operator-judged over a reference line, with a fully labeled
  subset supporting whole-store metrics). Its open questions 4 (pooling), 6 (goal-text realism), 7
  (refresh cadence) and 9 (cold vs warm regimes) stay open there.
- **`Idea.SemanticGoalActivation.md`** — precursor; its shadow-scoring sketch is superseded by
  Phases 12–13.
- **`Plan.IndexedMemoryStorage.md`** (unstarted) — complementary: it changes storage, this plan
  changes scoring. Phase 3's once-per-turn off-thread store load is compatible with either backing
  store, and the candidate-shortlist question becomes a consumer of indexed storage when large stores
  make judging every record impractical.
- **`Plan.SemanticEvaluationFoundation.md`** — partly landed despite its header (Verified item 16);
  this plan extends its crates and `evaluation/` tree and generalizes its runner from `goal_ref` to
  `candidate_ref`.
- **`Plan.GoalRelevancePanelLabeling.md`** / **`Design.GoalRelevancePanelLabeling.md`** — neither
  blocked nor absorbed; methodology inherited, v1 labels not gold and not deleted, merge-or-retire
  deferred and recorded.
- **`Plan.HeadlessConversationProbe.md`** — the verification instrument; see branch coordination.

## Exit criteria (whole plan)

- A judge seam exists that the rest of the system talks to in terms of candidates and probabilities,
  with one contract, one record schema, one failure enum and interchangeable backends.
- Retrieval is deterministic given a frozen store and an explicit evaluation time.
- Judge admission, both combination policies and the judge-independent eligibility rule exist in
  production code and are verified through the whole retrieval → assembly path.
- Latency and **measured cost** come from this machine and are frozen in `evaluation/reports/`.
- A shared invented scoring set — sparse weighted pool plus a fully labeled evaluation subset, with a
  development split and a sealed split under a logged look budget — grades the lexical baselines and
  any backend through production code.
- The operator evaluated latency, cost and quality side by side against the pre-registered reference
  numbers and recorded a go or halt.
- On a go: a thematically relevant, lexically disjoint memory is injected in the same turn when the
  judge meets the deadline and on the next turn when it does not; goals qualify at both gates under a
  conservative threshold with a stated durable-state boundary; the injection allowance matches the
  recall-oriented cut-off; added time to first audio stays within the fixed 300 ms limit for the
  combined memory-and-goal workload.
- The receive loop remains responsive while judgments are in flight, and no artifact can be appended
  after finalization.
- Every judgment, deadline miss, cancellation, stale completion and fallback is visible in artifacts
  that a test parses, including the actually persisted provenance.
- Judge-influenced selections never reach associations, reinforcement or the excluded volition
  accumulators.
- Judge usage appears in the token ledger with a declared row and raw usage logged at the boundary.
- On a halt: the seam, the pure policies, the eligibility rule, the scoring set, the latency/cost
  report and the selection diagnostics stand; the halt is recorded with its evidence; and the
  re-opening condition is written down.

## Rollback plan

Each phase reverts cleanly. Configuration gives the fast path: `-RelevanceJudgeMode off` through the
launcher restores byte-identical lexical behavior (never the default — an explicit rollback), and
selecting the fixture backend removes all external calls and cost while keeping the code path alive.
No memory or volition state is written by this work beyond the narrow permitted initiative path; the
only persisted additions are optional serde-defaulted fields and new diagnostic record kinds, both
backward compatible.

## Documents to create or update (`ProjectWorkflow.md`)

**Create**

- This plan; the nine `Experiment.*.md` scaffolds named above, each added to
  `docs/Experiments/Experiment.Backlog.md`.
- `crates/qsf_semantics` (crate, tests, binary).
- `evaluation/contracts/SemanticRelevance.TaskContract.md` with its two family annexes;
  `evaluation/frozen/semantic-relevance/` (invented stores, goal roster, sparse weighted pool, fully
  labeled evaluation subset, development and sealed splits, lineage, look ledger, reference-number
  revision log); annotation guidelines for the new families under `evaluation/annotations/`.
- Frozen reports in `evaluation/reports/`: the latency/cost report and the gate report.
- Consider `docs/Plans/Design.RelevanceJudgeAdmission.md` if the combination-policy and
  eligibility-counterfactual decisions need more room than the task contract gives them.

**Update**

- `docs/Plans/Plan.SharedSemanticInfrastructure.md` — adopt the canonical contract, record and
  failure enum defined here (a substantive edit, not a cross-reference).
- `docs/Architecture/Architecture.MemorySystem.md` — Implementation Status, the judge admission path,
  the skip reasons, the eligibility counterfactual, and the explicit evaluation time.
- `docs/Architecture/Architecture.VolitionSystem.md` — judge qualification at both gates and the
  durable-state boundary.
- `docs/Architecture/Architecture.ModelRoles.md` — a model used by the system that does not go
  through `qsf_models`, and its ledger row.
- `docs/Architecture/Architecture.RealtimeSessionServer.md` — the judge effect, the action lifecycle,
  the two budgets, cancellation and the shutdown drain.
- `docs/Architecture/Architecture.StateAndObservability.md` — the lifecycle record kinds and the
  `relevance_judgment` trace contract.
- `docs/Architecture/Architecture.ContextManagement.md` and `docs/Concepts/Concept.ContextBudget.md`
  — the split between the retrieval candidate limit and the injection budget, the raised allowance,
  and assembly's ordering input.
- `docs/Architecture/Architecture.Overview.md` — Implementation Status.
- `docs/Glossary.md` — relevance judge, verdict, admission basis, selection eligibility, injection
  deadline vs request timeout, deadline outcome, carry-over, fixture backend, sealed-split look
  budget.
- `README.md` — the new launcher flags, environment variables, the secret, and how to run the bench
  and the scoring-set runner.
- `scripts/qsf.ps1` and `scripts/qsf.Tests.ps1` — the new realtime/probe parameters and their
  precedence, the managed-environment delta, the shared effective-configuration resolver, the
  conditional secret check, and help text.
- `docs/Handoff.md` — pointer updates whenever a phase or the gate changes a recommendation.
- `docs/Plans/Idea.SemanticMatchingCorpus.md`, `docs/Plans/Plan.GoalRelevancePanelLabeling.md`,
  `evaluation/contracts/GoalRelevance.TaskContract.md`, and the stale "not started" header on
  `docs/Plans/Plan.SemanticEvaluationFoundation.md`.

**Proposed Decision Log entries** (committed only when they land)

Already recorded on 2026-09-20, as operator commitments made during planning: items 4, 5, and 10–11
(the last two as one entry, which also carries the halt-rather-than-scrap rule). Do not add them
again; extend by a new entry only if implementation changes the rule.

1. Per-turn relevance judgment is a swappable seam: the system asks about candidates and receives
   probabilities plus a recorded model identity; no judge model is wired in directly.
2. One canonical pair-scoring contract, one trace record schema and one failure enum serve remote,
   helper and in-process backends; a synchronous scorer is adapted onto the asynchronous service
   rather than given its own abstraction; caller-owned session and selection fields wrap the
   inference record and are never inputs to it.
3. The relevance judge backend is selected explicitly by configuration, never by the presence of a
   key; the in-code default is a fixture backend that exercises the whole path and whose verdicts are
   never relevance evidence; the live launcher profiles pin the paid backend and require its secret
   only when they actually call it.
4. Judge-influenced memory selections are identified by a lexical-only counterfactual, not by
   admission basis, and are excluded from associations and reinforcement until deliberately enabled.
5. Memory relevance errs generous and goal relevance stays conservative; the two families keep
   separate operating points over one scoring set.
6. Live model calls on the realtime critical path run as isolated effects whose results re-enter the
   session loop as actions; the injection deadline and the request timeout are distinct, and a missed
   injection deadline never cancels the request.
7. A relevance verdict that misses the injection deadline is used on the next turn, never discarded,
   and every fallback is recorded with its reason.
8. Memory retrieval takes its evaluation time as an explicit input, so frozen evaluations are
   reproducible.
9. The shared semantic-relevance scoring set combines a weighted sparse pool with a fully labeled
   evaluation subset; gold labels are invented, dual-model blind and operator-reviewed; the sealed
   split is consulted once per candidate model against a logged look budget.
10. The relevance stop/go decision is an operator judgment over latency, measured cost and quality
    presented side by side against pre-registered reference numbers that may be revised only before
    the first sealed-split look.
11. The acceptable added time to first audio for live judging is fixed independently of any judge
    measurement and is revised only deliberately, with the reason recorded.
12. Relevance-judge usage is a first-class token-ledger row, declared at session start, logging raw
    provider usage at the boundary.
13. The pinned judge model version and the question wording are part of the operating point;
    changing either invalidates the admission threshold.
14. Judge-qualified goals qualify at arbitration as well as at selection, and may shape the current
    turn without contributing to cross-turn volition accumulators.
15. The gate outcome — go, with the evidence; or halt, with the evidence and the re-opening
    condition.

**Do not** cite this plan's phase numbers from any durable artifact; name the behavior.

## Open Questions (surfaced, not silently resolved)

1. **Which combination policy wins.** The two alternatives (a bounded additive judge term, or
   reserved budget slots with an explicit ordering input to context assembly) are both implemented
   before the gate and chosen on the development split, verified through the whole
   retrieval → assembly path. Consequence if this were left to implementation instead: context
   assembly's re-sort would silently undo reserved-slot ordering, and a lexically disjoint memory
   would be admitted and then lost at the budget — the whole point of the work would be invisible in
   the live path.
2. **What conversational context beyond the last utterance enters the judged state.** The starting
   point is today's retrieval query (final transcript plus pending volition retrieval hints), with a
   wider window measured as a development-split variant. Consequence: a wider state costs more tokens
   and increases exposure to the documented distractor weakness, while too narrow a state makes
   follow-up turns ("what about the other one?") unjudgeable.
3. **At what store size judging every candidate stops being viable, and what replaces it.** A
   shortlist or prefilter changes the recall ceiling and interacts with the provider's request-rate
   limit; the scoring set is built to show where the line is, but the replacement design (lexical
   prefilter, indexed narrowing, chunked requests) is not chosen here. Consequence: without a
   decision, large stores either exceed the rate limit or quietly revert to lexical behavior.
4. **How "the user asked something that wants an answer" is detected for the bridging line.** A wrong
   detector makes the assistant stall on statements or answer questions with filler. Consequence:
   this is a product-feel question that the listening test, not a metric, must settle.
5. **What happens to the pending goal-relevance panel campaign.** This plan builds new gold for the
   goal family; the campaign is neither blocked nor absorbed. Consequence: leaving both alive
   indefinitely risks two competing goal-relevance gold sets with different rubrics — a
   merge-or-retire decision is owed once this plan's goal family is frozen.
6. **How far the judge-qualified goal durable-state boundary should extend.** The plan permits a
   judge-qualified winner to execute its initiative and records `InitiativeExecuted` with a
   `qualified_by: judge` marker, while excluding salience accumulation, satisfaction credit and
   status transitions. Two effects sit on the line and are *not* resolved here: whether such an
   execution may set `last_initiative_output`, and whether it may advance the anti-nag
   "previously surfaced goal" state. Consequence: permitting them lets a judge mistake shape
   subsequent turns; forbidding them makes the assistant repeat itself and lets a judge-qualified
   goal re-fire every turn. This needs an operator decision before Phase 13 implements it.
