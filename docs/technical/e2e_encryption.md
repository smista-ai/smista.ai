# End-to-end encryption

- [End-to-end encryption](#end-to-end-encryption)
  - [What it protects](#what-it-protects)
  - [What it does not hide](#what-it-does-not-hide)
  - [Turning it on](#turning-it-on)
  - [What gets encrypted](#what-gets-encrypted)
  - [One key per session](#one-key-per-session)
  - [How content travels during a task](#how-content-travels-during-a-task)
  - [Status](#status)

End-to-end encryption keeps the content of your sessions unreadable at rest. The
router stores your messages, plans, tool payloads, diffs, trace events and
per-session memory as ciphertext, and it never holds a key to open them. Only
your machine can read them back.

## What it protects

Anyone who reaches the stored data sees ciphertext, not your content. That
includes a stolen laptop, a database backup, or a hosted (SaaS) router whose
disk you do not control. The encryption key never leaves your machine, so the
stored data is useless without it.

## What it does not hide

The router still sees your content **while a task runs**, because it is the part
that calls the model for you. Encryption protects what is _stored_, not what is
processed live. In short: nothing readable is kept on disk, and the router never
holds a key. It is not a promise that the router never sees your text.

Some information always stays readable, because the router routes and recalls on
it: the message role, the provider and model, timestamps, and which session and
user a row belongs to. Only the free-form body of each row is encrypted.

## Turning it on

Encryption is chosen per session, when the session is created. Ask for it on
`POST /api/v1/sessions`:

```json
{ "title": "Refactor auth middleware", "encrypted": true, "key_id": "kf_ab12" }
```

`encrypted` defaults to `false`. When it is `true`, `key_id` is required: it is
the fingerprint of the key your client generated for this session. The choice is
fixed for the life of the session and cannot be turned on or off later, because
content already stored could no longer be matched to a key.

## What gets encrypted

Every content row of an encrypted session is sealed:

- Message bodies
- Plan snapshots
- Tool-call arguments, results and errors
- Diff bodies
- Trace event payloads
- Per-session memory facts

Long-term, user-wide memory (`user_memory`) is **not** covered yet. It belongs
to your account rather than to a single session, so the per-session switch does
not reach it. It stays readable until a separate, account-level option is added.

Each sealed value is stored as an envelope that names the algorithm and the key
that sealed it, plus a one-time value and the ciphertext. The envelope is opaque
to the router; only your client can open it.

## One key per session

Each encrypted session has its own key, generated on your machine when the
session is created. The key never travels to the router; only its fingerprint
(`key_id`) does.

Giving every session its own key keeps the damage from a leak small: if one key
is exposed, only that single session can be read, and every other session stays
sealed.

The client stores each key as a file under `~/.smista/e2e/`, readable and
writable only by you (`0600`), in a directory only you can enter (`0700`).

## How content travels during a task

The router never has a key, so your client does the encrypting and decrypting as
a task runs:

- New content you send (your prompt) travels in clear so the model can be
  called, together with its sealed form for storage. The router keeps only the
  sealed form. Tool results work the same way: your client seals them itself and
  sends both forms together.
- To build context from earlier messages, the router picks which past rows it
  needs and asks your client to decrypt just those. This is a standalone step:
  the router sends a map of sealed records and your client returns the readable
  text before the model is called.
- Content the router produces during the task (the model's reply, tool-call
  arguments, plans, trace events, an interrupted partial) is handed back to your
  client to seal before it is stored. This rides the same response that delivers
  the result, so you see the output immediately and only its stored copy waits to
  be sealed.

Each record in these maps is named by a reference of the form `kind:id`, so the
router dispatches every payload to the right content store. This keeps routing
and context selection on the router while the key stays on your machine. See the
[run state machine](./run-state-machine.md) for exactly where each step fits in a
turn.

## Status

The storage layer described here, the per-session switch, and the envelope
format are in place. The encrypt and decrypt steps during a task, and the
client-side key files under `~/.smista/e2e/`, are being built next and are
tracked separately. Until they land, create sessions without `encrypted` set.
