---
name: knap
description: >
  Use knap's headless CLI (lint, index, rename-*, apply) to safely edit
  Markdown docs in a workspace. Trigger whenever a task edits, creates, or
  restructures docs in a directory with a knap.toml or an existing
  doc/link structure — verify every edit with `knap lint --suggest` and
  resolve what it reports with `knap rename-*`, `knap apply`, or a hand edit
  before finishing.
---

# knap

knap is a linter, indexer, and refactoring tool for plain-Markdown docs
(`[text](path/to/doc.md)` links, no wiki-link syntax). When editing docs in
a workspace that has it installed, use its CLI to catch and fix broken links
instead of eyeballing the Markdown by hand.

## When to reach for this

Any task that edits, creates, moves, or deletes Markdown docs in a workspace
with a `knap.toml` at its root, or that already has a recognizable
doc/link structure. If `knap --help` runs successfully in the workspace, use
the loop below rather than manually re-checking links.

## The edit → verify loop

Not every kind of change needs a lint check right after it — only the ones
that can actually leave something broken. Match the check to the edit:

- **Deliberate restructures** (moving a file, renaming a heading or tag) —
  use the matching `knap rename-*` subcommand instead of hand-editing:
  `knap rename-file <old> <new>`, `knap rename-heading <file> <old> <new>`,
  `knap rename-tag <old> <new>`. See `reference/rename.md` for argument
  shapes, what "atomic rewrite" covers, and failure-mode examples. Chain as
  many `rename-*` calls as the task needs back to back, with **no lint in
  between** — verify once at the end of the batch, not after each call.
- **Hand-edits** (a new doc, a manually added or rewritten link, a
  manually fixed target) — these are where mistakes actually happen, so
  lint right after each one:
  1. Make the edit.
  2. `knap lint --suggest --json` (read-only). Add
     `--since <git-ref>` to narrow the report to files changed since a
     commit.
  3. For each diagnostic in the report, branch on its `code`:

     | `code`                   | Meaning                                                          | Resolution                                                                       |
     | ------------------------ | ---------------------------------------------------------------- | -------------------------------------------------------------------------------- |
     | `broken-link`            | Link target file doesn't exist                                   | Pick from `data.suggestions` (or override) → `repoint-link` in the apply batch   |
     | `broken-anchor`          | `#slug` doesn't match any heading in the target                  | Pick from `data.suggestions` (or override) → `repoint-anchor` in the apply batch |
     | `missing-frontmatter`    | Doc has no frontmatter block at all, but the schema requires one | Add frontmatter by hand                                                          |
     | `missing-required-field` | Frontmatter exists but a required key is absent                  | Add the field by hand                                                          |
     | `invalid-field-value`    | A field's value isn't in the schema's allowed list               | Fix the value by hand                                                          |
     | `unknown-field`          | A frontmatter key isn't in the schema                             | Fix the typo, or extend the schema                                             |

     `data.suggestions` (from `--suggest`) gives you the ranked candidates
     for `broken-link`/`broken-anchor`; pick one and apply it with a
     `repoint-link`/`repoint-anchor` entry in a `knap apply --json` batch —
     see `reference/apply.md` for the round-trip example, the range-copy
     warning, and how rename ops compose with repoint ops in the same
     batch. The four frontmatter codes always need a human/agent decision
     about the right value and are still fixed by hand.

     For the full diagnostic-code table, the `--suggest` ranking/tie-break
     rules (`combined`/`distance`/`text_distance`, `text_mismatch`), and
     the `--json` examples, see `reference/lint.md`.

**Always finish with one `knap lint --json` (or plain `knap lint` — exit
code `0`, no output means clean) over the whole task's changes**, whichever
path was taken to get there — that's the check that actually backs a "no
broken links left" claim, and it's the one step there's no shortcut for.
`reference/lint.md` also covers `blocking_count` vs. `problem_count` for
scripting a stricter `--fail-on` threshold.

## Inspecting one doc: `knap index <file> --json`

After editing a single doc, get just its neighborhood — headings, outgoing
links (resolved/broken), backlinks, tags — without paging the whole
workspace snapshot. See `reference/index.md` for the single-doc vs.
workspace-wide `--json` shapes and `index`'s `hint` field.

## Full reference

This skill covers the edit-verify loop `knap lint`/`knap rename-*`/`knap
apply` were built for. For every flag, exit code, and config option, see
the workspace's `knap`-installing project's `README.md`, or run
`knap <command> --help`.
