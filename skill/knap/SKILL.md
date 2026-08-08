---
name: knap
description: >
  Use knap's headless CLI (lint, fix, index, rename-*) to safely edit
  Markdown notes in a vault. Trigger whenever a task edits, creates, or
  restructures notes in a directory with a knap.toml or an existing
  note/link structure — verify every edit with `knap lint` and resolve
  what it flags with `knap fix` or `knap rename-*` before finishing.
---

# knap

knap is a linter, indexer, and refactoring tool for plain-Markdown notes
(`[text](path/to/note.md)` links, no wiki-link syntax). When editing notes in
a vault that has it installed, use its CLI to catch and fix broken links
instead of eyeballing the Markdown by hand.

## When to reach for this

Any task that edits, creates, moves, or deletes Markdown notes in a vault
with a `knap.toml` at its root, or that already has a recognizable
note/link structure. If `knap --help` runs successfully in the vault, use
the loop below rather than manually re-checking links.

## The edit → verify loop

1. Make your edit (create/edit/move a note, add or change a link).
2. `knap lint --json` (add `--since <git-ref>` to scope to files changed
   since a commit, instead of paging the whole vault on every check).
3. For each diagnostic, branch on its `code`:

   | `code`                    | Meaning                                      | Resolution                                  |
   | ------------------------- | --------------------------------------------- | -------------------------------------------- |
   | `broken-link`              | Link target file doesn't exist                | `knap fix` (creates a stub file)             |
   | `broken-anchor`            | `#slug` doesn't match any heading in the target | `knap fix` (rewrites to the closest heading, when unambiguous) |
   | `missing-frontmatter`      | Note has no frontmatter block at all, but the schema requires one | Add frontmatter by hand |
   | `missing-required-field`   | Frontmatter exists but a required key is absent | Add the field by hand |
   | `invalid-field-value`      | A field's value isn't in the schema's allowed list | Fix the value by hand |
   | `unknown-field`            | A frontmatter key isn't in the schema          | Fix the typo, or extend the schema           |

   `broken-link` and `broken-anchor` are the two codes `knap fix` can
   resolve automatically. The four frontmatter codes need a human/agent
   decision about the right value, so `knap fix` never touches them.
4. For a deliberate restructure (moving a file, renaming a heading or tag)
   rather than a lint fix, use the matching `knap rename-*` subcommand —
   it rewrites every affected link atomically, in one step, instead of
   leaving broken links for `knap fix` to patch up afterward:
   `knap rename-file <old> <new>`, `knap rename-heading <file> <old> <new>`,
   `knap rename-tag <old> <new>`.
5. `knap lint` again to confirm clean (exit code `0`, no output).

## Example: `--json` diagnostics with `code`

```
$ knap lint . --json
{
  "diagnostics": [
    {
      "path": "notes/index.md",
      "diagnostics": [
        {
          "range": { "start": { "line": 11, "character": 2 }, "end": { "line": 11, "character": 26 } },
          "severity": 2,
          "code": "broken-link",
          "source": "knap",
          "message": "Link target not found: 'notes/missing.md'"
        }
      ]
    }
  ],
  "problem_count": 1,
  "file_count": 1,
  "blocking_count": 1
}
```

Branch on `diagnostics[].diagnostics[].code`, not on `message` — the code
is stable across releases; the message text is not.

## Example: `blocking_count` vs. `problem_count`

`problem_count` is every diagnostic found; `blocking_count` is how many
are at or above the `--fail-on` threshold (default `warning`, so today the
two numbers are always equal — every diagnostic knap emits is a warning).
Use `--fail-on error` in a script that should only fail on the more severe
class of problem once one exists:

```
$ knap lint . --fail-on error --json
{ "...": "...", "problem_count": 3, "blocking_count": 0 }
$ echo $?
0
```

Exit code follows `blocking_count`, not `problem_count` — a non-zero
`problem_count` with `blocking_count: 0` still exits `0`.

## Inspecting one note: `knap index <file> --json`

After editing a single note, get just its neighborhood — headings, outgoing
links (resolved/broken), backlinks, tags — without paging the whole
workspace snapshot:

```
$ knap index notes/index.md --json
{
  "headings": [...],
  "links": [...],
  "backlinks": [...],
  "tags": [...]
}
```

(A directory argument, e.g. `knap index . --json`, still returns the
full-workspace `{ "notes": [...], "tags": {...} }` envelope.)

## Full reference

This skill covers the edit-verify loop `knap lint`/`knap fix`/`knap
rename-*` were built for. For every flag, exit code, and config option, see
the vault's `knap`-installing project's `README.md`, or run `knap <command>
--help`.
