# Production pool review results: `goalrel-gen-v6`

Date: 2026-07-23  
Reviewed: `readable-pool.md` against `PoolReviewBrief.md` and
`anchors-approved.jsonl`

## Verdict

**GO** — the production pool is cleared for paid mini labeling, followed by the blind Fable
cross-label and full human review.

All 18 zero-margin records are usable, all ten cluster batches preserve their approved anchors,
and both unconditioned pools retain 9/10 clean negatives. The voice, mode-adherence, and repetition
regression checks also pass.

IDs below use the brief’s batch/line notation, for example `pool-a-16 line 02`.

## Check 1 — zero-margin slices

**Pass.** I read every subject-confusion, synthetic-ASR, and rare-high-cost line. None is
irrecoverably unnatural or too garbled to label.

### Pool A

- Subject confusion: `pool-a-5 lines 01–03` — all three require resolving the asker, reluctant or
  protected person, and information owner. Line 03’s distinction between the breakup question and
  project-timeline follow-up is slightly artificial but remains meaningful and labelable.
- Synthetic ASR: `pool-a-16 lines 01–02`, `pool-a-18 line 01` — entity splitting and punctuation
  loss are conspicuous but the underlying human speech remains recoverable.
- Rare high cost: `pool-a-6 lines 01–03` — court/protective-order, hospital-compliance, and emergency
  dispatch/custody consequences are dense but plausible user reports with clear boundary stakes.

### Pool B

- Subject confusion: `pool-b-25 lines 01–03` — all three preserve attribution work. Line 01’s final
  “the project wasn’t his business” is imprecise about which matter is protected, but the Ben →
  Mara → job-departure information chain remains understandable and labelable.
- Synthetic ASR: `pool-b-36 lines 01–02`, `pool-b-38 line 01` — corruption remains readable and
  does not turn any record into word salad.
- Rare high cost: `pool-b-26 lines 01–03` — custody, hospital credentialing, and defamation/election
  consequences are severe and concrete without making the utterances meaningless.

Irredeemable ids: none. No regeneration is required for a zero-margin slice.

## Check 2 — anchor fidelity

**Pass.** Every one of the 28 cluster records preserves the approved anchor’s actors, event,
stance, and consequence.

| Cluster batch | Lines | Result |
|---|---:|---|
| `pool-a-cluster-1` | `pool-a-0 lines 01–02` | Maya; speaker’s own limit; project alternative; future welcome terms preserved. |
| `pool-a-cluster-2` | `pool-a-1 lines 01–02` | Jordan’s stated limit; changed handoff questions; Jordan’s chosen terms preserved. |
| `pool-a-cluster-3` | `pool-a-2 lines 01–02` | Priya pressures Sam; user intervenes and takes the schedule question; Sam avoids defending himself alone. |
| `pool-a-cluster-4` | `pool-a-3 lines 01–02` | Tomorrow with Elena; AI advice; design ideas versus recent divorce; avoids putting Elena on the spot. |
| `pool-a-hard-cluster` | `pool-a-4 lines 01–05` | Launch group; offered product/timing/decision material; next-step decision without invented reasons. |
| `pool-b-cluster-1` | `pool-b-20 lines 01–02` | Nia/dinner overstep; apology; cancelled follow-up; Nia controls contact; delayed reply accepted. |
| `pool-b-cluster-2` | `pool-b-21 lines 01–02` | Alex’s entrusted health information; group request refused; check-in timeline substituted; confidence preserved. |
| `pool-b-cluster-3` | `pool-b-22 lines 01–02` | Dana’s discomfort cue; presentation redirect before refusal; discomfort never becomes a decline. |
| `pool-b-cluster-4` | `pool-b-23 lines 01–02` | User coaches Lena about Omar; ideas/project focus and cue-following; Omar controls disclosure. |
| `pool-b-hard-cluster` | `pool-b-24 lines 01–05` | Self-initiated restraint about Noor’s job change; book alternative; conversation stays on Noor’s offered ground. |

Drifting ids: none. Cross-pool structural distinctness was settled at the anchor gate and was not
re-litigated.

## Check 3 — `none_of_roster` threshold

**Pass.** Each pool has one conservative goal-brushing record and therefore retains exactly 9/10
clean negatives, meeting the required threshold.

| Pool | Clean | Required | Result |
|---|---:|---:|---|
| Pool A | **9/10** | 9/10 | Pass |
| Pool B | **9/10** | 9/10 | Pass |

Known-borderline review-queue items:

- `pool-a-19 line 03` — **Keep theses distinct from fact**: “The soup at lunch was better than I
  expected” contrasts direct observation with a prior expectation. The engagement is weak, but it
  should not be asserted as a known clean negative.
- `pool-b-39 line 08` — **Respect a person’s boundaries**: “My sister showed up early, and I wasn’t
  ready for company” expresses the present person’s interpersonal limit even though it does not use
  privacy vocabulary.

Clean Pool A lines: `pool-a-19 lines 01–02, 04–10`.

Clean Pool B lines: `pool-b-39 lines 01–07, 09–10`.

`pool-b-39 line 02` is an ordinary request to help interpret a message. Under the breadth policy,
an ordinary ask does not by itself engage **Serve the present person**, and the missing message
content does not establish a thesis/fact or boundaries connection. `pool-b-39 line 03` is generic
task-memory trouble rather than a request to preserve learned observations or theses, so it does
not engage **Grow the library**.

## Check 4 — voice

**Pass.** The pool consistently reads as users speaking to an AI about their own requests,
observations, work, or interactions. There is no assistant voice and no line treats the AI as a
human with its own family or employment.

Some quoted-speech and rare-high-cost lines are unusually long, but they remain plausible spoken
user reports rather than assistant prose. Offending ids: none.

## Check 5 — mode adherence

**Pass.** Spot checks confirm the required semantic behavior:

- Explicit negation: `pool-a-7 lines 01–04` and `pool-b-27 lines 01–04` visibly negate claims while
  still engaging thesis/fact separation.
- Implicit negation: `pool-a-9 lines 01–04` and `pool-b-29 lines 01–04` prioritize the person’s
  stated request without grammatical negators.
- Quoted speech: `pool-a-11`, `pool-a-13`, `pool-b-31`, and `pool-b-33` contain actual quoted
  utterances carrying the relevant content.
- Hypotheticals: `pool-a-15`, `pool-a-17`, `pool-b-35`, and `pool-b-37` remain genuinely imagined
  scenarios rather than habits with decorative framing.
- Subject confusion: `pool-a-5` and `pool-b-25` require attribution of questions, refusals, and
  information ownership, as detailed under Check 1.

Offending ids: none.

## Check 6 — repetition

**Pass.** No single sentence skeleton dominates outside the intentionally coherent paraphrase
clusters. The seven-goal schedule supplies materially different speech acts and structures:
epistemic correction, request prioritization, memory/library reuse, motivation inquiry, AI-work
transition scenarios, world-model synthesis, and boundary attribution. Settings, actor roles,
tense, and consequences also vary across pools.

Offending ids: none.

## Final decision

**GO** — proceed with the paid mini labeling run. After that, use the unchanged blind
`labeling-input.jsonl` for the independent Fable cross-label, then begin full human review with
disagreements first.
