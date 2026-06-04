# knap

![Version](https://img.shields.io/badge/version-0.4.1-blue)
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
  headings; inserts the GFM slug automatically (`## My Section` → `my-section`)

### Navigation

- **Go to Definition** — jump to the linked note; navigates to the heading line
  when an anchor is present (`[text](note.md#heading)`)
- **Document Symbols** — outline of every heading in the current file, jumpable
  from your editor's symbol panel
- **Workspace Symbols** — fuzzy-search headings across the entire vault

### Finding references

- **Find References** — every standard Markdown link pointing to the current
  file

### Refactoring

- **Rename a file** — all incoming and outgoing links rewritten atomically via
  `workspace/willRenameFiles`
- **Rename a heading** — all `[text](note.md#old-slug)` anchor links updated in
  place to the new slug

### Diagnostics & fixes

- **Broken link diagnostics** — warnings for links to missing files or headings;
  attachment links (images, PDFs) resolve against the full workspace
- **Quick Fix** — create a missing file from a broken link, or pick a valid
  heading to replace a broken anchor; both via standard `textDocument/codeAction`

### Workspace

- Incremental index — stays live as files change, no restart needed
- Configurable file extensions (e.g. `.md`, `.mdx`) and new-note inbox folder
  (`newNoteDir`) via `initializationOptions`

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

v0.4.1 — Code Actions. See the [roadmap](docs/ROADMAP.md) for planned releases.

## Documentation

- [Getting Started](docs/GETTING_STARTED.md) — install the server, connect your
  editor, and understand what each feature does
- [User Stories](docs/USER_STORIES.md) — what knap does, told from the writer's
  perspective
- [Roadmap](docs/ROADMAP.md) — features grouped into releases, starting with the
  MVP
- [Architecture](docs/ARCHITECTURE.md) — component design and contracts
