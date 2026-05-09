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

## v0.3 — Heading Navigation & Anchors _(released 2026-04-16)_

**Goal:** Navigate within notes, not just between them.

| Story | Feature                                                                                          |
| ----- | ------------------------------------------------------------------------------------------------ |
| US-06 | `[[Note#Heading]]` — Go to Definition navigates to the heading line (v0.1 navigates to file top) |
| US-08 | Diagnostic when heading anchor no longer exists                                                  |
| US-11 | Document Symbols — jump to heading within file                                                   |
| US-12 | Workspace Symbols — search headings across all files                                             |
| US-28 | Rename a heading → all `[[Note#OldHeading]]` anchor links updated                                |

**LSP capabilities delivered:** `textDocument/documentSymbol`,
`workspace/symbol`, `textDocument/rename`

---

## v0.4 — Hover Previews _(released 2026-04-19)_

**Goal:** See note contents without switching files.

| Story | Feature                                                           |
| ----- | ----------------------------------------------------------------- |
| US-09 | Hover on `[[wiki-link]]` → preview first N lines of target        |
| US-10 | Hover on standard Markdown link/image → summary                   |
| US-23 | Frontmatter `title` used as display name in completions and hover |

**LSP capabilities delivered:** `textDocument/hover`

---

## v0.5 — Tags _(released 2026-04-20)_

**Goal:** Explore and maintain your topic taxonomy via frontmatter tags.

| Story | Feature                                                  |
| ----- | -------------------------------------------------------- |
| US-14 | Frontmatter `tags:` completions from workspace tag index |
| US-15 | Find References on a tag value → all files using it      |
| US-13 | Go to Definition on a tag → all files using it           |

**LSP capabilities delivered:** `textDocument/completion` (frontmatter),
`textDocument/references`, `textDocument/definition` (tags)

---

## v0.6 — Code Actions _(released 2026-04-25)_

**Goal:** Fix broken links without leaving the editor.

| Story | Feature                                                                                        |
| ----- | ---------------------------------------------------------------------------------------------- |
| US-18 | Code action: create missing file from broken `[[link]]`                                        |
| US-29 | Code action: fix broken anchor by picking from available headings                              |
| US-30 | Config: `newNoteDir` — new notes from Quick Fix land in a configured folder                    |
| US-31 | Zed extension: JSON schema for `initialization_options` — autocompletion and inline validation |

**LSP capabilities delivered:** `textDocument/codeAction`

---

## v0.7 — Backlinks _(released 2026-04-28)_

**Goal:** Surface connections to the current note passively.

| Story | Feature                                                        |
| ----- | -------------------------------------------------------------- |
| US-25 | Backlinks code lens — `↑ N backlinks` at the top of every note |

**LSP capabilities delivered:** `textDocument/codeLens`

> Clicking the lens opens the references panel in VS Code. Zed support is
> pending an upcoming Zed release that adds code lens rendering.

---

## v0.8 — Frontmatter Schema _(released 2026-04-28)_

**Goal:** Enforce structure in notes that need it.

| Story | Feature                                                                         |
| ----- | ------------------------------------------------------------------------------- |
| US-24 | Completions and validation for frontmatter keys/values via user-provided schema |

**LSP capabilities delivered:** `textDocument/completion` (schema-driven),
`textDocument/publishDiagnostics` (frontmatter)

---

## v0.9 — Diagnostics & Validation

**Goal:** Every link type and document structure in your notes is validated.

| Story | Feature                                                                                             |
| ----- | --------------------------------------------------------------------------------------------------- |
| US-32 | Duplicate heading diagnostic — warn when two headings share the same text (ambiguous anchor target) |
| US-33 | Dead standard Markdown link diagnostic — `[text](./missing.md)` validated like wiki-links           |
| US-34 | Self-referential link diagnostic — warn when a `[[wiki-link]]` points to the file it appears in     |

**LSP capabilities delivered:** `textDocument/publishDiagnostics` (expanded)

---

## v0.10 — Editor Experience

**Goal:** Editors treat Markdown as a first-class language with rich visual feedback.

| Story | Feature                                                                               |
| ----- | ------------------------------------------------------------------------------------- |
| US-35 | Semantic tokens — wiki-links and tags styled as distinct token types per editor theme |
| US-36 | Folding ranges — collapse heading sections and fenced code blocks                     |

**LSP capabilities delivered:** `textDocument/semanticTokens`, `textDocument/foldingRange`

---

## v0.11 — Inline Tags & Tag Refactoring

**Goal:** Your tag taxonomy spans the full document, not just frontmatter, and can be renamed safely.

| Story | Feature                                                                                          |
| ----- | ------------------------------------------------------------------------------------------------ |
| US-40 | Inline `#tag` body syntax — tags in note body included in the tag index, completions, references |
| US-37 | Rename tag — update all frontmatter and inline occurrences across the workspace                  |

**LSP capabilities delivered:** `textDocument/rename` (extended), `textDocument/completion` (inline tags), `textDocument/references` (inline tags)

---

## v0.12 — Workspace Insight

**Goal:** See the health and connectivity of your knowledge base at a glance.

| Story | Feature                                                                                |
| ----- | -------------------------------------------------------------------------------------- |
| US-38 | Orphan note detection — hint-level diagnostic on notes with no incoming links          |
| US-39 | Inlay hints — show the human-readable `title:` next to `[[slug-style-filename]]` links |

**LSP capabilities delivered:** `textDocument/publishDiagnostics` (hints), `textDocument/inlayHint`

---

## v0.13 — Extract & Templates

**Goal:** Restructure and scaffold notes without leaving your editor.

| Story | Feature                                                                                      |
| ----- | -------------------------------------------------------------------------------------------- |
| US-19 | Extract selection to new note — code action replaces selection with `[[link]]` to new file   |
| US-42 | Note templates — configurable `templateDir`; new notes expanded with `{{title}}`, `{{date}}` |

**LSP capabilities delivered:** `textDocument/codeAction` (extended)

---

## v0.14 — Daily Notes

**Goal:** Open today's journal entry with one command, creating it from a template if it doesn't exist.

| Story | Feature                                                                                                                                                                 |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US-43 | `knap.openDailyNote` command — server advertises the command; editor extensions bind it to a key or palette entry; server sends `window/showDocument` to navigate there |

**LSP capabilities delivered:** `workspace/executeCommand`, `window/showDocument`

> Requires `dailyNotePattern` config (e.g. `journal/%Y/%m/%d.md`). The user-visible trigger lives in the editor, not the server: VS Code via a registered extension command; Neovim via `vim.lsp.buf.execute_command`. Zed does not currently support registering arbitrary command palette actions from an extension, so Zed support depends on future Zed extension API expansion.

---

---

## Backlog

These were explicitly deferred and are not scheduled:

- Block-level links (US-41) — `[[Note^block-id]]` Obsidian block reference syntax; high complexity, revisit if demand grows
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
