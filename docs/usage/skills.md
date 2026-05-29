# Skills

A **skill** is a reusable set of instructions you can attach to a task — for
example a code-review checklist or your project's Rust conventions. smista.ai
discovers skills from disk so routing rules can target them and the CLI can send
the right instructions along with a task.

## Where skills live

smista.ai looks for skills in two locations:

- **Project**: `<your project>/.agents/skills` — skills committed alongside a
  repository.
- **Global**: `~/.agents/skills` — skills shared across all your projects and
  other agent tools.

Each skill is a directory whose **name is the skill's identity**. A directory
named `rust-conventions` defines a skill called `rust-conventions`, regardless
of what its `SKILL.md` says. This is the same convention used by tools such as
Claude Code.

```text
.agents/skills/
├── rust-conventions/
│   └── SKILL.md
└── code-review/
    └── SKILL.md
```

## Project skills win over global skills

When a skill of the same name exists both in your project and globally, the
**project version wins**. This lets a repository override a shared skill with a
project-specific one. The global skill of that name is ignored entirely.

## Writing a `SKILL.md`

Every skill directory must contain a `SKILL.md` file. It has two parts: a YAML
front matter block with metadata, followed by the Markdown body — the
behavioural instructions.

```markdown
---
name: rust-conventions
description: Enforce the project's Rust style and clippy rules.
---

Follow the project rustfmt config. Run clippy with `-D warnings`.
Use `module_name.rs`, never `mod.rs`.
```

The front matter fields are:

- `name` — should match the directory name. If it differs, smista keeps the
  directory name as the identity and reports a warning.
- `description` — a one-line summary of what the skill does.

smista reads only the front matter when discovering skills, and reads the body
on demand when a skill is actually used. You can keep long instructions in a
skill without any cost until it is invoked.

## Warnings

Discovery never fails; instead, smista reports advisory warnings for skill
directories that look misconfigured.

| Warning              | Meaning                                                 | How to fix                                                     |
| -------------------- | ------------------------------------------------------- | -------------------------------------------------------------- |
| Missing `SKILL.md`   | A directory under a skills folder has no `SKILL.md`     | Add a `SKILL.md`, or remove the directory                      |
| Missing description  | The `SKILL.md` front matter has no `description`        | Add a `description` line to the front matter                   |
| Name mismatch        | The front matter `name` differs from the directory name | Rename the directory or the `name` so they match               |
| Invalid front matter | The `---` block is missing or is not valid YAML         | Add a valid `---` front matter block with `name`/`description` |
