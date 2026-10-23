# Interface map: human LSP and agent CLI

knap has two front ends over one core (the `NoteIndex`, `config`, and the
`compute_*` functions in `src/handlers.rs`). This page lists each operation
once, with the way a human does it through an editor talking to `knap lsp`
and the way an agent does it with the headless CLI, so the two faces cannot
drift apart. It covers what is built; anything not listed is not built.

> **Rule: every change to the user-facing surface updates this table.**
> Adding, renaming, or removing a CLI subcommand or flag, an LSP capability,
> a code action, or a JSON output field edits the table in the same change,
> for both columns. When one side has no equivalent, the cell says so and the
> asymmetry is listed under "Deliberate asymmetries". A change that touches
> the surface without touching this file is incomplete.

This table owns the command and capability surface. The README and
`docs/GETTING_STARTED.md` explain features at length, and
`skills/knap/SKILL.md` teaches the agent loop; they link here instead of
restating the mapping.

## Operations

### Checking

| Operation | Agent (CLI) | Human (LSP) | Core call |
|---|---|---|---|
| Find broken links | `knap lint [path]`: `broken-link` for a missing file. Attachment links (images, PDFs) and directory links resolve against the whole workspace | `textDocument/publishDiagnostics` warning on the link, pushed on `didOpen`, `didChange` and watched-file changes, for every doc whose state changed | `compute_diagnostics` |
| Find broken anchors | `knap lint`: `broken-anchor` for a cross-file `doc.md#slug` or same-file `#slug` that matches no heading | Same diagnostic, squiggled on the anchor | `compute_diagnostics` |
| Check frontmatter against the schema | `knap lint`: `missing-frontmatter`, `missing-required-field`, `invalid-field-value`, `unknown-field` | Same diagnostics | `compute_diagnostics` |
| Suppress a known-broken target | `--ignore-link-target <glob>` (repeatable), unioned with `knap.toml` `ignore_link_targets` and the doc's `ignore-link-targets:` frontmatter | `initializationOptions.ignoreLinkTargets`, unioned with `knap.toml` and frontmatter the same way | `Config` |
| Lint only what changed | `knap lint --since <git-ref>` (tracked diffs plus untracked files; needs git) | n/a: diagnostics are already per-doc and live | none |
| Gate CI on severity | `knap lint --fail-on error\|warning\|info\|hint` (default `warning`); exit follows `blocking_count` | n/a | none |
| Machine-readable report | `knap lint --json`: `diagnostics` (each with a stable `code`), `problem_count`, `file_count`, `blocking_count`, `hint` (only when there's something to follow up) | n/a: the editor renders diagnostics | none |

### Fixing

| Operation | Agent (CLI) | Human (LSP) | Core call |
|---|---|---|---|
| See ranked fix candidates | `knap lint --suggest [N]` (default 3): `data.suggestions[]` with `target`, `distance`, `text_distance`; `data.text_mismatch` when path and text rankings disagree; `data.same_basename` when two `broken-link` candidates share a filename | n/a: diagnostics carry no `data` (see asymmetries) | `compute_diagnostics_with_suggestions` |
| Repoint a broken link | `knap apply` with `{"op":"repoint-link","file","range","target"}`; `range` copied verbatim from the diagnostic | Hand edit, helped by path completion | `compute_link_fix` |
| Repoint a broken anchor | `knap apply` with `{"op":"repoint-anchor","file","range","anchor"}` | Quick fix `Change anchor to "<slug>"`, one per heading in the target, unranked | `compute_anchor_fix` |
| Create the missing target of a broken link | n/a: the agent writes the file itself | Quick fix `Create note`: creates the file (under `new_note_dir` when set) and escapes the link target with `<...>` if needed | `compute_create_missing_file_fix` |

### Refactoring

| Operation | Agent (CLI) | Human (LSP) | Core call |
|---|---|---|---|
| Rename / move a file | `knap rename-file <old> <new>` (alias `move-file`); fails if `<old>` is missing or `<new>` exists. Or `{"op":"rename-file","old","new"}` in `knap apply` | Rename the file in the editor's file tree; the client sends `workspace/willRenameFiles` and applies the returned edit to incoming and outgoing links | `handle_will_rename_files` |
| Rename a heading | `knap rename-heading <file> <old> <new>` (`<old>` is the text or its GFM slug); fails if nothing matches. Or `{"op":"rename-heading",...}` in `apply` | Rename symbol on the heading line (`prepareRename` pre-fills the text, `textDocument/rename` rewrites it and every same-file and cross-file anchor) | `compute_heading_rename` |
| Rename a tag | `knap rename-tag <old> <new>`; fails if no file uses `<old>`. Or `{"op":"rename-tag",...}` in `apply` | Rename symbol on a frontmatter tag (pre-filled with the tag); all three YAML forms | `compute_tag_rename` |
| Batch several changes, all-or-nothing | `knap apply` reads a JSON array on stdin, runs ops in order against a scratch copy, writes only if every op succeeds | n/a: each rename is one `WorkspaceEdit` | `apply::run` |
| Preview before writing | `knap apply --dry-run` (exact file count, no `hint`) | The client's rename preview, where it has one | none |
| Report what changed | Text summary line; `--json` gives `{op, summary, files_touched, hint}` for `rename-*`, `{dry_run, operations, files_touched, hint}` for `apply` | The client applies the edit and shows it in open buffers | none |

### Reading the workspace

| Operation | Agent (CLI) | Human (LSP) | Core call |
|---|---|---|---|
| Follow a link | `knap index <file> --json`: each `links[]` entry has `target`, `anchor`, and `resolved` (path, or `null` if broken) | Go to Definition on the link; lands on the heading line when there's an anchor | `handle_definition` |
| Who links to this doc | `knap index <file> --json`: `backlinks[]` (paths) | Find References in the doc; code lens `↑ N backlinks` on the first line | `handle_references`, `handle_code_lens` |
| Who links to this heading | n/a: `backlinks` is per file; grep `#<slug>` or use `lint` after a change | Find References on the heading; code lens `↑ N anchor link(s)` above it | `handle_references`, `handle_code_lens` |
| Where a tag is used | `knap index <dir> --json`: top-level `tags` map | Find References or Go to Definition on the tag | `handle_references` |
| Outline of a doc | `knap index <file> --json`: `headings[]` with `text`, `level`, `range` | Document Symbols | `handle_document_symbols` |
| Search headings and tags across the workspace | `knap index <dir> --json`, then filter | Workspace Symbols (fuzzy; tags as `SymbolKind::KEY`) | `handle_workspace_symbols` |
| Whole-workspace snapshot | `knap index <dir> [--json]`: `{notes: [...], tags: {...}}` | n/a | `NoteIndex::report` |
| One doc's neighborhood | `knap index <file> [--json]`: a single doc object; `hint` points at `lint --suggest` when it has an unresolved link | n/a: the editor shows it in place | `NoteIndex::note_report` |
| See a linked doc's title | `knap index --json`: `title` on the target doc | Inlay hint `-> <title>` after the link path | `handle_inlay_hints` |

### Writing

| Operation | Agent (CLI) | Human (LSP) | Core call |
|---|---|---|---|
| Insert a link path | n/a: the agent knows its target (`index` lists every doc) | Completion after `(`: folder-by-folder browser, or type any filename segment to jump to any doc or attachment; an item links to the folder itself | `handle_completion` |
| Insert an anchor | n/a: `index <file> --json` lists headings; slug them with GFM rules | Completion after `#` in a link (other file) or `[text](#` (this file); inserts the GFM slug | `handle_completion` |
| Insert a tag | n/a: `index --json` lists tags | Completion inside `tags:`; tags already on the doc are left out | `handle_completion` |
| Insert a frontmatter key or value | n/a: the agent reads `knap.toml` | Completion of unused schema keys (`FIELD`) and allowed values (`VALUE`) | `handle_completion` |
| Fold sections and code blocks | n/a | `textDocument/foldingRange` | `handle_folding_ranges` |
| Expand / shrink the selection | n/a | `textDocument/selectionRange`: word → link → paragraph → section → document | `handle_selection_range` |

### Running and debugging

| Operation | Agent (CLI) | Human (LSP) | Core call |
|---|---|---|---|
| Start | Each subcommand indexes from scratch and exits. `lint`/`index` root at `path` (its parent for a file); `rename-*` and `apply` root at the current directory | The editor runs `knap lsp` on stdio; the index stays live | `server::run` |
| Learn the tool | `knap --help`, `knap <cmd> --help`; `skills/knap/SKILL.md` and its `reference/` for the edit → verify loop | README, `docs/GETTING_STARTED.md`; the editor's own UI | none |
| Check the server works | `knap check`: in-process LSP smoke test | Same command, from a terminal | `check::run` |
| Inspect one file's parse | `knap parse <file>` | n/a | `parser::parse` |
| Version | `knap version`, `knap --version` | `serverInfo` in the `initialize` response | none |
| More logging | `-v` (debug), `-vv` (trace), or `KNAP_LOG` (wins over the flag) | Same flags on `knap lsp`; logs go to stderr, i.e. the client's server log | none |

## Cross-cutting

| Concern | Agent (CLI) | Human (LSP) |
|---|---|---|
| Configuration | `knap.toml` at the root | `initializationOptions` layered over `knap.toml` field by field; `exclude` and `ignoreLinkTargets` union with it, `skipDirs` replaces it |
| Excluding files | `--exclude <glob>` (repeatable, `lint` and `index`), unioned with `knap.toml` `exclude` | `initializationOptions.exclude`, unioned with `knap.toml` |
| Output | Text by default; `--json` on `lint`, `index`, `rename-*`, `apply` | Rendered by the editor |
| Next-step hints | `hint` field (JSON) or trailing line (text) naming what to run next | n/a |
| Seeing the other side's writes | Every call re-reads the workspace from disk | `workspace/didChangeWatchedFiles` (registered at startup) re-indexes files changed outside the editor, including an agent's `rename-*` or `apply` |
| Atomicity | `rename-*` and `apply` write all files or none | The client applies the `WorkspaceEdit` |

## Exit codes

| Exit | Meaning |
|---|---|
| `0` | Success; for `lint`, no diagnostics at or above `--fail-on` |
| `1` | `lint` found blocking problems (`Error: N problem(s) found` on stderr), or any command failed (missing file, heading or tag not found, a failed `apply` op, `--since` outside git) |
| `2` | Usage error from argument parsing |

## Deliberate asymmetries

- **Ranked suggestions are CLI-only.** The agent needs every candidate in
  one call to choose without a UI; the human gets path completion and an
  unranked anchor quick fix instead. LSP diagnostics carry no `data`.
- **Repointing a link is CLI-only; creating a missing note is LSP-only.**
  `repoint-link` exists because an agent can't hand-edit a link by cursor;
  `Create note` exists because a human wants a stub one keystroke away. An
  agent creates files with its own tools.
- **Batching is CLI-only.** `knap apply` saves an agent one subprocess per
  change; an editor applies one rename at a time.
- **Completions, folding, selection range, inlay hints and code lenses are
  LSP-only.** They help a human type and see; an agent reads `index --json`.
- **Per-heading references are LSP-only.** `knap index` reports backlinks
  per file, not per heading.
- **`--since`, `--fail-on`, `--suggest` and `--json` are CLI-only.** The LSP
  is already incremental and has no exit code.
