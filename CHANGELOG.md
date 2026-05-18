# Changelog

All notable changes to knap are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

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
