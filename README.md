# knap

![Version](https://img.shields.io/badge/version-0.9.0-blue)
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

- `[text](path/to/note.md)` completions — triggered by `(`, inserts the path
  relative to the current file
- Go to Definition — jumps to the linked note; navigates to the heading when an
  anchor is present (`[text](note.md#heading)`)
- Find References — all standard Markdown links pointing to a file
- Broken link diagnostics — warnings for links to missing files or headings
- Incremental index — the workspace index stays live as files change
- Configurable file extensions (e.g. `.md`, `.mdx`) via `initializationOptions`

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

v0.9.0 — Standard Markdown link MVP. See the [roadmap](docs/ROADMAP.md) for planned releases.

## Documentation

- [Getting Started](docs/GETTING_STARTED.md) — install the server, connect your
  editor, and understand what each feature does
- [User Stories](docs/USER_STORIES.md) — what knap does, told from the writer's
  perspective
- [Roadmap](docs/ROADMAP.md) — features grouped into releases, starting with the
  MVP
- [Architecture](docs/ARCHITECTURE.md) — component design and contracts
