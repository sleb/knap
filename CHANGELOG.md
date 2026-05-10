# Changelog

All notable changes to knap are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [0.9.0] — 2026-05-09

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

## [Unreleased]
