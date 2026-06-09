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

## v0.5 — Tags _(released 2026-06-06)_

**Goal:** Explore and maintain your topic taxonomy via frontmatter tags.

| Story | Feature                                                                              |
| ----- | ------------------------------------------------------------------------------------ |
| US-14 | Frontmatter `tags:` completions from the workspace tag index                         |
| US-15 | Find References on a tag value → all files using it                                  |
| US-13 | Go to Definition on a tag value → all files using it                                 |
| [#50](https://github.com/sleb/knap/issues/50) | Workspace Symbols include tags (`SymbolKind::KEY`) alongside headings |

**LSP capabilities delivered:** `textDocument/completion` (frontmatter),
`textDocument/references` (tags), `textDocument/definition` (tags),
`workspace/symbol` (tags)

---

## v0.6 — Backlinks _(released 2026-06-08)_

**Goal:** Surface connections to the current note passively.

| Story | Feature                                                        |
| ----- | -------------------------------------------------------------- |
| US-25 | Backlinks code lens — `↑ N backlinks` at the top of every note |

**LSP capabilities delivered:** `textDocument/codeLens`

> Clicking the lens opens the references panel in VS Code. Zed supports code
> lens but it is disabled by default — enable it with `"code_lens": true` in
> your Zed settings.

---

## v0.7 — Same-file Anchor Links _(released 2026-06-08)_

**Goal:** Navigate within the current note using bare anchor links.

`[see Appendix A](#appendix-a)` is valid Markdown but v0.3 only handled
cross-file anchors (`note.md#section`). This release extends all anchor
features to bare `#slug` links that target a heading in the same file.

| Story | Feature                                                                                          |
| ----- | ------------------------------------------------------------------------------------------------ |
| US-48 | Go to Definition on `[text](#slug)` — navigates to the matching heading in the current file      |
| US-49 | Find References on a heading — includes same-file bare anchor links alongside cross-file results |
| US-50 | Diagnostic when a bare anchor doesn't match any heading in the current file                      |
| US-51 | Anchor completions for `[text](#` — heading list scoped to the current file                      |

**LSP capabilities delivered:** `textDocument/definition` (same-file anchors),
`textDocument/references` (same-file anchors),
`textDocument/publishDiagnostics` (same-file anchors),
`textDocument/completion` (same-file anchor completions)

---

## v0.8 — Frontmatter Schema

**Goal:** Enforce structure in notes that need it.

| Story | Feature                                                                         |
| ----- | ------------------------------------------------------------------------------- |
| US-24 | Completions and validation for frontmatter keys/values via user-provided schema |

**LSP capabilities delivered:** `textDocument/completion` (schema-driven),
`textDocument/publishDiagnostics` (frontmatter)

---

## v0.9 — Editor Experience

**Goal:** Editors treat Markdown as a first-class language with rich visual feedback.

| Story | Feature                                                                                                          |
| ----- | ---------------------------------------------------------------------------------------------------------------- |
| US-35 | Semantic tokens — tags styled as a distinct token type                                                           |
| US-36 | Folding ranges — collapse heading sections and fenced blocks                                                     |
| US-52 | Selection range — smart expand/contract: word → link → paragraph → heading section → document                   |
| US-53 | Inlay hints — show the `title:` frontmatter of a linked note inline next to its path                            |
| US-54 | Code lens on headings — `↑ N anchor links` on headings that are the target of one or more `#slug` links         |

**LSP capabilities delivered:** `textDocument/semanticTokens`,
`textDocument/foldingRange`, `textDocument/selectionRange`,
`textDocument/inlayHint`, `textDocument/codeLens` (extended)

---

## v0.10 — Tag Rename

**Goal:** Rename a tag across the entire workspace without a find-and-replace.

| Story | Feature                                                                           |
| ----- | --------------------------------------------------------------------------------- |
| US-37 | Rename tag — update all frontmatter occurrences across the workspace atomically   |

**LSP capabilities delivered:** `textDocument/rename` (tags),
`textDocument/prepareRename` (tags)

---

## v0.11 — Extract to New Note

**Goal:** Restructure notes without leaving your editor.

| Story | Feature                                                                                    |
| ----- | ------------------------------------------------------------------------------------------ |
| US-19 | Extract selection to new note — code action replaces selection with a link to the new file |

**LSP capabilities delivered:** `textDocument/codeAction` (extended)

---

## v0.12 — Daily Notes

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

- **Hover Previews** (US-09, US-10, US-23) — hover on a link to preview note contents; `title:` frontmatter as display name
- **Diagnostics & Validation** (US-32, US-34) — duplicate heading warnings; self-link warnings
- **Inline Tags** (US-40) — `#tag` body syntax included in tag index and completions
- **Orphan Note Detection** (US-38) — hint-level diagnostic on notes with no incoming links
- **Note Templates** (US-42) — `templateDir` config; new notes expanded with `{{title}}`, `{{date}}`
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
