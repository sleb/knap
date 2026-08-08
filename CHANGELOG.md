# Changelog

All notable changes to knap are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [0.13.0] — 2026-08-08

### Added

- **Stable diagnostic codes.** Every diagnostic from `knap lint`/`knap
lint --json` and every editor `textDocument/publishDiagnostics`
  notification now carries a stable `code` — `broken-link`, `broken-anchor`,
  `missing-frontmatter`, `missing-required-field`, `invalid-field-value`,
  `unknown-field` — so scripts and agents can branch on a fixed identifier
  instead of matching message text, which is free to reword between
  releases.
- **`knap lint --fail-on <severity>`** — only diagnostics at or above the
  given severity (`error`, `warning` (default), `info`, `hint`) cause a
  non-zero exit. `--json` output gains `blocking_count` alongside the
  existing `problem_count`.
- **`knap lint --since <git-ref>`** — scope linting to files changed since a
  git ref (tracked diffs plus untracked new files), instead of rescanning
  the whole workspace on every check. Requires a git repository.
- **`knap lint --suggest [N]`** — attach up to `N` ranked candidate fixes
  (closest match first, by edit distance) to each `broken-link`/
  `broken-anchor` diagnostic's `data` field in `--json` output. Bare
  `--suggest` defaults to 3.
- **`knap lint --fix`** — apply every safe fix `knap fix` would make before
  computing the report, so the diagnostics shown reflect the post-fix
  state. Collapses the usual `lint` → `fix` → `lint`-again sequence into one
  call. `--json` output gains `fixes_applied` listing what was applied —
  the one case where `knap lint` mutates files on disk.
- **`knap index <file>`** — a file path now scopes output to that one
  note's neighborhood (headings, outgoing links, backlinks, tags) instead
  of the full workspace snapshot. `knap index <dir>` is unchanged.
- **`knap fix [path] [--dry-run]`** — headless equivalent of the two safe
  quick-fix code actions an editor session already offers: creates a
  missing linked file, and repoints a broken link or anchor to the one
  unambiguous best-matching existing note or heading. Fixes that would be
  ambiguous (two or more equally close candidates) are left alone.
  `--dry-run` previews the plan without touching disk.
- **`skill/knap/SKILL.md`** — a shippable Claude Code skill documenting the
  lint → fix/rename → lint edit-verify loop, copyable into a vault's
  `.claude/skills/` to teach a coding agent knap's conventions directly.

### Fixed

- `NoteIndex`'s `links_to` reverse index (backlinks, Find References) now
  unescapes an already-wrapped link target (`[text](<My File>)`) the same
  way `resolve()` already did, so a link to a file whose name needs
  escaping is no longer silently missing from backlinks even though it
  resolved correctly (#61).

## [0.12.0] — 2026-08-07

### Added

- **Headless rename, for agents and scripts working without an editor.**
  Three new CLI subcommands compute and apply the same `WorkspaceEdit`s the
  editor's rename UI produces, in one step, with no LSP session required:
  - `knap rename-file <old> <new>` — moves a note on disk and rewrites every
    incoming and outgoing link affected by the move.
  - `knap rename-heading <file> <old> <new>` — renames a heading (`<old>`
    may be its text or GFM slug) and rewrites every same-file and
    cross-file anchor link that targets it.
  - `knap rename-tag <old> <new>` — rewrites every frontmatter occurrence of
    a tag across the workspace, in all three YAML forms (bare scalar,
    inline list, block list).

  All three fail loudly and leave the filesystem untouched when their
  target doesn't exist or already exists, and apply their edit atomically.
  See the "Headless rename" section of the README for usage and examples.

### Fixed

- **`knap rename-file`/`knap rename-heading` now see the whole vault, not
  just the target file's own directory.** Both were resolving their config
  root from the target file's path — `config::for_path` treats a file
  argument's parent directory as the entire index root, so any note (and
  any incoming link) living outside that one directory was invisible to
  the rename. Both now scope off the current directory instead, matching
  `knap rename-tag`'s existing behavior.

---

## [0.11.1] — 2026-08-05

### Fixed

- **Empty-target links (`[text](#heading)`) now resolve as same-file
  references everywhere.** `NoteIndex::resolve()` treats an empty target as
  pointing back at the source file directly, instead of each caller
  special-casing it separately. One side effect: Find References on a
  same-file anchor link now returns real backlinks to the current file,
  where it previously returned nothing. (#60)
- **`knap lint .` (and other relative, `./`-prefixed roots) no longer
  reports false-positive broken links.** Paths collected while walking the
  directory are now normalized, fixing a mismatch with the paths link
  resolution computes that made every valid relative link under a
  `./`-prefixed root look broken. (#62)
- **A link-shaped span inside inline code (`` `[text](path)` ``) is no
  longer recovered as a broken link** by the fallback scan for hand-typed,
  unwrapped link destinations. (#63)

## [0.11.0] — 2026-08-04

### ⚠ Breaking

- **Bare `knap` (no subcommand) no longer starts the LSP server.** Use
  `knap lsp` instead. `zed-knap` (v0.2.0) and `vscode-knap` (v0.1.0) have
  both been updated to invoke `knap lsp`. (US-D06)

### Added

- **`knap lsp`** — explicit subcommand to start the LSP server on stdio,
  replacing the old no-args fallback. (US-D06)
- **`knap lint [path] [--json]`** — headless link/anchor/frontmatter
  diagnostics, the same checks the editor shows, without a running LSP
  session. Text output is rustc/clippy-style (`path:line:col: severity:
message`); `--json` emits a structured report. Exit code `1` if any
  diagnostic was found, `0` otherwise. (US-D04)
- **`knap.toml`** — optional project config file at a workspace root,
  mirroring `initializationOptions` (`extensions`, `new_note_dir`,
  `frontmatter_schema`) in snake_case TOML. Read by `knap lsp`, `knap lint`,
  and `knap index` alike; when running under `knap lsp`, editor-supplied
  `initializationOptions` win over `knap.toml` field-by-field. (US-D07)

### Changed

- **`knap index --json`** — now serializes a structured `IndexReport`
  (notes, headings, links with resolved/broken status, backlinks, tags)
  instead of the old ad hoc format. Text output is unchanged. `knap index`
  now loads `knap.toml` the same way `lsp`/`lint` do, fixing a bug where it
  always used the hardcoded `["md"]` extension list regardless of
  configuration. (US-D05)

## [0.10.2] — 2026-08-03

### Fixed

- **Links to files with spaces or parentheses in the name now resolve.**
  Completion and the "Create note" quick action wrap the inserted link
  destination in angle brackets (`<...>`) whenever it contains whitespace,
  control characters, or parentheses — characters that otherwise break or
  truncate a bare Markdown link destination per CommonMark. Directory-item
  (folder) completions are left unwrapped since they're an intentionally
  incomplete destination the user keeps typing after. (#57)
- **"Create note" now fixes the link text, not just the file on disk.**
  Selecting "Create note" on a broken link whose target needs escaping
  rewrites the link's text in the document, in addition to creating the
  file, so the link actually resolves afterward. (#57)
- **Hand-typed links with spaces are now recognized as links at all.**
  Previously, a link like `[text](My File)` — written directly rather than
  inserted via completion — was invisible to knap entirely (no broken-link
  diagnostic, no quick action) because the underlying Markdown parser
  doesn't recognize a bare, unescaped-space destination as a link. knap now
  recovers these via a fallback scan so they're treated as ordinary
  (typically broken) links. (#57)
- **"Create note" now appends `.md` when the link target has no extension**
  — previously a link like `[text](My New Note)` created a file with no
  extension at all.

---

## [0.10.1] — 2026-06-14

### Added

- **`knap version` subcommand** — prints the installed version (`knap 0.10.1`)
  without starting the LSP server, so you can confirm which release is active
  from any terminal. (US-D03)

---

## [0.10.0] — 2026-06-10

### Added

- **Tag rename** — place the cursor on any frontmatter tag and trigger rename
  (F2 / `textDocument/rename`). Every note in the workspace that carries that
  tag is updated atomically. The rename dialog pre-fills with the tag text so
  you never type from scratch. All three YAML tag forms are handled: bare scalar
  (`tags: rust`), inline list (`tags: [rust, go]`), and block list (`- rust`
  under `tags:`). Matching is case-insensitive. (US-37)

---

## [0.9.0] — 2026-06-10

### Added

- **Folding ranges** — heading sections and fenced code blocks are now
  collapsible in editors that support `textDocument/foldingRange`. Each heading
  folds its section down to the line before the next same-or-higher-level
  heading; fenced blocks fold from the opening ` ``` ` to the closing ` ``` `.
  (US-36)
- **Selection range** — smart expand/contract (`Shift+Alt+→` in VS Code, `C-a`
  in Helix) grows the selection through a chain of word → link → paragraph →
  heading section → document. (US-52)
- **Inlay hints** — when a link target has a `title:` frontmatter field, the
  title appears inline next to the link path (e.g. `-> My Note`). Suppressed
  for external URLs and broken links. (US-53)
- **Heading anchor-link lenses** — headings that are the target of one or more
  `#slug` anchor links now show a `↑ N anchor link(s)` code lens alongside the
  existing backlinks lens. Counts both same-file bare anchors
  (`[text](#slug)`) and cross-file anchors (`[text](file.md#slug)`). (US-54)

---

## [0.8.0] — 2026-06-09

### Added

- **Frontmatter schema** — define allowed keys and values for your notes'
  YAML frontmatter via `frontmatterSchema` in `initializationOptions`. knap
  uses the schema to offer key completions (`FIELD` items) and value completions
  (`VALUE` items) in the frontmatter block. (US-24)
- **Schema diagnostics** — warnings for required keys that are absent, values
  that are not in the allowed list (exact-case match), and unknown keys. Each
  check is opt-in: `required` per field, `requireFrontmatter` and
  `warnOnUnknownKeys` at the schema level. (US-24)

---

## [0.7.0] — 2026-06-08

### Added

- **Same-file anchor completions** — typing `[text](#` now opens a heading
  picker for the **current file**, the same way `[text](file.md#` does for
  cross-file anchors. (US-51)
- **Go to Definition on bare anchors** — `[text](#my-section)` navigates
  directly to the matching heading in the same file; falls back to the top of
  the file if no heading matches. (US-48)
- **Broken same-file anchor diagnostics** — `[text](#missing)` produces a
  warning when no heading in the current file has that GFM slug.
  Message: `Heading not found: '#missing'`. (US-50)
- **Find References on a heading** — cursor on a heading line now returns all
  anchor references to that heading: same-file bare anchors and cross-file
  anchors from other notes, alongside existing backlink behaviour. (US-49)

---

## [0.6.0] — 2026-06-08

### Added

- Backlinks code lens: a `↑ N backlinks` annotation now appears above the
  first line of any note that has at least one incoming link. In VS Code,
  clicking the lens opens the References panel pre-populated with every file
  that links to the current note. Notes with no incoming links show no lens.
  (US-25)

---

## [0.5.1] — 2026-06-08

### Fixed

- Workspace Symbols: headings whose text is entirely or partially inside
  backtick inline-code spans (e.g. `` ### `textDocument/didOpen` ``) now show
  the correct name instead of a blank entry. (fixes #52)

---

## [0.5.0] — 2026-06-06

### Added

- Frontmatter `tags:` completions: place the cursor inside a `tags:` value
  (bare scalar, inline list `[…]`, or block list `- …`) and your tag index
  appears as a pick list. Already-used tags are excluded; prefix filtering
  narrows the list as you type. (US-14)
- Find References on a tag value → every note in the workspace that carries
  that tag, each location pointing directly at the tag range. (US-15)
- Go to Definition on a tag value → same set of locations as Find References,
  letting you jump to any note using the tag. (US-13)
- Workspace Symbols now includes frontmatter tags alongside headings. Tags
  appear with `SymbolKind::KEY` so editors can style them distinctly; the
  container name is the filename. Query filtering is case-insensitive. (#50)

---

## [0.4.1] — 2026-06-04

### Fixed

- Links to external URLs containing `#` fragments (e.g. `https://example.com/page#section`)
  no longer produce a spurious "Heading not found" diagnostic. (fixes #48)

---

## [0.4.0] — 2026-05-21

### Added

- Quick Fix: place the cursor on a broken `[text](missing.md)` link and trigger
  code actions (`Cmd+.` / lightbulb) to create the missing file instantly. The
  file is created empty; triggering the action a second time is a no-op. (US-18)
- Quick Fix: place the cursor on a link with a broken anchor
  (`[text](note.md#nonexistent)`) to see one "Change anchor to …" action per
  heading in the target note. Selecting an action rewrites the anchor to the
  correct GFM slug. (US-29)
- `newNoteDir` configuration option: set a folder path relative to the workspace
  root and all "Create note" Quick Fixes will place new files there instead of
  next to the linking note. Useful for inbox-style workflows. (US-30)
- JSON Schema for `initializationOptions` at `schemas/initialization_options.json`.
  Reference it with `$schema` in your editor's LSP config to get inline
  completions and validation for knap options. (US-31)

## [0.3.5] — 2026-05-18

### Fixed

- All LSP positions (`character` fields in ranges) are now UTF-16 code unit
  offsets as required by the spec. Previously `LineIndex.position()` returned
  raw byte offsets, which are identical to UTF-16 for ASCII but diverge for any
  multi-byte character (e.g. the em dash `—` in headings). This caused the
  rename dialog to be rejected by editors for headings containing such characters.
- `text_range` for headings now spans the full raw heading text including any
  trailing inline markup characters (e.g. the closing `_` in `_..._`).
  Previously, `text_range` ended at the last pulldown-cmark Text event boundary,
  which excludes surrounding markup — leaving the closing `_` outside the range
  and making placeholder ≠ text-at-range. (issue #4 / follow-up to #3)

## [0.3.4] — 2026-05-18

### Fixed

- `prepareRename` now returns a placeholder that matches the raw source text at
  the rename range. Previously, for headings with inline Markdown formatting
  (e.g. `_(released 2026-05-16)_`), the placeholder was the pulldown-cmark
  rendered text (formatting stripped), which differed from the text editors read
  at the range. Editors that validate `placeholder == text-at-range` silently
  refused to show the rename dialog. (issue #3)

## [0.3.3] — 2026-05-18

### Fixed

- Heading rename (`F2`) now works for any readable Markdown file, even when the
  file was not yet indexed (e.g. server started without workspace folders
  configured, or the editor did not send `didOpen` before triggering rename).
  `prepareRename` and `rename` fall back to reading the file from disk when it
  is absent from the index. Incoming anchor links are still updated when the
  file is fully indexed; the heading text and self-anchors are always updated.
  (issue #2)

## [0.3.2] — 2026-05-17

### Changed

- Path completions inside `[text](` now show every file in the workspace
  alongside the directory items. Workspace-wide files appear as a third tier
  below immediate subdirectories and immediate files, sorted with `sort_text` so
  editors preserve the order. Selecting a global item replaces the entire typed
  partial path cleanly. (US-47)
- Global items use `filter_text` set to the full relative path (e.g.
  `notes/work/meeting.md`) so typing any path segment — filename, parent
  directory, or prefix — surfaces the item in the editor's fuzzy picker.
- Global items show a frontmatter `title` as the label when one is present;
  the full relative path is shown as `detail` (secondary text in the picker).

## [0.3.1] — 2026-05-16

### Changed

- Path completions now show only immediate children of the current directory
  segment rather than every file in the vault. Subdirectories appear as folder
  items (e.g. `notes/`) and selecting one re-triggers completion to show its
  contents — drill down one level at a time without seeing hundreds of files at
  once. (US-46)
- Typing `/` inside a Markdown link path now triggers completion, so you can
  navigate into subdirectories without manually invoking the picker.

## [0.3.0] — 2026-05-16

### Added

- Go to Definition on `[text](note.md#my-section)` navigates to the heading
  line, not just the top of the file. Anchors are matched using the GFM slug
  convention: `## My Section` → `#my-section`. (US-06)
- Broken-anchor diagnostic: a warning is shown when the anchor in a
  `[text](note.md#heading)` link does not match any heading in the target file
  (compared via GFM slug). (US-08)
- Document Symbols (`textDocument/documentSymbol`) lists every heading in the
  current file so you can jump to any section from the outline panel. (US-11)
- Workspace Symbols (`workspace/symbol`) lets you search headings across all
  indexed notes. Results include the containing filename as the container name.
  (US-12)
- Rename a heading with your editor's Rename Symbol command (`F2`) and all
  `[text](note.md#old-slug)` anchor links across the workspace — including
  anchor-only self-links within the same file — are rewritten to the new GFM
  slug atomically. (US-28)
- Anchor completions: typing `#` after a file path inside a Markdown link
  (`[text](file.md#`) triggers a heading picker for the target file. Each item
  shows the heading text as written and inserts the GFM slug form — no leading
  `#`. (US-45)

### Changed

- Anchor matching throughout (diagnostics, Go to Definition) now uses the GFM
  slug algorithm rather than plain case-insensitive string comparison. Links
  written as `#my-section` correctly match headings like `## My Section!` whose
  slug is `my-section`.

## [0.2.0] — 2026-05-10

### Added

- Rename a file from your editor's file tree and all standard Markdown links are
  updated atomically — links in other files pointing at the renamed file
  (incoming) and links inside the renamed file whose base path changed
  (outgoing). Requires an editor that sends `workspace/willRenameFiles`. (US-04)
- Path completions now include non-Markdown files (images, PDFs, etc.) alongside
  notes. (US-44)
- Attachment links (`![alt](image.png)`, `[doc](file.pdf)`) resolve against all
  files in the workspace — no broken-link diagnostic when the target exists. (US-26)

### Changed

- The file watcher now covers the entire workspace root (`**/*`) rather than
  per-extension patterns. Attachment diagnostics update live as files are added
  and deleted, with no configuration needed. (US-21)

### Removed

- `attachmentsDir` configuration option — no longer needed since the whole
  workspace is watched.

## [0.1.0] — 2026-05-09

### Changed

- Switched from wiki-link (`[[note]]`) to standard Markdown links
  (`[text](path/to/note.md)`). Links are now path-relative — no stem lookup,
  no ambiguity. Notes render correctly in any Markdown tool without knap
  present. (US-01, US-02, US-03, US-07)
- Completion trigger character changed from `[` to `(`. Completions fire inside
  the `()` of a Markdown link and insert the path relative to the current file.
- Diagnostics now report broken targets at the path range and broken anchors at
  the anchor range within the link.

### Removed

- Wiki-link (`[[…]]`) support — all link syntax is now standard Markdown.
- Handlers removed in this release (planned for later milestones): hover,
  document symbols, workspace symbols, code actions, code lens, prepare rename,
  rename, file rename.
- `frontmatterSchema` and `newNoteDir` configuration options (re-introduced in
  later milestones when those features land).
