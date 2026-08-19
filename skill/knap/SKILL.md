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
  `knap rename-tag <old> <new>`. Each rewrites every affected link
  atomically, in one step — there's nothing a lint check immediately after
  one of these would catch that the command's own contract didn't already
  guarantee. Chain as many `rename-*` calls as the task needs back to back,
  with **no lint in between** — verify once at the end of the batch, not
  after each call.
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
     | `missing-required-field` | Frontmatter exists but a required key is absent                  | Add the field by hand                                                            |
     | `invalid-field-value`    | A field's value isn't in the schema's allowed list               | Fix the value by hand                                                            |
     | `unknown-field`          | A frontmatter key isn't in the schema                            | Fix the typo, or extend the schema                                               |

     `data.suggestions` (from `--suggest`) gives you the ranked candidates
     for `broken-link`/`broken-anchor`; pick one and apply it with a
     `repoint-link`/`repoint-anchor` entry in a `knap apply --json` batch —
     see the example below. The four frontmatter codes always need a
     human/agent decision about the right value and are still fixed by hand.

     **How to pick, not just that you must:** `data.suggestions` is ranked
     by a blended `combined` score that factors in both the path edit distance
     and the link's own visible text — so the closest-ranked candidate balances
     both signals but doesn't guarantee a correct pick. Before repointing, read
     the link's own visible text (and, if that's generic, the surrounding
     sentence) and check it against each candidate's filename or heading.

     When `data.text_mismatch: true` is present, the ranking's own two signals
     disagree — treat it as a hard stop, not a hint: don't repoint from
     `suggestions[0]` when this is set without finding the right target yourself
     (`grep`, `knap index`). Its absence doesn't guarantee the pick is right —
     it only means the two signals agreed, and both can still be wrong together —
     so the existing advice to read the link text against the candidate name
     before repointing still applies to every diagnostic, not just flagged ones.

     A link labeled `[Sync 835]` pointing at a candidate named `sync-800.md` is
     a mismatch worth noticing even though `sync-800.md` may be closer by path
     distance to the broken target string. **Never mechanically apply
     `suggestions[0]` to every diagnostic in a loop or script** — that
     reintroduces exactly the false-positive risk a blind bulk auto-apply
     carries, with none of the unambiguous-only discipline the ranking is
     built for — a script that always takes `suggestions[0]` has no
     tie-safety at all. If no candidate's name
     plausibly matches the link text, say so and go find the right target
     by hand (`grep`, `knap index`) rather than picking the least-wrong
     option.

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
      "path": "docs/index.md",
      "diagnostics": [
        {
          "range": { "start": { "line": 11, "character": 2 }, "end": { "line": 11, "character": 26 } },
          "severity": 2,
          "code": "broken-link",
          "source": "knap",
          "message": "Link target not found: 'docs/missing.md'"
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
      "path": "docs/index.md",
      "diagnostics": [
        {
          "range": { "start": { "line": 11, "character": 2 }, "end": { "line": 11, "character": 26 } },
          "severity": 2,
          "code": "broken-link",
          "source": "knap",
          "message": "Link target not found: 'docs/missing.md'",
          "data": {
            "suggestions": [
              { "target": "docs/mission.md", "distance": 2, "text_distance": 5 },
              { "target": "docs/missions.md", "distance": 3, "text_distance": 6 }
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

## Example: `--suggest` with `text_mismatch` (signals disagree)

When the ranking's two signals disagree on which candidate is best, `text_mismatch: true` is set as a warning flag:

```json
{
  "diagnostics": [
    {
      "path": "docs/index.md",
      "diagnostics": [
        {
          "range": {
            "start": { "line": 5, "character": 1 },
            "end": { "line": 5, "character": 19 }
          },
          "severity": 2,
          "code": "broken-link",
          "source": "knap",
          "message": "Link target not found: 'docs/sync-835.md'",
          "data": {
            "suggestions": [
              { "target": "sync-800.md", "distance": 1, "text_distance": 2 },
              {
                "target": "archive/sync-835.md",
                "distance": 9,
                "text_distance": 0
              }
            ],
            "text_mismatch": true
          }
        }
      ]
    }
  ]
}
```

Here, the path/slug signal picks `sync-800.md` (closest by `distance`), but the link's visible text `[Sync 835]` aligns with `archive/sync-835.md` (closest by `text_distance`). The `text_mismatch: true` flag signals this disagreement — don't auto-apply the first candidate without verifying the link's intent yourself.

`data.suggestions` lists every candidate `--suggest` found, ranked by their
blended `combined` score (path distance + text distance), closest first, capped
at N. This is the ranking `--suggest` exposes in full, not just a filtered
leftover list. Each suggestion carries both `distance` (path edit distance) and
`text_distance` (visible link text vs. candidate name). Two candidates this
close together by combined score count as a tie — treat it as ambiguous and
don't repoint on ranking alone. A diagnostic with no `data` field at all had
zero candidates in the workspace.

## Example: `lint --suggest` → pick → `apply` round trip

Given the ambiguous `broken-link` diagnostic from the `--suggest` example
above (`docs/index.md`, range `11:2`–`11:26`, candidates `docs/mission.md`
and `docs/missions.md`), pick a target and repoint it at that exact range:

```
$ echo '[{"op":"repoint-link","file":"docs/index.md","range":{"start":{"line":11,"character":2},"end":{"line":11,"character":26}},"target":"docs/mission.md"}]' \
  | knap apply --json
{
  "dry_run": false,
  "operations": [
    {
      "op": "repoint-link",
      "summary": "docs/index.md: repoint → 'docs/mission.md'",
      "files_touched": 1
    }
  ],
  "files_touched": 1
}
```

`repoint-anchor` works the same way, with an `anchor` field instead of
`target` (a leading `#` on `anchor` is optional — `knap apply` strips it
either way). Both operations use the diagnostic's own `range` — copy it
byte-for-byte into the apply operation. **Never re-derive `range`'s
`line`/`character` values by hand, and never recompute them from a
separately re-read copy of the file** — even a small skew (off by one
character, a stray `+1`) can eat part of the link's syntax (its closing `)`,
for instance) and leave behind text a markdown parser no longer recognizes
as a link at all, which `knap lint` then reports as clean because it never
sees a link there to check. `knap apply` rejects an operation that would do
this (see the "produced unparseable markdown" error below) — but the fix is
cheap enough that you shouldn't rely on the rejection catching it: copy
`range` as-is.

```
$ echo '[{"op":"repoint-anchor","file":"a.md","range":{"start":{"line":11,"character":35},"end":{"line":11,"character":43}},"anchor":"dashboard-overview"}]' \
  | knap apply --json
Error: operation 1 (repoint-anchor)

Caused by:
    0: repoint-anchor produced unparseable markdown
    1: no well-formed link found at line 12 after the edit — the markdown is likely corrupted (e.g. a missing closing ')')
```

If you see this error, re-fetch the diagnostic's `range` from a fresh
`knap lint --suggest --json` rather than adjusting the numbers you already
have — an adjusted-but-still-guessed range is exactly how this happens in
the first place.

They compose with `rename-*` in the same batch — mix as many operations as
the task needs into one array; `apply` applies all of them or none.

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

## Inspecting one doc: `knap index <file> --json`

After editing a single doc, get just its neighborhood — headings, outgoing
links (resolved/broken), backlinks, tags — without paging the whole
workspace snapshot:

```
$ knap index docs/index.md --json
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

This skill covers the edit-verify loop `knap lint`/`knap rename-*`/`knap
apply` were built for. For every flag, exit code, and config
option, see the workspace's `knap`-installing project's `README.md`, or run
`knap <command> --help`.
