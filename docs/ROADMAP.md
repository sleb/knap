# knap — Roadmap

Each release is designed to be independently useful. A writer should get value
from v0.1 alone and accumulate more with each release.

| Version                                                                           | Title                                      | Status              |
| --------------------------------------------------------------------------------- | ------------------------------------------ | ------------------- |
| [v0.1](#v01--mvp-navigate-your-workspace-released-2026-05-09)                     | MVP: Navigate your workspace               | Released 2026-05-09 |
| [v0.2](#v02--rename--refactor-released-2026-05-10)                                | Rename & Refactor                          | Released 2026-05-10 |
| [v0.3](#v03--heading-navigation--anchors-released-2026-05-16)                     | Heading Navigation & Anchors               | Released 2026-05-16 |
| [v0.3.1](#v031--smarter-path-completion-released-2026-05-16)                      | Smarter Path Completion                    | Released 2026-05-16 |
| [v0.3.2](#v032--global-jump-in-completions-released-2026-05-17)                   | Global Jump in Completions                 | Released 2026-05-17 |
| [v0.3.3](#v033--rename-for-unindexed-files-released-2026-05-18)                   | Rename for Unindexed Files                 | Released 2026-05-18 |
| [v0.3.4](#v034--rename-dialog-for-formatted-headings-released-2026-05-18)         | Rename Dialog for Formatted Headings       | Released 2026-05-18 |
| [v0.3.5](#v035--lsp-range-correctness-released-2026-05-18)                        | LSP Range Correctness                      | Released 2026-05-18 |
| [v0.4](#v04--code-actions-released-2026-05-21)                                    | Code Actions                               | Released 2026-05-21 |
| [v0.5](#v05--tags-released-2026-06-06)                                            | Tags                                       | Released 2026-06-06 |
| [v0.6](#v06--backlinks-released-2026-06-08)                                       | Backlinks                                  | Released 2026-06-08 |
| [v0.7](#v07--same-file-anchor-links-released-2026-06-08)                          | Same-file Anchor Links                     | Released 2026-06-08 |
| [v0.8](#v08--frontmatter-schema-released-2026-06-09)                              | Frontmatter Schema                         | Released 2026-06-09 |
| [v0.9](#v09--editor-experience-released-2026-06-10)                               | Editor Experience                          | Released 2026-06-10 |
| [v0.10](#v010--tag-rename-released-2026-06-10)                                    | Tag Rename                                 | Released 2026-06-10 |
| [v0.10.1](#v0101--version-subcommand-released-2026-06-14)                         | Version Subcommand                         | Released 2026-06-14 |
| [v0.10.2](#v0102--escaped-link-targets-for-paths-with-spaces-released-2026-08-03) | Escaped Link Targets for Paths with Spaces | Released 2026-08-03 |
| [v0.11](#v011--headless-cli-released-2026-08-04)                                  | Headless CLI                               | Released 2026-08-04 |
| [v0.11.1](#v0111--lint--index-false-positives-released-2026-08-05)                | Lint & Index False Positives               | Released 2026-08-05 |
| [v0.12](#v012--headless-rename)                                                   | Headless Rename                            | Planned             |
| [v0.13](#v013--agent-ergonomics)                                                  | Agent Ergonomics                           | Planned             |
| [v0.14](#v014--daily-notes)                                                       | Daily Notes                                | Planned             |
| [v0.15](#v015--extract-to-new-note)                                               | Extract to New Note                        | Planned             |

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

| Story                                         | Feature                                                               |
| --------------------------------------------- | --------------------------------------------------------------------- |
| US-14                                         | Frontmatter `tags:` completions from the workspace tag index          |
| US-15                                         | Find References on a tag value → all files using it                   |
| US-13                                         | Go to Definition on a tag value → all files using it                  |
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

## v0.8 — Frontmatter Schema _(released 2026-06-09)_

**Goal:** Enforce structure in notes that need it.

| Story | Feature                                                                         |
| ----- | ------------------------------------------------------------------------------- |
| US-24 | Completions and validation for frontmatter keys/values via user-provided schema |

**LSP capabilities delivered:** `textDocument/completion` (schema-driven),
`textDocument/publishDiagnostics` (frontmatter)

---

## v0.9 — Editor Experience _(released 2026-06-10)_

**Goal:** Editors treat Markdown as a first-class language with rich visual feedback.

| Story | Feature                                                                                                 |
| ----- | ------------------------------------------------------------------------------------------------------- |
| US-36 | Folding ranges — collapse heading sections and fenced blocks                                            |
| US-52 | Selection range — smart expand/contract: word → link → paragraph → heading section → document           |
| US-53 | Inlay hints — show the `title:` frontmatter of a linked note inline next to its path                    |
| US-54 | Code lens on headings — `↑ N anchor links` on headings that are the target of one or more `#slug` links |

**LSP capabilities delivered:** `textDocument/foldingRange`,
`textDocument/selectionRange`, `textDocument/inlayHint`,
`textDocument/codeLens` (extended)

---

## v0.10 — Tag Rename _(released 2026-06-10)_

**Goal:** Rename a tag across the entire workspace without a find-and-replace.

| Story | Feature                                                                         |
| ----- | ------------------------------------------------------------------------------- |
| US-37 | Rename tag — update all frontmatter occurrences across the workspace atomically |

**LSP capabilities delivered:** `textDocument/rename` (tags),
`textDocument/prepareRename` (tags)

---

## v0.10.1 — Version Subcommand _(released 2026-06-14)_

**Goal:** Let users confirm the installed binary version without starting the LSP server.

| Story  | Feature                                                        |
| ------ | -------------------------------------------------------------- |
| US-D03 | `knap version` — prints `knap <version>` and exits immediately |

---

## v0.10.2 — Escaped Link Targets for Paths with Spaces _(released 2026-08-03)_

**Goal:** Links to files with spaces in the name (`My File.md`) actually
resolve, whether inserted via completion or created via the "Create note"
quick action — and are recognized as links at all when hand-typed without
`<...>` wrapping.

| Story | Type | Feature                                                                                                                                            |
| ----- | ---- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| #57   | Bug  | Completion inserts link targets wrapped in `<...>` when the path needs it (whitespace, control chars, parentheses)                                 |
| #57   | Bug  | "Create note" quick action rewrites the broken link's text, not just the file it creates on disk                                                   |
| #57   | Bug  | Bare, unwrapped link destinations with spaces/parens (`[text](My File)`) are recognized as (broken) links instead of silently parsed as plain text |

See `docs/design/releases/archive/v0.10.2/design.md` for the full design,
including a correction to the initial premise (found during implementation)
and the two related fixes it required — `link.target` unescaping and
`new_note_path`'s missing `.md` extension — documented in
`docs/design/releases/archive/v0.10.2/plan.md` Step 3.5.

---

## v0.11 — Headless CLI _(released 2026-08-04)_

**Goal:** Make knap usable outside a live editor session — by scripts, CI, and
coding agents that can't rely on a running LSP session for diagnostics.

**Breaking change:** bare `knap` (no subcommand) no longer starts the LSP
server — use `knap lsp`. `zed-knap` (v0.2.0) and `vscode-knap` (v0.1.0) were
updated to invoke `knap lsp` before this release shipped.

| Story  | Feature                                                                    |
| ------ | -------------------------------------------------------------------------- |
| US-D07 | `knap.toml` project config, shared by `lsp`/`lint`/`index`                 |
| US-D06 | `knap lsp` explicit subcommand; bare `knap` no longer starts it            |
| US-D04 | `knap lint [path] [--json]` — headless link/anchor/frontmatter diagnostics |
| US-D05 | `knap index <path> --json` — structured workspace snapshot for agents      |

See `docs/design/releases/archive/v0.11/design.md` for the full design.

---

## v0.11.1 — Lint & Index False Positives _(released 2026-08-05)_

**Goal:** Fix three false positives in `knap lint`/`knap index` and the
editor diagnostics that share the same resolution logic — surfaced by
running v0.11's new headless CLI against knap's own repository.

| Story | Feature                                                                                                                                                                                    |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| #60   | `resolve()` treats an empty target as a same-file reference; redundant empty-target special cases deleted from `compute_diagnostics`, `handle_definition`, and completion's anchor trigger |
| #60   | Find References on a same-file anchor link now returns real backlinks instead of nothing (side effect of the #60 fix)                                                                      |
| #62   | `walk_dir()` normalizes collected paths, fixing false positives when linting/indexing a relative, `./`-prefixed root                                                                       |
| #63   | `find_fallback_links()` excludes inline code spans                                                                                                                                         |

See `docs/design/releases/archive/v0.11.1/plan.md` for the full implementation plan.

---

## v0.12 — Headless Rename

**Goal:** An agent editing a workspace without a running editor session can
rename a file, a heading, or a tag and get the same atomic, index-aware
rewrite of every affected link that the LSP `rename` handlers already give an
editor user — without hand-tracking every affected file itself.

`knap lint`/`knap index` (v0.11) gave agents read access to the same engine
editors use. This release gives them write access to the one thing that was
still LSP-only: the three refactors (`willRenameFiles`, heading `rename`, tag
`rename`) that touch multiple files atomically. Everything else an editor's
rename UI does — locating the item under a cursor — doesn't apply headlessly,
since the CLI is told the target directly instead of inferring it from a
position.

| Story  | Feature                                                                                   |
| ------ | ----------------------------------------------------------------------------------------- |
| US-D08 | `knap rename-file <old> <new>` — move a note, rewrite incoming + outgoing links           |
| US-D09 | `knap rename-heading <file> <old> <new>` — rewrite heading text + all anchor links        |
| US-D10 | `knap rename-tag <old> <new>` — rewrite every frontmatter occurrence across the workspace |

See `docs/design/releases/v0.12/design.md` for the full design.

---

## v0.13 — Agent Ergonomics

**Goal:** Make the headless CLI's output something an agent can act on
programmatically, not just read — and make repeated lint/index calls cheap
enough to run after every edit.

A fast-follow to v0.12's headless rename. Where v0.12 gave agents parity with
the LSP's mutating capabilities, this release removes the friction points
that show up once an agent is actually looping on `lint`/`index`/`rename-*` as
its edit-verify cycle: diagnostics that can only be triaged by matching
message prose, a full-workspace lint on every check, and no way to see one
note's neighborhood without paging through the whole index snapshot.

Candidate stories (to be scoped in detail via `/knap-design` when this release
starts):

- Stable `code` field on each `--json` lint diagnostic (`broken-link`,
  `broken-anchor`, `missing-required-field`, `unknown-field`,
  `missing-frontmatter`) so an agent can branch on a code instead of matching
  message text
- `knap lint --since <git-ref>` — scope linting to files changed since a ref
- Scoped index query for a single file's neighborhood (backlinks, outgoing
  links, headings, tags) instead of the full workspace snapshot
- Headless quick-fix apply (`knap fix`) for the safe code actions (create a
  missing file, replace a broken anchor with a suggested heading)
- A `SKILL.md` documenting the agent lint/index/rename usage loop

Also related, already in the Backlog below and worth folding in here when
this release is scoped: **`--fail-on <severity>` threshold on `knap lint`**.

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

## v0.15 — Extract to New Note

**Goal:** Restructure notes without leaving your editor.

| Story | Feature                                                                                    |
| ----- | ------------------------------------------------------------------------------------------ |
| US-19 | Extract selection to new note — code action replaces selection with a link to the new file |

**LSP capabilities delivered:** `textDocument/codeAction` (extended)

---

## Backlog

Explicitly deferred — not scheduled:

- **`--fail-on <severity>` threshold on `knap lint`** — let CI/agents treat
  only diagnostics at or above a given severity as a failure, instead of any
  diagnostic at all
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
