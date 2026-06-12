# Memory

- [Memory](#memory)
  - [Two kinds of memory](#two-kinds-of-memory)
  - [How memory is recorded](#how-memory-is-recorded)
  - [How recall works](#how-recall-works)
  - [Enabling memory on a model](#enabling-memory-on-a-model)
  - [Where memory is stored](#where-memory-is-stored)
    - [User memory](#user-memory)
    - [Context memory](#context-memory)

smista.ai can remember things so you don't have to repeat yourself. There are
two kinds of memory: **user memory**, which follows you across every session,
and **context memory**, which is scoped to a single session. Both are written
by the model through a dedicated memory tool, while recall stays deterministic
and never depends on an LLM.

## Two kinds of memory

| Kind           | Belongs to       | Lives                         |
| -------------- | ---------------- | ----------------------------- |
| User memory    | you (the user)   | across all of your sessions   |
| Context memory | a single session | until that session is deleted |

User memory is for durable facts — your preferences, recurring constraints, how
you like work done. Context memory is for facts that only matter to the task at
hand and should not leak into unrelated work.

## How memory is recorded

The model decides what is worth remembering. When it judges that a fact should
be stored, changed, or dropped, it calls the memory tool with one of three
operations:

- `record` — store a new fact.
- `update` — replace a fact that has changed.
- `forget` — drop a fact that no longer holds.

Every call is mediated by the router, like any other tool call: it is validated
against your tool permissions, persisted, and recorded in the trace. The model
never writes to the database directly.

On providers that offer constrained outputs — for example OpenAI structured
outputs — smista.ai uses that path so the recorded operation is guaranteed to
match the expected shape. On other providers it relies on standard tool calling.
Either way, the behaviour is the same from your side.

## How recall works

Recall is deterministic. Before a task runs, the router loads your user memory
and the context memory of the active session, then adds them to the context it
sends to the model — within the usual context-selection and privacy limits. No
model chooses what is recalled, so routing stays predictable and remains
explainable through the trace.

## Enabling memory on a model

Memory only runs on models that can drive the memory tool. Whether a model has
the `memory` capability is a fact the provider reports — you do not declare it
in your configuration. To require memory for a particular kind of task, gate a
routing rule on the capability:

```toml
[[routing.rules]]
name = "remember across the project"
requires_capabilities = { memory = true }
model = "openai/gpt-5.5-mini"
```

A task gated this way is never routed to a model that cannot record memory.

## Where memory is stored

User memory and context memory live in two separate tables. Every row is owned
by a user, so a query can never reach another user's memory.

### User memory

| Field        | Type             | Description                          |
| ------------ | ---------------- | ------------------------------------ |
| `id`         | UUIDv7           | Unique identifier.                   |
| `user`       | user reference   | Owner of the memory.                 |
| `key`        | string, optional | Topic; lets an update target a fact. |
| `content`    | string           | The remembered fact.                 |
| `created_at` | datetime         | When the fact was first recorded.    |
| `updated_at` | datetime         | When the fact was last changed.      |

### Context memory

| Field        | Type              | Description                          |
| ------------ | ----------------- | ------------------------------------ |
| `id`         | UUIDv7            | Unique identifier.                   |
| `session`    | session reference | Session the memory belongs to.       |
| `user`       | user reference    | Owner, enforced on every query.      |
| `key`        | string, optional  | Topic; lets an update target a fact. |
| `content`    | string            | The remembered fact.                 |
| `created_at` | datetime          | When the fact was first recorded.    |
| `updated_at` | datetime          | When the fact was last changed.      |

Context memory is removed when its session is deleted. User memory persists and
is subject to your retention settings.
