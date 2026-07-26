# Gate feasibility census — per-goal auditor gate for goal-relevance v1

Date: 2026-07-25
Measures: `docs/Plans/Design.GoalRelevancePanelLabeling.md`, *The gate* → *Gate conditions, per hard
slice and per split*.
Source data: `runs/goalrel-production/` (gitignored, local only) — `generation-output.jsonl`,
`label-mini.jsonl`, `label-fable.jsonl`, 114 utterances × 7 goals = 798 pairs, `goalrel-label-v1`.
Status: evidence for external review. No code changed; nothing was spent.

## Verdict

**The per-goal gate is not viable as designed, and the reason is structural rather than statistical.**

Each hard slice was generated conditioned on **exactly two roster goals**:

| Hard slice | Conditioning goals | Utterances |
|---|---|---|
| negation (explicit + implicit) | *Keep theses distinct from fact* (8), *Serve the present person* (8) | 16 |
| quoted_speech | *Grow the library* (6), *Learn what drives this person* (6) | 12 |
| hypothetical | *Track the AI transition* (6), *Assemble a world picture* (6) | 12 |

A hard slice therefore carries genuine relevant support for about two goals plus incidental
spillover. `min_evaluated_goals ≥ 4` per slice cannot be met by any amount of labeling, and would
not be met by doubling the pool either — it would require *generation* conditioned on all seven
goals within each hard slice. The design's denominator rule ("if fewer than `min_evaluated_goals`
qualify in a slice, the slice cannot be gated and the freeze fails") would therefore fail the freeze
by construction.

Dropping the slice intersection fixes it: per `(goal × split)` over the whole split, **every cell
clears `min_relevant_support = 3` under both bounds** (third finding below).

## Method

- **Proxy gold.** No panel labels exist, so gold is bounded by the two existing labelers:
  - `strict` = mini **and** Fable both say `relevant` (lower bound);
  - `optimistic` = mini **or** Fable says `relevant` (upper bound).
  True support sits between them; the panel will resolve some of the 207 disagreements each way.
- **Slices.** Real `intended_slice_tags` from `generation-output.jsonl`.
- **Split.** Not simulated — **forced**. The pool holds exactly two `session_id`s and two
  `semantic_cluster_id`s (`pool-a`, `pool-b`, 57 utterances each), so the session↔cluster graph has
  exactly two connected components and the only non-crossing split is pool-a / pool-b. There is no
  freedom to rebalance thin cells across splits.
- Whole-pool (unsplit) counts are reported as the ceiling.
- Sanity check: the script recovers exactly 207 pair disagreements, matching the recorded
  mini/Fable agreement of 591/798 = 74.1%.

## Cell table — gold-relevant pairs per (split × hard slice × goal)

`strict` / `optimistic`.

### negation (validation 8 utterances, test 8)

| Goal | validation | test | whole pool |
|---|---|---|---|
| Respect a person's boundaries | 1 / 1 | 2 / 3 | 3 / 4 |
| Keep theses distinct from fact | 4 / 4 | 4 / 5 | 8 / 9 |
| Serve the present person | 4 / 4 | 4 / 4 | 8 / 8 |
| Grow the library | **0 / 4** | **0 / 7** | **0 / 11** |
| Learn what drives this person | 0 / 0 | 0 / 0 | 0 / 0 |
| Track the AI transition | 0 / 0 | 0 / 0 | 0 / 0 |
| Assemble a world picture | **0 / 4** | **0 / 7** | **0 / 11** |

### quoted_speech (validation 6, test 6)

| Goal | validation | test | whole pool |
|---|---|---|---|
| Respect a person's boundaries | 2 / 4 | 1 / 3 | 3 / 7 |
| Keep theses distinct from fact | 2 / 6 | 1 / 4 | 3 / 10 |
| Serve the present person | 0 / 0 | 0 / 2 | 0 / 2 |
| Grow the library | 3 / 4 | 3 / 4 | 6 / 8 |
| Learn what drives this person | 3 / 3 | 3 / 3 | 6 / 6 |
| Track the AI transition | 0 / 0 | 0 / 0 | 0 / 0 |
| Assemble a world picture | **0 / 6** | **0 / 5** | **0 / 11** |

### hypothetical (validation 6, test 6)

| Goal | validation | test | whole pool |
|---|---|---|---|
| Respect a person's boundaries | 0 / 0 | 0 / 0 | 0 / 0 |
| Keep theses distinct from fact | 0 / 4 | 1 / 6 | 1 / 10 |
| Serve the present person | 0 / 1 | 0 / 3 | 0 / 4 |
| Grow the library | **0 / 5** | **0 / 6** | **0 / 11** |
| Learn what drives this person | 0 / 1 | 0 / 0 | 0 / 1 |
| Track the AI transition | 3 / 4 | 3 / 3 | 6 / 7 |
| Assemble a world picture | 6 / 6 | 6 / 6 | 12 / 12 |

## Threshold sweep — goals clearing `min_relevant_support`, of 7

| Slice | Split | ≥3 (strict/opt) | ≥5 | ≥10 |
|---|---|---|---|---|
| negation | validation | 2 / 4 | 0 / 0 | 0 / 0 |
| negation | test | 2 / 5 | 0 / 3 | 0 / 0 |
| negation | whole | 3 / 5 | 2 / 4 | 0 / 2 |
| quoted_speech | validation | 2 / 5 | 0 / 2 | 0 / 0 |
| quoted_speech | test | 2 / 5 | 0 / 1 | 0 / 0 |
| quoted_speech | whole | 4 / 5 | 2 / 5 | 0 / 2 |
| hypothetical | validation | 2 / 4 | 1 / 2 | 0 / 0 |
| hypothetical | test | 2 / 5 | 1 / 3 | 0 / 0 |
| hypothetical | whole | 2 / 5 | 2 / 4 | 1 / 3 |

`min_evaluated_goals` reachability across **every** slice × split cell:

| `min_relevant_support` | `min_evaluated_goals` | strict | optimistic |
|---|---|---|---|
| 3 | 4 | FAIL | PASS |
| 3 | 5 | FAIL | FAIL |
| 5 | 4 | FAIL | FAIL |
| 5 | 5 | FAIL | FAIL |
| 10 | 4 | FAIL | FAIL |
| 10 | 5 | FAIL | FAIL |

Only the single most permissive combination — support ≥ 3, ≥ 4 evaluated goals — passes, and only
under the optimistic bound, which assumes the panel resolves essentially every disagreement toward
`relevant`. Under the conservative bound nothing passes. `min_relevant_support = 5` is out of reach
in every configuration.

## Second finding: the disagreement is a relevance threshold, not a two-goal confusion

The design attributes labeler disagreement to a blurred boundary between *Grow the library* and
*Assemble a world picture*, and sharpens the guideline there. The data says the problem is wider.

**199 of the 207 disagreements are mini `relevant` against Fable `not_relevant`** — one direction.
The remainder is 5 the other way and 3 `ambiguous`/`not_relevant`. This is not adjudication noise
between two similar goals; it is a systematic difference in **how much bearing on a goal counts as
relevant**, with mini liberal and Fable conservative.

Disagreements per goal, of 114 pairs each:

| Goal | Disagreements | Rate |
|---|---|---|
| Grow the library | 58 | 50.9% |
| Assemble a world picture | 48 | 42.1% |
| Keep theses distinct from fact | 42 | 36.8% |
| Serve the present person | 25 | 21.9% |
| Learn what drives this person | 21 | 18.4% |
| Respect a person's boundaries | 10 | 8.8% |
| Track the AI transition | 3 | 2.6% |

The two named goals do carry the most (106 of 207, 51%), so the design's instinct is right about
where it is worst — but *Keep theses distinct from fact* at 36.8% is not explained by a
library/world-picture worked example, and the one-directional pattern says the rubric is missing a
threshold statement rather than a goal-separation example.

Consequence for the strict/optimistic bounds above: because Fable's `relevant` set is nearly a
subset of mini's, `strict` ≈ *Fable's* labels and `optimistic` ≈ *mini's*. The wide gaps in the cell
table — *Grow the library* and *Assemble a world picture* at 0/11 in several cells — are not sample
noise. They are the unresolved rubric question. **Their true support is genuinely unknown until the
sharpened guideline is applied**, and the census cannot narrow them further.

## Third finding: per-goal support is adequate when the slice intersection is dropped

Measuring per `(goal × split)` over the **whole split, all slices** — the denominator a per-goal
floor would use if it were not intersected with a hard slice — 57 utterances per split:

| Goal | validation (strict / opt) | test (strict / opt) |
|---|---|---|
| Respect a person's boundaries | 20 / 25 | 23 / 28 |
| Keep theses distinct from fact | 8 / 32 | 8 / 26 |
| Serve the present person | 4 / 13 | 4 / 20 |
| Grow the library | 5 / 33 | 5 / 33 |
| Learn what drives this person | 4 / 13 | 4 / 16 |
| Track the AI transition | 5 / 7 | 5 / 6 |
| Assemble a world picture | 9 / 34 | 9 / 31 |

| `min_relevant_support` | cells clearing (of 14) | worst split (of 7) |
|---|---|---|
| 3 | 14 strict / 14 optimistic | 7 strict / 7 optimistic |
| 5 | 10 strict / 14 optimistic | 5 strict / 7 optimistic |
| 10 | 2 strict / 12 optimistic | 1 strict / 6 optimistic |
| 15 | 2 strict / 10 optimistic | 1 strict / 4 optimistic |

**At `min_relevant_support = 3`, every `(goal × split)` cell clears under both bounds.** The
per-goal floor is therefore gateable without weakening `min_evaluated_goals` — *provided* the cell
being gated is `(goal × split)` rather than `(goal × slice × split)`. It is the intersection with
the hard slice, not the pool size, that makes the designed gate impossible.

At `min_relevant_support = 5` four cells starve under the conservative bound (*Serve the present
person* and *Learn what drives this person* at 4 in both splits). At 10, only two of fourteen
clear. So a floor above 3 is a pool-size question, not a gate-shape question.

Two consequences worth weighing before accepting 3:

- **A recall rate over n = 4–5 is quantized in steps of 20–25%.** `R_floor` on such a cell is a
  coarse instrument; a single auditor miss moves the measured recall by a fifth.
- **The three highest-disagreement goals have the widest bounds** — *Grow the library* 5/33,
  *Assemble a world picture* 9/34, *Keep theses distinct from fact* 8/32. Their post-`v2` support
  could land anywhere in that range, so a threshold chosen at 3 today may be far below what the
  panel actually yields, or exactly at it.

**Growing the pool is a live option and belongs on the table.** `generate --production` already
exists and generation is nano/mini-cheap, but every added utterance also multiplies across all
eight labeling runs, so pool growth is paid for twice and the labeling side dominates. Doubling the
pool would lift the conservative floors toward 8–10 and make a `min_relevant_support = 5` gate
comfortable. It would also be the moment to fix the two-goals-per-slice conditioning that broke the
per-slice variant. Deciding pool size is upstream of both the gate threshold and the labeling
spend, so it is a decision to take before any labeling run.

Whichever cell is gated, the design's *insufficient evidence is a failure, not a pass* rule must
follow it: a `(goal × split)` cell below `min_relevant_support` fails the freeze rather than
silently dropping out of the average.

## Fourth finding: the conservative breadth policy already exists, and did not settle the question

`AnnotationGuidelines.GoalRelevance.md` already carries a *Breadth policy for standing goals*:

> Do not label an utterance `relevant` to such a goal merely because the goal could operate on it.
> It is `relevant` only when the utterance offers or solicits specific content for that goal's
> tension space.

It names *Grow the library*, *Assemble a world picture*, *Learn what drives this person*, and
*Serve the present person* — four of the five highest-disagreement goals — and it carries worked
examples. It landed 2026-07-21 (`d935c77`), **before both labeling runs** (Fable's run is
`fable-20260723-prod`). Both labelers had it and still split 199 times in one direction.

So `goalrel-label-v2` cannot discharge this by restating a conservative policy that is already
there. The policy states the right direction but is not **determinate**: two capable readers apply
"offers or solicits specific content" and reach opposite answers on half the *Grow the library*
pairs. The v2 work is to make the existing test decidable — a procedure a reader executes rather
than a disposition they interpret — not to add a new stance.

## What this implies

1. **Gate the per-goal floor on `(goal × split)`, not `(goal × slice × split)`.** Per-goal
   `relevant_recall` is gated over the whole split regardless of slice, where every cell clears
   support 3. The per-goal × hard-slice claim is lost; the census shows it was never purchasable
   from this pool.
2. **Retire macro `relevant_recall` as a per-slice gate condition.** Because a slice spans only two
   conditioning goals, a per-slice macro averages over whichever goals happen to appear there — it
   would read as "negation handling across the roster" while measuring a two-goal subset. That is a
   validity defect in the *statistic*, not a support shortfall, and no threshold tuning repairs it.
   The per-slice gate keeps `utterance_relevant_set_match`, `F_max`, and `A_max`, none of which
   decompose by goal; recall is gated per `(goal × split)`.
3. **Keep the insufficient-evidence failure attached to the gated cell.** A `(goal × split)` cell
   below `min_relevant_support` fails the freeze. The rule must not disappear along with the slice
   intersection.
4. **Decide pool size and slice coverage together, after rubric validation, before any labeling
   run.** Every number in this census is proxy gold, and the 199 one-directional disagreements are
   exactly the pairs a sharpened `v2` will move — the `5/33`-style bounds span a factor of six. A
   pool decision taken now commits real money to a number about to change several-fold. The
   `v2` validation already produces the needed input; re-deriving this census against the post-`v2`
   labels turns the checkpoint into an informed decision rather than a deferral. The checkpoint
   is hard-gated: it falls before the eight runs with no path around it.
5. **`utterance_relevant_set_match` becomes the load-bearing per-slice condition.** Denominator of
   6–8 utterances per slice per split — thin, but not divided by 7 goals, and unsatisfiable by
   negatives. It survives this census best.
6. **Snapshot selection must admit more than one run per panel member, or deferral is not cheap.**
   The design names "exactly one `labeling_run_id` per `panel_member_id`". Under that rule, adding
   utterances after v1 forces every member to re-label the *whole* pool, writing off the v1 spend —
   so a later pool decision costs far more than an earlier one, which is not a cost this census
   otherwise surfaces. Relaxing selection to a **set of runs per member with disjoint utterance
   coverage whose union is complete** preserves every property the rule exists for (no "latest"
   inference, exactly one verdict per `(snapshot, member, utterance, goal)`, dense coverage) while
   making incremental growth cost only the new utterances.
7. **Rubric sharpening is worth more than the census suggested, and is a determinacy problem.** v2
   must extend the existing conservative breadth policy into an executable test, not restate it,
   and must be validated on whether a careful reader reaches a repeatable answer — not on whether
   disagreement disappears.
8. **Hard-slice generation must condition on all seven goals — recorded as a v1-known limitation.**
   The current 2-goals-per-slice conditioning is what makes per-goal, per-slice gating impossible
   and what forces the macro retirement above. v1 freezes with the defect named rather than fixed;
   recording it durably is what keeps a future version from inheriting it. Regeneration stays live
   at the pool-size checkpoint, since coverage and depth are answered by the same post-`v2` support
   numbers and should be decided together, once.
9. **The parent plan's per-slice floors are met.** negation 8/8, quoted_speech 6/6, hypothetical 6/6
   per split against floors of 6/5/5. The pool is not short on slice coverage; it is short on
   per-goal spread *within* a slice.

## Caveats

- Proxy gold is two labelers under `goalrel-label-v1`, one of whom (mini) is a low-weight panel
  member and the other (Fable) a full-weight one. It is a bound, not a measurement.
- The seven-member panel under `goalrel-label-v2` will produce a different relevant set. The cells
  reported as 0/11 are precisely the ones that could move most.
- The forced pool-a/pool-b split is a property of this pool, not of the split algorithm. A pool
  generated with more sessions and clusters would have real split freedom.
