# Changelog

All notable changes to knap are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

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

## [Unreleased]
