# `knap apply` reference

`knap apply [--dry-run] [--json]` reads a JSON array of change operations
from stdin and applies them all-or-nothing against the current workspace:
if any operation fails, nothing on disk changes. `--dry-run` reports what
would happen without writing anything.

An operation is one of:

- `{"op":"rename-file","old":<path>,"new":<path>}`
- `{"op":"rename-heading","file":<path>,"old":<text>,"new":<text>}`
- `{"op":"rename-tag","old":<tag>,"new":<tag>}`
- `{"op":"repoint-link","file":<path>,"range":<range>,"target":<path>}`
- `{"op":"repoint-anchor","file":<path>,"range":<range>,"anchor":<slug>}`

`rename-*` ops compose with `repoint-*` ops in the same batch — mix as many
operations as the task needs into one array; `apply` applies all of them or
none.

## Example: `lint --suggest` → pick → `apply` round trip

Given an ambiguous `broken-link` diagnostic from `knap lint --json
--suggest` (`docs/index.md`, range `2:14`–`2:29`, candidate
`docs/mission.md`), pick a target and repoint it at that exact range:

```
$ echo '[{"op":"repoint-link","file":"docs/index.md","range":{"start":{"line":2,"character":14},"end":{"line":2,"character":29}},"target":"docs/mission.md"}]' \
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
  "files_touched": 1,
  "hint": "run `knap lint` to confirm the workspace is clean."
}
```

`repoint-anchor` works the same way, with an `anchor` field instead of
`target` (a leading `#` on `anchor` is optional — `knap apply` strips it
either way):

```
$ echo '[{"op":"repoint-anchor","file":"docs/a.md","range":{"start":{"line":2,"character":25},"end":{"line":2,"character":38}},"anchor":"dashboard-overview"}]' \
  | knap apply --json
{
  "dry_run": false,
  "operations": [
    {
      "op": "repoint-anchor",
      "summary": "docs/a.md: anchor → '#dashboard-overview'",
      "files_touched": 1
    }
  ],
  "files_touched": 1,
  "hint": "run `knap lint` to confirm the workspace is clean."
}
```

## Never re-derive `range` by hand

Both operations use the diagnostic's own `range` — copy it byte-for-byte
into the apply operation. **Never re-derive `range`'s `line`/`character`
values by hand, and never recompute them from a separately re-read copy of
the file** — even a small skew (off by one character, a stray `+1`) can eat
part of the link's syntax (its closing `)`, for instance) and leave behind
text a markdown parser no longer recognizes as a link at all, which `knap
lint` then reports as clean because it never sees a link there to check.
`knap apply` rejects an operation that would do this — but the fix is cheap
enough that you shouldn't rely on the rejection catching it: copy `range`
as-is.

```
$ echo '[{"op":"repoint-anchor","file":"docs/a.md","range":{"start":{"line":2,"character":25},"end":{"line":2,"character":39}},"anchor":"dashboard-overview"}]' \
  | knap apply --json
Error: operation 1 (repoint-anchor)

Caused by:
    0: repoint-anchor produced unparseable markdown — re-fetch range from a fresh `knap lint --suggest --json` rather than adjusting it by hand
    1: no well-formed link found at line 3 after the edit — the markdown is likely corrupted (e.g. a missing closing ')')
```

If you see this error, re-fetch the diagnostic's `range` from a fresh `knap
lint --suggest --json` rather than adjusting the numbers you already have —
an adjusted-but-still-guessed range is exactly how this happens in the
first place.

## `hint`

`apply`'s `hint` is present whenever the run wasn't `--dry-run`, and always
reads: `run \`knap lint\` to confirm the workspace is clean.` — under
`--dry-run` nothing was written, so there's nothing for a follow-up lint to
confirm, and `hint` is absent (`null`/omitted) instead.
