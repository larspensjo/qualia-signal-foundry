# Goal-relevance annotation guidelines

Guideline version: `goalrel-label-v2`

## The relevance threshold, and why it sits here

**The threshold.** A goal is relevant to an utterance only when the utterance carries specific
content that the goal bears on. A goal is *not* relevant merely because it could plausibly be
brought to bear on the utterance, or because the utterance belongs to a domain the goal cares
about.

**Worked boundary.** Take the utterance *"I told Maya I'm not ready to discuss my personal life,
and asked her to stick to our project plans instead."*

- Against **person respect** — *never pressing past a decline* — this is `relevant`. The utterance
  is a decline; that is the specific content the goal bears on.
- Against **epistemic integrity** — *what is observed, inferred, and speculated stays
  distinguishable* — this is `not_relevant`. The person is reporting a conversation, and reporting
  can in principle blur observation and inference, but nothing here is actually presented as fact,
  inference, or speculation in a way the goal speaks to. The goal *could* be brought to bear; the
  utterance does not carry content it bears on. That distinction is the threshold.

**Why the threshold sits here.** Determinacy cannot decide this. Two rubrics — one liberal, one
conservative — can each be perfectly determinate, with every annotator agreeing on every case, and
still produce different systems. So the direction is anchored to a third thing: what should make a
goal activate in the realtime system.

The task contract names both costs symmetrically. A false positive "can misdirect present-turn
framing or crowd out a more pertinent goal"; a false negative "can miss a person-relevant concern."
Only the false-positive cost has been observed in a live session, as arbitration letting
`present-person-priority` crowd out weaker world-goal matches. In the worked boundary above, a
liberal reading would wake `epistemic-integrity` on an utterance whose real subject is a declined
boundary — putting it in competition with `person-respect`, the goal that actually applies. That is
the failure this threshold is set to avoid.

The prior guideline version's two labelers split 199 times in one direction, at 39.1% against 14.8%
raw positives. Both were working from the same conservative breadth policy, so that split was a
disagreement about where the line sits rather than confusion about the task — which is why the line
is now stated rather than left to be inferred.

**A disclosure, recorded here deliberately.** Tightening the threshold will probably *improve* the
exact-token failure floor, because conservative relevance skews toward specific topical content and
that is what a keyword scorer matches. This section was written and committed before the
rubric-sensitivity measurement existed, so the direction was not chosen for that effect. Any
measurement of the failure floor is conditioned on this threshold and is not comparable across
guideline versions.

**Still to come.** This section fixes the direction. The executable determinacy test and the
worked examples for the cases that most divided the prior labelers are added when the rubric is
sharpened for determinacy; those examples are authored fresh rather than lifted from the labeling
pool, so that no pooled pair has its label decided here.

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
- Quoted or reported speech can likewise be `relevant`: “My manager asked about Ana's affairs,
  and I said, ‘I can only speak about her deliverables and decisions’” is `relevant` to **Respect
  a person's boundaries** because the speaker declines to disclose an absent person's affairs and
  confines the answer to work deliverables.
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

## Breadth policy for standing goals

Some roster goals are standing dispositions that any conversational turn could in principle feed —
for example **Grow the library**, **Assemble a world picture**, **Learn what drives this person**,
and **Serve the present person**. Do not label an utterance `relevant` to such a goal merely
because the goal could operate on it. It is `relevant` only when the utterance offers or solicits
specific content for that goal's tension space: a concrete observation or claim to place or keep, a
direct engagement with the present person's own work, beliefs, or drives, or an explicit stance on
the priority the goal names.

Worked examples:

- “Nvidia and OpenAI are changing how teams plan” is `relevant` to **Assemble a world picture**: it
  offers a specific claim that wants a place in a larger explanation.
- “I forgot where I put my keys” is `not_relevant` to **Grow the library**: it is generic everyday
  memory trouble, not an observation, thesis, or learned item to preserve.
- “I am under a lot of financial pressure lately” is `not_relevant` to **Track the AI transition**:
  personal financial pressure alone does not engage that goal without an AI-adoption connection.
- “What if the evidence changes tomorrow?” is `not_relevant` to **Grow the library**: it names
  nothing to keep, recall, or revisit, even though the library revisits theses as evidence
  accumulates. (It is `relevant` to **Keep theses distinct from fact**.)
- An ordinary question or request is not `relevant` to **Serve the present person** merely because
  it is an explicit ask. That goal's tension is engaged when the utterance bears on prioritizing
  the person's ask — for example, “answer my question before you go off on your tangent.”

Apply this policy before reaching for `ambiguous`: uncertainty about whether a standing goal is
engaged is usually resolved by the specificity test above, and `ambiguous` remains reserved for a
missing referent or context per the policy below.

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

Direct reluctance or refusal to talk is not a safe `none_of_roster` negative for **Respect a
person's boundaries**. “I'm not really in the mood to talk much right now” is `relevant` to that
goal because it expresses a limit another person should follow, even without words such as
“private” or “personal.”

## Consistent use

This rubric is the source for every panel member's labeling prompt and for the independent
auditor's. A change requires a new guideline version and re-evaluation of any label set that claims
this version.

The interchange validator enforces the shared invariant that `none_of_roster: true` cannot coexist
with a `relevant` per-goal label.

## Procedures that belong to the prior version

Two sections of `goalrel-label-v1` are **not part of this version and must not be followed**: the
two-labeler cross-label ritual, and the human-review artifacts (`review-decisions.jsonl`,
`blind-qa-decisions.jsonl`, and folding into a reviewed pool). Independent per-pair human
adjudication was retired because it could not be performed reliably, and it is replaced by a
weighted panel with an independent auditor. Their text remains retrievable at the `goalrel-label-v1`
revision, which is what the label sets bound to that version should be read against.

The labeling ritual for this version — session preparation, workspace isolation, the provenance a
run must record, and how a member's chunked runs compose into one pass — is specified with the
campaign machinery rather than here. This document governs *how to judge a pair*; that one governs
*how a run is conducted*.
