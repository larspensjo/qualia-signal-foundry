# Pre-sort brief: mini/Fable label disagreements for goal_relevance review

You are pre-sorting labeler disagreements ahead of the operator's authoritative
review. Your output is a recommendation sheet the operator takes into the review
tool — you recommend dispositions; **the operator decides every pair**. Nothing you
write becomes a label.

Data, in this directory:
- `disagreements-readable.md` — the 207 disagreeing (utterance, goal) pairs of 798
  total, grouped by goal and disagreement direction, with full utterance text.
- The rubric: `evaluation/annotations/AnnotationGuidelines.GoalRelevance.md`
  (repo-relative). Judge strictly by it — especially the **breadth policy for
  standing goals** and the relevance/negation policy (countering a goal is still
  relevant).

Context you should know: 199 of the 207 disagreements are `mini=relevant /
fable=not_relevant`, concentrated on the standing goals (Grow the library,
Assemble a world picture, Keep theses distinct from fact, Serve the present
person, Learn what drives this person). The mini labeler systematically treats
utterances a standing goal *could* operate on as relevant; the rubric's breadth
policy instead requires that the utterance **offers or solicits specific content
for the goal's tension space**. Do not assume either labeler is right per pair —
apply the rubric, not deference. The 8 pairs in the other directions
(`not_relevant→relevant`, `ambiguous→not_relevant`) deserve individual care.

## Task

1. **Per group** (each `goal × direction` section in the data file): state a
   recommended default disposition — "resolve to relevant", "resolve to
   not_relevant", or "no group default; judge individually" — with a two-to-three
   sentence rationale grounded in the rubric's policy text.
2. **Per pair, flag the contested minority.** Within each group, list the pair ids
   that should NOT follow the group default (the genuinely contested items), each
   with one sentence of evidence. Aim for precision: a pair is contested when the
   rubric genuinely supports both readings or when the utterance materially
   differs from its group's pattern — not merely when the call requires thought.
3. **Consistency sweep.** Note any pairs whose recommended outcome would conflict
   with another recommendation for the same utterance under a different goal
   (e.g. an utterance recommended none-of-roster-like everywhere yet relevant
   somewhere), and any place where your recommendations imply the utterance-level
   `none_of_roster` flag should flip.

## Output format

For each of the seven goals, in the data file's order:
- The group recommendation(s) with rationale.
- `CONTESTED:` the flagged pair ids with one-line evidence each (or "none").

Then a final section:
- `CONSISTENCY:` cross-goal conflicts and implied `none_of_roster` flips, by
  utterance id.
- `SUMMARY:` counts — how many pairs follow a group default vs contested, and the
  handful of pairs you consider hardest, ranked.

Keep the whole result skimmable; the operator will step through the review tool
with your sheet beside it, disagreements first.
