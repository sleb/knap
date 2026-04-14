# knap

![Version](https://img.shields.io/badge/version-0.2.0-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A
[Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server for Markdown, bringing Obsidian-style wiki linking and navigation to any
LSP-compatible editor.

## What it does

- `[[wiki-link]]` completions, Go to Definition, and Find References
- Broken and ambiguous link diagnostics
- Rename a file — all `[[links]]` pointing to it are updated automatically
- Aliased links `[[Note|display text]]` — rename preserves the alias
- Attachment links `[[image.png]]` resolve against non-note files
- Configurable file extensions (e.g. `.md`, `.mdx`) via `initializationOptions`
- Incremental index — the workspace index stays live as files change

Works with any editor that speaks LSP: Neovim, VS Code, Helix, Zed, and others.

## How it works

knap indexes your workspace on startup and keeps the index live via LSP file
change notifications. It requires no external tools and no editor-specific
plugins — just a standard LSP client configuration pointing at the server
binary.

Configuration (vault subdirectory, file extensions, link resolution strategy) is
passed via your editor's native LSP settings, using `initializationOptions`.

## Status

v0.2.0 — Rename & Refactor. See the [roadmap](docs/ROADMAP.md) for planned releases.

## Documentation

- [User Stories](docs/USER_STORIES.md) — what knap does, told from the writer's
  perspective
- [Roadmap](docs/ROADMAP.md) — features grouped into releases, starting with the
  MVP
- [Architecture](docs/ARCHITECTURE.md) — component design and contracts
