# knap

![Version](https://img.shields.io/badge/version-0.10.2-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Tooling for Markdown notes, built on standard Markdown syntax with no
proprietary extensions. Its core is a
[Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server (`knap lsp`) that brings IDE-quality linking and navigation to any
LSP-compatible editor. The same engine is also available headlessly from the
command line — `knap lint` and `knap index --json` — for CI, scripts, and
coding agents that don't have an editor in the loop.

## What it does

knap uses plain `[text](path/to/note.md)` links. Notes stay valid Markdown that
renders correctly anywhere — GitHub, static site generators, other editors —
without knap present. The tooling provides the convenience; the files stay clean.
See [Architecture](docs/ARCHITECTURE.md) for the full design tenets.

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

### Workspace

- Incremental index — stays live as files change, no restart needed
- Configurable via `initializationOptions` or `knap.toml`: file extensions
  (`extensions`, e.g. `["md", "mdx"]`), new-note inbox folder (`newNoteDir` /
  `new_note_dir`), and frontmatter schema (`frontmatterSchema` /
  `frontmatter_schema`)

Works with any editor that speaks LSP: Neovim, VS Code, Helix, Zed, and others.
Dedicated extensions are available for [VS Code](https://github.com/sleb/vscode-knap) and [Zed](https://github.com/sleb/zed-knap).

## How it works

knap indexes your workspace on startup and keeps the index live via LSP file
change notifications. It requires no external tools and no editor-specific
plugins — just a standard LSP client configuration pointing at the server
binary.

Configuration (note subdirectory, file extensions, frontmatter schema) comes
from two sources: your editor's native LSP settings via
`initializationOptions`, or a `knap.toml` file at your workspace root. Both
are read by every command below — `knap lint` and `knap index` see the same
config an editor session would.

## Command-line usage

knap is a single binary with several subcommands. There is no default —
running `knap` with no subcommand prints usage and exits non-zero.

### `knap lsp`

Starts the LSP server on stdio. This is what your editor's LSP client should
invoke — see your editor's extension docs for how to point it at the `knap`
binary. (Prior to v0.11, bare `knap` did this; that fallback is gone —
editor extensions must invoke `knap lsp` explicitly.)

### `knap lint [path] [--json]`

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
`problem_count`, `file_count`). Exit code is `0` if no problems were found,
`1` otherwise.

### `knap index <path> [--json]`

Builds and prints the note index for a directory: notes, headings, links
(with resolved/broken status), backlinks, and tags. `--json` emits a
structured snapshot — handy for an agent to get a fast structural view of a
workspace without grepping.

### `knap.toml`

An optional project config file at a workspace root, read by `knap lsp`,
`knap lint`, and `knap index` alike (for `lint`/`index`, the root is the
target directory, or the parent directory when the target is a single
file):

```toml
extensions = ["md"]
new_note_dir = "inbox"

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

v0.10.2. See the [roadmap](docs/ROADMAP.md) for planned releases.

## Documentation

- [Getting Started](docs/GETTING_STARTED.md) — install the server, connect your
  editor, and understand what each feature does
- [User Stories](docs/USER_STORIES.md) — what knap does, told from the writer's
  perspective
- [Roadmap](docs/ROADMAP.md) — features grouped into releases, starting with the
  MVP
- [Architecture](docs/ARCHITECTURE.md) — component design and contracts
