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

- `[text](path)` completions, Go to Definition, and Find References
- Broken link diagnostics — warnings for links to missing files or headings
- Rename a file — all links pointing to it are updated automatically
- `[text](note.md#heading)` Go to Definition navigates to the heading line
- Broken anchor diagnostics — warning when a linked heading no longer exists
- Rename a heading — all anchor links updated automatically
- Attachment links (`![alt](image.png)`, `[doc](report.pdf)`) resolve without false warnings
- Configurable file extensions (e.g. `.md`, `.mdx`) via `initializationOptions`
- Incremental index — the workspace index stays live as files change
- Document Symbols — jump to any heading within the current file
- Workspace Symbols — search headings by name across all files
- Hover on a link → preview of the target note (title + first 10 lines)
- Frontmatter `title:` used as the display label in completions and hover
- Frontmatter `tags:` completions from the workspace tag index
- Go to Definition and Find References on a tag value → all files sharing that tag
- Quick Fix on a broken link → create the missing file instantly
- Quick Fix on a broken anchor → pick from available headings to fix it
- Backlinks code lens — `↑ N backlinks` at the top of every note, click to open the references panel (VS Code; Zed pending an upcoming Zed release)
- Frontmatter schema — define allowed keys and enum values via `initializationOptions`; get completions and warnings for unknown keys, invalid values, and missing required fields

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

v0.7.0 — Backlinks code lens. See the [roadmap](docs/ROADMAP.md) for planned releases.

## Documentation

- [Getting Started](docs/GETTING_STARTED.md) — install the server, connect your
  editor, and understand what each feature does
- [User Stories](docs/USER_STORIES.md) — what knap does, told from the writer's
  perspective
- [Roadmap](docs/ROADMAP.md) — features grouped into releases, starting with the
  MVP
- [Architecture](docs/ARCHITECTURE.md) — component design and contracts
