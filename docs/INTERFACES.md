# Interface map: human LSP and agent CLI

knap has two front ends over one core (the `NoteIndex`, `config`, and the
`compute_*` functions in `src/handlers.rs`). This page lists each operation
once, with the way a human does it through an editor talking to `knap lsp`
and the way an agent does it with the headless CLI, so the two faces cannot
drift apart. It covers what is built; anything not listed is not built.

> **Rule: every change to the user-facing surface updates these tables.**
> Adding, renaming, or removing a CLI subcommand or flag, an LSP capability,
> a code action, or a JSON output field edits this page in the same change.
> An operation both sides have goes in the shared tables, with both columns
> filled. An operation only one side has goes under "LSP only" or "CLI
> only", with the reason the other side doesn't have it. A change that
> touches the surface without touching this file is incomplete.

These tables own the command and capability surface. The README and
`docs/GETTING_STARTED.md` explain features at length, and
`skills/knap/SKILL.md` teaches the agent loop; they link here instead of
restating the mapping.

## Shared operations

### Checking

| Operation | Agent (CLI) | Human (LSP) |
|---|---|---|
| Find broken links | `knap lint [path]`: `broken-link` for a missing file. Attachment links (images, PDFs) and directory links resolve against the whole workspace | `textDocument/publishDiagnostics` warning on the link, pushed on `didOpen`, `didChange` and watched-file changes, for every doc whose state changed |
| Find broken anchors | `knap lint`: `broken-anchor` for a cross-file `doc.md#slug` or same-file `#slug` that matches no heading | Same diagnostic, squiggled on the anchor |
| Check frontmatter against the schema | `knap lint`: `missing-frontmatter`, `missing-required-field`, `invalid-field-value`, `unknown-field` | Same diagnostics |
| Suppress a known-broken target | `--ignore-link-target <glob>` (repeatable), unioned with `knap.toml` `ignore_link_targets` and the doc's `ignore-link-targets:` frontmatter | `initializationOptions.ignoreLinkTargets`, unioned with `knap.toml` and frontmatter the same way |

### Fixing

| Operation | Agent (CLI) | Human (LSP) |
|---|---|---|
| Repoint a broken anchor | `knap apply` with `{"op":"repoint-anchor","file","range","anchor"}`; `range` copied verbatim from the diagnostic | Quick fix `Change anchor to "<slug>"`, one per heading in the target, unranked |

### Refactoring

| Operation | Agent (CLI) | Human (LSP) |
|---|---|---|
| Rename / move a file | `knap rename-file <old> <new>` (alias `move-file`); fails if `<old>` is missing or `<new>` exists. Or `{"op":"rename-file","old","new"}` in `knap apply` | Rename the file in the editor's file tree; the client sends `workspace/willRenameFiles` and applies the returned edit to incoming and outgoing links |
| Rename a heading | `knap rename-heading <file> <old> <new>` (`<old>` is the text or its GFM slug); fails if nothing matches. Or `{"op":"rename-heading",...}` in `apply` | Rename symbol on the heading line (`prepareRename` pre-fills the text, `textDocument/rename` rewrites it and every same-file and cross-file anchor) |
| Rename a tag | `knap rename-tag <old> <new>`; fails if no file uses `<old>`. Or `{"op":"rename-tag",...}` in `apply` | Rename symbol on a frontmatter tag (pre-filled with the tag); all three YAML forms |
| Preview before writing | `knap apply --dry-run` (exact file count, no `hint`) | The client's rename preview, where it has one |
| Report what changed | Text summary line; `--json` gives `{op, summary, files_touched, hint}` for `rename-*`, `{dry_run, operations, files_touched, hint}` for `apply` | The client applies the edit and shows it in open buffers |

### Reading the workspace

| Operation | Agent (CLI) | Human (LSP) |
|---|---|---|
| Follow a link | `knap index <file> --json`: each `links[]` entry has `target`, `anchor`, and `resolved` (path, or `null` if broken) | Go to Definition on the link; lands on the heading line when there's an anchor |
| Who links to this doc | `knap index <file> --json`: `backlinks[]` (paths) | Find References in the doc; code lens `↑ N backlinks` on the first line |
| Where a tag is used | `knap index <dir> --json`: top-level `tags` map | Find References or Go to Definition on the tag |
| Outline of a doc | `knap index <file> --json`: `headings[]` with `text`, `level`, `range` | Document Symbols |
| Search headings and tags across the workspace | `knap index <dir> --json`, then filter | Workspace Symbols (fuzzy; tags as `SymbolKind::KEY`) |
| See a linked doc's title | `knap index --json`: `title` on the target doc | Inlay hint `-> <title>` after the link path |

### Running and debugging

| Operation | Agent (CLI) | Human (LSP) |
|---|---|---|
| Start | Each subcommand indexes from scratch and exits. `lint`/`index` root at `path` (its parent for a file); `rename-*` and `apply` root at the current directory | The editor runs `knap lsp` on stdio; the index stays live |
| Learn the tool | `knap --help`, `knap <cmd> --help`; `skills/knap/SKILL.md` and its `reference/` for the edit → verify loop | README, `docs/GETTING_STARTED.md`; the editor's own UI |
| Check the server works | `knap check`: in-process LSP smoke test | Same command, from a terminal |
| Version | `knap version`, `knap --version` | `serverInfo` in the `initialize` response |
| More logging | `-v` (debug), `-vv` (trace), or `KNAP_LOG` (wins over the flag) | Same flags on `knap lsp`; logs go to stderr, i.e. the client's server log |

### Cross-cutting

| Concern | Agent (CLI) | Human (LSP) |
|---|---|---|
| Configuration | `knap.toml` at the root | `initializationOptions` layered over `knap.toml` field by field; `exclude` and `ignoreLinkTargets` union with it, `skipDirs` replaces it |
| Excluding files | `--exclude <glob>` (repeatable, `lint` and `index`), unioned with `knap.toml` `exclude` | `initializationOptions.exclude`, unioned with `knap.toml` |
| Seeing the other side's writes | Every call re-reads the workspace from disk | `workspace/didChangeWatchedFiles` (registered at startup) re-indexes files changed outside the editor, including an agent's `rename-*` or `apply` |
| Atomicity | `rename-*` and `apply` write all files or none | The client applies the `WorkspaceEdit` |

## LSP only

| Operation | Human (LSP) | Why the CLI doesn't have it |
|---|---|---|
| Create the missing target of a broken link | Quick fix `Create note`: creates the file (under `new_note_dir` when set) and escapes the link target with `<...>` if needed | A human wants a stub one keystroke away; an agent creates files with its own tools and knows what to put in them |
| Who links to this heading | Find References on the heading; code lens `↑ N anchor link(s)` above it | `knap index` reports backlinks per file, not per heading; an agent renaming a heading uses `rename-heading`, which finds every anchor for it |
| Insert a link path | Completion after `(`: folder-by-folder browser, or type any filename segment to jump to any doc or attachment; an item links to the folder itself | An agent already knows its target; `knap index` lists every doc |
| Insert an anchor | Completion after `#` in a link (other file) or `[text](#` (this file); inserts the GFM slug | `knap index <file> --json` lists the headings; `lint` catches a wrong slug |
| Insert a tag | Completion inside `tags:`; tags already on the doc are left out | `knap index --json` lists every tag |
| Insert a frontmatter key or value | Completion of unused schema keys (`FIELD`) and allowed values (`VALUE`) | An agent reads `knap.toml`; `lint` catches what doesn't match |
| Fold sections and code blocks | `textDocument/foldingRange` | Display only; an agent reads the whole file |
| Expand / shrink the selection | `textDocument/selectionRange`: word → link → paragraph → section → document | An agent has no cursor or selection |

## CLI only

| Operation | Agent (CLI) | Why the LSP doesn't have it |
|---|---|---|
| Lint only what changed | `knap lint --since <git-ref>` (tracked diffs plus untracked files; needs git) | The LSP is already incremental: it re-checks only the docs an edit affects |
| Gate CI on severity | `knap lint --fail-on error\|warning\|info\|hint` (default `warning`); exit follows `blocking_count` | An editor session has no exit code |
| Machine-readable report | `--json` on `lint` (`diagnostics` with a stable `code` each, `problem_count`, `file_count`, `blocking_count`, `hint`), `index`, `rename-*` and `apply` | The editor renders LSP responses itself |
| See ranked fix candidates | `knap lint --suggest [N]` (default 3): `data.suggestions[]` with `target`, `distance`, `text_distance`; `data.text_mismatch` when path and text rankings disagree; `data.same_basename` when two `broken-link` candidates share a filename | An agent needs every candidate in one call to choose without a UI; a human picks with path completion and sees the result. LSP diagnostics carry no `data` |
| Repoint a broken link | `knap apply` with `{"op":"repoint-link","file","range","target"}`; `range` copied verbatim from the diagnostic | A human retypes the link with path completion; an agent needs a guarded edit that fails instead of corrupting the file if `range` is stale |
| Batch several changes, all-or-nothing | `knap apply` reads a JSON array on stdin, runs ops in order against a scratch copy, writes only if every op succeeds | Saves an agent one subprocess per change; an editor applies one rename at a time as the human makes it |
| Whole-workspace snapshot | `knap index <dir> [--json]`: `{notes: [...], tags: {...}}` | A human navigates with symbols and references instead of reading a dump |
| One doc's neighborhood | `knap index <file> [--json]`: a single doc object; `hint` points at `lint --suggest` when it has an unresolved link | The editor shows the same facts in place: diagnostics, lenses, inlay hints |
| Next-step hints | `hint` field (JSON) or trailing line (text) naming what to run next | Teaches an agent the loop as it goes; the editor surfaces the next step as quick fixes |
| Inspect one file's parse | `knap parse <file>` | A debugging aid for knap itself, not an editor feature |

## Exit codes

| Exit | Meaning |
|---|---|
| `0` | Success; for `lint`, no diagnostics at or above `--fail-on` |
| `1` | `lint` found blocking problems (`Error: N problem(s) found` on stderr), or any command failed (missing file, heading or tag not found, a failed `apply` op, `--since` outside git) |
| `2` | Usage error from argument parsing |
