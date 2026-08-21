# knap — Roadmap

Each release is designed to be independently useful. A writer should get value
from v0.1 alone and accumulate more with each release.

| Version                                                                           | Title                                        | Status              |
| --------------------------------------------------------------------------------- | -------------------------------------------- | ------------------- |
| [v0.1](#v01--mvp-navigate-your-workspace-released-2026-05-09)                     | MVP: Navigate your workspace                 | Released 2026-05-09 |
| [v0.2](#v02--rename--refactor-released-2026-05-10)                                | Rename & Refactor                            | Released 2026-05-10 |
| [v0.3](#v03--heading-navigation--anchors-released-2026-05-16)                     | Heading Navigation & Anchors                 | Released 2026-05-16 |
| [v0.3.1](#v031--smarter-path-completion-released-2026-05-16)                      | Smarter Path Completion                      | Released 2026-05-16 |
| [v0.3.2](#v032--global-jump-in-completions-released-2026-05-17)                   | Global Jump in Completions                   | Released 2026-05-17 |
| [v0.3.3](#v033--rename-for-unindexed-files-released-2026-05-18)                   | Rename for Unindexed Files                   | Released 2026-05-18 |
| [v0.3.4](#v034--rename-dialog-for-formatted-headings-released-2026-05-18)         | Rename Dialog for Formatted Headings         | Released 2026-05-18 |
| [v0.3.5](#v035--lsp-range-correctness-released-2026-05-18)                        | LSP Range Correctness                        | Released 2026-05-18 |
| [v0.4](#v04--code-actions-released-2026-05-21)                                    | Code Actions                                 | Released 2026-05-21 |
| [v0.5](#v05--tags-released-2026-06-06)                                            | Tags                                         | Released 2026-06-06 |
| [v0.6](#v06--backlinks-released-2026-06-08)                                       | Backlinks                                    | Released 2026-06-08 |
| [v0.7](#v07--same-file-anchor-links-released-2026-06-08)                          | Same-file Anchor Links                       | Released 2026-06-08 |
| [v0.8](#v08--frontmatter-schema-released-2026-06-09)                              | Frontmatter Schema                           | Released 2026-06-09 |
| [v0.9](#v09--editor-experience-released-2026-06-10)                               | Editor Experience                            | Released 2026-06-10 |
| [v0.10](#v010--tag-rename-released-2026-06-10)                                    | Tag Rename                                   | Released 2026-06-10 |
| [v0.10.1](#v0101--version-subcommand-released-2026-06-14)                         | Version Subcommand                           | Released 2026-06-14 |
| [v0.10.2](#v0102--escaped-link-targets-for-paths-with-spaces-released-2026-08-03) | Escaped Link Targets for Paths with Spaces   | Released 2026-08-03 |
| [v0.11](#v011--headless-cli-released-2026-08-04)                                  | Headless CLI                                 | Released 2026-08-04 |
| [v0.11.1](#v0111--lint--index-false-positives-released-2026-08-05)                | Lint & Index False Positives                 | Released 2026-08-05 |
| [v0.12](#v012--headless-rename-released-2026-08-07)                               | Headless Rename                              | Released 2026-08-07 |
| [v0.13](#v013--agent-ergonomics-released-2026-08-08)                              | Agent Ergonomics                             | Released 2026-08-08 |
| [v0.14](#v014--batch-apply-released-2026-08-08)                                   | Batch Apply                                  | Released 2026-08-08 |
| [v0.15](#v015--judged-repoints--text-aware-repoint-ranking-released-2026-08-15)   | Judged Repoints & Text-Aware Repoint Ranking | Released 2026-08-15 |
| [v0.16](#v016--exclude-paths-released-2026-08-15)                                 | Exclude Paths                                | Released 2026-08-15 |
| [v0.17](#v017--drop-knap-fix--directory-links-released-2026-08-17)                | Drop `knap fix` & Directory Links            | Released 2026-08-17 |
| [v0.18](#v018--publish-to-cratesio-released-2026-08-18)                           | Publish to crates.io                         | Released 2026-08-18 |
| [v0.19](#v019--apply-round-trip-guard-released-2026-08-18)                        | Apply Round-Trip Guard                       | Released 2026-08-18 |
| [v0.20](#v020--configurable-skip-dirs--ignore-link-targets-released-2026-08-20)   | Configurable Skip-Dirs & Ignore Link Targets | Released 2026-08-20 |
| [v0.21](#v021--knap-skill-command-released-2026-08-20)                            | `knap skill` Command                         | Released 2026-08-20 |
| [v0.22](#v022--schema-sync-released-2026-08-21)                                   | Schema Sync                                  | Released 2026-08-21 |
| [v0.23](#v023--knaptoml-schema--suggest-text-output-released-2026-08-21)          | `knap.toml` Schema & `--suggest` Text Output | Released 2026-08-21 |

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

## v0.12 — Headless Rename _(released 2026-08-07)_

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

See `docs/design/releases/archive/v0.12/design.md` for the full design.

---

## v0.13 — Agent Ergonomics _(released 2026-08-08)_

**Goal:** Make the headless CLI's output something an agent can act on
programmatically, not just read — and make repeated lint/index calls cheap
enough to run after every edit.

A fast-follow to v0.12's headless rename. Where v0.12 gave agents parity with
the LSP's mutating capabilities, this release removes the friction points
that show up once an agent is actually looping on `lint`/`index`/`rename-*`/
`fix` as its edit-verify cycle: diagnostics that can only be triaged by
matching message prose, a full-workspace lint on every check, no way to see
one note's neighborhood without paging through the whole index snapshot, and
no headless equivalent of the two safe code actions an editor session already
offers. No new LSP capability ships this release — every change is either
CLI surface or internal to already-shared `handlers::` logic.

| Story  | Feature                                                                                         |
| ------ | ----------------------------------------------------------------------------------------------- |
| US-D11 | Stable `code` field on every diagnostic (`knap lint` and editor diagnostics alike)              |
| US-D16 | `knap lint --fail-on <severity>` — only fail on diagnostics at or above a threshold             |
| US-D12 | `knap lint --since <git-ref>` — scope linting to files changed since a ref                      |
| US-D13 | `knap index <file>` — one note's neighborhood, not the full workspace index                     |
| US-D14 | `knap fix [path] [--dry-run]` — headless quick-fix apply for safe code actions                  |
| US-D15 | `skill/knap/SKILL.md` — shippable skill documenting the agent lint/fix/rename loop              |
| US-D17 | `knap lint --suggest [N]` / `--fix` — ranked candidate fixes, and apply-then-report in one call |

See `docs/design/releases/archive/v0.13/design.md` for the full design, including two
corrections found while scoping: a sixth diagnostic code
(`invalid-field-value`) the original candidate list omitted, and why
`--fail-on` ships as a mechanical threshold without reassigning any
diagnostic's severity.

---

## v0.14 — Batch Apply _(released 2026-08-08)_

**Goal:** Let an agent make a whole set of write operations in one call
instead of one subprocess per change.

`knap lint --suggest` (v0.13) already surfaces ranked candidate fixes for
every diagnostic; an agent applies its own judgement to pick the right one
per finding but still has to shell out to `rename-file`/`rename-heading`/
`rename-tag`/`fix` once per change today. This release adds a batch runner
that takes the whole set of chosen changes as one JSON payload and applies
them sequentially, all-or-nothing.

| Story  | Feature                                                                                                                                                                            |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US-D18 | `knap apply --json` — apply a JSON array of change operations (rename-file, rename-heading, rename-tag, fix) from stdin, sequentially and all-or-nothing, with `--dry-run` support |

See `docs/design/releases/archive/v0.14/design.md` for the full design.

---

## v0.15 — Judged Repoints & Text-Aware Repoint Ranking _(released 2026-08-15)_

### Judged Repoints

**Goal:** Let an agent's own judged pick from `knap lint --suggest`'s ranked
candidates ride in the same all-or-nothing `knap apply` batch as any
`rename-*`/`fix` operations, instead of falling back to a hand edit outside
the batch.

v0.14's `knap apply` batches structural changes, but had no operation for an
agent's judged fix to a `broken-link`/`broken-anchor` diagnostic — `--suggest`
surfaces ranked candidates precisely for the ambiguous cases `fix` declines
to touch, and picking one still meant a separate `Edit` call outside `apply`.
This release adds two operations that apply an agent-chosen target at a
diagnostic's own range.

| Story  | Feature                                                                                                                                                                                           |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US-D19 | `knap apply` gains `repoint-link`/`repoint-anchor` operations — apply an agent-picked target from `lint --suggest`'s candidates at a diagnostic's own range, inside the same all-or-nothing batch |

See `docs/design/releases/archive/v0.15/judged-repoints/design.md` for the
full design.

### Text-Aware Repoint Ranking

**Goal:** Stop `knap fix`/`lint --suggest`'s repoint ranking from being fooled
by a candidate that's merely closer by raw path/slug edit distance to the
broken target string, when the link's own visible text points somewhere else
entirely.

The [agentic efficiency benchmark](design/experiments/agentic-efficiency-benchmark.md)'s
Trial 4 found the ranking picking a plausible-but-wrong link target 4 of 12
times against a weaker model — every case had a visible link-text mismatch
(`[Sync 835]` repointed to `sync-800.md` instead of `sync-835.md`) that the
ranking had no signal to catch, because it only ever compared the broken
target/slug string against candidate paths/slugs, never the link's own text
against candidate names. This release adds a second edit-distance signal from
link text, blends it into the ranking, and flags disagreement between the two
signals so an agent (or `knap fix`'s auto-apply) has something to distrust
instead of a silently confident top pick.

| Story  | Feature                                                                                                                                                                                 |
| ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US-D20 | Ranking blends link-text distance with path/slug distance; `text_mismatch` flag on `lint --suggest` output; `knap fix`/`lint --fix` decline to auto-apply when the two signals disagree |

See `docs/design/releases/archive/v0.15/text-aware-ranking/design.md` for the
full design.

---

## v0.16 — Exclude Paths _(released 2026-08-15)_

### Exclude paths

**Goal:** Keep files that are deliberately part of the repo but not part of
the vault — most notably `tests/fixtures/**`, whose intentionally broken
links exist to exercise diagnostics — from ever muddying the diagnostics
page or any other index-driven feature.

Everything so far treats every `.md` file under `index_roots` as a note.
That's right for a real vault but wrong for a workspace like knap's own,
where test fixtures with intentionally broken links live next to real docs.
This release adds an `exclude` glob-pattern list, read from `knap.toml` so
`knap lsp`, `knap lint`, and `knap index` all agree on what's excluded, plus
a `--exclude` flag on `knap lint`/`knap index` for one-off exclusions.
Excluded paths are left out of indexing entirely, not just diagnostics — no
completions, no navigation, no backlinks either.

| Story | Feature                                                                      |
| ----- | ---------------------------------------------------------------------------- |
| US-55 | `knap.toml` `exclude` glob patterns; `knap lint`/`knap index --exclude` flag |

See `docs/design/releases/archive/v0.16/exclude-paths/design.md` for the
full design.

### Path filter authority

**Goal:** Fix a gap in the exclude-paths feature above: a path excluded on
startup didn't necessarily stay excluded — the initial crawl (and `knap
lint`/`knap index`) respected `exclude`, but the three live-index LSP
handlers (`didOpen`, `didChange`, `didChangeWatchedFiles`) never consulted
the same rules, so an excluded file created or changed while `knap lsp` was
running could still slip into the index (issue #68).

This release introduces `PathFilter`, a single compiled exclude/index
authority built once from `exclude` and `extensions`, consulted by
`index::build`'s crawl and all three live-index handlers alike — so an
excluded path stays excluded for the life of the session.

| Story   | Feature                                                                                                                                                                               |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Bug #68 | `PathFilter`, a single exclude/index authority consulted by the crawl and the three live-index LSP handlers, so a path excluded on startup stays excluded for the rest of the session |

See `docs/design/releases/archive/v0.16/path-filter-authority/design.md`
for the full design.

---

## v0.17 — Drop `knap fix` & Directory Links _(released 2026-08-17)_

### Drop `knap fix`

**Goal:** An agent can no longer ask knap to blindly rewrite links, anchors,
and stub files across a whole vault in one unreviewed call. `knap fix`, `knap
lint --fix`, and `knap apply`'s `{"op":"fix"}` all shared one mechanism: pick
the single "unambiguous" candidate a ranking algorithm produced and write it
to disk with nobody — human or agent — looking at the specific edit first.
This release removes the capability instead of just the advice not to use
it. What stays: the LSP's quick-fix code actions (human reviews per cursor
position), `knap lint --suggest` (agent reads and judges the ranked
candidates), and `knap apply`'s `repoint-link`/`repoint-anchor` ops (agent
supplies the exact target it already chose).

| Story            | Change                                                                                                                 |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------- |
| US-D14 (removed) | `knap fix [path] [--dry-run]` — deleted outright                                                                       |
| US-D17 (amended) | Drops the `--fix`-collapses-the-loop half of the story; `--suggest` itself is unchanged                                |
| US-D18 (amended) | `knap apply`'s op list drops `fix`; `rename-file`/`rename-heading`/`rename-tag`/`repoint-link`/`repoint-anchor` remain |
| US-D20 (amended) | Drops the "`knap fix`'s auto-apply ... decline to auto-apply" half; `--suggest`'s `text_mismatch` flag is unchanged    |

See `docs/design/releases/archive/v0.17/drop-fix/design.md` for the full design.

### Directory Links

**Goal:** A writer can link to a whole folder — `[LLDs](../docs/lld/)` — and
have it behave like a link to a file instead of a permanent false-positive
broken-link diagnostic. Today `NoteIndex` only ever tracks file paths, so any
link whose target resolves to a directory is unconditionally `Broken`: no Go
to Definition, no Find References, no way to accept a directory as a
completion's finished target. This release extends resolution, navigation,
and completion to treat an existing directory as a valid link target
alongside files.

| Story | Feature                                                                                                                         |
| ----- | ------------------------------------------------------------------------------------------------------------------------------- |
| US-56 | Links to an existing directory resolve (no broken-link diagnostic); Go to Definition navigates to it; Find References tracks it |
| US-57 | Path completions let a directory be accepted as the finished link target, not just a step to drill further into                 |

---

## v0.18 — Publish to crates.io _(released 2026-08-18)_

**Goal:** `knap` becomes installable with `cargo install knap`, not just from
GitHub Releases or building from source. This release adds the crates.io
package metadata, trims the published library API to only what `main.rs`
needs across the crate boundary, folds `cargo publish` into the release
process itself, and fixes a directory-deletion gap in the live index
surfaced while trimming that API.

| Change                    | Description                                                                                                                                                                      |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Crate metadata            | `description`, `license`, `repository`, `readme`, `keywords`, `categories` added to `Cargo.toml`; package trimmed via `exclude` to just what the binary needs                    |
| Library API trim          | Only `cli` is `pub`; everything else is `pub(crate)`. `server::run`/`handlers::slug` stay reachable to this crate's own `tests`/`examples` via an opt-in `test-support` feature  |
| Release flow              | `cargo publish --dry-run` then `cargo publish` folded into `/knap-release`, right after commit/tag/push                                                                          |
| Fixed: directory deletion | Deleting a directory a note links to now clears the link live — the watched-files handler had no path for directory deletions and misrouted the event through attachment removal |

See `docs/design/releases/archive/v0.18/crates-io-publishing/design.md` for
the full design.

---

## v0.19 — Apply Round-Trip Guard _(released 2026-08-18)_

### Apply Round-Trip Guard

**Goal:** `knap apply` stops silently corrupting a file when a
`repoint-link`/`repoint-anchor` operation's `range` is stale or skewed (e.g.
computed against an earlier diagnostic, then shifted by another edit earlier
in the same batch). Previously `edit::apply` would write whatever the range
said, even if the result was no longer valid markdown — a link missing its
closing `)`, for instance — with no error. This release re-parses the file
immediately after each repoint write and rejects the operation if a
well-formed link/anchor doesn't come back where expected, and names which
operation failed (position and kind) in batch errors.

| Story  | Feature                                                                                  |
| ------ | ---------------------------------------------------------------------------------------- |
| US-D21 | `knap apply` rejects a `repoint-link`/`repoint-anchor` op producing unparseable markdown |
| US-D15 | `SKILL.md` instructs copying `range` verbatim, never recomputing it                      |

See `docs/design/releases/archive/v0.19/apply-round-trip-guard/design.md` for
the full design.

---

## v0.20 — Configurable Skip-Dirs & Ignore Link Targets _(released 2026-08-20)_

### Configurable Skip-Dir Defaults

**Goal:** The crawl-prune list — the directory names skipped outright while
building the index — stops being hardcoded. `knap.toml`'s `skip_dirs`
replaces the built-in `.*`/`node_modules`/`target` defaults with a
workspace's own list, matched against bare directory names rather than
paths, so a directory like `.notes/` that the default dotfile pattern would
otherwise prune can be opted into indexing, or an additional directory can
be pruned beyond the defaults. `initializationOptions.skipDirs` overrides
`knap.toml`'s `skip_dirs` entirely — the same override precedence
`extensions` uses, not the union `exclude` uses.

| Story | Feature                                                                  |
| ----- | ------------------------------------------------------------------------- |
| US-58 | `knap.toml` `skip_dirs` — configurable, overridable crawl-prune defaults |

See `docs/design/releases/archive/v0.20/configurable-skip-dirs/design.md`
for the full design.

### Ignore Link Targets

**Goal:** A relative link that intentionally points outside the workspace —
into a sibling repo's docs, say — stops being reported as broken, without
excluding anything from indexing. `knap.toml`'s `ignore_link_targets` (union
precedence with `initializationOptions.ignoreLinkTargets`, same as
`exclude`) sets workspace-wide patterns; a doc-scoped `ignore-link-targets`
frontmatter key sets per-note ones; a repeatable `--ignore-link-target
<pattern>` flag on `knap lint`/`knap index` adds one-off exceptions. All
three suppress only the `broken-link` diagnostic — `knap index --json`
still reports an ignored link's true resolution status.

| Story | Feature                                                          |
| ----- | ------------------------------------------------------------------- |
| US-59 | Doc-scoped `ignore-link-targets` frontmatter key                    |
| US-60 | `knap.toml` `ignore_link_targets` + `--ignore-link-target` flag     |

See `docs/design/releases/archive/v0.20/ignore-link-targets/design.md` for
the full design.

---

## v0.21 — `knap skill` Command _(released 2026-08-21)_

**Goal:** A `cargo install knap`-only setup — no source checkout on disk —
still gets the shipped skill, and that skill never drifts from the running
binary's version. Previously the only way to install `skill/knap/SKILL.md`
was `cp -r skill/knap ...` from a cloned repository, which doesn't exist for
a `cargo install` user and can silently go stale against an upgraded binary.
This release embeds `SKILL.md` into the binary at compile time and adds a
`knap skill` subcommand that writes it to `~/.claude/skills/knap` (
`--global`) or an arbitrary directory (`--path <dir>`), write-if-different so
re-running it is a no-op once the installed copy is current.

| Story  | Feature                                                           |
| ------ | ------------------------------------------------------------------ |
| US-D22 | `knap skill --global \| --path <dir>` installs/updates `SKILL.md`  |

See `docs/design/releases/archive/v0.21/knap-skill-command/design.md` for the
full design.

---

## v0.22 — Schema Sync _(released 2026-08-21)_

### Schema Sync

**Goal:** `schemas/v1/initialization_options.json` had drifted from the real
`initializationOptions` wire contract — it described a never-shipped
`attachmentsDir` property, was missing `exclude`/`skipDirs`/
`ignoreLinkTargets`, and described `frontmatterSchema` with a stale
`properties`/`required` shape instead of the actual
`requireFrontmatter`/`warnOnUnknownKeys`/`fields` shape. This is a
docs/schema-only bugfix (no Rust production code changes) that rewrites the
schema file to match `InitOptions`, fixes the stale `schemas/` path in
`docs/GETTING_STARTED.md`, and links `schemas/README.md` from both places a
reader would look for it.

| Story | Feature                                                                    |
| ----- | --------------------------------------------------------------------------- |
| #72   | `schemas/v1/initialization_options.json` matches the current `initializationOptions` wire contract |

See `docs/design/releases/archive/v0.22/schema-sync/design.md` for the full
design.

---

## v0.23 — `knap.toml` Schema & `--suggest` Text Output _(released 2026-08-21)_

### `knap.toml` Schema

**Goal:** A workspace owner can point a taplo-aware editor (e.g. Zed, or VS
Code with "Even Better TOML") at knap's published `knap.toml` JSON Schema —
via an inline `#:schema` directive at the top of the file, or a
`taplo.toml`/`.taplo.toml` glob association — and get autocompletion and
inline validation for `knap.toml`'s actual snake_case keys. This is
`knap.toml`'s equivalent of US-31's `initializationOptions` schema — a
separate schema file, because `knap.toml`'s snake_case shape isn't a pure
casing mirror of `initializationOptions`' camelCase one.

| Story | Feature                                                                                                  |
| ----- | --------------------------------------------------------------------------------------------------------- |
| US-61 | `schemas/v1/knap_toml.json` — autocompletion and validation for `knap.toml` in taplo-aware editors (#75) |

See `docs/design/releases/archive/v0.23/knap-toml-schema/design.md` for the
full design.

### `--suggest` Text Output

**Goal:** Fix a silent gap where `knap lint --suggest`'s ranked candidate
fixes had no visible effect in text-mode output — only `--json` reported
them, even though `--suggest` never documented a `--json` requirement.

| Story | Type | Feature                                                                                     |
| ----- | ---- | -------------------------------------------------------------------------------------------- |
| #74   | Bug  | `knap lint --suggest` prints ranked candidate fixes as indented lines under each diagnostic, in text-mode output too |

See `docs/design/releases/archive/v0.23/lint-suggest-text-output/design.md`
for the full design.

---

## Backlog

Explicitly deferred — not scheduled:

- **`--fail-on <severity>` threshold on `knap lint`** — let CI/agents treat
  only diagnostics at or above a given severity as a failure, instead of any
  diagnostic at all
- **Hover Previews** (US-09, US-10, US-23) — hover on a link to preview doc contents; `title:` frontmatter as display name
- **Diagnostics & Validation** (US-32, US-34) — duplicate heading warnings; self-link warnings
- **Inline Tags** (US-40) — `#tag` body syntax included in tag index and completions
- **Orphan Doc Detection** (US-38) — hint-level diagnostic on docs with no incoming links
- **Doc Templates** (US-42) — `templateDir` config; new docs expanded with `{{title}}`, `{{date}}`
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
