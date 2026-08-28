# `knap index` reference

`knap index <path> [--json]`. Use it after editing a single doc to get just
its neighborhood — headings, outgoing links (resolved/broken), backlinks,
tags — without paging the whole workspace snapshot. A directory argument
returns the full-workspace snapshot instead.

## Single-doc shape: `knap index <file> --json`

```
$ knap index docs/index.md --json
{
  "path": "docs/index.md",
  "title": null,
  "tags": [],
  "headings": [
    {
      "text": "Index",
      "level": 1,
      "range": {
        "start": { "line": 0, "character": 0 },
        "end": { "line": 1, "character": 0 }
      }
    }
  ],
  "links": [
    { "target": "docs/missing.md", "anchor": null, "resolved": null }
  ],
  "backlinks": [],
  "hint": "broken link(s) found — run `knap lint --suggest` for fix candidates."
}
```

A link's `resolved` field is `null` when the target doesn't exist —
matching what a `knap lint` `broken-link` diagnostic would flag for the
same link.

## Workspace-wide shape: `knap index <dir> --json`

A directory argument (`.` included) returns every note's own single-doc
shape, nested under `notes`, plus a workspace-wide `tags` map instead of a
per-note `tags` array:

```
$ knap index . --json
{
  "notes": [
    {
      "path": "docs/index.md",
      "title": null,
      "tags": [],
      "headings": [ { "text": "Index", "level": 1, "range": { "...": "..." } } ],
      "links": [ { "target": "docs/missing.md", "anchor": null, "resolved": null } ],
      "backlinks": []
    }
  ],
  "tags": {},
  "hint": "broken link(s) found — run `knap lint --suggest` for fix candidates."
}
```

Each entry under `notes` is the same shape `knap index <file> --json`
returns for that one note, minus the top-level `hint` (there's only one
`hint`, at the envelope's top level).

## `hint`

`index`'s `hint` is present (`"broken link(s) found — run \`knap lint
--suggest\` for fix candidates."`) whenever any link in the indexed
scope — the one file, or any note under the indexed directory — has
`resolved: null`. It's absent when every link resolves.
