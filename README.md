# knap

![Version](https://img.shields.io/badge/version-0.15.0-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Tooling that keeps linked Markdown notes correct — for the human writing them
and the agent editing alongside — built on standard Markdown syntax with no
proprietary extensions.

## Overview

A vault of linked notes breaks quietly: a file gets renamed and every link to
it goes stale, a heading gets reworded and its anchors dangle, a tag gets
retired in one note but not the other forty. knap keeps `[text](path.md)`
links, `#anchor`s, and frontmatter tags correct as both a human and a coding
agent edit the same files — the same index, the same diagnostics, the same
refactors, whichever one is holding the pen. Notes stay plain Markdown that
renders correctly anywhere — GitHub, static site generators, other editors —
without knap present; the tooling supplies the correctness, the files stay
clean. See [Architecture](docs/ARCHITECTURE.md) for the full design tenets.

knap ships as a single binary with two faces built on the same engine:

- **`knap lsp`** — for the human — a
  [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
  server that brings IDE-quality linking, navigation, and refactoring to any
  LSP-compatible editor: Neovim, VS Code, Helix, Zed, and others. Dedicated
  extensions are available for [VS Code](https://github.com/sleb/vscode-knap)
  and [Zed](https://github.com/sleb/zed-knap).
- **`knap lint` / `knap index` / `knap fix` / `knap rename-*` / `knap
apply`** — for the agent — the same checks and refactors, headlessly from
  the command line, so a coding agent without an editor in the loop can
  verify its own edits, fix or rename with the same guarantees a human gets
  from the LSP, and apply a whole batch of chosen changes in one call.

Both faces share one indexing and configuration core: `src/config.rs` loads
`initializationOptions` or `knap.toml` the same way for every entry point, and
the note index — files, headings, links, backlinks, tags — is built once and
reused across all commands. A human renaming a file in their editor and an
agent running `knap rename-file` get the identical rewrite; a diagnostic the
LSP would have squiggled is the same one `knap lint` reports.

## LSP server

`knap lsp` starts the server on stdio. This is what your editor's LSP client
should invoke — see your editor's extension docs for how to point it at the
`knap` binary. (Prior to v0.11, bare `knap` did this; that fallback is gone —
editor extensions must invoke `knap lsp` explicitly.) The index stays live as
files change, so there's no restart needed after edits.

### Linking & completions

- **Path completions** — type `(` inside a Markdown link for a directory
  browser; drill into subfolders one level at a time, or type any filename
  segment to jump directly to any note or attachment in the workspace (images
  and PDFs included)
- **Anchor completions** — type `#` after a file path to pick from that file's
  headings, or `[text](#` to pick from the current file's headings; inserts the
  GFM slug automatically (`## My Section` → `my-section`)

### Navigation

- **Go to Definition** — jump to the linked note; navigates to the heading line
  when an anchor is present (`[text](note.md#heading)` or `[text](#heading)` for
  same-file headings)
- **Document Symbols** — outline of every heading in the current file, jumpable
  from your editor's symbol panel
- **Workspace Symbols** — fuzzy-search headings across the entire vault

### Frontmatter schema

- **Key completions** — define allowed keys in `frontmatterSchema`; typing in
  a frontmatter key position offers unused schema keys as `FIELD` items
- **Value completions** — when a key has a `values` list, typing after the `:`
  offers allowed values as `VALUE` items with prefix filtering
- **Schema diagnostics** — warnings for required keys that are absent, values
  outside the allowed list (exact-case), unknown keys (opt-in via
  `warnOnUnknownKeys`), and notes missing a frontmatter block entirely (opt-in
  via `requireFrontmatter`)

### Tags

- **Tag completions** — inside a frontmatter `tags:` value, your workspace tag
  index appears as a pick list; already-used tags are excluded and prefix
  filtering narrows results as you type
- **Find References on a tag** — shows every note that carries the tag, with
  each result pointing directly at the tag range
- **Go to Definition on a tag** — same set of locations, letting you jump to
  any note using the tag
- **Workspace Symbols includes tags** — tags appear alongside headings in the
  symbol search with `SymbolKind::KEY` so editors can style them distinctly

### Editor experience

- **Folding ranges** — heading sections and fenced code blocks fold in any
  editor that supports `textDocument/foldingRange`
- **Selection range** — smart expand/contract grows the selection through
  word → link → paragraph → heading section → document
- **Inlay hints** — linked notes with a `title:` frontmatter field show the
  title inline next to the link path (e.g. `-> My Note`)

### Backlinks & code lens

- **Backlinks code lens** — a `↑ N backlinks` annotation above the first line
  of any note with incoming links; click to open the References panel in VS Code
- **Heading anchor-link lens** — headings that are anchor targets show
  `↑ N anchor link(s)` counting both same-file and cross-file `#slug` references

### Finding references

- **Find References** — every standard Markdown link pointing to the current
  file; on a heading, collects same-file bare anchors and cross-file anchors to
  that heading; or every note using a tag when the cursor is on a tag value

### Refactoring

- **Rename a file** — all incoming and outgoing links rewritten atomically via
  `workspace/willRenameFiles`
- **Rename a heading** — all `[text](note.md#old-slug)` anchor links updated in
  place to the new slug
- **Rename a tag** — every frontmatter occurrence of the tag across the workspace
  updated atomically; rename dialog pre-fills with the current tag text; all
  three YAML tag forms supported (bare scalar, inline list, block list)

### Diagnostics & fixes

- **Broken link diagnostics** — warnings for links to missing files, cross-file
  missing anchors, and same-file bare anchors that don't match any heading;
  attachment links (images, PDFs) resolve against the full workspace
- **Quick Fix** — create a missing file from a broken link, or pick a valid
  heading to replace a broken anchor; both via standard `textDocument/codeAction`

## Linter (`knap lint`)

Headless diagnostics — the same broken-link, broken-anchor, and frontmatter
checks the editor shows, without a running LSP session. Useful in CI or for
an agent to check whether its edits broke any links.

```
$ knap lint .
notes/index.md:12:3: warning: broken link to 'notes/missing.md'

1 problem(s) in 1 file(s)
```

`path` defaults to the current directory; pass a single file to lint just
that file. `--json` emits a machine-readable report (`diagnostics`,
`problem_count`, `file_count`, `blocking_count`). Every diagnostic carries a
stable `code` (`broken-link`, `broken-anchor`, `missing-frontmatter`,
`missing-required-field`, `invalid-field-value`, `unknown-field`) — branch
on `code`, not on the message text, when scripting against `--json` output.
Exit code is `0` if no problems were found, `1` otherwise.

Usage: `knap lint [path] [--json] [--fail-on <severity>] [--since <git-ref>] [--suggest [N]] [--fix]`

- `--fail-on <severity>` — minimum severity that causes a non-zero exit
  (`error`, `warning` (default), `info`, or `hint`). `blocking_count` in
  `--json` output counts diagnostics at or above this threshold; exit code
  follows `blocking_count`, not `problem_count`. Every diagnostic knap
  emits today is `warning`, so `--fail-on warning` (the default) preserves
  today's exit behavior exactly, and `--fail-on error` always passes until
  some future diagnostic is promoted to `error`.
- `--since <git-ref>` — scope linting to files changed since `<git-ref>`
  (tracked diffs plus untracked new files), instead of the whole workspace.
  Requires a git repository; errors otherwise.
- `--suggest [N]` — attach up to `N` ranked candidate fixes to each
  `broken-link`/`broken-anchor` diagnostic's `data` field in `--json` output,
  bare `--suggest` defaults to 3. Ranking blends two distance signals: how
  close the broken target/slug is to each candidate's path or heading
  (`distance`), and how close the link's own visible text is to each
  candidate's name (`text_distance`, `null` when the link has no usable
  text). This is the same ranking `knap fix` uses to decide whether a fix is
  unambiguous — `--suggest` just shows the whole ranked list instead of
  collapsing it to one answer, so an agent that's already running
  `knap lint --json` to verify an edit gets the candidates for the ambiguous
  cases `fix` declined to touch, in the same call, instead of needing a
  separate `knap fix --dry-run` round-trip. When the two signals disagree on
  the top candidate, the diagnostic's `data` also carries
  `"text_mismatch": true` — a signal that the top-ranked candidate may be
  wrong even though it looks unambiguous by path distance alone:
  ```
  $ knap lint . --json --suggest
  { "...": "...",
    "code": "broken-link",
    "data": { "suggestions": [
      { "target": "reference/config.md", "distance": 8, "text_distance": 0 },
      { "target": "reference/cache.md", "distance": 11, "text_distance": 6 }
    ] } }
  ```
- `--fix` — apply every safe fix `knap fix` would make _before_ computing
  the report, so the diagnostics shown are what's actually left rather than
  what was true when the command started. Collapses the usual `lint` →
  `fix` → `lint`-again sequence an agent would otherwise run after every
  hand-edit into one call. The fix pass itself always runs over the whole
  `path` root regardless of `--since` (a fix elsewhere can resolve a
  diagnostic in a file `--since` wouldn't otherwise show); only the report
  is `--since`-scoped. `--json` output gains a `fixes_applied` field listing
  what was applied — the one case where `knap lint` mutates files on disk:
  ```
  $ knap lint . --json --fix --suggest
  { "...": "...", "problem_count": 1, "fixes_applied": [
    "notes/a.md: repoint 'notes/old-name.md' → 'notes/new-name.md'"
  ] }
  ```

## Indexer (`knap index`)

Builds and prints the note index for a directory: notes, headings, links
(with resolved/broken status), backlinks, and tags. `--json` emits a
structured snapshot — handy for an agent to get a fast structural view of a
workspace without grepping.

Usage: `knap index <path> [--json]`

When `<path>` is a single file, `knap index` scopes to that one note's
neighborhood instead: `--json` emits a single note object (`headings`,
`links`, `backlinks`, `tags`) rather than the `{ "notes": [...], "tags":
{...} }` workspace envelope. Useful for an agent to inspect just-edited
note without paging the full index. A directory `<path>` prints the
full-workspace listing, unchanged.

## Fixing (`knap fix`)

Applies safe quick fixes headlessly: retarget a broken link to the one
existing note that's an unambiguous best match (by the same combined
path/text-distance ranking `knap lint --suggest` exposes), falling back to
creating the missing file when no candidate is unambiguous; retarget a
broken anchor to the one heading in its target note that's an unambiguous
best match. Fixes that would be ambiguous — a link or anchor equally close
to two or more candidates, or one where the broken target's path distance
and the link's own visible text disagree on the winner (`text_mismatch`) —
are left alone for a human (or an agent reading `knap lint --suggest`'s
ranked candidates, including each one's `text_distance`) to resolve by
hand.

```
$ knap fix --dry-run
would notes/a.md: repoint 'notes/old-name.md' → 'notes/new-name.md'
would create notes/missing.md
would notes/a.md: anchor '#old-slug' → '#current-slug'

$ knap fix
applied 3 fix(es) in 3 file(s)
notes/a.md: repoint 'notes/old-name.md' → 'notes/new-name.md'
create notes/missing.md
notes/a.md: anchor '#old-slug' → '#current-slug'
```

`path` defaults to the current directory; pass a single file to fix just
that file's links. `--dry-run` prints the plan without changing anything on
disk.

Usage: `knap fix [path] [--dry-run]`

## Headless rename (`knap rename-*`)

The same rename computations the editor's rename dialog uses
(`workspace/rename`, `workspace/willRenameFiles`), run against a workspace
without a client — the edit is computed and written to disk in one step.
Useful for scripting a rename across a vault, or for an agent that needs to
retarget links without opening an editor.

```
$ knap rename-file notes/old.md notes/new.md
notes/old.md → notes/new.md (2 file(s) touched)

$ knap rename-heading notes/a.md "Old Section" "New Section"
"Old Section" → "New Section" in notes/a.md (2 file(s) touched)

$ knap rename-tag draft published
#draft → #published (3 file(s) touched)
```

- `knap rename-file <old> <new>` (alias: `knap move-file`) — moves `<old>` to
  `<new>` on disk and rewrites every incoming and outgoing link affected by
  the move. Fails if `<old>` doesn't exist or `<new>` already does.
- `knap rename-heading <file> <old> <new>` — renames a heading in `<file>`
  (`<old>` may be the heading's text or its GFM slug) and rewrites every
  same-file and cross-file anchor link that targets it. Fails if no heading
  in `<file>` matches `<old>`.
- `knap rename-tag <old> <new>` — rewrites every frontmatter occurrence of
  `<old>` across the workspace, in all three YAML tag forms (bare scalar,
  inline list, block list). Fails if no note uses `<old>`.

All three scope their index to the current directory (like `knap lint .`),
apply their edit atomically, and print a summary line on success.

## Batch apply (`knap apply`)

Applies a whole set of change operations in one call instead of one
subprocess per change. Reads a JSON array of operations from stdin — one
`rename-file`/`rename-heading`/`rename-tag`/`fix`/`repoint-link`/
`repoint-anchor` per entry — and applies them in order, all-or-nothing: the
workspace ends up either fully changed or untouched, never partially
applied.

```
$ echo '[
  {"op":"rename-tag","old":"wip","new":"draft"},
  {"op":"fix"}
]' | knap apply
applied rename-tag: #wip → #draft
applied fix: applied 2 fix(es) in 2 file(s)
2 operation(s), 3 file(s) touched
```

- `rename-file`/`rename-heading`/`rename-tag`/`fix` — same field names as
  that subcommand's arguments.
- `repoint-link { file, range, target }` / `repoint-anchor { file, range,
anchor }` — retarget a broken link or anchor at exactly `range` (a
  diagnostic's own range, e.g. from `knap lint --suggest --json`) to a
  candidate you've already picked, rather than letting `fix` re-derive one.
  Useful for the ambiguous or `text_mismatch` cases `knap fix` declines to
  touch on its own:
  ```
  $ echo '[{"op":"repoint-link","file":"notes/a.md",
    "range":{"start":{"line":0,"character":7},"end":{"line":0,"character":18}},
    "target":"reference/config.md"}]' | knap apply
  applied repoint-link: notes/a.md: repoint → 'reference/config.md'
  1 operation(s), 1 file(s) touched
  ```

Useful after `knap lint --suggest --json` — pick the right fix for each
finding yourself, then apply the whole batch in one call rather than
shelling out once per change.

- `--dry-run` — prints the plan without changing anything in the real
  workspace; the reported file count is exactly what a real run would touch.
- `--json` — emits an `ApplyReport` (`dry_run`, `operations`, `files_touched`)
  instead of the text summary.

Operations run in the order given against a scratch copy of the workspace,
so a later operation in the same batch sees an earlier one's effects (e.g.
renaming a heading in a file the same batch just renamed). If any operation
fails, nothing in the real workspace is touched — the same all-or-nothing
guarantee `rename-*` gives a single operation, extended across the whole
batch.

Usage: `knap apply [--dry-run] [--json]`

## Coding agents

This is the other half of the synergy: an agent editing a vault a human also
writes in shouldn't have to re-derive knap's conventions from `--help` text,
and shouldn't leave broken links behind for the human to find later.
`skill/knap/SKILL.md` documents the `lint` → `fix`/`rename-*` → `lint`
edit-verify loop for a coding agent working in a vault that has `knap`
installed — the six diagnostic `code`s, `--fail-on`/`--since`, and
`knap index <file> --json` for a fast, scoped read of just the note it
touched. Copy it into a vault's skill directory to teach an agent knap's
conventions directly:

```
cp -r skill/knap ~/.claude/skills/
# or, project-scoped:
cp -r skill/knap <vault>/.claude/skills/
```

## Configuration

Configuration (note subdirectory, file extensions, frontmatter schema) comes
from two sources, both read by every command above — `knap lint` and `knap
index` see the same config an editor session would:

- your editor's native LSP settings via `initializationOptions`, or
- a `knap.toml` file at your workspace root

### `knap.toml`

An optional project config file at a workspace root, read by `knap lsp`,
`knap lint`, and `knap index` alike (for `lint`/`index`, the root is the
target directory, or the parent directory when the target is a single
file):

```toml
extensions = ["md"]
new_note_dir = "inbox"
exclude = ["tests/fixtures/**"]

[frontmatter_schema]
require_frontmatter = false
warn_unknown_keys = false

[frontmatter_schema.fields.title]
required = true

[frontmatter_schema.fields.status]
values = ["draft", "published"]
```

When running under `knap lsp`, `initializationOptions` from the editor
layers over `knap.toml` field-by-field — the editor value wins where
present, `knap.toml` fills in the rest.

## Status

v0.15.0 — Judged Repoints & Text-Aware Repoint Ranking: `knap apply` gains
`repoint-link`/`repoint-anchor` operations, applying an agent-picked
candidate from `lint --suggest` directly at a diagnostic's range inside an
all-or-nothing batch; and repoint ranking now blends the broken
link/anchor's path or slug distance with how close the link's own visible
text is to each candidate, declining to auto-apply (`knap fix`/
`lint --fix`) and flagging `text_mismatch: true` (`lint --suggest`) when the
two signals disagree. See the [roadmap](docs/ROADMAP.md) for planned
releases.

## Documentation

- [Getting Started](docs/GETTING_STARTED.md) — install the server, connect your
  editor, and understand what each feature does
- [User Stories](docs/USER_STORIES.md) — what knap does, told from the writer's
  perspective
- [Roadmap](docs/ROADMAP.md) — features grouped into releases, starting with the
  MVP
- [Architecture](docs/ARCHITECTURE.md) — component design and contracts
