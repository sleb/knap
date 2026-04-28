# knap

![Version](https://img.shields.io/badge/version-0.8.0-blue)
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
- `[[Note#Heading]]` Go to Definition navigates to the heading line
- Broken anchor diagnostics — warning when a linked heading no longer exists
- Document Symbols — jump to any heading within the current file
- Workspace Symbols — search headings by name across all files
- Rename a heading — all `[[Note#OldHeading]]` anchor links updated automatically
- Hover on `[[wiki-link]]` → preview of the target note (title + first 10 lines)
- Hover on `[text](./note.md)` → same note preview; images and external URLs show a summary
- Frontmatter `title:` used as the display label in completions and hover
- Frontmatter `tags:` completions from the workspace tag index
- Go to Definition and Find References on a tag value → all files sharing that tag
- Quick Fix on a broken `[[link]]` → create the missing file instantly
- Quick Fix on a broken `[[note#anchor]]` → pick from available headings to fix the anchor
- Backlinks code lens — `↑ N backlinks` at the top of every note, click to open the references panel (VS Code; Zed pending an upcoming Zed release)
- Frontmatter schema — define allowed keys and enum values via `initializationOptions`; get completions and warnings for unknown keys, invalid values, and missing required fields

Works with any editor that speaks LSP: Neovim, VS Code, Helix, Zed, and others.

## How it works

knap indexes your workspace on startup and keeps the index live via LSP file
change notifications. It requires no external tools and no editor-specific
plugins — just a standard LSP client configuration pointing at the server
binary.

Configuration (vault subdirectory, file extensions, link resolution strategy) is
passed via your editor's native LSP settings, using `initializationOptions`.

## Status

v0.7.0 — Backlinks code lens. See the [roadmap](docs/ROADMAP.md) for planned releases.

## Documentation

- [Getting Started](docs/GETTING_STARTED.md) — install the server, connect your
  editor, and understand what each feature does
- [User Stories](docs/USER_STORIES.md) — what knap does, told from the writer's
  perspective
- [Roadmap](docs/ROADMAP.md) — features grouped into releases, starting with the
  MVP
- [Architecture](docs/ARCHITECTURE.md) — component design and contracts
