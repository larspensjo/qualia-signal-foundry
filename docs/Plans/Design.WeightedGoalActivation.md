# Design: Weighted Goal Activation and Arbitration Qualification

## Status

Candidate design, approved in discussion 2026-07-04 — **not yet an implementation plan.**
Next step: `Plan.WeightedGoalActivation.md` (three phases: fixture schema + pure scoring;
arbitration gating + traces + realtime adapter; voice retest) with
`Experiment.WeightedGoalActivation.md` as the durable validation gate, plus the decision-log,
architecture, and experiment updates required by the project workflow.

The long-term semantic direction (a local specialized relevance model) is preserved separately
in `docs/Plans/Idea.SemanticGoalActivation.md`. This design is the deterministic lexical layer
that ships first and later doubles as that idea's no-GPU fallback and evaluation harness.

## Summary

Goal activation and arbitration in `qsf_volition` are lexically brittle. Activation is a binary
token gate (`selection.rs::matched_keywords`), and arbitration ignores match strength entirely —
its sort key is `(biased_tier asc, base_priority desc, goal_id asc)`. Together they mean one
near-stopword at a protected tier beats a five-term on-topic match at a malleable tier.

Live evidence (voice session 2026-07-04, Results in `Experiment.CuriosityPersonaSeed.md` and
`Experiment.LiveGoalFormationAndCoherence.md`):

- "Do you believe machines will replace many jobs, and **what** does that **do** to the
  economy?" — `track-the-ai-transition` matched five terms yet lost to
  `serve-the-present-person` matching only `what`/`do`.
- "For **what** it's worth, the hospital near me…" — the same protected goal won on a stopword
  inside an idiom.

This design adds **match strength** as a first-class quantity: activation keywords carry coarse
weight classes in fixture data, a goal's match strength is the sum of its matched-term weights,
and a goal must clear a single global qualification threshold before it can win arbitration.
Tier ordering is unchanged *among qualified goals*. When no goal qualifies, volition stays
quiet for that turn and says so in the trace.

## Decisions

Resolved during the 2026-07-04 brainstorm (record in `docs/DecisionLog.md` when the plan is
created):

1. **Coarse weight classes, not free weights or corpus IDF.** Each keyword is
   `Weak = 1 | Normal = 4 | Strong = 8`, curated in fixture data. Consistent curation beats
   numeric precision at this goal-set size; weights stay readable in the fixture and the
   persona stays data-only.
2. **One global qualification threshold**, a fixture-level constant, default **4**: one Normal
   word qualifies, Weak words qualify only in combination (e.g. 4 × Weak). Per-tier thresholds
   are deliberately deferred until live evidence demands them.
3. **No initiative when nothing qualifies.** A stopword match is not "having something to say."
   The turn records a new suppression reason instead of promoting a weak winner or falling back
   to a fixture-designated default goal.
4. **No qualification exemption for protected tiers.** Protected goals stealing the initiative
   line on weak evidence is the observed failure. Their *protection* — never cancelled by
   coherence, decline-backoff, floor semantics — is untouched; this changes who speaks, not who
   is protected.
5. **Qualification lives inside arbitration** as a pure partition step before the existing
   sort, not as a new pipeline stage and not folded into `relevance_score` ordering. One sort
   implementation remains one sort implementation.

## Scope

In scope:

- Fixture schema: `activation_keywords` become `(term, weight_class)` pairs; a global
  `arbitration_qualification_threshold` on the fixture. Both `realtime_seed_fixture()` and
  `static_fixture()` get curated classes. Defaults exercise the new path (threshold 4 > 0).
- Pure scoring: per-goal `match_strength = Σ weight(matched term)`, carried on `GoalSelection`.
- One strength concept: `compute_relevance`'s `terms.len() × 100` bonus is replaced by
  `match_strength × 25` (a Normal word scores as one term does today), so the ranked display
  and qualification derive from the same quantity.
- Effect selector: `STRONG_MATCH_EFFECT_THRESHOLD` (2 distinct terms) becomes
  `match_strength ≥ 8` **and** at least two distinct non-Weak matched terms. Strength alone is
  not the contract: a single Strong keyword scores 8 but is not a "rich match," so the
  distinct-term requirement stays explicit rather than implied by the old count rule.
- Arbitration: qualification is a structured outcome, not a loser-reason string. Sub-threshold
  selections are partitioned into a separate below-threshold list carrying `match_strength`,
  the threshold in force, and a qualification reason, before the `(biased_tier, base_priority,
  goal_id)` sort. Ordinary loser reasons stay reserved for candidates that actually reached the
  sort — "qualified but lost on tier" and "activated but not eligible to arbitrate" are the two
  outcomes this design exists to distinguish. If nothing qualifies, arbitration yields no
  winner.
- Realtime adapter: the current record shapes cannot express a no-winner turn —
  `RealtimeBoundedInitiativeTrace` requires a winner and is only built when an initiative
  executed, and `build_volition_turn_context_packet` returns `None` when there is no
  arbitration winner and no declined-candidate context. Decision: the bounded-initiative trace
  stays reserved for executed initiatives; a no-qualifier turn instead records a dedicated
  no-winner turn-decision outcome. `VolitionSuppressionReason` gains
  `below_qualification_threshold`, the UI `VolitionTurnDecisionSummary` parser learns the
  no-winner shape and the new reason, and the packet builder emits for
  activated-but-unqualified turns so the suppression is visible in traces and inspection.
- Traces: for every selected, below-threshold, and arbitration-losing candidate, record the
  matched terms *with their weight classes*, the candidate `match_strength`, and the threshold
  in force. Strength + threshold alone audits the comparison but does not survive fixture
  re-curation; terms alone lack the weights. Artifact-parsing verification recomputes
  `match_strength` from the recorded terms and weights and checks the winner/no-winner result.
- Realtime reason categorization: below-threshold candidates render as
  `below_qualification_threshold`, never as `lower_arbitration_rank`.

Not in scope:

- Semantic/model-based scoring (`Idea.SemanticGoalActivation.md`).
- Changes to activation itself: weak matches still activate goals, bump salience, and appear in
  the ranked selection exactly as today — the threshold gates only the arbitration win.
- Changes to protected-floor coherence semantics, mode bias mechanics, or the live-formation
  judge (its decline-path issue is a separate open thread).
- Per-tier thresholds, keyword stemming, or multi-word phrase matching.

## Behavior Consequences (intended)

- The natural step-2 probe phrase — "…**what** does that **do** to the economy?" — now lets
  `track-the-ai-transition` win: `serve-the-present-person` scores 2 (two Weak) < 4 while the
  AI-transition terms qualify easily. Human tests stop being phrase-engineering exercises.
- Idle/stopword-only turns produce a quiet volition line rather than a protected-goal
  initiative, which slightly reduces initiative frequency; the anti-nag layer already
  established that quieter is acceptable.
- `ProposeExperiment` still requires a genuinely rich AI-transition match (strength ≥ 8 with at
  least two distinct non-Weak terms), preserving the intent of the two-term rule under the new
  scale — a single Strong keyword does not fire it.

## Testing

Reducer-level (pure, deterministic — the bulk of the verification):

- Weight/threshold invariants: every fixture keyword carries a class; threshold > 0; strength
  sums and relevance stay consistent (single-source-of-truth test between `match_strength` and
  `compute_relevance`).
- Paraphrase-robustness probes: the same meaning in three wordings selects the same winner;
  a stray-word injection ("for what it's worth," prefixed to an on-topic sentence) does not
  flip the winner; an all-stopword turn yields no winner with the recorded reason.
- Arbitration: sub-threshold protected goal loses to qualified malleable goal; qualified
  protected goal still wins on tier; existing tie-break tests updated, not removed.
- Effect selector: `ProposeExperiment` reachable with two Normal fixture terms; unreachable via
  a single Strong term alone or Weak-word combinations (the distinct non-Weak term rule).
- Compatibility: legacy continuity-snapshot and reviewed-seed JSON containing string
  `activation_keywords` load through the compatibility reader (defaulting to Normal) without
  error.
- Live-formed goals: an accepted candidate with one model-supplied keyword (defaulted to
  Normal) still qualifies at the default threshold — the interim contract in Compatibility
  Notes, asserted so a later formation-schema change is a deliberate break.
- Realtime: an all-stopword/weak-only turn emits the no-winner turn-decision record naming
  `below_qualification_threshold` and the threshold in force; below-threshold candidates
  categorize as `below_qualification_threshold`, not `lower_arbitration_rank`.

Human gate (one voice session, protocol in `Experiment.WeightedGoalActivation.md`):

- Step-2 persona probe with the *natural* phrasing wins for `track-the-ai-transition` and, on a
  rich match, exercises `ProposeExperiment` — closing the remaining persona-experiment gate
  honestly instead of via phrase engineering.
- A deliberately weak turn ("for what it's worth, thanks") produces the
  `below_qualification_threshold` suppression in the diagnostics.
- Latency parity: the scoring change is arithmetic on the existing hot path; injection must
  stay at 0 ms.

## Compatibility Notes

- The goal-id fixture guard is **not** sufficient for this change. Persisted state carries full
  `Goal` values — `VolitionState::accepted_candidates` in continuity snapshots and
  `ReviewedVolitionSeed.accepted_goals` — and both `load_or_upgrade` paths deserialize the
  whole document before any fixture-id check runs, so old `["keyword"]` arrays would fail to
  parse before the guard could discard or upgrade them. `activation_keywords` therefore needs a
  compatibility reader accepting both legacy plain strings (defaulting to Normal) and weighted
  entries, with `VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION` and
  `REVIEWED_VOLITION_SEED_SCHEMA_VERSION` bumps and regression tests that load legacy JSON.
  Snapshot survival depends on this reader, not on the goal-id guard.
- Live-formed candidates (`ProposedGoalCandidate`) carry plain `activation_keywords`; formed
  goals default their keyword class to Normal until the formation prompt schema is extended
  (follow-up noted in the plan, not blocking). Intentional interim consequence: one
  model-supplied keyword scores 4 and clears the default threshold, so live-formed goals
  qualify at least as easily as fixture goals with a single Normal term. This mirrors today's
  behavior (any single keyword match can win) and is pinned by a reducer test above.
- Changing fixture data does not alter the formation judge's stable prefix (goal id/title/
  summary only), so prompt-cache behavior is unaffected.

## Documents To Update (per ProjectWorkflow)

- `docs/DecisionLog.md` — the five decisions above, dated 2026-07-04.
- `docs/Architecture/Architecture.VolitionSystem.md` — selection/arbitration mechanics section.
- `docs/Experiments/Experiment.CuriosityPersonaSeed.md` — close the keyword-tuning open item by
  pointing at this design; its step-2 gate is retested under the new mechanics.
- `docs/Experiments/Experiment.WeightedGoalActivation.md` — new durable gate (created by the
  plan); its trace completeness contract must match the no-winner turn-decision shape and the
  terms-with-weights trace fields specified in Scope.
- `docs/Handoff.md` — Now-level recommendation once the plan exists.

## Open Decisions

- Exact keyword-class curation for every existing fixture keyword (done in plan phase 1;
  reviewed as fixture-data diff, not code).
- Whether the threshold default of 4 survives the first voice retest (tunable fixture data;
  revisit in the experiment's Results).
