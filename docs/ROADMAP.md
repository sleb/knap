# Knap — Roadmap

Each release is designed to be independently useful. A writer should get value
from v0.1 alone and accumulate more with each release.

---

## v0.1 — MVP: Navigate your workspace _(released 2026-04-12)_

**Goal:** The minimum useful knowledge base tool. A writer can jump between
notes and catch broken links.

The core loop: type a link, follow it, find what links back, fix what's broken.

| Story | Feature                                        |
| ----- | ---------------------------------------------- |
| US-01 | `[[` completion for all notes in the workspace |
| US-02 | Go to Definition on `[[wiki-link]]`            |
| US-03 | Find References on a file                      |
| US-07 | Broken link diagnostics                        |
| US-16 | Incremental file watching (index stays live)   |

The server indexes all LSP workspace folders by default — no configuration
required for the common single-folder case.

**LSP capabilities delivered:** `textDocument/completion`,
`textDocument/definition`, `textDocument/references`,
`textDocument/publishDiagnostics`, `workspace/didChangeWatchedFiles`

---

## v0.2 — Rename & Refactor _(released 2026-04-13)_

**Goal:** Reorganizing your workspace doesn't break links.

| Story  | Feature                                                         |
| ------ | --------------------------------------------------------------- |
| US-04  | Rename file → update all `[[links]]`                            |
| US-05  | Aliased links `[[Note\|display text]]` — rename preserves alias |
| US-07b | Diagnostic for ambiguous stems (multiple files, same name)      |
| US-21  | Config: file extensions treated as notes                        |
| US-26  | Attachment links (`[[image.png]]`) resolve against non-md files |

**LSP capabilities delivered:** `workspace/willRenameFiles`

---

## v0.3 — Heading Navigation & Anchors

**Goal:** Navigate within notes, not just between them.

| Story | Feature                                                                                          |
| ----- | ------------------------------------------------------------------------------------------------ |
| US-06 | `[[Note#Heading]]` — Go to Definition navigates to the heading line (v0.1 navigates to file top) |
| US-08 | Diagnostic when heading anchor no longer exists                                                  |
| US-11 | Document Symbols — jump to heading within file                                                   |
| US-12 | Workspace Symbols — search headings across all files                                             |

**LSP capabilities delivered:** `textDocument/documentSymbol`,
`workspace/symbol`

---

## v0.4 — Hover Previews

**Goal:** See note contents without switching files.

| Story | Feature                                                           |
| ----- | ----------------------------------------------------------------- |
| US-09 | Hover on `[[wiki-link]]` → preview first N lines of target        |
| US-10 | Hover on standard Markdown link/image → summary                   |
| US-23 | Frontmatter `title` used as display name in completions and hover |

**LSP capabilities delivered:** `textDocument/hover`

---

## v0.5 — Tags

**Goal:** Explore and maintain your topic taxonomy via frontmatter tags.

| Story | Feature                                                  |
| ----- | -------------------------------------------------------- |
| US-14 | Frontmatter `tags:` completions from workspace tag index |
| US-15 | Find References on a tag value → all files using it      |
| US-13 | Go to Definition on a tag → all files using it           |

**LSP capabilities delivered:** `textDocument/completion` (frontmatter),
`textDocument/references`, `textDocument/definition` (tags)

---

## v0.6 — Code Actions

**Goal:** Fix broken links without leaving the editor.

| Story | Feature                                                 |
| ----- | ------------------------------------------------------- |
| US-18 | Code action: create missing file from broken `[[link]]` |

**LSP capabilities delivered:** `textDocument/codeAction`

---

## v0.7 — Backlinks

**Goal:** Surface connections to the current note passively.

| Story | Feature                                                      |
| ----- | ------------------------------------------------------------ |
| US-25 | Optional backlinks panel / virtual document for current note |

**LSP capabilities delivered:** virtual document provider

---

## v0.8 — Frontmatter Schema

**Goal:** Enforce structure in notes that need it.

| Story | Feature                                                                         |
| ----- | ------------------------------------------------------------------------------- |
| US-24 | Completions and validation for frontmatter keys/values via user-provided schema |

**LSP capabilities delivered:** `textDocument/completion` (schema-driven),
`textDocument/publishDiagnostics` (frontmatter)

---

---

## Post-v1 / Backlog

These were explicitly deferred and are not scheduled:

- Extract selection to new note (US-19)
- Full Markdown formatting (bold, italic, tables)
- Git integration
- Graph visualization
- Sync / publishing
- US-22 — Config: link resolution strategy (stem vs. full path). Path-mode adds
  significant index complexity for a niche use case. Revisit if stem collisions
  become a real pain point.
- US-27 — External URL links (`[[https://...]]`) never flagged broken. Not
  idiomatic wiki-link syntax; external URLs belong in standard `[text](url)`
  links. Defer unless user complaints surface.

---

## Debug CLI

Developer-facing subcommands that let you invoke components directly without a
running editor. Each command becomes available when its underlying component is
implemented.

| Subcommand          | Story  | Available in |
| ------------------- | ------ | ------------ |
| `knap parse <file>` | US-D01 | v0.1 Step 2  |
| `knap index <dir>`  | US-D02 | v0.1 Step 3  |

---

## Principles

- **Each release ships a complete loop.** No half-features that only work after
  the next release.
- **Configuration grows with features.** Don't expose config knobs until the
  feature they control exists.
- **LSP-first.** Avoid editor-specific APIs until a feature genuinely can't be
  expressed in standard LSP.
