# knap

![Version](https://img.shields.io/badge/version-0.8.0-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A
[Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server for Markdown. It brings IDE-quality linking and navigation to any
LSP-compatible editor — using standard Markdown syntax, no proprietary
extensions.

## Philosophy

knap uses plain `[text](path/to/note.md)` links. Notes stay valid Markdown that
renders correctly anywhere — GitHub, static site generators, other editors —
without knap present. The tooling provides the convenience; the files stay clean.
See [Architecture](docs/ARCHITECTURE.md) for the full design tenets.

## What it does

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
  outside the allowed list (exact-case), and unknown keys (opt-in via
  `warnOnUnknownKeys`)

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

### Backlinks

- **Backlinks code lens** — a `↑ N backlinks` annotation above the first line
  of any note with incoming links; click to open the References panel in VS Code

### Finding references

- **Find References** — every standard Markdown link pointing to the current
  file; on a heading, collects same-file bare anchors and cross-file anchors to
  that heading; or every note using a tag when the cursor is on a tag value

### Refactoring

- **Rename a file** — all incoming and outgoing links rewritten atomically via
  `workspace/willRenameFiles`
- **Rename a heading** — all `[text](note.md#old-slug)` anchor links updated in
  place to the new slug

### Diagnostics & fixes

- **Broken link diagnostics** — warnings for links to missing files, cross-file
  missing anchors, and same-file bare anchors that don't match any heading;
  attachment links (images, PDFs) resolve against the full workspace
- **Quick Fix** — create a missing file from a broken link, or pick a valid
  heading to replace a broken anchor; both via standard `textDocument/codeAction`

### Workspace

- Incremental index — stays live as files change, no restart needed
- Configurable file extensions (e.g. `.md`, `.mdx`), new-note inbox folder
  (`newNoteDir`), and frontmatter schema (`frontmatterSchema`) via
  `initializationOptions`

Works with any editor that speaks LSP: Neovim, VS Code, Helix, Zed, and others.
Dedicated extensions are available for [VS Code](https://github.com/sleb/vscode-knap) and [Zed](https://github.com/sleb/zed-knap).

## How it works

knap indexes your workspace on startup and keeps the index live via LSP file
change notifications. It requires no external tools and no editor-specific
plugins — just a standard LSP client configuration pointing at the server
binary.

Configuration (note subdirectory, file extensions) is passed via your editor's
native LSP settings, using `initializationOptions`.

## Status

v0.8.0 — Frontmatter Schema. See the [roadmap](docs/ROADMAP.md) for planned releases.

## Documentation

- [Getting Started](docs/GETTING_STARTED.md) — install the server, connect your
  editor, and understand what each feature does
- [User Stories](docs/USER_STORIES.md) — what knap does, told from the writer's
  perspective
- [Roadmap](docs/ROADMAP.md) — features grouped into releases, starting with the
  MVP
- [Architecture](docs/ARCHITECTURE.md) — component design and contracts
