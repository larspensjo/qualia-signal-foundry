# Goal-relevance annotation guidelines

Guideline version: `goalrel-label-v1`

## Purpose and unit of annotation

Annotate each utterance independently against every goal in the supplied frozen roster. The unit is
one `(utterance_id, goal_ref)` pair, plus one utterance-level `none_of_roster` annotation. Use only
the utterance and the roster's goal descriptions. Do not infer a generator's intent, a slice, or
provenance.

## Pair labels

- `relevant`: The utterance bears on the goal's tension space. It may support, question, resist,
  or counter the goal; agreement with the goal is not required. For example, “I don't want to
  discuss my friend's private life” is `relevant` to **Respect a person's boundaries**: it states a
  boundary and is valuable counter-information to an unrealistic goal.
- `not_relevant`: The utterance is genuinely not about that goal's tension space. A negation is
  `not_relevant` only when it removes the topic altogether, such as “I am not asking about a
  private friend” for **Respect a person's boundaries**; that is a stray disclaimer, not a stance
  about how to handle private information.
- `ambiguous`: The available wording does not support a reliable relevant/not-relevant judgment.
  Use this only for real uncertainty about the utterance's connection to that goal, not merely
  because the utterance is terse, hypothetical, quoted, negative, or inconvenient. For example,
  “That is not the point” is `ambiguous` for a goal when no referent establishes which tension it
  concerns.

## Relevance and negation policy

Negating a goal topic does **not** make an utterance irrelevant. Relevance means that the
utterance bears on the goal's tension space, including by opposing or countering it. “I don't want
to discuss my friend's private life” is `relevant` to the boundaries goal, even though it rejects a
possible line of discussion. A negation is `not_relevant` only when it makes the utterance genuinely
not about the goal at all, as in the stray disclaimer “I am not asking about a private friend.”

Apply this policy equally to explicit negation, implicit negation, quoted speech, and hypothetical
wording. Do not use a generation category as evidence; it is intentionally unavailable during
annotation.

## Ambiguous policy

Choose `ambiguous` when two careful readers could reasonably reach different binary labels because
the utterance lacks a needed referent or context. Do not resolve ambiguity by guessing unstated
context. Conversely, do not use `ambiguous` to avoid a clear judgment: an utterance that plainly
engages a goal, including in opposition, is `relevant`; one plainly outside it is `not_relevant`.

## Utterance-level none-of-roster rule

Set `none_of_roster` to `true` only when no goal in the full supplied roster is `relevant`.
When it is true, no per-goal label may be `relevant`; labels may be `not_relevant` or
`ambiguous`. Set it to `false` whenever at least one roster goal is `relevant`, including a goal
the utterance counters. This utterance-level decision does not replace the required label for every
roster goal.

## Consistent use

This rubric is the source for the automated mini label prompt, the independent Claude Fable ritual,
and operator corrections. A change requires a new guideline version and re-evaluation of any label
set that claims this version.

## Independent Claude Fable cross-label ritual

1. Give Claude Fable this document and the unchanged `labeling-input.jsonl` artifact. Do not give
   it generation output, intended goals, slice tags, prior labels, reconciliation, or review
   decisions.
2. Ask it to apply this rubric independently to every utterance and every supplied roster goal.
   It returns one JSONL `LabelInterchange` record per utterance with
   `labeler_id: "claude-fable"`, its own `labeling_run_id`, this `guideline_version`, all
   `per_goal` labels, and `none_of_roster`.
3. Schema-check `label-fable.jsonl` through the same `parse_label_interchange` / validation path
   used for `label-mini.jsonl`, against the frozen roster. Do not repair output by hand: resolve
   malformed output with Fable and validate the replacement.
4. Reconcile the two valid files and record the resulting mini/Fable agreement rate with the
   dataset methodology artifacts. Review every pair; disagreements are first in the queue.

The interchange validator enforces the shared invariant that `none_of_roster: true` cannot coexist
with a `relevant` per-goal label.

## Human-review artifacts

Authoritative accept/correct actions are append-only records in `review-decisions.jsonl`. This is
the only human-decision artifact folded into `reviewed-pool.jsonl`, with the latest decision for a
field taking precedence. A cold blind-QA session writes the same decision interchange shape to the
separate `blind-qa-decisions.jsonl` artifact. Blind-QA decisions measure the accepted labels; they
must never be appended to, substituted for, or folded as `review-decisions.jsonl`.
