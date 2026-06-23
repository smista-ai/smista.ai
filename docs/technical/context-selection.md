# Context selection

- [Context selection](#context-selection)
  - [What the router draws on](#what-the-router-draws-on)
  - [Building the candidate set](#building-the-candidate-set)
  - [Relevance and privacy](#relevance-and-privacy)
  - [Fitting the window](#fitting-the-window)
  - [Opening sealed history](#opening-sealed-history)
  - [What gets recorded](#what-gets-recorded)
  - [Where it sits in a turn](#where-it-sits-in-a-turn)

Context selection decides what actually goes into the prompt. It runs **on the
router**, it is **deterministic** (never an LLM), and it is **filter-only**: it
ranks and trims material the router already holds, and never reads the
filesystem and never rewrites or summarizes content. It sits between
[routing](./routing.md) and the model call, and it feeds model selection,
because what is in the context decides which models may serve the turn.

This document follows the `smista-core` types `Attachments`, `ContextOutcome`,
`PrivacyPolicy`, `ModelDescriptor` and the crypto payloads (`SealedRecord`,
`PlainRecord`), plus the storage entities `session_message`, `user_memory`,
`context_memory` and `session_context_reference`.

## What the router draws on

The candidate material comes from two places, never from disk directly:

| Source             | Comes from                                                                                              |
| ------------------ | ------------------------------------------------------------------------------------------------------- |
| Session history    | `session_message` rows recalled from storage, each tagged with the provider and model that produced it. |
| Memory             | user-wide `user_memory` and per-session `context_memory` facts.                                         |
| Client attachments | the request's `attachments`: files (with content and a `required` flag), instructions and skills.       |
| Workspace metadata | the request's `workspace`: git branch and diff, referenced paths and the active file.                   |

The filesystem-derived parts (attachments, workspace) arrive in the request
because the router cannot read them; everything else is recalled from storage.

Skills arrive in two roles. **Invoked** skills are authoritative: their
instructions join the model preamble, so they are part of what the turn must
say. **Available** skills are offered for the model to activate on its own, so
they are supplementary.

## Building the candidate set

The router assembles these into a single, model-agnostic **candidate set**. Each
candidate carries a kind (message, file, instruction, skill, memory, diff), a
path when it has one, and a size estimate from the router's deterministic token
estimator. This is the universe the rest of the stage filters down.

The size estimate is a deterministic approximation (a fixed characters-per-token
ratio), not a provider-exact count. It is consistent rather than precise; the
window budget below keeps a margin so an approximate estimate stays safe.

Identical material is collapsed. When the same path, or the same content,
appears more than once (for example a file that is both attached and quoted in
history), the duplicates are merged into one candidate, keeping the required or
higher-ranked copy.

## Relevance and privacy

Two deterministic passes shape the set.

**Required or discardable.** Each candidate is marked. A candidate is
**required**, and so can never be trimmed away, when it is:

- a file the client flagged required,
- a file whose path the user referenced,
- a file whose path matches the globs that drove the route,
- an instruction document, or
- an invoked skill.

Everything else is **discardable** supplementary context: history, memory, the
diff, available skills and any other file.

**Relevance ranking.** Discardable candidates are ranked by a deterministic
score so that, when the window is tight, the most useful context is kept and the
least useful is dropped first. The score combines three signals:

- **Kind.** Attached files rank above the diff, the diff above history, history
  above memory, and offered (available) skills lowest.
- **Recency.** Among history messages, newer messages outrank older ones, so a
  tight window keeps the recent conversation rather than the start of it.
- **Path affinity.** A candidate whose path matches a referenced path or a
  route-driving glob is boosted, so context about the files in play is preferred.

Required candidates are never ranked against this score: they are always kept.

**Privacy.** Each candidate is also marked **restricted for remote** when its
path matches `PrivacyPolicy::is_restricted_for_remote` (the union of
`restricted_paths`, `remote.blocked_paths` and any `local_only` rule's paths).
Candidates without a path are never restricted.

These marks feed [model selection](./routing.md#locality-and-privacy): required
restricted content forecloses remote models, and discardable restricted content
is dropped when the route is remote. Privacy here is a routing input, not a
redaction after the fact.

## Fitting the window

Once the model is selected, the set is finalized to fit. The router does not fit
against the raw context window: it first reserves room for the model's reply
(the model's `max_output_tokens`, or a default when the model leaves it
unbounded). The remaining **effective budget** is what context must fit, so the
reply always has space.

The router keeps every **required** candidate plus as much relevant discardable
context as fits the effective budget, taking discardable candidates in ranked
order until the budget is full. The rest are excluded. This is the minimum
viable context.

A required candidate is never dropped to make room, and content is never
truncated or summarized to squeeze it in. If even the required context cannot
fit the effective budget, the turn does not silently cut anything: it raises
`context_window_exceeded`, which the selection stage treats as a
fallback-eligible failure and walks the fallback chain toward a model with a
larger window.

## Opening sealed history

In an [encrypted session](./e2e.md) the recalled history is ciphertext: the
router is blind at rest and holds no key. Selection still works, because it runs
on the cleartext metadata (role, provider, paths, timestamps) while only the
content is sealed. Once the router knows which past rows it needs, it emits an
`awaiting_decrypt` turn carrying those rows as `SealedRecord`s; the client opens
them with the session key and returns `PlainRecord`s in the `/continue` bundle,
and the router builds the prompt. A plaintext session skips this entirely.

## What gets recorded

The selection is reported to the client as a `ContextOutcome` (human-readable
`included` and `excluded` descriptions) and persisted as `session_context_reference`
rows: `path`, `kind`, `included` and a `reason`. These are references and
metadata only, never the file contents, and restricted content is not persisted
unless policy allows. A trace therefore shows exactly what was included or
excluded and why.

## Where it sits in a turn

```mermaid
flowchart LR
    A[Classify] --> B[Build candidates + mark privacy and size]
    B --> C[Select model under constraints]
    C --> D[Finalize: trim to budget, decrypt]
    D --> E[Invoke model]
```

Context selection straddles model selection: the candidate set and its privacy
and size marks are built **before** the model is chosen (so they can constrain
the choice), and the set is trimmed and decrypted **after** (against the chosen
model's effective budget). Like the rest of the pipeline it re-runs on
[every turn](./execution-protocol.md#the-turn-loop), so the context tracks the
work as it evolves.
