# Weighted Goal Activation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give goal activation a first-class match strength (coarse per-keyword weight classes)
and gate arbitration wins behind a global qualification threshold, so a protected goal can no
longer win on a stopword while a five-term on-topic match loses.

**Design authority:** `docs/Plans/Design.WeightedGoalActivation.md` (approved 2026-07-04).
All five decisions there are settled; do not re-litigate them. The decision-log entry was
recorded when this plan was created.

**Architecture:** Pure-reducer change in `qsf_volition` (keyword schema, scoring, arbitration
partition), then adapter/trace propagation in `qsf_realtime_server` + its UI, then a human
voice gate. Unidirectional flow is untouched: selection and arbitration stay pure functions;
the realtime adapter only carries new structured fields into traces and inspection.

**Tech Stack:** Rust workspace (`cargo`), serde compatibility readers, TypeScript UI under
`crates/qsf_browser_server/ui` is *not* involved — the volition UI lives in
`crates/qsf_realtime_server/ui` (`npm run check` / `npm run fmt` from that directory).

## Global Constraints

- After each task: `cargo build`, then the task's test command. At each phase end:
  `cargo clippy --all-targets -- -D warnings` and `cargo fmt`.
- For `crates/qsf_realtime_server/ui` changes: `npm run check` and `npm run fmt` from that
  directory (use `npm.cmd` if launching through `Start-Process`).
- Defaults must exercise the new path: `arbitration_qualification_threshold` defaults to **4**
  (> 0), so qualification gating is live in both shipped fixtures without configuration.
- Weight classes: `Weak = 1`, `Normal = 4`, `Strong = 8`. Threshold default **4**.
- Relevance: `match_strength × 25.0` replaces `terms.len() × 100.0` (one Normal keyword scores
  exactly what one term scored before).
- Effect gate: `ProposeExperiment` requires `match_strength ≥ 8` **and** ≥ 2 distinct non-Weak
  matched terms.
- Activation itself is unchanged: weak matches still activate goals, bump salience, and appear
  in ranked selection. Only the arbitration *win* is gated.
- Protected-floor semantics, mode bias mechanics, and the live-formation judge are out of
  scope. No per-tier thresholds, no stemming, no phrases.
- Reducers stay pure; tests assert structured fields, never rendered reason strings.
- Never cite this plan's phase numbers from durable documents; name the behavior instead.

## Trace Completeness Contract

(Per `docs/ProjectFrame/ProjectWorkflow.md`; `Experiment.WeightedGoalActivation.md` — created
in Task 1 — is the durable home of this contract.)

Required trace fields, per trusted realtime turn that emits a volition context packet (a
qualified winner, a below-threshold candidate, or a declined candidate exists; a trusted turn
with no lexical activation at all emits no packet and is outside this contract's scope):

```text
input                      — transcript ref (existing)
events_applied             — existing
selector_output            — existing + per-selected-goal matched keywords with weight
                             classes and match_strength
omitted_or_suppressed      — existing + matched keywords with weight classes and
  _candidates                match_strength on every below-threshold and
                             arbitration-losing candidate; below-threshold candidates
                             categorized `below_qualification_threshold`, never
                             `lower_arbitration_rank`
arbitration_result         — existing summary when a goal qualified; absent on a
                             no-qualifier turn
qualification_threshold    — the threshold in force, on the packet summary and the
                             turn-decision record
turn decision              — winner block optional; a no-qualifier turn records winner =
                             none plus suppression reason `below_qualification_threshold`
bounded_or_external_output — unchanged; the bounded-initiative trace stays reserved for
                             executed initiatives
```

Artifact boundary: diagnostics JSONL records (`VolitionContextInjected`, inspection captures)
carry the structured chain; the UI volition panel is a derived read-only view. Artifact-parsing
verification (Task 13) reparses serialized trace JSON, recomputes `match_strength` from the
recorded terms-with-weights, and checks the winner/no-winner outcome against the recorded
threshold.

## File Structure

| File | Responsibility in this plan |
|---|---|
| `crates/qsf_volition/src/model.rs` | New `KeywordWeightClass` + `ActivationKeyword` (with legacy-string compat serde); `Goal.activation_keywords` type change; fixture threshold field |
| `crates/qsf_volition/src/fixture.rs` | Curated weight classes for both shipped fixtures |
| `crates/qsf_volition/src/selection.rs` | Weighted `matched_keywords`, `match_strength`, relevance, effect selector |
| `crates/qsf_volition/src/initiative.rs` | `GoalSelection` carries matched keywords + strength |
| `crates/qsf_volition/src/arbitration.rs` | Qualification partition (`ModeArbitrationOutcome`, `BelowThresholdCandidate`) |
| `crates/qsf_volition/src/continuity.rs` | Schema version bumps + reviewed-seed upgrade path |
| `crates/qsf_volition/src/candidate.rs` | Live-formed goals default keywords to Normal |
| `crates/qsf_volition/src/opportunity.rs`, `coherence.rs`, `consolidation.rs`, `shaping.rs` | Mechanical ripple (`.term`, constructors) |
| `crates/qsf_volition/src/lib.rs` | Re-exports for the new types |
| `crates/qsf_app/src/volition.rs` + `crates/qsf_app/src/experiments/*` | Mechanical ripple + `.qualified` call-site updates |
| `crates/qsf_realtime_server/src/realtime/volition.rs` | Mechanical ripple (event derivation reads `.term`) |
| `crates/qsf_realtime_server/src/realtime/volition_injection.rs` | Packet emits on no-qualifier turns; weighted candidate summaries; threshold |
| `crates/qsf_realtime_server/src/realtime/volition_inspection_capture.rs` | No-winner turn-decision shape |
| `crates/qsf_realtime_server/src/realtime/sideband_turn_injection.rs` | No-qualifier turn wiring + new suppression reason |
| `crates/qsf_realtime_server/src/realtime/volition_tools.rs` | Tool output over the new arbitration outcome |
| `crates/qsf_realtime_server/ui/src/realtime.ts` + `realtime.test.ts` | Parser + panel for no-winner shape and new reason |
| `docs/Experiments/Experiment.WeightedGoalActivation.md` | New durable validation gate |
| `docs/Architecture/Architecture.VolitionSystem.md`, `docs/Experiments/Experiment.CuriosityPersonaSeed.md`, `docs/Handoff.md` | Documentation updates |

## Open questions surfaced (do not resolve silently)

1. **Curation table (Task 6) is a proposal.** The design leaves exact keyword classes as an
   open decision "reviewed as fixture-data diff". Implement the table below, then ask the
   human to review the diff before committing Task 6.
2. **No-qualifier packet text is injected to the model** (a short "no goal qualified" line).
   The design requires the packet builder to emit for activated-but-unqualified turns so the
   suppression is visible in traces; this plan renders a minimal neutral line rather than
   suppressing injection entirely. Flag to the human at Task 10 if they'd rather trace-only.
3. **Whether threshold 4 survives the voice retest** is explicitly revisited in the
   experiment's Results (fixture-data tunable, not code).

---

# Phase 1 — Weighted keyword schema and pure scoring (`qsf_volition`)

Phase outcome: every activation keyword carries a weight class end-to-end, scoring derives
from one strength quantity, persistence compatibility holds, and both fixtures are curated.
Arbitration behavior is *unchanged* until Phase 2.

Phase verification: `cargo test --workspace` green; `cargo clippy --all-targets -- -D warnings`;
`cargo fmt`. **Human review recommended:** the fixture-curation diff (Task 6).

### Task 1: Experiment scaffold (trace contract lives in its durable home)

**Files:**
- Create: `docs/Experiments/Experiment.WeightedGoalActivation.md`

**Interfaces:** none (documentation).

- [ ] **Step 1: Write the scaffold** following the structure of an existing planned experiment
  (see `docs/Experiments/Experiment.Template.md` if present, else mirror
  `Experiment.CuriosityPersonaSeed.md`'s section order). Required sections and content:
  - *Hypothesis*: with coarse keyword weights and a global qualification threshold of 4,
    the natural step-2 persona probe ("…what does that do to the economy?") selects
    `track-the-ai-transition` over `serve-the-present-person`, and stopword-only turns
    produce a recorded `below_qualification_threshold` suppression instead of a
    protected-goal initiative.
  - *Scope*: deterministic lexical layer only; semantic scoring stays in
    `Idea.SemanticGoalActivation.md`.
  - *Trace Completeness Contract*: copy the contract from this plan's section above
    verbatim (it must match the implemented no-winner turn-decision shape and the
    terms-with-weights trace fields).
  - *Procedure* (human voice session):
    1. Start a realtime session via `.\scripts\qsf.ps1 realtime`.
    2. Step-2 persona probe with natural phrasing: "Do you believe machines will replace
       many jobs, and what does that do to the economy?" — expect
       `track-the-ai-transition` to win and, on a rich match, `ProposeExperiment` to fire.
    3. Deliberately weak turn: "For what it's worth, thanks." — expect no initiative and a
       `below_qualification_threshold` suppression in the diagnostics/inspection panel.
    4. Latency parity: confirm the recorded
       `final_transcript_received_to_volition_context_injected` latency shows no
       regression (injection stays at 0 ms as established by the anti-nag work).
  - *Success criteria*: the three observations above, each tied to a trace field.
  - *Results / Interpretation / Final Status*: empty scaffolding, filled by Task 17.
- [ ] **Step 2: Commit**

```powershell
git add docs/Experiments/Experiment.WeightedGoalActivation.md
git commit -m "docs(volition): scaffold weighted-goal-activation experiment gate"
```

### Task 2: `KeywordWeightClass` and `ActivationKeyword` types with legacy-string compat

**Files:**
- Modify: `crates/qsf_volition/src/model.rs` (add types; do NOT change `Goal` yet)
- Modify: `crates/qsf_volition/src/lib.rs` (re-export the two new types alongside the
  existing `model` re-exports)
- Test: inline `#[cfg(test)]` in `model.rs`

**Interfaces:**
- Produces: `KeywordWeightClass { Weak, Normal, Strong }` with `pub fn weight(self) -> u32`
  (1/4/8); `ActivationKeyword { pub term: String, pub weight_class: KeywordWeightClass }`
  with constructors `new/weak/normal/strong` and `pub fn weight(&self) -> u32`. Deserializes
  from both `"word"` (→ Normal) and `{"term": "...", "weight_class": "..."}`. All later
  tasks depend on exactly these names.

- [ ] **Step 1: Write the failing tests** (in `model.rs` tests module):

```rust
#[test]
fn activation_keyword_deserializes_from_legacy_plain_string_as_normal() {
    let keyword: ActivationKeyword = serde_json::from_str("\"economy\"").unwrap();
    assert_eq!(keyword.term, "economy");
    assert_eq!(keyword.weight_class, KeywordWeightClass::Normal);
}

#[test]
fn activation_keyword_roundtrips_weighted_form() {
    let original = ActivationKeyword::strong("automation");
    let json = serde_json::to_string(&original).unwrap();
    assert_eq!(json, r#"{"term":"automation","weight_class":"strong"}"#);
    let reparsed: ActivationKeyword = serde_json::from_str(&json).unwrap();
    assert_eq!(reparsed, original);
}

#[test]
fn weight_class_values_are_one_four_eight() {
    assert_eq!(KeywordWeightClass::Weak.weight(), 1);
    assert_eq!(KeywordWeightClass::Normal.weight(), 4);
    assert_eq!(KeywordWeightClass::Strong.weight(), 8);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p qsf_volition activation_keyword`
Expected: compile error, `ActivationKeyword` not found.

- [ ] **Step 3: Implement** in `model.rs`:

```rust
/// Coarse activation-keyword weight class. Coarse on purpose: consistent curation beats
/// numeric precision at this goal-set size, and the persona stays data-only
/// (DecisionLog 2026-07-04, weighted goal activation).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeywordWeightClass {
    Weak,
    #[default]
    Normal,
    Strong,
}

impl KeywordWeightClass {
    pub fn weight(self) -> u32 {
        match self {
            Self::Weak => 1,
            Self::Normal => 4,
            Self::Strong => 8,
        }
    }
}

/// One activation keyword with its curated weight class. Serializes in the weighted form;
/// deserializes from either the weighted form or a legacy plain string (default Normal) so
/// pre-weight continuity snapshots, reviewed seeds, and live-formed goals still load.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActivationKeyword {
    pub term: String,
    pub weight_class: KeywordWeightClass,
}

impl ActivationKeyword {
    pub fn new(term: impl Into<String>, weight_class: KeywordWeightClass) -> Self {
        Self { term: term.into(), weight_class }
    }
    pub fn weak(term: impl Into<String>) -> Self {
        Self::new(term, KeywordWeightClass::Weak)
    }
    pub fn normal(term: impl Into<String>) -> Self {
        Self::new(term, KeywordWeightClass::Normal)
    }
    pub fn strong(term: impl Into<String>) -> Self {
        Self::new(term, KeywordWeightClass::Strong)
    }
    pub fn weight(&self) -> u32 {
        self.weight_class.weight()
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ActivationKeywordCompat {
    Weighted { term: String, weight_class: KeywordWeightClass },
    Legacy(String),
}

impl<'de> Deserialize<'de> for ActivationKeyword {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match ActivationKeywordCompat::deserialize(deserializer)? {
            ActivationKeywordCompat::Weighted { term, weight_class } => Self { term, weight_class },
            ActivationKeywordCompat::Legacy(term) => Self::normal(term),
        })
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p qsf_volition activation_keyword` (2 tests), then
`cargo test -p qsf_volition weight_class` (1 test).
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_volition/src/model.rs crates/qsf_volition/src/lib.rs
git commit -m "feat(volition): activation keyword weight classes with legacy-string compat"
```

### Task 3: Switch `Goal.activation_keywords` to `Vec<ActivationKeyword>` (behavior-invariant)

This is the mechanical big-bang: the field type changes, every construction site wraps terms
in `ActivationKeyword::normal(..)` (real curation happens in Task 6), and every reader uses
`.term`. Scoring semantics do not change in this task — `matched_keywords` still returns
`Vec<String>` here — so **all existing tests must pass unchanged**.

**Files (construction sites — wrap in `ActivationKeyword::normal(...)` / `vec![...]`):**
- `crates/qsf_volition/src/model.rs:83` (the field: `pub activation_keywords: Vec<ActivationKeyword>`)
- `crates/qsf_volition/src/fixture.rs` (all 11 goal literals; keep terms, all Normal for now)
- `crates/qsf_volition/src/candidate.rs:127` (`into_goal`: `self.activation_keywords.into_iter().map(ActivationKeyword::normal).collect()` — the interim Normal default for live-formed goals; `ProposedGoalCandidate` itself keeps `Vec<String>` and its `json_schema_hint` is untouched — the formation-schema extension is a noted follow-up, not part of this plan)
- Test constructors: `crates/qsf_volition/src/arbitration.rs:296`, `continuity.rs:303,346,383,398,510`, `consolidation.rs:500`, `shaping.rs:225`, `coherence.rs:419`, `crates/qsf_app/src/volition.rs:1152`, `crates/qsf_app/src/experiments/live_goal_formation_and_coherence.rs:316`, `volition_goal_coherence.rs:292`, `crates/qsf_realtime_server/src/realtime/volition_initiative.rs:134`, `volition_tools.rs:722,802,865`

**Files (readers — compare/emit `.term`):**
- `crates/qsf_volition/src/selection.rs:19-29` (`matched_keywords`: compare `term == &keyword.term`, push `keyword.term.clone()`; return type still `Vec<String>` in this task)
- `crates/qsf_volition/src/opportunity.rs:95` (keyword match reads `.term`)
- `crates/qsf_realtime_server/src/realtime/volition.rs:64,83` (event derivation reads `.term`)
- `crates/qsf_app/src/volition.rs:260` (`build_fragment` tags: `goal.activation_keywords.iter().map(|keyword| keyword.term.clone()).collect()`), `:944` (test perturbation retains on `.term`)
- `crates/qsf_volition/src/fixture.rs:452` and `selection.rs:360` (tests read `.term` / compare terms)
- Any other site the compiler flags (`cargo build` is the authoritative list).

**Interfaces:**
- Consumes: `ActivationKeyword` from Task 2.
- Produces: `Goal.activation_keywords: Vec<ActivationKeyword>`. Everything downstream in
  this plan assumes this type.

- [ ] **Step 1: Change the field type and chase the compiler** across the files above.
- [ ] **Step 2: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: PASS with zero test-behavior changes (this task is a refactor).

- [ ] **Step 3: Commit**

```powershell
git add -A
git commit -m "refactor(volition): activation_keywords carry weight classes (all Normal, behavior-invariant)"
```

### Task 4: Weighted scoring — `match_strength`, relevance, `GoalSelection`

**Files:**
- Modify: `crates/qsf_volition/src/selection.rs`
- Modify: `crates/qsf_volition/src/initiative.rs` (`GoalSelection`)
- Modify: `crates/qsf_volition/src/lib.rs` (export `match_strength`, `RELEVANCE_PER_STRENGTH_POINT`)
- Modify (ripple): every `GoalSelection { .. }` literal (same file list as Task 3's test
  constructors) gains `matched_keywords` + `match_strength` and loses `matched_terms`

**Interfaces:**
- Produces:
  - `pub fn matched_keywords(goal: &Goal, input_terms: &[String]) -> Vec<ActivationKeyword>`
    (deduplicated by term, fixture order preserved)
  - `pub fn match_strength(matched: &[ActivationKeyword]) -> u32`
  - `pub const RELEVANCE_PER_STRENGTH_POINT: f64 = 25.0;`
  - `pub fn compute_relevance(goal: &Goal, fixture: &VolitionFixture, matched: &[ActivationKeyword]) -> f64`
    (matched bonus = `match_strength(matched) as f64 * RELEVANCE_PER_STRENGTH_POINT`;
    base-priority and tension bonus unchanged); `compute_relevance_with_salience` same shift
  - `GoalSelection { goal, relevance_score, matched_keywords: Vec<ActivationKeyword>, match_strength: u32, initiative }`
    plus helper `pub fn matched_terms(&self) -> Vec<String>` (maps `.term`) for readers that
    only need strings. `InitiativeProposal.matched_terms` stays `Vec<String>`.
  - `initiative_for_goal` / `initiative_for_effect` take `&[ActivationKeyword]` and build
    `matched_terms` strings internally.

- [ ] **Step 1: Write the failing single-source-of-truth test** (selection.rs tests):

```rust
#[test]
fn relevance_and_match_strength_derive_from_the_same_quantity() {
    let fixture = static_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "clarify-weak-evidence-topic")
        .unwrap();
    let matched = matched_keywords(goal, &normalize_terms("voice memory evidence"));
    let strength = match_strength(&matched);
    assert!(strength > 0);
    let with_match = compute_relevance(goal, &fixture, &matched);
    let without_match = compute_relevance(goal, &fixture, &[]);
    assert_eq!(
        with_match - without_match,
        strength as f64 * RELEVANCE_PER_STRENGTH_POINT,
        "ranked display and qualification must derive from one strength quantity"
    );
}

#[test]
fn goal_selection_carries_matched_keywords_and_strength() {
    let fixture = realtime_seed_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let result = select_goals_ranked("how can you help me", &state, &fixture);
    let selection = result
        .selected
        .iter()
        .find(|s| s.goal.id == "serve-the-present-person")
        .unwrap();
    assert_eq!(selection.match_strength, match_strength(&selection.matched_keywords));
    assert!(!selection.matched_keywords.is_empty());
}
```

- [ ] **Step 2: Run to verify failure**: `cargo test -p qsf_volition relevance_and_match_strength`
  → compile error (`match_strength` not found).
- [ ] **Step 3: Implement** the interface above; in `select_goals_ranked` build selections as:

```rust
let matched = matched_keywords(&goal, &input_terms);
// ... status branches keep using matched.iter().map(|k| k.term.clone()).collect() for
// OmittedGoal.matched_terms (OmittedGoal keeps plain strings) ...
let strength = match_strength(&matched);
let relevance_score = compute_relevance_with_salience(&goal, fixture, &matched, salience);
selected_candidates.push((goal, matched, strength, relevance_score));
// ... sort unchanged ...
.map(|(goal, matched, strength, relevance_score)| GoalSelection {
    initiative: initiative_for_goal(&goal, &matched),
    matched_keywords: matched,
    match_strength: strength,
    relevance_score,
    goal,
})
```

Update `compute_relevance_increases_with_more_matched_terms` and
`compute_relevance_with_salience_adds_salience_to_base` to pass `&[ActivationKeyword]`
(e.g. `vec![ActivationKeyword::normal("memory")]`). Update every reader of the removed
`GoalSelection.matched_terms` field to `selection.matched_terms()` (compiler-driven; known
sites include `crates/qsf_realtime_server/src/realtime/volition_tools.rs` model values and
`qsf_app` experiment renderers).

- [ ] **Step 4: Run**: `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "feat(volition): match strength as the single scoring quantity"
```

### Task 5: Effect selector under the new scale

**Files:**
- Modify: `crates/qsf_volition/src/selection.rs`

**Interfaces:**
- Produces: `pub const STRONG_MATCH_STRENGTH_THRESHOLD: u32 = 8;`,
  `pub const STRONG_MATCH_MIN_DISTINCT_NON_WEAK_TERMS: usize = 2;`,
  `pub fn select_effect_for_goal(goal: &Goal, matched: &[ActivationKeyword]) -> AllowedEffect`.
  Removes `STRONG_MATCH_EFFECT_THRESHOLD` (update its two doc-comment references).

- [ ] **Step 1: Write the failing tests:**

```rust
#[test]
fn propose_experiment_requires_strength_and_two_distinct_non_weak_terms() {
    let fixture = realtime_seed_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "track-the-ai-transition")
        .unwrap();
    // Two Normal terms: strength 8, two non-Weak — fires.
    let two_normal = vec![ActivationKeyword::normal("job"), ActivationKeyword::normal("replace")];
    assert_eq!(select_effect_for_goal(goal, &two_normal), AllowedEffect::ProposeExperiment);
    // A single Strong term scores 8 but is not a rich match — reflects.
    let single_strong = vec![ActivationKeyword::strong("ai")];
    assert_eq!(select_effect_for_goal(goal, &single_strong), AllowedEffect::Reflect);
    // A duplicated Normal term scores 8 but is one distinct term — reflects.
    let duplicated_normal = vec![ActivationKeyword::normal("job"), ActivationKeyword::normal("job")];
    assert_eq!(select_effect_for_goal(goal, &duplicated_normal), AllowedEffect::Reflect);
    // Weak-word combinations never fire regardless of count.
    let weak_pile = vec![
        ActivationKeyword::weak("future"),
        ActivationKeyword::weak("power"),
        ActivationKeyword::weak("what"),
        ActivationKeyword::weak("do"),
    ];
    assert_eq!(select_effect_for_goal(goal, &weak_pile), AllowedEffect::Reflect);
}
```

- [ ] **Step 2: Run to verify failure**: `cargo test -p qsf_volition propose_experiment_requires`
  → FAIL (single Strong currently... the old rule counts terms — expect assertion failure or
  compile error once constants are renamed).
- [ ] **Step 3: Implement:**

```rust
/// Match strength at which a goal that allows `ProposeExperiment` treats the match as a
/// strong thematic hit. Strength alone is not the contract — see the distinct-term rule.
pub const STRONG_MATCH_STRENGTH_THRESHOLD: u32 = 8;
/// A rich match also needs this many distinct non-Weak matched terms: a single Strong
/// keyword scores 8 but is not a rich match.
pub const STRONG_MATCH_MIN_DISTINCT_NON_WEAK_TERMS: usize = 2;

pub fn select_effect_for_goal(goal: &Goal, matched: &[ActivationKeyword]) -> AllowedEffect {
    let allows_propose = goal
        .allowed_effects
        .contains(&AllowedEffect::ProposeExperiment);
    // Distinct by term: `matched_keywords` deduplicates fixture matches today, but this
    // function also takes manually constructed slices, so it must not trust its input.
    let distinct_non_weak = matched
        .iter()
        .filter(|keyword| keyword.weight_class != KeywordWeightClass::Weak)
        .map(|keyword| keyword.term.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    if allows_propose
        && match_strength(matched) >= STRONG_MATCH_STRENGTH_THRESHOLD
        && distinct_non_weak >= STRONG_MATCH_MIN_DISTINCT_NON_WEAK_TERMS
    {
        return AllowedEffect::ProposeExperiment;
    }
    goal.allowed_effects
        .first()
        .copied()
        .unwrap_or(AllowedEffect::Reflect)
}
```

Update the existing effect tests (`track_ai_transition_proposes_experiment_on_rich_transition_match`,
`reflect_only_goal_always_reflects`, `static_fixture_clarify_goal_proposes_on_strong_match_reflects_on_thin`,
`initiative_for_goal_takes_first_effect_on_thin_match`, `initiative_for_effect_builds_proposal_with_correct_effect`)
to build `Vec<ActivationKeyword>` inputs — keep their assertions, they still hold under the
Task 6 curation (verify after Task 6).

- [ ] **Step 4: Run**: `cargo test -p qsf_volition selection` → PASS.
- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_volition/src/selection.rs
git commit -m "feat(volition): rich-match effect gate = strength >= 8 plus two non-weak terms"
```

### Task 6: Fixture threshold field + weight-class curation ⚠ human diff review

**Files:**
- Modify: `crates/qsf_volition/src/model.rs` (`VolitionFixture`)
- Modify: `crates/qsf_volition/src/fixture.rs` (curation)
- Modify (ripple): every `VolitionFixture { tensions, goals }` literal gains
  `arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD`
  (test literals in `arbitration.rs`, `volition_initiative.rs:221`, and any others the
  compiler flags)

**Interfaces:**
- Produces: `pub const DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD: u32 = 4;` and
  `VolitionFixture.arbitration_qualification_threshold: u32` with
  `#[serde(default = "default_arbitration_qualification_threshold")]`. Phase 2 arbitration
  reads this field.

- [ ] **Step 1: Add the field and constant** to `model.rs`:

```rust
/// Minimum match strength a selection needs before it may win arbitration: one Normal
/// keyword qualifies; Weak keywords qualify only in combination (e.g. 4 x Weak).
/// Fixture-level and global by design; per-tier thresholds are deferred until live
/// evidence demands them (DecisionLog 2026-07-04).
pub const DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD: u32 = 4;

fn default_arbitration_qualification_threshold() -> u32 {
    DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionFixture {
    pub tensions: Vec<Tension>,
    pub goals: Vec<Goal>,
    #[serde(default = "default_arbitration_qualification_threshold")]
    pub arbitration_qualification_threshold: u32,
}
```

- [ ] **Step 2: Curate both fixtures.** Replace the all-Normal placeholders from Task 3 with
  (W = `weak`, N = `normal`, S = `strong`):

  `realtime_seed_fixture()`:
  | Goal | Keywords |
  |---|---|
  | respect-persons-boundaries | he W, she W, they W, friend N, boss N, colleague N, family N, private N, personal N, secret S |
  | keep-theses-distinct-from-fact | sure W, certain N, true N, fact N, really W, actually W, know W, prove S, evidence S, why W |
  | serve-the-present-person | what W, how W, can W, please N, help N, want W, need W, do W, tell W, show W, explain N, make W |
  | grow-the-library | remember N, learned N, earlier W, before W, theory N, thesis S, idea W, notice N, pattern N |
  | learn-what-drives-this-person | i W, my W, me W, work N, job N, think W, believe N, feel N, hope N, plan N, project N |
  | track-the-ai-transition | ai S, job N, jobs N, economy S, money N, automation S, future W, country N, power W, technology N, replace N |
  | assemble-world-picture | world N, history N, society N, politics N, system W, change W, trend N, happen W |

  `static_fixture()`:
  | Goal | Keywords |
  |---|---|
  | clarify-weak-evidence-topic | voice N, memory N, evidence N, unclear N, unsettled N |
  | avoid-overstating-impl-status | implemented N, status N, complete N, done W, ready W |
  | resurface-open-thread | continuity S, thread N, revisit N, open W, unresolved N |
  | propose-followup-experiment | experiment N, compare N, perturbation S, fixture N, prototype N |

  Rationale anchors (assert in Step 3 tests): the design's live-evidence probes must come out
  right — "…what does that do to the economy?" gives `serve-the-present-person` strength 2
  (`what`+`do`, both Weak) and `track-the-ai-transition` strength 16 (`replace`+`jobs`+`economy`);
  "how can you help me" keeps `serve-the-present-person` qualified at strength 6 (existing
  adapter tests depend on this phrase producing a winner).

- [ ] **Step 3: Write invariant + curation tests** (fixture.rs tests):

```rust
#[test]
fn fixture_qualification_thresholds_are_positive_and_default() {
    for fixture in [realtime_seed_fixture(), static_fixture()] {
        assert_eq!(
            fixture.arbitration_qualification_threshold,
            DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD
        );
        assert!(fixture.arbitration_qualification_threshold > 0);
    }
}

#[test]
fn design_probe_strengths_come_out_as_specified() {
    let fixture = realtime_seed_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let probe = "Do you believe machines will replace many jobs, and what does that do to the economy?";
    let ranked = select_goals_ranked(probe, &state, &fixture);
    let strength_of = |id: &str| {
        ranked
            .selected
            .iter()
            .find(|s| s.goal.id == id)
            .map(|s| s.match_strength)
            .unwrap_or(0)
    };
    assert!(strength_of("serve-the-present-person") < fixture.arbitration_qualification_threshold);
    assert!(strength_of("track-the-ai-transition") >= fixture.arbitration_qualification_threshold);
    // Idiom stopword: "for what it's worth" alone leaves the protected goal unqualified.
    let idiom = select_goals_ranked("for what it's worth, thanks", &state, &fixture);
    assert!(
        idiom
            .selected
            .iter()
            .all(|s| s.match_strength < fixture.arbitration_qualification_threshold)
    );
}
```

- [ ] **Step 4: Run**: `cargo test --workspace` → PASS. If any existing selection/adapter test
  now fails, the curation table (not the test) is the first suspect — re-check strengths
  before touching test assertions, and record any deliberate assertion change in the commit
  message.
- [ ] **Step 5: STOP — ask the human to review the fixture-curation diff** (design open
  decision: "reviewed as fixture-data diff, not code").
- [ ] **Step 6: Commit**

```powershell
git add -A
git commit -m "feat(volition): qualification threshold on fixtures + curated keyword weights"
```

### Task 7: Persistence compatibility — schema bumps + legacy JSON regression

**Files:**
- Modify: `crates/qsf_volition/src/continuity.rs`
- Test: inline tests in `continuity.rs`

**Interfaces:**
- Produces: `VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION: u16 = 3`,
  `REVIEWED_VOLITION_SEED_SCHEMA_VERSION: u16 = 2`; `ReviewedVolitionSeed::load_or_upgrade`
  now accepts older versions and upgrades (bails only on *newer*), mirroring the snapshot
  loader; add `upgrade_schema_version` to `ReviewedVolitionSeed`.

- [ ] **Step 1: Write the failing regression tests:**

```rust
#[test]
fn legacy_snapshot_with_string_activation_keywords_loads_and_upgrades() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("volition-state.json");
    let mut snapshot = sample_snapshot();
    // Persisted state carries full Goal values in accepted_candidates; a legacy snapshot
    // stored their activation_keywords as plain strings.
    let goal = Goal {
        id: "legacy-goal".to_string(),
        title: "Legacy".to_string(),
        summary: "Legacy".to_string(),
        tension_ids: vec!["knowledge-stewardship".to_string()],
        status: GoalStatus::Accepted,
        scope: GoalScope::Session,
        base_priority: 70,
        activation_keywords: vec![ActivationKeyword::normal("legacy")],
        allowed_effects: vec![AllowedEffect::Reflect],
        satisfaction_condition_summary: "test".to_string(),
        evidence_refs: vec!["tests".to_string()],
        estimated_tokens: 10,
        source_reference: "tests".to_string(),
    };
    snapshot.state.accepted_candidates.insert(goal.id.clone(), goal);
    let mut value = serde_json::to_value(&snapshot).unwrap();
    value["schema_version"] = serde_json::json!(2);
    value["state"]["accepted_candidates"]["legacy-goal"]["activation_keywords"] =
        serde_json::json!(["legacy", "keywords"]);
    std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

    let loaded = VolitionContinuitySnapshot::load_or_upgrade(&path).unwrap();
    assert_eq!(loaded.schema_version, VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION);
    let upgraded = &loaded.state.accepted_candidates["legacy-goal"].activation_keywords;
    assert!(upgraded.iter().all(|k| k.weight_class == KeywordWeightClass::Normal));
    assert_eq!(upgraded[0].term, "legacy");
}

#[test]
fn legacy_reviewed_seed_with_string_keywords_loads_and_upgrades() {
    // Same shape: schema_version 1 + accepted_goals with string activation_keywords.
    // Build a v2 seed via ReviewedVolitionSeed, serialize, rewrite schema_version to 1 and
    // one goal's activation_keywords to ["continuity"], persist, then:
    // let loaded = ReviewedVolitionSeed::load_or_upgrade(&path).unwrap();
    // assert_eq!(loaded.schema_version, REVIEWED_VOLITION_SEED_SCHEMA_VERSION);
    // assert the keyword upgraded to Normal.
}

#[test]
fn live_formed_goal_with_one_model_keyword_meets_default_threshold() {
    // Interim contract from the design's Compatibility Notes: a live-formed candidate's
    // model-supplied keywords default to Normal (4) through `into_goal`, so one keyword
    // clears the default threshold (4). Pinned through the real candidate->goal path so a
    // later formation-schema change is a deliberate break, not an accident.
    let fixture = realtime_seed_fixture();
    let candidate: ProposedGoalCandidate = serde_json::from_value(serde_json::json!({
        "id": "follow-quantum-thread",
        "title": "Follow the quantum thread",
        "summary": "Track the person's interest in quantum computing.",
        "tension_ids": ["person-curiosity"],
        "scope": "session",
        "base_priority": 70,
        "allowed_effects": ["reflect"],
        "satisfaction_condition_summary": "The thread was followed.",
        "proposal_evidence": ["turn: mentioned quantum computing"],
        "source_description": "test",
        "activation_keywords": ["quantum"]
    }))
    .unwrap();
    // `into_goal` is pub(crate); this test lives in the qsf_volition crate.
    let goal = candidate.into_goal(EvidenceRef::try_new("test-acceptance").unwrap());
    let matched = matched_keywords(&goal, &normalize_terms("tell me about quantum computing"));
    assert!(
        match_strength(&matched) >= fixture.arbitration_qualification_threshold,
        "one model-supplied (Normal-default) keyword must qualify at the default threshold"
    );
    assert!(matched.iter().all(|k| k.weight_class == KeywordWeightClass::Normal));
}
```

(Fill in the second test body concretely; the comment shows the required steps.)

- [ ] **Step 2: Run to verify failure**: `cargo test -p qsf_volition legacy_` → FAIL (version
  constants unchanged; reviewed-seed loader rejects version 1... note the string-keyword parse
  already succeeds thanks to Task 2's compat reader — the failures here are version-handling).
- [ ] **Step 3: Implement** the version bumps and the reviewed-seed upgrade path (copy the
  snapshot loader's `> current → bail; else upgrade` pattern; keep the `!=` rejection only
  for *newer* versions).
- [ ] **Step 4: Run**: `cargo test -p qsf_volition continuity` → PASS. Also run
  `cargo test -p qsf_app volition_continuity` (its JSON fixtures at
  `crates/qsf_app/src/experiments/volition_continuity.rs:672` must still load).
- [ ] **Step 5: Phase 1 gate**: `cargo test --workspace`;
  `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.
- [ ] **Step 6: Commit**

```powershell
git add -A
git commit -m "feat(volition): schema v3/v2 bumps with legacy keyword compatibility"
```

---

# Phase 2 — Qualification gate in arbitration, traces, realtime adapter, UI

Phase outcome: sub-threshold selections cannot win arbitration; no-qualifier turns are quiet
and fully traced; the UI panel shows the no-winner shape and the new suppression reason.

Phase verification: `cargo test --workspace`; clippy; fmt; `npm run check` + `npm run fmt`
in `crates/qsf_realtime_server/ui`. **Human testing recommended** at phase end (Task 17's
voice protocol can be dry-run early).

### Task 8: Arbitration qualification partition

**Files:**
- Modify: `crates/qsf_volition/src/arbitration.rs`
- Modify: `crates/qsf_volition/src/lib.rs` (export `ModeArbitrationOutcome`,
  `ArbitrationOutcome`, `BelowThresholdCandidate`)

**Interfaces:**
- Produces:

```rust
/// A selection that activated but did not reach the qualification threshold. It never
/// entered the arbitration sort — "activated but not eligible to arbitrate" is a distinct
/// outcome from "qualified but lost on tier". Tests assert the structured fields, not
/// `reason`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BelowThresholdCandidate {
    pub selection: GoalSelection,
    pub match_strength: u32,
    pub threshold: u32,
    /// Rendered convenience reason, e.g. "match strength 2 below qualification threshold 4".
    pub reason: String,
}

/// Full mode-aware arbitration outcome: the qualification partition plus (if any selection
/// qualified) the existing sorted result. `qualified: None` with non-empty
/// `below_threshold` is a no-winner turn: volition stays quiet and the trace says why.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModeArbitrationOutcome {
    pub mode: Mode,
    pub qualification_threshold: u32,
    pub qualified: Option<ModeArbitrationResult>,
    pub below_threshold: Vec<BelowThresholdCandidate>,
}

/// Neutral-mode analogue wrapping ArbitrationResult; `arbitrate` still delegates to
/// `arbitrate_with_mode(.., Mode::Neutral)` — one sort implementation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArbitrationOutcome {
    pub qualification_threshold: u32,
    pub qualified: Option<ArbitrationResult>,
    pub below_threshold: Vec<BelowThresholdCandidate>,
}

pub fn arbitrate_with_mode(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
    mode: Mode,
) -> Option<ModeArbitrationOutcome>; // None only for empty input

pub fn arbitrate(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
) -> Option<ArbitrationOutcome>;
```

  `ModeArbitrationResult` / `ArbitrationResult` / `ModeArbitrationLoser` / `BiasOutcome` keep
  their exact current shapes — losers are only candidates that reached the sort. Order the
  `below_threshold` list by `match_strength` descending then `goal_id` ascending (stable and
  legible in traces). Partition rule:
  `selection.match_strength >= fixture.arbitration_qualification_threshold`.

- [ ] **Step 1: Write the failing tests** (arbitration.rs tests; the existing
  `make_goal_for_arbitration` helper gains a `match_strength: u32` parameter and sets
  `matched_keywords: vec![ActivationKeyword::normal("test")]` for strength-4 defaults, or a
  `Vec<ActivationKeyword>` parameter — pick one and update all its callers):

```rust
#[test]
fn sub_threshold_protected_goal_loses_to_qualified_malleable_goal() {
    let fixture = VolitionFixture {
        tensions: vec![make_tension("protected-tension", 1), make_tension("band-tension", 5)],
        goals: vec![],
        arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
    };
    // Protected goal matched only weak evidence (strength 2 < 4).
    let protected = make_goal_for_arbitration_with_match(
        "protected-goal",
        vec!["protected-tension".to_string()],
        100,
        vec![ActivationKeyword::weak("what"), ActivationKeyword::weak("do")],
    );
    // Malleable goal with a qualified match (strength 16).
    let malleable = make_goal_for_arbitration_with_match(
        "malleable-goal",
        vec!["band-tension".to_string()],
        90,
        vec![
            ActivationKeyword::normal("replace"),
            ActivationKeyword::normal("jobs"),
            ActivationKeyword::strong("economy"),
        ],
    );
    let outcome = arbitrate_with_mode(vec![protected, malleable], &fixture, Mode::Neutral).unwrap();
    let result = outcome.qualified.expect("malleable goal qualified");
    assert_eq!(result.winner.goal.id, "malleable-goal");
    assert_eq!(outcome.below_threshold.len(), 1);
    let below = &outcome.below_threshold[0];
    assert_eq!(below.selection.goal.id, "protected-goal");
    assert_eq!(below.match_strength, 2);
    assert_eq!(below.threshold, DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD);
    assert!(result.losers.is_empty(), "below-threshold candidates never reach the sort");
}

#[test]
fn qualified_protected_goal_still_wins_on_tier() {
    // Same fixture; both goals qualified (strength >= 4) -> protected tier-1 goal wins,
    // malleable goal appears in losers (not below_threshold).
}

#[test]
fn no_qualified_selection_yields_no_winner_with_recorded_partition() {
    // Two selections, both weak-only (strength < 4) -> Some(outcome) with qualified: None
    // and both selections in below_threshold; empty input still returns None.
}
```

- [ ] **Step 2: Run**: `cargo test -p qsf_volition arbitration` → compile FAIL.
- [ ] **Step 3: Implement** the partition inside `arbitrate_with_mode` before the existing
  sort (the sort body itself does not change), and map `arbitrate` through the neutral
  wrapper as today. Update the existing arbitration tests in this file mechanically
  (`.unwrap()` → `.unwrap().qualified.unwrap()` where a winner is expected; give their
  selections strength ≥ 4 via `ActivationKeyword::normal("test")` so tie-break tests keep
  testing tie-breaks, updated, not removed).
- [ ] **Step 4: Run**: `cargo test -p qsf_volition` → PASS (only `qsf_volition`; the
  workspace still has broken call sites — fixed next task).
- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_volition
git commit -m "feat(volition): qualification partition gates arbitration wins"
```

### Task 9: Update every arbitration call site + paraphrase-robustness tests

**Files (mechanical `.qualified` chase — the compiler is the checklist):**
- `crates/qsf_volition/src/continuity.rs:436`, `shaping.rs` tests
- `crates/qsf_app/src/volition.rs` (arbitrate tests ~1132-1402, mode tests ~2120-2324)
- `crates/qsf_app/src/experiments/volition_arbitration_conflict.rs`,
  `volition_bounded_initiative_execution.rs`, `volition_mode_bias.rs`
- `crates/qsf_realtime_server/src/realtime/volition_injection.rs` tests,
  `volition_inspection_capture.rs` tests, `volition_tools.rs:160`,
  `sideband_turn_injection.rs:171` (temporary: take `.and_then(|o| o.qualified)` here; the
  real no-qualifier wiring lands in Task 11)

**Interfaces:**
- Consumes: Task 8's outcome types.

- [ ] **Step 1: Chase the compiler** through the list above. Where a test asserted "winner
  exists", unwrap `qualified`; where an experiment serializes the arbitration value, serialize
  the outcome (richer JSON is fine — experiments' reports are regenerated artifacts).
- [ ] **Step 2: Add the paraphrase-robustness reducer tests** (selection/arbitration boundary,
  in `crates/qsf_volition/src/arbitration.rs` tests, using real fixtures end-to-end):

```rust
#[test]
fn same_meaning_in_three_wordings_selects_the_same_winner() {
    let fixture = realtime_seed_fixture();
    let state = VolitionState::from_fixture(&fixture);
    for probe in [
        "Do you believe machines will replace many jobs, and what does that do to the economy?",
        "Will automation replace jobs and reshape the economy?",
        "I wonder how many jobs the economy loses when machines replace people.",
    ] {
        let ranked = select_goals_ranked(probe, &state, &fixture);
        let outcome = arbitrate_with_mode(ranked.selected, &fixture, Mode::Neutral).unwrap();
        assert_eq!(
            outcome.qualified.expect("qualified winner").winner.goal.id,
            "track-the-ai-transition",
            "wording: {probe}"
        );
    }
}

#[test]
fn stray_idiom_prefix_does_not_flip_the_winner() {
    let fixture = realtime_seed_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let plain = "Will automation replace jobs and reshape the economy?";
    let prefixed = format!("For what it's worth, {plain}");
    let winner = |text: &str| {
        let ranked = select_goals_ranked(text, &state, &fixture);
        arbitrate_with_mode(ranked.selected, &fixture, Mode::Neutral)
            .unwrap()
            .qualified
            .unwrap()
            .winner
            .goal
            .id
    };
    assert_eq!(winner(plain), winner(&prefixed));
}

#[test]
fn all_stopword_turn_yields_no_winner_with_recorded_reason() {
    let fixture = realtime_seed_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let ranked = select_goals_ranked("for what it's worth, thanks", &state, &fixture);
    let outcome = arbitrate_with_mode(ranked.selected, &fixture, Mode::Neutral).unwrap();
    assert!(outcome.qualified.is_none());
    assert!(!outcome.below_threshold.is_empty());
    for below in &outcome.below_threshold {
        assert!(below.match_strength < outcome.qualification_threshold);
    }
}
```

- [ ] **Step 3: Run**: `cargo test --workspace` → PASS.
- [ ] **Step 4: Commit**

```powershell
git add -A
git commit -m "refactor: arbitration call sites read the qualification outcome; paraphrase probes pinned"
```

### Task 10: Suppression reason + turn packet for unqualified turns

**Files:**
- Modify: `crates/qsf_volition/src/continuity.rs` (`VolitionSuppressionReason`)
- Modify: `crates/qsf_realtime_server/src/realtime/volition_injection.rs`

**Interfaces:**
- Produces:
  - `VolitionSuppressionReason::BelowQualificationThreshold` (serde:
    `below_qualification_threshold`)
  - `VolitionCandidateSummary` gains `#[serde(default)] pub matched_keywords: Vec<ActivationKeyword>`
    and `#[serde(default)] pub match_strength: u32` (empty/0 for status-filtered candidates)
  - `VolitionSelectorSummary` gains `pub selected_match_details: Vec<VolitionSelectedMatchDetail>`
    where `VolitionSelectedMatchDetail { goal_id: String, matched_keywords: Vec<ActivationKeyword>, match_strength: u32 }`
  - `VolitionTurnPacketSummary` (and the mirrored fields on `VolitionContextInjectionTrace`)
    gain `pub qualification_threshold: u32` and
    `pub below_threshold_candidates: Vec<VolitionCandidateSummary>` (also folded into
    `omitted_or_suppressed_candidates` with `reason_category: "below_qualification_threshold"`,
    never `lower_arbitration_rank`)
  - `build_volition_turn_context_packet` takes `arbitration: Option<ModeArbitrationOutcome>`
    and returns a packet when **any** of: a qualified winner exists (and selection non-empty),
    `below_threshold` is non-empty, or `declined_candidates` is non-empty.

- [ ] **Step 1: Write the failing tests** (volition_injection.rs tests):

```rust
#[test]
fn unqualified_turn_emits_packet_with_below_threshold_categorization() {
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot { state: state.clone(), fixture: fixture.clone() };
    let ranked = select_goals_ranked("for what it's worth, thanks", &state, &fixture);
    let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    assert!(outcome.as_ref().unwrap().qualified.is_none(), "precondition: no qualifier");
    let packet = build_volition_turn_context_packet(
        &snapshot, &ranked, outcome, &[], ShapingIntensity::None,
        "stable-baseline-hash".to_string(), None, &[],
    )
    .expect("activated-but-unqualified turn must emit a packet");
    assert!(packet.summary.arbitration_result.is_none());
    assert!(!packet.summary.below_threshold_candidates.is_empty());
    for candidate in &packet.summary.below_threshold_candidates {
        assert_eq!(candidate.reason_category, "below_qualification_threshold");
        assert_ne!(candidate.reason_category, "lower_arbitration_rank");
        assert!(!candidate.matched_keywords.is_empty());
        assert!(candidate.match_strength < packet.summary.qualification_threshold);
    }
    assert!(packet.text.contains("No goal qualified"));
    assert!(!packet.text.contains("Active goal:"));
}

#[test]
fn winner_turn_still_records_below_threshold_candidates_and_threshold() {
    // "how can you help me" + a weak-only co-activated goal: qualified winner present AND
    // any sub-threshold selections listed with matched keywords + strength; threshold on
    // the summary equals the fixture's.
}

#[test]
fn selected_match_details_cover_every_selected_goal() {
    // selector_output.selected_match_details has one entry per ranked.selected, each with
    // match_strength == match_strength(&matched_keywords).
}
```

- [ ] **Step 2: Run to verify failure**: `cargo test -p qsf_realtime_server volition_injection`.
- [ ] **Step 3: Implement.** Emission rule at the top of the builder:

```rust
let qualified = arbitration
    .as_ref()
    .and_then(|outcome| outcome.qualified.clone())
    .filter(|_| !ranked.selected.is_empty());
let below_threshold = arbitration
    .as_ref()
    .map(|outcome| outcome.below_threshold.clone())
    .unwrap_or_default();
let qualification_threshold = arbitration
    .as_ref()
    .map(|outcome| outcome.qualification_threshold)
    .unwrap_or(snapshot.fixture.arbitration_qualification_threshold);
if qualified.is_none() && below_threshold.is_empty() && declined_candidates.is_empty() {
    return None;
}
```

  Render, for the no-qualifier case (no Active-goal section, mirroring the coherence-only
  branch and keeping the same simulation framing and guardrail):

```text
Simulated volition context for this turn (internal state only; not a claim of real desire or consciousness).
No goal qualified to lead this turn: {n} candidate(s) matched only below the qualification threshold ({threshold}). Volition stays quiet this turn.
{coherence_section}Guidance: Respond naturally to the person. Do not state internal goals as literal desires and do not take any external action.
```

  Declare the `dynamic volition turn packet` injected layer whenever the packet carries either
  the Active-goal section or the no-qualifier section (layers stay derived from rendered text,
  preserving the A6 honesty rule). Keep `lower_arbitration_rank` for sort losers only; extend
  `build_candidate_summaries` to take the outcome and append below-threshold entries with
  matched keywords + strength (losers likewise get theirs from `loser.selection`).

- [ ] **Step 4: Run**: `cargo test -p qsf_realtime_server` → PASS.
- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "feat(realtime): turn packet records qualification partition; unqualified turns emit quiet packet"
```

### Task 11: No-winner turn decision + sideband wiring

**Files:**
- Modify: `crates/qsf_realtime_server/src/realtime/volition_inspection_capture.rs`
- Modify: `crates/qsf_realtime_server/src/realtime/sideband_turn_injection.rs`

**Interfaces:**
- Produces (wire shape — the UI parser in Task 12 must match exactly):

```rust
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionTurnWinnerSummary {
    pub winner_goal_id: String,
    pub winner_goal_title: String,
    pub winner_effective_tier: u8,
    pub winner_biased_tier: u8,
    pub protected_tier_active: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionBelowThresholdSummary {
    pub goal_id: String,
    pub goal_title: String,
    pub matched_keywords: Vec<ActivationKeyword>,
    pub match_strength: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionTurnDecisionSummary {
    /// None on a no-qualifier turn — the dedicated no-winner turn-decision outcome.
    pub winner: Option<VolitionTurnWinnerSummary>,
    pub qualification_threshold: u32,
    pub below_threshold: Vec<VolitionBelowThresholdSummary>,
    pub mode_bias_outcomes: Vec<VolitionModeBiasOutcome>,
    pub selected_goal_ids: Vec<String>,
    pub omitted_or_suppressed_goal_ids: Vec<String>,
    pub shaping_intensity: ShapingIntensity,
    pub last_initiative_output_kind: Option<String>,
    pub last_initiative_surfaced: bool,
    pub last_initiative_suppression_reason: Option<VolitionSuppressionReason>,
    pub last_initiative_rendered_line_present: bool,
}
```

  `build_volition_turn_decision_summary` takes `&ModeArbitrationOutcome` (plus the existing
  args, with `initiative_output: Option<&InitiativeOutput>`) and fills the winner block and
  `mode_bias_outcomes` from `qualified`; when `qualified` is `None`, `mode_bias_outcomes` is
  empty (below-threshold candidates never enter the arbitration sort, so there is no
  `ModeArbitrationResult` to derive outcomes from). Sideband rules:
  - Qualified-winner path: exactly today's behavior (initiative event applied, bounded
    initiative trace, decision with winner block).
  - No-qualifier path (`qualified: None`, `below_threshold` non-empty): **no**
    `InitiativeExecuted` event, **no** bounded-initiative trace (reserved for executed
    initiatives), `previous_turn_surfaced_goal_id = None`, packet emitted (Task 10), and the
    inspection capture carries the no-winner decision with
    `last_initiative_suppression_reason: Some(BelowQualificationThreshold)`,
    `last_initiative_output_kind: None`, `surfaced: false`,
    `rendered_line_present: false`, `shaping_intensity: None`.
  - Keep the existing `debug_assert!` (initiative mutation ⇒ packet) valid.

- [ ] **Step 1: Write the failing tests:**

```rust
// volition_inspection_capture.rs
#[test]
fn no_qualifier_turn_builds_no_winner_decision_with_reason_and_threshold() {
    let fixture = realtime_seed_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let ranked = select_goals_ranked("for what it's worth, thanks", &state, &fixture);
    let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral).unwrap();
    assert!(outcome.qualified.is_none());
    let decision = build_volition_turn_decision_summary(
        &ranked, &outcome, None, false,
        Some(VolitionSuppressionReason::BelowQualificationThreshold),
        false, ShapingIntensity::None,
    );
    assert!(decision.winner.is_none());
    assert_eq!(decision.qualification_threshold, fixture.arbitration_qualification_threshold);
    assert!(!decision.below_threshold.is_empty());
    for below in &decision.below_threshold {
        assert_eq!(below.match_strength,
            below.matched_keywords.iter().map(|k| k.weight()).sum::<u32>());
    }
    assert!(decision.mode_bias_outcomes.is_empty());
    assert_eq!(
        decision.last_initiative_suppression_reason,
        Some(VolitionSuppressionReason::BelowQualificationThreshold)
    );
    assert!(decision.last_initiative_output_kind.is_none());
}
```

  Plus: update the existing capture tests to the new builder signature/winner block, and add
  a serde test asserting the wire field `"below_qualification_threshold"` appears when that
  reason serializes.

- [ ] **Step 2: Run to verify failure**, **Step 3: implement** (builder + both sideband
  branches), **Step 4:** `cargo test -p qsf_realtime_server` → PASS, then
  `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "feat(realtime): no-winner turn decision with below_qualification_threshold suppression"
```

### Task 12: Realtime select tool over the new outcome

**Files:**
- Modify: `crates/qsf_realtime_server/src/realtime/volition_tools.rs`

**Interfaces:**
- Consumes: `ModeArbitrationOutcome` from Task 8.
- Produces: tool JSON `"arbitration"` value reports either the qualified winner (as today)
  or `{"status": "no_qualified_winner", "qualification_threshold": N, "below_threshold": [...]}`
  with per-candidate `goal_id`, `match_strength`, `matched_keywords`. Selected-goal values
  include `match_strength` and matched keywords with weight classes.

- [ ] **Step 1: Write a failing test** (in this file's tests, following its existing
  patterns): a weak-only query returns `"no_qualified_winner"` with the threshold, and a
  qualified query keeps the current winner shape.
- [ ] **Step 2–4: Red → implement (`model_arbitration_value` + `select_model_goal_value`) →
  green** (`cargo test -p qsf_realtime_server volition_tools`).
- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_realtime_server/src/realtime/volition_tools.rs
git commit -m "feat(realtime): select tool reports qualification outcome"
```

### Task 13: Artifact-parsing trace verification

**Files:**
- Modify: `crates/qsf_realtime_server/src/realtime/volition_injection.rs` (tests module)

**Interfaces:** consumes serialized `VolitionContextInjectionTrace` JSON only (untyped
`serde_json::Value` — this is the artifact-boundary check, deliberately not reusing the Rust
types for field access).

- [ ] **Step 1: Write the test:**

```rust
#[test]
fn serialized_trace_satisfies_the_weighted_activation_trace_contract() {
    // Build two packets end-to-end (one qualified-winner turn: "how can you help me";
    // one no-qualifier turn: "for what it's worth, thanks"), wrap each with
    // build_volition_context_injection_trace, serialize with serde_json::to_value.
    // For each trace Value, assert:
    // 1. trace["qualification_threshold"] is a positive integer T.
    // 2. Every entry in trace["omitted_or_suppressed_candidates"] with reason_category
    //    "below_qualification_threshold" has matched_keywords (term + weight_class) whose
    //    recomputed strength (weak=1, normal=4, strong=8 — recomputed HERE from the wire
    //    strings, not via KeywordWeightClass) equals its match_strength, and
    //    match_strength < T.
    // 3. Every entry in selector_output["selected_match_details"] recomputes the same way.
    // 4. Winner turn: trace["arbitration_result"] is present; no-qualifier turn: it is
    //    null AND at least one below_qualification_threshold candidate exists.
    // 5. No candidate carries reason_category "lower_arbitration_rank" unless
    //    arbitration_result is present.
    // Contract scope check: a trusted turn with no lexical activation at all (no selected,
    // below-threshold, or declined candidate) gets None from
    // build_volition_turn_context_packet — such turns are outside the trace contract.
}
```

- [ ] **Step 2–3: Red → implement the test body → green**
  (`cargo test -p qsf_realtime_server trace_contract` — name accordingly).
- [ ] **Step 4: Mark the automated trace criteria in
  `docs/Experiments/Experiment.WeightedGoalActivation.md` as implemented** (the human-review
  criteria stay unchecked until Task 17).
- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "test(realtime): artifact-parsing verification of the weighted-activation trace contract"
```

### Task 14: UI — parser and volition panel learn the no-winner shape

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts`
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces (mirror Task 11's wire shape exactly):**
- `VolitionSuppressionReason` union gains `"below_qualification_threshold"` (type at line ~90
  and `isVolitionSuppressionReason` at ~707).
- New interfaces `VolitionTurnWinnerSummary { winnerGoalId; winnerGoalTitle; winnerEffectiveTier; winnerBiasedTier; protectedTierActive }`
  and `VolitionBelowThresholdSummary { goalId; goalTitle; matchedKeywords: Array<{ term: string; weightClass: "weak" | "normal" | "strong" }>; matchStrength: number }`.
- `VolitionTurnDecisionSummary` becomes
  `{ winner: VolitionTurnWinnerSummary | null; qualificationThreshold: number; belowThreshold: VolitionBelowThresholdSummary[]; ...rest unchanged }`.
- Guard + converter (`isVolitionTurnDecisionSummary`, `convertVolitionTurnDecisionSummary`)
  accept `winner: null`; panel view-model (~line 1032) renders either the winner rows as
  today or, for no-winner: headline row
  `value: "no goal qualified (threshold ${decision.qualificationThreshold})"` plus a
  below-threshold row listing `goalId (strength N: term/class, ...)`. Keep view logic in the
  pure view-model function, not components.

- [ ] **Step 1: Write failing tests** in `realtime.test.ts`: (a) parser accepts a no-winner
  wire decision (`winner: null`, `below_threshold` populated,
  `last_initiative_suppression_reason: "below_qualification_threshold"`) and converts it;
  (b) parser still accepts the winner shape (update existing fixtures at ~482/699/834 to the
  nested `winner` block); (c) panel view-model renders the no-winner headline and the
  suppression reason row.
- [ ] **Step 2: Run to verify failure**: `npm test` (from `crates/qsf_realtime_server/ui`).
- [ ] **Step 3: Implement**, **Step 4: Run**: `npm test`, then `npm run check` and
  `npm run fmt`.
- [ ] **Step 5: Phase 2 gate**: `cargo test --workspace`;
  `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.
- [ ] **Step 6: Commit**

```powershell
git add crates/qsf_realtime_server/ui
git commit -m "feat(ui): volition panel renders no-winner decisions and the qualification reason"
```

---

# Phase 3 — Documentation and the human voice gate

### Task 15: Architecture and persona-experiment updates

**Files:**
- Modify: `docs/Architecture/Architecture.VolitionSystem.md`
- Modify: `docs/Experiments/Experiment.CuriosityPersonaSeed.md`

- [ ] **Step 1: Architecture** — in the selection/arbitration mechanics section (~lines
  23–58) and the Implementation Status section: describe weight classes, `match_strength`
  as the single scoring quantity, the qualification threshold as a fixture-level constant,
  the arbitration partition (qualification gates the win; tier ordering unchanged among
  qualified goals; no exemption for protected tiers — protection still governs cancellation,
  not speaking), and the no-winner turn decision + `below_qualification_threshold`
  suppression. Name behaviors, never this plan's phase numbers. Refresh `Last reviewed:`.
- [ ] **Step 2: Persona experiment** — close the *Keyword tuning* open item (~line 246):
  point at `Design.WeightedGoalActivation.md` and state that broad keywords are now Weak
  by curation and that the step-2 gate is retested under the new mechanics via
  `Experiment.WeightedGoalActivation.md`.
- [ ] **Step 3: Commit**

```powershell
git add docs/Architecture/Architecture.VolitionSystem.md docs/Experiments/Experiment.CuriosityPersonaSeed.md
git commit -m "docs(volition): weighted activation mechanics in architecture; close keyword-tuning open item"
```

### Task 16: Handoff update

**Files:**
- Modify: `docs/Handoff.md`

- [ ] **Step 1:** Set *Now* to: run the `Experiment.WeightedGoalActivation.md` voice protocol
  (one line + link; keep the existing formation-retest as the alternate if still open).
  Rewrite in place; keep the two-minute budget.
- [ ] **Step 2: Commit**

```powershell
git add docs/Handoff.md
git commit -m "docs: handoff points at the weighted-activation voice gate"
```

### Task 17: 🧪 HUMAN GATE — voice retest

**Files:**
- Modify: `docs/Experiments/Experiment.WeightedGoalActivation.md` (Results,
  Interpretation, Final Status)
- Possibly modify: `crates/qsf_volition/src/fixture.rs` (threshold/curation tuning only —
  fixture data, not code)

- [ ] **Step 1: Human runs the protocol** from Task 1 (step-2 natural probe →
  `track-the-ai-transition` wins and `ProposeExperiment` fires on a rich match; "for what
  it's worth, thanks" → `below_qualification_threshold` visible in diagnostics; injection
  latency unchanged at 0 ms).
- [ ] **Step 2: Record Results** (Observed / Interpreted / Uncertain), including whether the
  threshold default of 4 survives (open decision — revisit here, tune as fixture data if not).
- [ ] **Step 3:** If results promote anything to settled design (e.g. threshold value), add
  the decision-log entry; update Handoff *Now* to the next recommendation.
- [ ] **Step 4: Commit**

```powershell
git add -A
git commit -m "docs(volition): weighted-activation voice retest results"
```

### Task 18: Plan retirement

- [ ] Per repo convention (plans are ephemeral): once all phases are verified and documented,
  delete `docs/Plans/Plan.WeightedGoalActivation.md` and
  `docs/Plans/Design.WeightedGoalActivation.md` (the durable content now lives in the
  decision log, architecture doc, and experiment). Keep `Idea.SemanticGoalActivation.md`.

```powershell
git rm docs/Plans/Plan.WeightedGoalActivation.md docs/Plans/Design.WeightedGoalActivation.md
git commit -m "docs: retire implemented weighted-goal-activation plan and design"
```
