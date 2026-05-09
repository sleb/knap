# Knap — Markdown LSP Server: User Stories

A language server for Markdown that brings Obsidian-style wiki linking and
navigation to any LSP-compatible editor.

**Link resolution default:** shortest-path by filename stem (e.g. `[[my-note]]`
resolves to `path/to/my-note.md`). Full relative path is available via
configuration (US-22). Ambiguous stems — where multiple files share the same
name — are surfaced as a diagnostic warning.

---

## Core Linking

**US-01** — As a writer, I can type `[[` and get completions for all Markdown
files in my workspace, so I can link to notes without remembering exact
filenames.

**US-02** — As a writer, I can `Go to Definition` on a `[[wiki-link]]` to open
the target file, so I can navigate my knowledge base from the keyboard.

**US-03** — As a writer, I can `Find References` on a file to see every other
file that links to it, so I understand how notes are connected.

**US-04** — As a writer, I can rename a file and have all `[[wiki-links]]`
pointing to it updated automatically, so my links don't break when I reorganize
notes.

**US-05** — As a writer, I can use `[[Note Title|display text]]` aliased links
and still get Go to Definition and completions, so my prose reads naturally.

**US-06** — As a writer, I can link to a heading within a file using
`[[Note#Heading]]` syntax and navigate directly to that heading.

**US-28** — As a writer, I can rename a heading and have all
`[[Note#OldHeading]]` anchor links across my workspace updated automatically,
so reorganising a note's structure doesn't silently break cross-file references.

---

## Diagnostics

**US-07** — As a writer, broken links (references to files that don't exist) are
surfaced as diagnostics (warnings), so I can find dead links without manually
checking.

**US-07b** — As a writer, a diagnostic is shown when a `[[link]]` stem is
ambiguous (matches multiple files), so I know to qualify it.

**US-08** — As a writer, I can see when a heading anchor in a `[[Note#Heading]]`
link no longer exists, so heading renames don't silently break links.

**US-32** — As a writer, I see a warning when a file contains two or more
headings with the same text, so I know that `[[Note#Heading]]` anchor links
targeting that heading are ambiguous.

**US-33** — As a writer, broken standard Markdown links to local files
(`[text](./missing.md)`) are surfaced as diagnostics, so every link type in my
notes is validated — not just wiki-links.

**US-34** — As a writer, a diagnostic is shown when a `[[wiki-link]]` points to
the file it appears in, so accidental self-links are caught rather than silently
ignored.

---

## Hover & Previews

**US-09** — As a writer, hovering over a `[[wiki-link]]` shows a preview of the
first N lines of the target file, so I can recall note contents without
switching files.

**US-10** — As a writer, hovering over a standard Markdown image or link shows a
summary/preview, so context is always one hover away.

---

## Symbols & Navigation

**US-11** — As a writer, Document Symbols lists all headings in the current file
so I can jump to any section quickly.

**US-12** — As a writer, Workspace Symbols lets me search headings across all
files, so I can navigate the entire knowledge base by heading name.

**US-13** — As a writer, I can use `Go to Definition` on a frontmatter tag value
to see all files that use that tag, so I can explore topics by tag.

---

## Tags

**US-14** — As a writer, I get completions for frontmatter `tags` values based
on tags already used across the workspace, so my taxonomy stays consistent.

**US-15** — As a writer, `Find References` on a frontmatter tag value shows
every file that uses that tag.

**US-40** — As a writer, I can use inline `#tag` syntax anywhere in the body of
a note (not just in frontmatter `tags:`) and have those tags included in the
workspace tag index, so my full tag taxonomy is captured wherever tags appear.
Inline tags participate in completions, Find References, and Go to Definition
alongside frontmatter tags.

**US-37** — As a writer, I can rename a frontmatter or inline tag and have every
file that uses that tag — in frontmatter or in the note body — updated
automatically, so my taxonomy stays consistent when I restructure it.

---

## Editor Experience

**US-35** — As a writer, wiki-links and tags are highlighted as distinct
semantic token types, so my editor theme can color them independently of plain
text and standard Markdown syntax — for example, coloring a broken wiki-link
differently from a valid one.

**US-36** — As a writer, I can collapse heading sections and fenced code blocks
in the current file using my editor's folding controls, so I can focus on the
section I'm working on in long notes.

---

## Workspace Awareness

**US-16** — As a writer, the server watches for new, renamed, and deleted files
and updates its index incrementally, so completions and diagnostics are always
current without restarting.

**US-26** — As a writer, `[[image.png]]` and `![[image.png]]` links to
non-Markdown files (images, PDFs, audio, etc.) that exist in my workspace
resolve correctly and do not produce broken-link diagnostics, so notes with
pasted attachments aren't cluttered with false warnings.

_Resolution rule for attachments: match by full filename (stem + extension),
since attachment links always include the extension. Notes continue to resolve
by stem only._

**US-27** — As a writer, `[[https://example.com]]` links to external URLs are
recognised as intentional and never produce broken-link diagnostics.

---

## Backlinks

**US-25** — As a writer, I can optionally display a backlinks section at the
bottom of the current note (via a virtual document or inlay) showing all files
that link to it, so I can see the note's context in my knowledge base without
leaving the file.

---

## Workspace Insight

**US-38** — As a writer, notes with no incoming links (orphans) are surfaced as
hint-level diagnostics, so I can identify isolated notes that may need to be
connected or archived.

**US-39** — As a writer, when a `[[slug-style-filename]]` link's target has a
`title:` frontmatter field that differs from the slug, the human-readable title
is shown as an inlay hint next to the link, so I can see what the link points to
without hovering.

---

## Code Actions & Refactoring

**US-18** — As a writer, when I'm on a broken `[[link]]`, a code action lets me
create the missing file, so I can stub out notes without leaving my editor.

**US-29** — As a writer, when I'm on a `[[Note#MissingAnchor]]` diagnostic, a
code action shows me the available headings from the target file so I can pick
the right one and fix the broken anchor without leaving my editor.

**US-30** — As a markdown author, I can optionally set `newNoteDir` in
`initializationOptions` to a folder path (e.g. `"0-Inbox"`) so that all notes
created by the Quick Fix "Create note" action land in that folder — relative to
the workspace root — instead of next to the current file. This lets me keep all
unprocessed stubs in one place (an inbox) regardless of where the link appears.

**US-31** — As a Zed user, I can add a `$schema` key to my knap
`initialization_options` block in `settings.json` and immediately get
autocompletion and inline validation for all recognized keys (`extensions`,
`attachmentsDir`, `newNoteDir`), so I can configure the server without
consulting external documentation and the editor flags unknown keys on the spot.

**US-19** — As a writer, I can select text in a note and apply a code action to
extract it into a new note, replacing the selection with a `[[link]]` to the new
note, so I can split overgrown notes without manual copy-paste.

**US-42** — As a writer, I can optionally configure a `templateDir` in
`initializationOptions` pointing to a folder of Markdown templates; when a new
note is created (via Quick Fix or extract), the server picks a matching template
and expands it with variables like `{{title}}` and `{{date}}`, so new notes
start with consistent structure.

---

## Daily Notes

**US-43** — As a writer, I can invoke an "open daily note" command from my
editor's command palette or a keyboard shortcut to open today's note, creating
it from a template if it doesn't exist. The server registers a
`workspace/executeCommand` handler for `knap.openDailyNote` and uses a
configured `dailyNotePattern` (e.g. `journal/%Y/%m/%d.md`) to determine the
path, then sends `window/showDocument` to navigate the editor there.

The user-visible trigger depends on the editor. In VS Code, vscode-knap
registers a named command that can be bound to a key. In Neovim, users can call
`vim.lsp.buf.execute_command` directly and bind it to any key. In Zed, the
extension API does not currently support registering arbitrary command palette
entries or keybindable actions, so this command is not accessible from
zed-knap; Zed support depends on future extension API expansion.

---

## Configuration

**US-20** — As an editor integrator, I can optionally configure a `noteRoot` to
restrict indexing to a subdirectory of the workspace (e.g. a `docs/` folder
inside a monorepo), so the server doesn't index unrelated files. When omitted,
all workspace folders are indexed.

**US-21** — As an editor integrator, I can configure file extensions the server
should treat as notes (e.g. `.md`, `.mdx`, `.markdown`).

**US-22** — As an editor integrator, I can configure link resolution strategy:
shortest-path stem (default) or full relative path.

---

## Frontmatter

**US-23** — As a writer, the server parses YAML frontmatter `title` fields and
uses them as the display name in completions, so I see human-readable titles
instead of filenames.

**US-24** — As a writer, I get completions and validation for frontmatter keys
and values defined in a schema I provide, so structured metadata stays
consistent.

---

## Developer / Debug CLI

These stories are for development and debugging — they let you invoke individual
components from the command line to verify behavior without a running editor.

**US-D01** — As a developer, I can run `knap parse <file>` to see the stem,
wiki-links, and their LSP ranges extracted from a Markdown file, so I can verify
parser behavior without a running editor.

**US-D02** — As a developer, I can run `knap index <dir>` to see the full note
index built from a directory, including which stems are found, broken, or
ambiguous, so I can verify link resolution without a running editor.

---

## Deferred / Out of Scope

**US-41** — Block-level links (`[[Note^block-id]]`): target a specific paragraph
or block within a note using Obsidian block reference syntax. Requires defining
block IDs, tracking them in the index, and completing/resolving them. Deferred
due to complexity; revisit when block references become a common pain point.

- Full Markdown formatting (bold, italic, tables) — handled by other tools like
  `marksman` or `prettier`
- Git integration
- Graph visualization
- Sync / publishing
