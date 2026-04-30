# Changelog

All notable changes to knap are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [0.8.1] — 2026-04-28

### Fixed

- Server panic on malformed URI in `workspace/willRenameFiles`
- Channel errors in `publishDiagnostics` now logged instead of silently dropped
- Duplicate-tags bug when a note repeats the same tag in frontmatter (regression from v0.5.2 refactor)

### Changed

- Parser now makes a single pulldown-cmark pass instead of three (faster indexing)

---

## [0.8.0] — 2026-04-28

### Added

- Frontmatter schema support (US-24): provide a schema in `initializationOptions.frontmatterSchema`
  to get completions and diagnostics for frontmatter keys and values

---

## [0.7.0] — 2026-04-28

### Added

- Backlinks code lens (US-25): `↑ N backlinks` displayed at the top of every note; clicking
  opens the references panel

---

## [0.6.2] — 2026-04-25

### Added

- JSON schema for `initializationOptions` (US-31): add `"$schema": "..."` to your config for
  autocompletion and inline validation in Zed and VS Code
- `newNoteDir` config option (US-30): new notes created by the Quick Fix action land in a
  configured folder instead of the workspace root

### Changed

- Server now logs its version and executable path on startup

---

## [0.6.1] — 2026-04-25

### Changed

- Updated CI and release workflow (internal)

---

## [0.6.0] — 2026-04-25

### Added

- Code action: create a missing file from a broken `[[link]]` (US-18)
- Code action: fix a broken anchor by picking from the target note's available headings (US-29)

---

## [0.5.2] — 2026-04-23

### Fixed

- Server crash on non-`file://` URIs (e.g. `untitled:` buffers in VS Code)
- UTF-16/byte confusion in completion trigger detection
- `walk_dir` now skips hidden directories (`.git`, `.obsidian`, …) and `node_modules`/`target`;
  symlinks are skipped to prevent infinite loops
- Ambiguous-link diagnostic now shows full file paths instead of stems

### Changed

- `initializationOptions` parse failures are now logged as warnings instead of silently ignored

---

## [0.5.1] — 2026-04-20

### Added

- Windows (`x86_64-pc-windows-msvc`) release binary

---

## [0.5.0] — 2026-04-20

### Added

- Frontmatter `tags:` completions sourced from the workspace tag index (US-14)
- Go to Definition on a tag value → all files that carry that tag (US-13)
- Find References on a tag value → all files that carry that tag (US-15)

---

## [0.4.0] — 2026-04-19

### Added

- Hover on `[[wiki-link]]` → inline preview of the first N lines of the target note (US-09)
- Hover on a standard Markdown link or image → inline summary (US-10)
- Frontmatter `title:` used as the display label in completions (US-23)

---

## [0.3.0] — 2026-04-16

### Added

- `[[Note#Heading]]` anchor links now navigate to the heading line, not just the file top (US-06)
- Diagnostic when a heading anchor no longer exists in the target file (US-08)
- Document Symbols: jump to any heading within the current file (US-11)
- Workspace Symbols: search headings across all notes in the workspace (US-12)
- Rename a heading → all `[[Note#OldHeading]]` anchor links updated automatically (US-28)

---

## [0.2.0] — 2026-04-13

### Added

- Rename a file → all `[[links]]` pointing to it updated automatically (US-04)
- Aliased links `[[Note|display text]]` — rename preserves the alias (US-05)
- Diagnostic for ambiguous stems: multiple files with the same name (US-07b)
- `extensions` config option — specify which file types are treated as notes (US-21)
- Attachment links `[[image.png]]` resolve against non-Markdown files in the workspace (US-26)

---

## [0.1.0] — 2026-04-12

### Added

- `[[` completion for all notes in the workspace (US-01)
- Go to Definition on `[[wiki-link]]` (US-02)
- Find References on a file (US-03)
- Broken link diagnostics (US-07)
- Incremental file watching — the index stays live as files change (US-16)
