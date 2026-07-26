# Review brief: PRODUCTION goal_relevance pool from approved anchors (goalrel-gen-v6)

You are reviewing the production generation pool — the required gate before any paid
labeling (DecisionLog 2026-07-22: review can relabel but never exclude an utterance,
so anything irredeemable must be caught HERE, where regeneration costs cents; after
labeling starts it would cost the paid pass and review time).

Data: `readable-pool.md` (114 utterances, 40 batches; each heading shows batch id,
slice tags, conditioning goal, generator model). Approved anchors:
`anchors-approved.jsonl` (the `generation-anchors.jsonl` sidecar is byte-identical —
already verified).

Mechanically verified, do not re-litigate: 114 records with unique ids; per-goal,
per-mode, per-model, and multi-tag counts all exactly match the approved two-pool
schedule (57 per pool); mode validators passed (no negator words in implicit
negation, framing markers present in hypotheticals, no forbidden vocabulary in
hard batches, punctuation fully stripped in punctuation-loss lines, entity mangling
applied in ASR lines); batch sizes exact — under-delivery is a hard failure.

## Checks, in priority order

1. **Zero-margin slices — strictest check (18 lines, read every one).**
   `subject_confusion` (3 per pool), `synthetic_asr` (3 per pool, tagged with
   punctuation_casing_loss), and `rare_high_cost` (3 per pool) are generated at
   exactly their freeze floor. A single irredeemable line in any of these forces
   regenerating the run. "Irredeemable" means: not something a person could
   plausibly say to their AI, or so garbled/degenerate that labeling it would be
   meaningless. A line that merely brushes another goal or is imperfect is fine —
   labels handle that. For ASR lines, mangled entities and lost punctuation are the
   *intended* corruption; fail only if the line stops reading as human speech.
2. **Anchor fidelity (10 cluster batches: regular clusters have 2 lines, hard
   clusters 5).** Every line must be a wording-level paraphrase of exactly its
   approved anchor — same actors, same event, same stance, same consequence. Flag
   any line that drifts to a different event or actor set. (Cross-pool anchor
   distinctness was settled at the anchor gate; do not re-argue it.)
3. **none_of_roster — threshold check, not purity (20 lines, read all).** Final
   status is assigned by blind labeling and human review, so goal-brushing lines
   are reclassified, not corrupting. PASS threshold: at least 9 of 10 lines per
   pool are clean negatives under the annotation guidelines' breadth policy (floor
   of 8 per split must survive review attrition with margin). List brushing lines
   with the goal touched — they become known-borderline items for the review
   queue, not blockers, unless a pool drops below 9 clean.
4. **Voice spot-check (all batches, skim).** First-person user speaking to their
   AI about their own life; no assistant voice; no addressing the AI as a human
   with its own family or job.
5. **Mode adherence spot-check.** Explicit negation (4 lines, Theses goal, pool
   each) visibly negates; implicit negation (4 lines, Serve-person goal, pool
   each) counters without negator words; quoted-speech lines contain actual quoted
   utterances; hypotheticals stay hypothetical; subject-confusion lines (Boundaries
   goal) pose real attribution puzzles (who asked, who declined, whose info).
6. **Repetition (pool-wide, skim).** No single sentence skeleton dominating across
   batches outside the intentionally coherent cluster batches.

## Output format

Per check: pass, or fail with batch/line ids (e.g. `pool-a-16 line 02`) and
one-sentence evidence. For check 3 report per-pool clean counts (n/10) and the
brushing lines with goals. End with:

- **GO** — the pool is cleared for paid labeling (mini run, then the Fable
  cross-label and full human review).
- **NO-GO** — name the failing check(s) and whether the remedy is a full
  regeneration (zero-margin slice failure) or something narrower.
