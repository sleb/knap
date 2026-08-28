# `knap lint` reference

`knap lint <path> [--json] [--suggest[=N]] [--fail-on error|warning|info|hint] [--since <git-ref>]`.
`path` defaults to `.`; pass a single file to lint just that doc.

## Diagnostic codes

| `code`                   | Meaning                                                          | Resolution                                                                       |
| ------------------------ | ------------------------------------------------------------------ | -------------------------------------------------------------------------------- |
| `broken-link`            | Link target file doesn't exist                                   | Pick from `data.suggestions` (or override) → `repoint-link` in the apply batch   |
| `broken-anchor`          | `#slug` doesn't match any heading in the target                  | Pick from `data.suggestions` (or override) → `repoint-anchor` in the apply batch |
| `missing-frontmatter`    | Doc has no frontmatter block at all, but the schema requires one | Add frontmatter by hand                                                          |
| `missing-required-field` | Frontmatter exists but a required key is absent                  | Add the field by hand                                                            |
| `invalid-field-value`    | A field's value isn't in the schema's allowed list                | Fix the value by hand                                                            |
| `unknown-field`          | A frontmatter key isn't in the schema                             | Fix the typo, or extend the schema                                               |

Branch on `diagnostics[].diagnostics[].code`, not on `message` — the code is
stable across releases; the message text is not.

`data.suggestions` (from `--suggest`) gives you the ranked candidates for
`broken-link`/`broken-anchor`; pick one and apply it with a
`repoint-link`/`repoint-anchor` entry in a `knap apply --json` batch — see
`reference/apply.md`. The four frontmatter codes always need a human/agent
decision about the right value and are still fixed by hand.

## `--suggest`: how to pick, not just that you must

`data.suggestions` is ranked by a blended `combined` score that factors in
both the path edit distance and the link's own visible text — so the
closest-ranked candidate balances both signals but doesn't guarantee a
correct pick. Before repointing, read the link's own visible text (and, if
that's generic, the surrounding sentence) and check it against each
candidate's filename or heading.

Each suggestion carries `distance` (path edit distance) and `text_distance`
(visible link text vs. candidate name), plus the blended `combined` score
those two feed into internally (not itself part of the JSON — `distance`
and `text_distance` are what's exposed). Two candidates close together by
combined score count as a tie — treat it as ambiguous and don't repoint on
ranking alone. A diagnostic with no `data` field at all had zero candidates
in the workspace.

When `data.text_mismatch: true` is present, the ranking's own two signals
disagree — treat it as a hard stop, not a hint: don't repoint from
`suggestions[0]` when this is set without finding the right target yourself
(`grep`, `knap index`). Its absence doesn't guarantee the pick is right — it
only means the two signals agreed, and both can still be wrong together —
so the existing advice to read the link text against the candidate name
before repointing still applies to every diagnostic, not just flagged ones.

A link labeled `[Sync 835]` pointing at a candidate whose path is closer by
edit distance but whose name doesn't match the link text is a mismatch
worth noticing even though that candidate may be closer by path distance to
the broken target string. **Never mechanically apply `suggestions[0]` to
every diagnostic in a loop or script** — that reintroduces exactly the
false-positive risk a blind bulk auto-apply carries, with none of the
unambiguous-only discipline the ranking is built for — a script that always
takes `suggestions[0]` has no tie-safety at all. If no candidate's name
plausibly matches the link text, say so and go find the right target by
hand (`grep`, `knap index`) rather than picking the least-wrong option.

## Example: `--json` diagnostics with `code`

```
$ knap lint . --json
{
  "diagnostics": [
    {
      "path": "docs/index.md",
      "diagnostics": [
        {
          "range": {
            "start": { "line": 2, "character": 22 },
            "end": { "line": 2, "character": 37 }
          },
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
  "blocking_count": 1,
  "hint": "broken-link/broken-anchor diagnostics: rerun with --suggest and `knap apply` a candidate, or use `knap rename-file`/`rename-heading`/`rename-tag` if you're mid-restructure."
}
```

## Example: `--suggest` candidates for an ambiguous fix

```
$ knap lint . --json --suggest
{
  "diagnostics": [
    {
      "path": "docs/index.md",
      "diagnostics": [
        {
          "range": {
            "start": { "line": 2, "character": 22 },
            "end": { "line": 2, "character": 37 }
          },
          "severity": 2,
          "code": "broken-link",
          "source": "knap",
          "message": "Link target not found: 'docs/missing.md'",
          "data": {
            "suggestions": [
              { "distance": 7, "target": "mission.md", "text_distance": 10 },
              { "distance": 7, "target": "missions.md", "text_distance": 10 }
            ]
          }
        }
      ]
    }
  ],
  "problem_count": 1,
  "file_count": 1,
  "blocking_count": 1,
  "hint": "broken-link/broken-anchor diagnostics: rerun with --suggest and `knap apply` a candidate, or use `knap rename-file`/`rename-heading`/`rename-tag` if you're mid-restructure."
}
```

`mission.md` and `missions.md` are tied on both signals here — nothing in
the ranking alone picks between them; read the link's surrounding sentence
to decide, or ask.

## Example: `--suggest` with `text_mismatch` (signals disagree)

```
$ knap lint . --json --suggest
{
  "diagnostics": [
    {
      "path": "docs/index.md",
      "diagnostics": [
        {
          "range": {
            "start": { "line": 2, "character": 26 },
            "end": { "line": 2, "character": 42 }
          },
          "severity": 2,
          "code": "broken-link",
          "source": "knap",
          "message": "Link target not found: 'docs/sync-835.md'",
          "data": {
            "suggestions": [
              { "distance": 1, "target": "docs/sync-8350.md", "text_distance": 1 },
              { "distance": 6, "target": "archive/sync-835.md", "text_distance": 0 }
            ],
            "text_mismatch": true
          }
        }
      ]
    }
  ],
  "problem_count": 1,
  "file_count": 1,
  "blocking_count": 1,
  "hint": "broken-link/broken-anchor diagnostics: rerun with --suggest and `knap apply` a candidate, or use `knap rename-file`/`rename-heading`/`rename-tag` if you're mid-restructure."
}
```

Here, the path/slug signal ranks `docs/sync-8350.md` first (closest by
`distance`), but the link's visible text `[Sync 835]` aligns with
`archive/sync-835.md` (closest by `text_distance`, `0`). The
`text_mismatch: true` flag signals this disagreement — don't auto-apply the
first candidate without verifying the link's intent yourself.

## Example: `blocking_count` vs. `problem_count`

`problem_count` is every diagnostic found; `blocking_count` is how many are
at or above the `--fail-on` threshold (default `warning`, so by default the
two numbers are always equal — every diagnostic knap emits is a warning).
Use `--fail-on error` in a script that should only fail on the more severe
class of problem once one exists:

```
$ knap lint . --fail-on error --json
{ "...": "...", "problem_count": 1, "blocking_count": 0 }
$ echo $?
0
```

Exit code follows `blocking_count`, not `problem_count` — a non-zero
`problem_count` with `blocking_count: 0` still exits `0`. `hint` is present
here too — it's driven by which diagnostic `code`s are present, not by
`--fail-on`.
