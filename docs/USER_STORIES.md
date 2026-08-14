# knap — Markdown LSP Server: User Stories

A language server for Markdown that brings smart linking, navigation, and
diagnostics to any LSP-compatible editor — using standard Markdown syntax.

> **Scope note (2026-05-09):** knap dropped wiki-link (`[[...]]`) support in
> favour of standard Markdown links. Stories US-07b, US-22, US-27, US-39, and
> US-41 were removed as a result. Old design docs for v0.1–v0.8 reference the
> former story IDs — those are historical artifacts for shipped releases.
> See [ARCHITECTURE.md](ARCHITECTURE.md) for the design tenets behind this
> decision.

Stories are grouped by persona, not just feature:

- **Writer (Human)** — a person maintaining a personal knowledge base,
  through an editor (LSP) or the CLI directly.
- **Agent** — a coding agent or CI script scripting the same engine
  headlessly, via CLI + a shipped skill.
- **Knap Contributor** — someone developing knap itself; not a knap user.

---

## Writer (Human)

The primary persona: someone maintaining a personal knowledge base of
Markdown notes. Most stories below are LSP capabilities delivered through an
editor via `knap lsp`; the Configuration stories are what wires the editor up
in the first place. None of this is agent-specific — see **Agent**, below,
for the CLI-driven, script/edit-verify-loop persona layered on top of the
same engine.

### Core Linking

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

### Diagnostics

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

### Hover & Previews

**US-09** — As a writer, hovering over a `[text](path/to/note.md)` link shows a
preview of the first N lines of the target file, so I can recall note contents
without switching files.

**US-10** — As a writer, hovering over a standard Markdown image or link shows a
summary/preview, so context is always one hover away.

---

### Symbols & Navigation

**US-11** — As a writer, Document Symbols lists all headings in the current file
so I can jump to any section quickly.

**US-12** — As a writer, Workspace Symbols lets me search headings across all
files, so I can navigate the entire knowledge base by heading name.

**US-13** — As a writer, I can use `Go to Definition` on a frontmatter tag value
to see all files that use that tag, so I can explore topics by tag.

---

### Tags

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

### Editor Experience

**US-35** — As a writer, tags are highlighted as a distinct semantic token type,
so my editor theme can color them independently of plain text and standard
Markdown syntax — for example, coloring a broken link differently from a valid
one.

**US-36** — As a writer, I can collapse heading sections and fenced code blocks
in the current file using my editor's folding controls, so I can focus on the
section I'm working on in long notes.

**US-52** — As a writer, I can use my editor's expand/contract selection command
to grow or shrink my selection through the Markdown structure of the current note
— from the word under the cursor outward to link, paragraph, heading section, and
finally the whole document — so I can select exactly the text I need without
reaching for the mouse.

**US-53** — As a writer, when I have a link to a note that includes a `title:`
frontmatter key, I see that title displayed inline next to the link path (as an
inlay hint), so I know where the link leads without opening the target file.

**US-54** — As a writer, headings that are the target of one or more bare anchor
links (`[text](#slug)` same-file or `[text](note.md#slug)` cross-file) show an
`↑ N anchor links` code lens, so I know which headings are referenced and can
jump directly to those references.

---

### Workspace Awareness

**US-16** — As a writer, the server watches for new, renamed, and deleted files
and updates its index incrementally, so completions and diagnostics are always
current without restarting.

**US-26** — As a writer, standard Markdown links to non-Markdown files
(`![alt](attachments/image.png)`, `[doc](attachments/report.pdf)`) that exist in
my workspace resolve correctly and do not produce broken-link diagnostics, so
notes with pasted attachments aren't cluttered with false warnings.

---

### Backlinks

**US-25** — As a writer, I can optionally display a backlinks section at the
bottom of the current note (via a virtual document or inlay) showing all files
that link to it, so I can see the note's context in my knowledge base without
leaving the file.

---

### Workspace Insight

**US-38** — As a writer, notes with no incoming links (orphans) are surfaced as
hint-level diagnostics, so I can identify isolated notes that may need to be
connected or archived.

---

### Code Actions & Refactoring

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

**US-42** — As a writer, I can optionally configure a `templateDir` in
`initializationOptions` pointing to a folder of Markdown templates; when a new
note is created via Quick Fix, the server picks a matching template
and expands it with variables like `{{title}}` and `{{date}}`, so new notes
start with consistent structure.

---

### Configuration

**US-20** — As an editor integrator, I can optionally configure a `noteRoot` to
restrict indexing to a subdirectory of the workspace (e.g. a `docs/` folder
inside a monorepo), so the server doesn't index unrelated files. When omitted,
all workspace folders are indexed.

**US-21** — As an editor integrator, I can configure file extensions the server
should treat as notes (e.g. `.md`, `.mdx`, `.markdown`).

**US-31** — As a Zed user, I can add a `$schema` key to my knap
`initialization_options` block in `settings.json` and immediately get
autocompletion and inline validation for all recognized keys (`extensions`),
so I can configure the server without consulting external documentation and the
editor flags unknown keys on the spot.

**US-D06** — As an editor extension author, I invoke `knap lsp` explicitly to
start the server; bare `knap` no longer falls back to it, so the CLI's other
subcommands aren't shadowed by an implicit server start.

**US-D07** — As a workspace owner, I can define a `knap.toml` at my workspace
root, read the same way by `knap lsp`, `knap lint`, and `knap index`, so
headless commands see the same configuration an editor session would. (This is
also what makes the Agent persona's commands, below, behave predictably —
`knap.toml` is the one config surface both personas share.)

---

### Frontmatter

**US-23** — As a writer, the server parses YAML frontmatter `title` fields and
uses them as the display name in completions, so I see human-readable titles
instead of filenames when inserting a link.

**US-24** — As a writer, I get completions and validation for frontmatter keys
and values defined in a schema I provide, so structured metadata stays
consistent.

---

## Agent

A coding agent (Claude Code or similar) — or a CI script — editing a vault's
Markdown files with no editor session in the loop. Delivered entirely through
the CLI: `knap lint`, `knap index`, `knap rename-*`, `knap fix`, plus a
shipped skill (`skill/knap/SKILL.md`) that teaches the loop these commands
are built around — edit → `knap lint --json` → branch on each diagnostic's
`code` → `knap fix`/`rename-*` → `knap lint` again to confirm clean. Nothing
below is agent-exclusive machinery — a human can run any of these commands by
hand from a terminal too — but the design center is that loop, not a person
at a keyboard.

### Linting

**US-D04** — As an agent or CI script, I can run `knap lint [path] [--json]` to
get link/anchor/frontmatter diagnostics headlessly, so I can check whether an
edit broke any links without a running editor.

**US-D11** — As an agent, every diagnostic from `knap lint [--json]` (and the
identical diagnostics an editor sees via `textDocument/publishDiagnostics`)
carries a stable `code` (`broken-link`, `broken-anchor`, `missing-frontmatter`,
`missing-required-field`, `invalid-field-value`, `unknown-field`), so I can
branch on a fixed identifier instead of pattern-matching message text.

**US-D16** — As an agent or CI script, I can pass `--fail-on <severity>` to
`knap lint` so only diagnostics at or above a given severity cause a non-zero
exit, instead of any diagnostic at all.

**US-D12** — As an agent, I can run `knap lint --since <git-ref>` to scope
linting to files changed since a git ref — tracked changes plus untracked new
files — so a check I run after every edit doesn't re-scan the whole workspace
each time.

---

### Indexing

**US-D05** — As an agent, I can run `knap index <path> --json` to get a
structured snapshot of the workspace (notes, headings, links, backlinks,
tags), so I can get a fast structural view without grepping every file.

**US-D13** — As an agent, I can run `knap index <file>` to get just that
note's neighborhood (headings, outgoing links, backlinks, tags) instead of
the full workspace snapshot, so I can inspect a note I just edited without
paging through every note in a large vault. (`knap index <dir>` keeps
printing the full workspace snapshot, unchanged.)

---

### Renaming

**US-D08** — As an agent, I can run `knap rename-file <old> <new>` to move a
note and atomically rewrite every incoming link (from other notes) and
outgoing link (from the moved note itself), so I can restructure a workspace
without an editor session and without hand-tracking every affected file.

**US-D09** — As an agent, I can run
`knap rename-heading <file> <old> <new>` to rewrite a heading's text and every
`[text](note.md#old-slug)` / `[text](#old-slug)` anchor link that targets it
(same-file and cross-file), so a heading rename stays consistent across the
workspace without an editor session. `<old>` matches either the heading's
literal text or its GFM slug.

**US-D10** — As an agent, I can run `knap rename-tag <old> <new>` to rewrite
every frontmatter occurrence of a tag across the workspace atomically, so I
can normalize taxonomy without an editor session.

---

### Fixing

**US-D14** — As an agent, I can run `knap fix [path] [--dry-run]` to apply the
same safe fixes an editor's code actions offer — creating a missing linked
file, and replacing a broken anchor with the target file's best-matching
heading when the match is unambiguous — so I can clear straightforward lint
findings without hand-writing the edit myself. `--dry-run` previews the plan
without touching disk. When a broken link has one unambiguous best-matching
existing note (by the same edit-distance ranking `--suggest` exposes, below),
`knap fix` repoints the link there instead of creating a stub file.

**US-D17** — As an agent, I can pass `--suggest [N]` to `knap lint` to get up
to `N` ranked candidate fixes (closest match first) attached to each
`broken-link`/`broken-anchor` diagnostic's `data` field in `--json` output,
so I can see the same candidates `knap fix` uses to decide — including for
the ambiguous cases it declines to touch — without a separate `knap fix
--dry-run` call. I can also pass `--fix` to have `knap lint` apply every
safe fix first (same as running `knap fix`) and report only what's left,
collapsing the usual lint → fix → lint-again sequence into one call;
`fixes_applied` in `--json` output lists what was applied. `--fix` is the
one case where `knap lint` mutates files on disk.

**US-D20** — As an agent, `knap fix`'s auto-apply and `knap lint --suggest`'s
ranked candidates both weigh a `broken-link`/`broken-anchor`'s own visible
link text against each candidate's name, not just the broken target/slug
string's path distance, so a same-shape decoy that's closer by raw edit
distance (`sync-800.md`) doesn't outrank the candidate the link text actually
names (`sync-835.md`, for link text "Sync 835"). Every diagnostic's `data`
also carries a `text_mismatch` flag when the two signals disagree on which
candidate is best, so I get an explicit warning instead of a silently
plausible top pick — and `knap fix`/`knap lint --fix` decline to auto-apply
when that flag is set, even if the combined ranking otherwise found a single
unambiguous winner.

---

### Batch Apply

**US-D18** — As an agent, I can run `knap apply --json` and pipe a JSON array
of change operations (`rename-file`, `rename-heading`, `rename-tag`, `fix`)
on stdin, so after running `knap lint --suggest --json` and picking the right
fix for each finding myself, I can apply the whole batch in one call instead
of one subprocess per change. Changes are applied in the order given; the
batch is all-or-nothing — the workspace ends up either fully changed or
untouched, never partially applied. `--dry-run` previews the planned result
without touching disk.

**US-D19** — As an agent, I can include `repoint-link { file, range, target }`
and `repoint-anchor { file, range, anchor }` operations in the same `knap
apply --json` batch as any `rename-*`/`fix` operations, so a judgement call I
made from `knap lint --suggest`'s ranked candidates for a `broken-link`/
`broken-anchor` diagnostic can be applied atomically alongside everything
else in one call — instead of falling back to a hand edit outside the batch.
`file` and `range` are exactly the diagnostic's own `path`/`range` fields, so
no re-locating the edit is needed.

---

### The Skill

**US-D15** — As an agent, a `skill/knap/SKILL.md` shipped with knap documents
the lint → fix/rename → lint loop — which flags to pass, how to read `--json`
output, which diagnostic codes each subcommand resolves — so I can pick up
the intended workflow without reverse-engineering it from `--help` text.

---

## Knap Contributor

Not a knap user at all — someone developing knap itself. These stories exist
purely to exercise the parser, index, and CLI wiring from a terminal while
working on knap's own source, without a running editor in the loop.

**US-D01** — As a developer, I can run `knap parse <file>` to see the Markdown
links and their LSP ranges extracted from a file, so I can verify parser behavior
without a running editor.

**US-D02** — As a developer, I can run `knap index <dir>` to see the full note
index built from a directory, including which links are found, broken, or
unresolvable, so I can verify link resolution without a running editor.

**US-D03** — As a developer, I can run `knap version` to print the version of
the installed binary, so I can confirm which release is active without starting
the LSP server.

---

## Deferred / Out of Scope

- Full Markdown formatting (bold, italic, tables) — handled by other tools like
  `marksman` or `prettier`
- Wiki-link syntax (`[[note]]`) — intentionally out of scope; knap uses standard
  Markdown links only
- Git integration
- Graph visualization
- Sync / publishing
