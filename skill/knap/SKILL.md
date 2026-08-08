---
name: knap
description: >
  Use knap's headless CLI (lint, fix, index, rename-*) to safely edit
  Markdown notes in a vault. Trigger whenever a task edits, creates, or
  restructures notes in a directory with a knap.toml or an existing
  note/link structure — verify every edit with `knap lint --fix --suggest`
  and resolve what it can't fix itself with `knap rename-*` or a hand edit
  before finishing.
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

Not every kind of change needs a lint check right after it — only the ones
that can actually leave something broken. Match the check to the edit:

- **Deliberate restructures** (moving a file, renaming a heading or tag) —
  use the matching `knap rename-*` subcommand instead of hand-editing:
  `knap rename-file <old> <new>`, `knap rename-heading <file> <old> <new>`,
  `knap rename-tag <old> <new>`. Each rewrites every affected link
  atomically, in one step — there's nothing a lint check immediately after
  one of these would catch that the command's own contract didn't already
  guarantee. Chain as many `rename-*` calls as the task needs back to back,
  with **no lint in between** — verify once at the end of the batch, not
  after each call.
- **Hand-edits** (a new note, a manually added or rewritten link, a
  manually fixed target) — these are where mistakes actually happen, so
  lint right after each one:
  1. Make the edit.
  2. `knap lint --fix --suggest --json`. One call does three things: apply
     every unambiguous fix `knap fix` would make, then report what's left —
     so what you get back is the _post-fix_ state, not the state your edit
     actually left behind. Add `--since <git-ref>` to narrow the _report_ to
     files changed since a commit instead of paging the whole vault on every
     check (`--fix` itself always runs over the whole vault regardless —
     a fix elsewhere, e.g. a file `--since` wouldn't consider "yours", can
     still be what resolves a diagnostic in a file that is).
  3. For each diagnostic still in the report, branch on its `code`:

     | `code`                   | Meaning                                                           | Resolution                                                           |
     | ------------------------ | ----------------------------------------------------------------- | -------------------------------------------------------------------- |
     | `broken-link`            | Link target file doesn't exist                                    | Ambiguous — pick from `data.suggestions`, or none exist: fix by hand |
     | `broken-anchor`          | `#slug` doesn't match any heading in the target                   | Ambiguous — pick from `data.suggestions`, or none exist: fix by hand |
     | `missing-frontmatter`    | Note has no frontmatter block at all, but the schema requires one | Add frontmatter by hand                                              |
     | `missing-required-field` | Frontmatter exists but a required key is absent                   | Add the field by hand                                                |
     | `invalid-field-value`    | A field's value isn't in the schema's allowed list                | Fix the value by hand                                                |
     | `unknown-field`          | A frontmatter key isn't in the schema                             | Fix the typo, or extend the schema                                   |

     Any `broken-link`/`broken-anchor` with a single unambiguous closest
     match was already applied by the `--fix` pass above and won't appear
     here at all — what's left is genuinely ambiguous (two or more equally
     close candidates) or has no candidate to begin with. `data.suggestions`
     (from `--suggest`) gives you the ranked candidates for the ambiguous
     case; pick one and edit the link by hand. The four frontmatter codes
     always need a human/agent decision about the right value, so `--fix`
     never touches them regardless.

**Always finish with one `knap lint --json` (or plain `knap lint` — exit
code `0`, no output means clean) over the whole task's changes**, whichever
path was taken to get there — that's the check that actually backs a "no
broken links left" claim, and it's the one step there's no shortcut for.

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

## Example: `--suggest` candidates for an ambiguous fix

```
$ knap lint . --json --suggest
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
          "message": "Link target not found: 'notes/missing.md'",
          "data": {
            "suggestions": [
              { "target": "notes/mission.md", "distance": 2 },
              { "target": "notes/missions.md", "distance": 3 }
            ]
          }
        }
      ]
    }
  ],
  "problem_count": 1,
  "file_count": 1,
  "blocking_count": 1
}
```

`data.suggestions` lists every candidate `--suggest` found (closest first,
capped at N), whether or not it's ambiguous — it's the same ranking `knap
fix` uses to decide, not just the leftovers. Two candidates this close
together means `knap fix` would leave this one alone; if `suggestions[0]`
were strictly closer than `suggestions[1]`, `fix` would already have applied
it. A diagnostic with no `data` field at all had zero candidates in the
vault — `knap fix`'s create-a-stub case.

## Example: `--fix` applies first, then reports what's left

```
$ knap lint . --fix --suggest --json
{
  "diagnostics": [
    {
      "path": "notes/index.md",
      "diagnostics": [
        {
          "...": "...",
          "code": "broken-anchor",
          "message": "Heading not found: '#c'",
          "data": { "suggestions": [
            { "target": "#a", "distance": 1 },
            { "target": "#b", "distance": 1 }
          ] }
        }
      ]
    }
  ],
  "problem_count": 1,
  "file_count": 1,
  "blocking_count": 1,
  "fixes_applied": [
    "notes/index.md: repoint 'notes/old-name.md' → 'notes/new-name.md'"
  ]
}
```

`fixes_applied` (only present when `--fix` was passed) lists every fix that
was actually applied to disk before this report was computed — the broken
link in this example is already gone by the time you see the JSON. The one
diagnostic still shown is the one `--fix` couldn't resolve on its own (a tie
between `#a` and `#b`); its `data.suggestions` is where you pick from.
`--fix` mutates files — this is the one case where `knap lint` isn't
read-only.

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
