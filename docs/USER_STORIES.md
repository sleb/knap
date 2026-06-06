# knap — Markdown LSP Server: User Stories

A language server for Markdown that brings smart linking, navigation, and
diagnostics to any LSP-compatible editor — using standard Markdown syntax.

> **Scope note (2026-05-09):** knap dropped wiki-link (`[[...]]`) support in
> favour of standard Markdown links. Stories US-07b, US-22, US-27, US-39, and
> US-41 were removed as a result. Old design docs for v0.1–v0.8 reference the
> former story IDs — those are historical artifacts for shipped releases.
> See [ARCHITECTURE.md](ARCHITECTURE.md) for the design tenets behind this
> decision.

---

## Core Linking

**US-01** — As a writer, I can type `[` inside a Markdown link and get
completions for all Markdown files in my workspace, so I can link to notes
without remembering exact paths.

**US-44** — As a writer, path completions inside `[text](` include non-Markdown
files in my workspace (images, PDFs, etc.), so I can easily link to attachments
without remembering their paths.

**US-46** — As a writer, path completions inside `[text](` include directory
entries alongside files. Selecting a directory inserts the partial path and
re-triggers completion, so I can navigate deep folder structures one segment at
a time without knowing the full path upfront. I can finish by typing a new
filename to create a stub link (surfaced as a broken-link diagnostic until the
file exists).

**US-47** — As a writer, path completions inside `[text](` also show every file
in the workspace — not just the immediate directory contents — so I can jump
directly to any note or attachment by typing part of its path or title, without
drilling through folders. Directory items appear first; global items appear below
and can be filtered by typing any segment of their path.

**US-45** — As a writer, once I have typed a file path inside `[text](`, typing
`#` triggers completions for all headings in the target file. Each item shows
the heading as written (e.g. "My Section") and inserts the GFM slug form
(e.g. `my-section`), so I can link to a specific section without manually
computing the anchor.

**US-02** — As a writer, I can `Go to Definition` on a `[text](path/to/note.md)`
link to open the target file, so I can navigate my knowledge base from the
keyboard.

**US-03** — As a writer, I can `Find References` on a file to see every other
file that links to it, so I understand how notes are connected.

**US-04** — As a writer, I can rename a file and have all standard Markdown links
pointing to it updated automatically, so my links don't break when I reorganize
notes.

**US-05** — As a writer, Go to Definition and Find References work regardless of
what display text I use in a link, so my prose reads naturally without affecting
navigation.

**US-06** — As a writer, I can link to a heading within a file using
`[text](note.md#my-heading)` syntax (GFM slug form — lowercase, spaces to
hyphens, punctuation stripped) and navigate directly to that heading.

**US-28** — As a writer, I can rename a heading and have all
`[text](note.md#old-heading)` anchor links across my workspace updated
automatically to the new GFM slug, so reorganising a note's structure doesn't
silently break cross-file references.

---

## Diagnostics

**US-07** — As a writer, broken links (references to files that don't exist) are
surfaced as diagnostics (warnings), so I can find dead links without manually
checking.

**US-08** — As a writer, I can see when a heading anchor in a
`[text](note.md#heading)` link no longer exists (matched against the GFM slug
of each heading), so heading renames don't silently break links.

**US-32** — As a writer, I see a warning when a file contains two or more
headings with the same text, so I know that anchor links targeting that heading
are ambiguous.

**US-34** — As a writer, a diagnostic is shown when a link points to the file it
appears in, so accidental self-links are caught rather than silently ignored.

---

## Hover & Previews

**US-09** — As a writer, hovering over a `[text](path/to/note.md)` link shows a
preview of the first N lines of the target file, so I can recall note contents
without switching files.

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

**US-35** — As a writer, tags are highlighted as a distinct semantic token type,
so my editor theme can color them independently of plain text and standard
Markdown syntax — for example, coloring a broken link differently from a valid
one.

**US-36** — As a writer, I can collapse heading sections and fenced code blocks
in the current file using my editor's folding controls, so I can focus on the
section I'm working on in long notes.

---

## Workspace Awareness

**US-16** — As a writer, the server watches for new, renamed, and deleted files
and updates its index incrementally, so completions and diagnostics are always
current without restarting.

**US-26** — As a writer, standard Markdown links to non-Markdown files
(`![alt](attachments/image.png)`, `[doc](attachments/report.pdf)`) that exist in
my workspace resolve correctly and do not produce broken-link diagnostics, so
notes with pasted attachments aren't cluttered with false warnings.

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

---

## Code Actions & Refactoring

**US-18** — As a writer, when I'm on a broken `[text](path/to/missing.md)` link,
a code action lets me create the missing file, so I can stub out notes without
leaving my editor.

**US-29** — As a writer, when I'm on a `[text](note.md#missing-anchor)`
diagnostic, a code action shows me the available headings from the target file so
I can pick the right one and fix the broken anchor without leaving my editor.

**US-30** — As a markdown author, I can optionally set `newNoteDir` in
`initializationOptions` to a folder path (e.g. `"0-Inbox"`) so that all notes
created by the Quick Fix "Create note" action land in that folder — relative to
the workspace root — instead of next to the current file. This lets me keep all
unprocessed stubs in one place (an inbox) regardless of where the link appears.

**US-31** — As a Zed user, I can add a `$schema` key to my knap
`initialization_options` block in `settings.json` and immediately get
autocompletion and inline validation for all recognized keys (`extensions`),
so I can configure the server without consulting external documentation and the
editor flags unknown keys on the spot.

**US-19** — As a writer, I can select text in a note and apply a code action to
extract it into a new note, replacing the selection with a standard Markdown link
to the new note, so I can split overgrown notes without manual copy-paste.

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

---

## Frontmatter

**US-23** — As a writer, the server parses YAML frontmatter `title` fields and
uses them as the display name in completions, so I see human-readable titles
instead of filenames when inserting a link.

**US-24** — As a writer, I get completions and validation for frontmatter keys
and values defined in a schema I provide, so structured metadata stays
consistent.

---

## Developer / Debug CLI

These stories are for development and debugging — they let you invoke individual
components from the command line to verify behavior without a running editor.

**US-D01** — As a developer, I can run `knap parse <file>` to see the Markdown
links and their LSP ranges extracted from a file, so I can verify parser behavior
without a running editor.

**US-D02** — As a developer, I can run `knap index <dir>` to see the full note
index built from a directory, including which links are found, broken, or
unresolvable, so I can verify link resolution without a running editor.

---

## Deferred / Out of Scope

- Full Markdown formatting (bold, italic, tables) — handled by other tools like
  `marksman` or `prettier`
- Wiki-link syntax (`[[note]]`) — intentionally out of scope; knap uses standard
  Markdown links only
- Git integration
- Graph visualization
- Sync / publishing
