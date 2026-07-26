# Review brief: proposed PRODUCTION cluster anchors for goal_relevance generation

You are reviewing ten proposed anchor propositions before the production batch
generation runs. This is the production campaign, not a smoke: each approved anchor
becomes the fixed semantic proposition that a batch of eight user utterances
paraphrases, and those utterances will be labeled, human-reviewed, and frozen into
the v1 validation/test sets. Approving a bad anchor wastes a paid batch, a paid
labeling pass, and later review time; rejecting a good one costs a cent to
regenerate. Judge strictly.

Data: `anchors-readable.md` (also `anchors-proposed.jsonl`) in this directory —
ten anchors, one per cluster id. Components are the neutral split-candidate pools:
`pool-a-cluster-1..4` + `pool-a-hard-cluster` (directive slots 1–5) and
`pool-b-cluster-1..4` + `pool-b-hard-cluster` (slots 6–10). A seed-recorded split
later assigns one whole pool to validation and the other to test, so treat the two
pools exactly as you treated validation vs test before: their anchors must be
structurally distinct across pools.

## Criteria (evaluate each anchor, then the set)

**Per anchor:**
1. **First-person user voice.** The "I" is the human user, present in the scene,
   speaking about their own conduct or plans, in language a person would actually
   say aloud to their AI. A third-person narrative, or stilted written-register
   phrasing that no one would speak, fails.
2. **Concrete and paraphrasable.** Specific actors (named), one event, a stance,
   and a consequence — concrete enough that eight wording-level paraphrases can
   preserve all four elements. A vague, multi-event, or internally muddled anchor
   (e.g. one whose subject shifts mid-sentence so it is unclear who does what)
   fails.
3. **Directive conformance.** The anchor must exhibit its cluster's required
   pattern (stated as the heading of each anchor in `anchors-readable.md`):
   - pool-a-cluster-1: speaker sets a boundary of their own and offers an
     alternative topic.
   - pool-a-cluster-2: speaker respects another person's stated limit by changing
     their own planned action.
   - pool-a-cluster-3: speaker pushes back on a third person pressuring someone
     else.
   - pool-a-cluster-4: speaker seeks the AI's advice before an upcoming
     conversation, honoring a known limit.
   - pool-a-hard-cluster: speaker scopes a group exchange to information
     voluntarily offered.
   - pool-b-cluster-1: speaker repairs a prior overstep (apology + changed
     approach).
   - pool-b-cluster-2: speaker declines to relay information entrusted by an
     absent person, offering a non-sensitive alternative.
   - pool-b-cluster-3: speaker reads discomfort cues and changes subject before
     anyone has to refuse.
   - pool-b-cluster-4: speaker coaches a junior person to leave disclosure
     voluntary.
   - pool-b-hard-cluster: speaker restrains their own curiosity; **no one ever
     declines or states a limit** — the restraint is entirely self-initiated. The
     stated-limit → stop → redirect-to-work → continued-engagement arc is
     forbidden here.
4. **Hard-cluster vocabulary.** The two `*-hard-cluster` anchors must avoid the
   obvious goal vocabulary (pry/private/personal/boundary/gossip/probe/press/
   reluctant/dig and inflections) — mechanically pre-checked, but confirm the
   *spirit*: the situation is shown through behavior and dialogue cues, never by
   naming the concept.

**The set (the historically failing criterion — check this pairwise):**
5. **Cross-pool structural distinctness.** No pool-a anchor may share its
   structural proposition — stance + speaker role + action + consequence — with
   any pool-b anchor. Different names, settings, or sensitive topics do NOT count
   as different propositions. Compare each pool-a anchor against all five pool-b
   anchors.

## Known flags from the pre-check (form your own judgment)

- `pool-b-cluster-2` opens "I, as the custodian of what Alex told me in
  confidence, …" — written-register phrasing; test against criterion 1.
- `pool-b-cluster-4` shifts subject mid-anchor: it starts as coaching Lena about
  the intern Omar, then describes the *speaker's own* behavior ("I keep my
  questions to his ideas … I follow his lead"), leaving unclear whose conduct the
  paraphrases should preserve. Test against criteria 2 and 3.

## Output format

For each anchor: pass, or fail with the violated criterion number and one sentence
of evidence. Then a pairwise verdict for criterion 5 naming any colliding pair and
the shared proposition. End with one of:

- **APPROVE** — all ten pass; production batch generation may run from this file
  unchanged.
- **APPROVE WITH EDITS** — supply corrected anchor text inline for the failing
  cluster ids (keep first person and the directive pattern); save the final ten
  anchors as `anchors-approved.jsonl` in this directory (or state the edits and
  the assistant will apply them), and batch generation runs from the edited file.
- **REJECT** — name the failing clusters; a fresh anchors-only run will replace
  them.
