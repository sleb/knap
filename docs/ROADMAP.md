# knap — Roadmap

Each release is designed to be independently useful. A writer should get value
from v0.1 alone and accumulate more with each release.

---

## v0.1 — MVP: Navigate your workspace _(released 2026-05-09)_

**Goal:** The minimum useful knowledge base tool. A writer can link to notes,
jump between them, find what links back, and catch broken links.

| Story  | Feature                                                        |
| ------ | -------------------------------------------------------------- |
| US-01  | Path completions inside `[text](` — all notes in the workspace |
| US-02  | Go to Definition on `[text](path/to/note.md)`                  |
| US-05  | Navigation works regardless of link display text               |
| US-03  | Find References on a file                                      |
| US-07  | Broken link diagnostics                                        |
| US-16  | Incremental file watching — index stays live as files change   |
| US-D01 | `knap parse <file>` — inspect parser output without an editor  |
| US-D02 | `knap index <dir>` — inspect index output without an editor    |

**LSP capabilities delivered:** `textDocument/completion`,
`textDocument/definition`, `textDocument/references`,
`textDocument/publishDiagnostics`, `workspace/didChangeWatchedFiles`

---

## v0.2 — Rename & Refactor _(released 2026-05-10)_

**Goal:** Reorganizing your workspace doesn't break links.

Relative-to-file paths mean that renaming a file requires updating both
_incoming_ links (other files pointing at it, recomputed from each linker's
location) and _outgoing_ links (links within the moved file, whose base has
changed). Both are handled atomically.

| Story | Feature                                                                     |
| ----- | --------------------------------------------------------------------------- |
| US-04 | Rename file → all standard Markdown links updated (incoming + outgoing)     |
| US-26 | Attachment links (`![alt](img.png)`, `[doc](file.pdf)`) resolve cleanly     |
| US-44 | Path completions inside `[text](` include non-Markdown files (images, PDFs) |
| US-21 | Config: file extensions treated as notes                                    |

**LSP capabilities delivered:** `workspace/willRenameFiles`

---

## v0.3 — Heading Navigation & Anchors _(released 2026-05-16)_

**Goal:** Navigate within notes, not just between them.

Anchors follow the **GFM slug convention**: `## My Section` → `#my-section`
(lowercase, spaces to hyphens, non-alphanumeric stripped). This is the format
GitHub, Obsidian, and VS Code Markdown Preview all use.

| Story | Feature                                                                            |
| ----- | ---------------------------------------------------------------------------------- |
| US-06 | `[text](note.md#my-section)` — Go to Definition navigates to the heading line      |
| US-08 | Diagnostic when a heading anchor (matched by GFM slug) no longer exists            |
| US-11 | Document Symbols — jump to any heading within the current file                     |
| US-12 | Workspace Symbols — search headings across all files                               |
| US-28 | Rename a heading → heading text and all `[text](note.md#old-slug)` links updated   |
| US-45 | Anchor completions — `[text](file.md#` → heading list; label = text, insert = slug |

**LSP capabilities delivered:** `textDocument/documentSymbol`,
`workspace/symbol`, `textDocument/rename`, `textDocument/completion` (anchors)

---

## v0.3.1 — Smarter Path Completion _(released 2026-05-16)_

**Goal:** Make typing relative paths feel effortless, even in deep vault structures.

| Story | Feature                                                                              |
| ----- | ------------------------------------------------------------------------------------ |
| US-46 | Segment-by-segment directory completion — drill into folders, stub new files by name |

**LSP capabilities delivered:** `textDocument/completion` (directory traversal,
re-trigger on `/`)

---

## v0.3.2 — Global Jump in Completions _(released 2026-05-17)_

**Goal:** Let writers jump directly to any file in the workspace without
drilling through directories, while keeping the directory-traversal items for
when the full path isn't known upfront.

| Story | Feature                                                                          |
| ----- | -------------------------------------------------------------------------------- |
| US-47 | Global file list alongside directory items — jump to any file by typing its path |

**LSP capabilities delivered:** `textDocument/completion` (global file index in path completions)

---

## v0.3.3 — Rename for Unindexed Files _(released 2026-05-18)_

**Goal:** Fix a silent failure where heading rename did nothing for files not in
the index.

| Story | Type | Feature                                                                                         |
| ----- | ---- | ----------------------------------------------------------------------------------------------- |
| #2    | Bug  | `prepareRename` and `rename` fall back to disk parse when the file is absent from the NoteIndex |

---

## v0.3.4 — Rename Dialog for Formatted Headings _(released 2026-05-18)_

**Goal:** Fix a silent failure where the rename dialog never appeared for headings
containing inline Markdown formatting.

| Story | Type | Feature                                                                                                   |
| ----- | ---- | --------------------------------------------------------------------------------------------------------- |
| #3    | Bug  | `prepareRename` returns raw placeholder text so editors that validate `placeholder == text-at-range` work |

---

## v0.3.5 — LSP Range Correctness _(released 2026-05-18)_

**Goal:** Fix two bugs that together prevented the rename dialog from appearing
for headings with multi-byte characters (em dash) or trailing inline markup.

| Story | Type | Feature                                                                                              |
| ----- | ---- | ---------------------------------------------------------------------------------------------------- |
| #4    | Bug  | `LineIndex` now emits UTF-16 `character` offsets; `text_range` end covers trailing markup characters |

---

## v0.4 — Code Actions _(released 2026-05-21)_

**Goal:** Fix broken links without leaving the editor.

| Story | Feature                                                                                        |
| ----- | ---------------------------------------------------------------------------------------------- |
| US-18 | Code action: create the missing file from a broken link                                        |
| US-29 | Code action: fix a broken anchor by picking from the target note's available headings          |
| US-30 | Config: `newNoteDir` — notes created by Quick Fix land in a configured folder                  |
| US-31 | Zed extension: JSON schema for `initialization_options` — autocompletion and inline validation |

**LSP capabilities delivered:** `textDocument/codeAction`

---

## v0.5 — Tags

**Goal:** Explore and maintain your topic taxonomy via frontmatter tags.

| Story | Feature                                                      |
| ----- | ------------------------------------------------------------ |
| US-14 | Frontmatter `tags:` completions from the workspace tag index |
| US-15 | Find References on a tag value → all files using it          |
| US-13 | Go to Definition on a tag value → all files using it         |

**LSP capabilities delivered:** `textDocument/completion` (frontmatter),
`textDocument/references` (tags), `textDocument/definition` (tags)

---

## v0.6 — Hover Previews

**Goal:** See note contents without switching files.

| Story | Feature                                                                |
| ----- | ---------------------------------------------------------------------- |
| US-09 | Hover on a link → preview of the first N lines of the target note      |
| US-10 | Hover on an image or external URL → inline summary                     |
| US-23 | Frontmatter `title:` used as the display name in completions and hover |

**LSP capabilities delivered:** `textDocument/hover`

---

## v0.7 — Backlinks

**Goal:** Surface connections to the current note passively.

| Story | Feature                                                        |
| ----- | -------------------------------------------------------------- |
| US-25 | Backlinks code lens — `↑ N backlinks` at the top of every note |

**LSP capabilities delivered:** `textDocument/codeLens`

> Clicking the lens opens the references panel in VS Code. Zed support is
> pending an upcoming Zed release that adds code lens rendering.

---

## v0.8 — Frontmatter Schema

**Goal:** Enforce structure in notes that need it.

| Story | Feature                                                                         |
| ----- | ------------------------------------------------------------------------------- |
| US-24 | Completions and validation for frontmatter keys/values via user-provided schema |

**LSP capabilities delivered:** `textDocument/completion` (schema-driven),
`textDocument/publishDiagnostics` (frontmatter)

---

## v0.9 — Diagnostics & Validation

**Goal:** Every link and document structure in your notes is validated.

| Story | Feature                                                                   |
| ----- | ------------------------------------------------------------------------- |
| US-32 | Duplicate heading diagnostic — warn when two headings share the same text |
| US-34 | Self-link diagnostic — warn when a link points to the file it appears in  |

**LSP capabilities delivered:** `textDocument/publishDiagnostics` (expanded)

---

## v0.10 — Editor Experience

**Goal:** Editors treat Markdown as a first-class language with rich visual feedback.

| Story | Feature                                                      |
| ----- | ------------------------------------------------------------ |
| US-35 | Semantic tokens — tags styled as a distinct token type       |
| US-36 | Folding ranges — collapse heading sections and fenced blocks |

**LSP capabilities delivered:** `textDocument/semanticTokens`,
`textDocument/foldingRange`

---

## v0.11 — Inline Tags & Tag Refactoring

**Goal:** Your tag taxonomy spans the full document, not just frontmatter, and
can be renamed safely.

| Story | Feature                                                                                 |
| ----- | --------------------------------------------------------------------------------------- |
| US-40 | Inline `#tag` body syntax — included in the tag index, completions, and Find References |
| US-37 | Rename tag — update all frontmatter and inline occurrences across the workspace         |

**LSP capabilities delivered:** `textDocument/rename` (extended),
`textDocument/completion` (inline tags), `textDocument/references` (inline tags)

---

## v0.12 — Workspace Insight

**Goal:** Surface the health and connectivity of your knowledge base.

| Story | Feature                                                                       |
| ----- | ----------------------------------------------------------------------------- |
| US-38 | Orphan note detection — hint-level diagnostic on notes with no incoming links |

**LSP capabilities delivered:** `textDocument/publishDiagnostics` (hints)

---

## v0.13 — Extract & Templates

**Goal:** Restructure and scaffold notes without leaving your editor.

| Story | Feature                                                                                      |
| ----- | -------------------------------------------------------------------------------------------- |
| US-19 | Extract selection to new note — code action replaces selection with a link to the new file   |
| US-42 | Note templates — configurable `templateDir`; new notes expanded with `{{title}}`, `{{date}}` |

**LSP capabilities delivered:** `textDocument/codeAction` (extended)

---

## v0.14 — Daily Notes

**Goal:** Open today's journal entry with one command, creating it from a
template if it doesn't exist.

| Story | Feature                                                                                                                                                                 |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US-43 | `knap.openDailyNote` command — server advertises the command; editor extensions bind it to a key or palette entry; server sends `window/showDocument` to navigate there |

**LSP capabilities delivered:** `workspace/executeCommand`, `window/showDocument`

> Requires `dailyNotePattern` config (e.g. `journal/%Y/%m/%d.md`). VS Code via
> a registered extension command; Neovim via `vim.lsp.buf.execute_command`. Zed
> does not currently support registering arbitrary command palette actions from
> an extension; Zed support depends on future extension API expansion.

---

## Backlog

Explicitly deferred — not scheduled:

- Full Markdown formatting (bold, italic, tables) — handled by other tools
- Git integration
- Graph visualization
- Sync / publishing

---

## Principles

- **Each release ships a complete loop.** No half-features that only work after
  the next release.
- **Configuration grows with features.** Don't expose config knobs until the
  feature they control exists.
- **LSP-first.** Avoid editor-specific APIs until a feature genuinely can't be
  expressed in standard LSP.
