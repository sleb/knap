# knap — Architecture

High-level component design. Each component is described by its responsibility
and the contracts it exposes or depends on. Per-feature implementation details
live in release-level design docs.

---

## Design Tenets

**Standard Markdown first.** Knap uses plain `[text](path/to/file.md)` links
throughout. No wiki-link extensions, no proprietary syntax. Notes written with
knap render correctly in any Markdown tool — GitHub, static site generators,
other editors — without knap present.

**Explicit paths, no ambiguity.** Links use standard relative paths — relative to
the current file's location (e.g. `[My Note](../projects/foo.md)`). There is no
stem-based resolution and no concept of an "ambiguous" link. What you write is
what resolves.

**Portable over convenient.** Where there is a tradeoff between a clever
shorthand and a format that is legible without tooling, knap chooses legibility.
The editor integration provides the convenience (completions, quick-fix, rename);
the files stay clean.

---

## Overview

```
┌──────────────────────────────────────────────────────┐
│                 LSP Client (Editor)                  │
└──────────────────────────────────────────────────────┘
                  │ JSON-RPC over stdio / TCP
┌──────────────────────────────────────────────────────┐
│                   Transport Layer                    │
└──────────────────────────────────────────────────────┘
                           │
┌──────────────────────────────────────────────────────┐
│                   Protocol Handler                   │
│     lifecycle · capability negotiation · routing     │
└──────────────────────────────────────────────────────┘
                  │                    │
         ┌────────┴────────┐  ┌────────┴────────┐
         │    Request      │  │   Note Index    │
         │    Handlers     │◄─┤                 │
         └────────┬────────┘  └────────┬────────┘
                  │                    │
    WorkspaceEdit │           ┌────────┴───────┐
        (editor   │           │    Markdown    │
      applies it) │           │    Parser      │
                  ▼           └────────────────┘
             (filesystem)

┌──────────────────────────────────────────────────────┐
│                         CLI                          │
│    lsp · lint · index · parse · check · version ·    │
│    rename-file · rename-heading · rename-tag · fix   │
└──────────────────────────────────────────────────────┘
                  │ WorkspaceEdit
                  │ (headless commands only —
                  │  no editor in the loop)
                  ▼
┌──────────────────────────────────────────────────────┐
│                   Edit Applicator                    │
│      edit::apply(WorkspaceEdit) → files touched      │
└──────────────────────────────────────────────────────┘
                  │
                  ▼
             (filesystem)
```

---

## Configuration

`src/config.rs` is the single config-loading module shared by `knap lsp`,
`knap lint`, and `knap index` — headless commands see the same `Config` the
LSP would build for the same workspace, rather than diverging on their own
defaults.

```
Config {
  index_roots: PathBuf[]       // workspace folders (lsp) or the target path's directory (lint/index)
  extensions: string[]         // default: ["md"]
  new_note_dir: Option<string> // inbox folder for Quick Fix "Create note"; relative to index_roots[0]
  frontmatter_schema: FrontmatterSchema // key/value constraints; default: empty (no validation)
}
```

There are two sources, and two loader entry points that combine them
differently depending on whether an editor is involved:

- **`initializationOptions`** — the client passes user settings inside the
  `initialize` request. This is how all major editors expose per-server
  config (VS Code `settings.json`, Neovim `lspconfig`, Helix
  `languages.toml`). Only available when an editor is driving the session.
- **`knap.toml`** — an optional project config file at a workspace root,
  written in idiomatic snake_case TOML (see `README.md` for the full
  shape). Available to every entry point, including headless ones with no
  editor in the loop.

```rust
fn for_lsp(init_params: &InitializeParams) -> Result<Config>   // knap lsp
fn for_path(root: &Path, extensions_override: Option<Vec<String>>) -> Result<Config> // knap lint, knap index, knap rename-*
```

- `for_lsp` — `index_roots` from `workspaceFolders` in the `initialize`
  request (not user-configurable — whatever the editor has open). Looks for
  `knap.toml` in `index_roots[0]`, then layers `initializationOptions` over
  it field-by-field: the editor value wins where present, `knap.toml` fills
  in what's left, and the built-in defaults are the final fallback. A
  malformed `knap.toml` fails `initialize` outright; a malformed
  `initializationOptions` payload keeps the existing lenient behavior —
  `warn!` and default that field — since it's an editor-side concern the
  user doesn't directly author, unlike a `knap.toml` they wrote themselves.
- `for_path` — used by `lint`/`index`, and by the `rename-*` subcommands
  (always given `cwd`, never the target file — see the CLI section below),
  no editor involved. If the given path is a file, its parent directory is
  the root; `knap.toml` is looked up there only (no ancestor-directory
  search).

Configuration is resolved once, at startup, and fixed for the lifetime of
the session — `workspace/didChangeConfiguration` is not processed.

---

## File Change Notifications

The server does not run its own filesystem watcher. Instead, it uses the
LSP-native `workspace/didChangeWatchedFiles` mechanism:

- At `initialized`, the server registers interest in its configured extensions
  via `workspace/didRegisterCapability`
- The client delivers `workspace/didChangeWatchedFiles` notifications for
  external changes (e.g. git checkouts, files edited outside the editor)
- The client does **not** send `workspace/didChangeWatchedFiles` for files
  currently open in the editor — those are managed exclusively by
  `textDocument/didChange`

This means deduplication is handled by the client. Both notification types
converge on the same Note Index update interface, with no risk of
double-indexing an open file.

---

## Components

### Transport Layer

Owns the wire protocol. Reads and writes JSON-RPC 2.0 messages over stdio
(default) or TCP.

**Responsibilities:**

- Framing: Content-Length header encoding/decoding
- Serialising and deserialising JSON-RPC request/response/notification envelopes
- Forwarding decoded messages to the Protocol Handler
- Writing encoded responses back to the client

**Does not** know anything about LSP semantics — it only handles bytes and JSON.

---

### Protocol Handler

The server's front door. Owns the LSP session lifecycle and routes every
incoming message to the right handler.

**Responsibilities:**

- Managing the `initialize` / `initialized` / `shutdown` / `exit` lifecycle
- Resolving `workspaceFolders` and `initializationOptions` from `initialize`
  into a `Config` struct
- Registering file watchers with the client via
  `workspace/didRegisterCapability` at `initialized`
- Advertising server capabilities during `initialize` based on what handlers are
  registered
- Routing `textDocument/*` and `workspace/*` requests to Request Handlers
- Routing `textDocument/didOpen`, `didChange`, `didClose` and
  `workspace/didChangeWatchedFiles` notifications to the Note Index
- Sending `textDocument/publishDiagnostics` notifications on behalf of handlers
- Error handling: returning well-formed JSON-RPC error responses

**Contract (inbound):** receives decoded LSP messages  
**Contract (outbound):** calls into Request Handlers and Note Index, passing
`Config` as needed

---

### Markdown Parser

Parses a single Markdown file and returns a structured `Note`. Stateless and
pure — given the same source text it always returns the same result.

**Responsibilities (full target state — fields added per release):**

- Extracting standard Markdown links and images with position, target path, and
  optional heading anchor _(v0.1)_
- Extracting all headings with their level and text _(v0.3)_
- Extracting YAML frontmatter (title, tags, arbitrary keys) _(v0.1, extended v0.3)_
- Extracting fenced code block positions as `CodeFence` entries _(v0.9)_

**Contract:**

```
parse(path: string, content: string) → Note
```

`Note` grows across releases. See the per-release design docs for the current
shape. The parser does not resolve links — it only records what is written in
the file.

---

### Note Index

The server's central knowledge base. Maintains a live, queryable model of all
notes in the workspace.

**Responsibilities:**

- Building the initial index by parsing all files under the configured roots on
  startup
- Accepting incremental updates (note added, changed, deleted) from the Protocol
  Handler
- Resolving standard Markdown link paths to file paths within the workspace
- Detecting broken links (references to files or anchors that don't exist)
- Maintaining a reverse index: for each file, which files link to it (backlinks)

**Contract (writes):**

```
index(note: Note) → IndexDelta             // add or replace; returns affected paths for diagnostics
remove(path: string) → IndexDelta          // delete; returns affected paths for diagnostics
add_attachment(path: PathBuf) → IndexDelta // register a non-note file; may clear broken-link diagnostics
remove_attachment(path: PathBuf) → IndexDelta
```

**Contract (reads):**

```
resolve(source: Path, target: string) → ResolvedLink  // resolves target relative to source file
get_note(path: string) → Note | null
all_notes() → Note[]
links_to(path: string) → LocatedLink[]  // standard links from other notes pointing here
all_tags() → string[]
notes_by_tag(tag: string) → Note[]
```

The index is the single source of truth. Request Handlers read from it
exclusively — they do not touch the filesystem directly.

---

### CLI

`src/cli/` — one module per subcommand (`mod.rs`, `lsp.rs`, `lint.rs`,
`index.rs`, `parse.rs`, `check.rs`, `version.rs`, `rename.rs`, `fix.rs`),
wired up with `clap`.
`main.rs` is just logging setup plus `knap::cli::run()`. There is no
argument-free fallback: a subcommand is required, and bare `knap` exits
non-zero with usage text (clap's built-in behavior for a required
subcommand). In particular, **`knap` no longer starts the LSP server on its
own — use `knap lsp`.**

| Subcommand       | Usage                                                                                          | Available from                                                                        |
| ---------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `lsp`            | `knap lsp`                                                                                     | v0.11 (previously the bare-args default, since v0.1)                                  |
| `lint`           | `knap lint [path] [--json] [--fail-on <severity>] [--since <git-ref>] [--suggest [N]] [--fix]` | v0.11, `--fail-on`/`--since`/`--suggest`/`--fix` added v0.13                          |
| `index`          | `knap index <path> [--json]`                                                                   | v0.1, rewritten v0.11; a file `<path>` scopes to that note's neighborhood since v0.13 |
| `parse`          | `knap parse <file>`                                                                            | v0.1                                                                                  |
| `rename-file`    | `knap rename-file <old> <new>`                                                                 | v0.12                                                                                 |
| `rename-heading` | `knap rename-heading <file> <old> <new>`                                                       | v0.12                                                                                 |
| `rename-tag`     | `knap rename-tag <old> <new>`                                                                  | v0.12                                                                                 |
| `fix`            | `knap fix [path] [--dry-run]`                                                                  | v0.13                                                                                 |
| `check`          | `knap check`                                                                                   | v0.2                                                                                  |
| `version`        | `knap version`                                                                                 | v0.10.1                                                                               |

The CLI shares the same library crate as the server. `lsp` boots the same
stdio server the LSP Client talks to. `lint` and `index` both resolve config
via `config::for_path` and build the index via `index::build` — this is what
makes their behavior match the LSP for the same workspace, including
`knap.toml`; `lint` then calls the existing `handlers::compute_diagnostics`
per target file, and `index --json` serializes `NoteIndex::report()` for a
directory target, or `NoteIndex::note_report()` alone for a single note when
given a file. `index`'s file-input case resolves config off `cwd`, not the
file itself, for the same reason `rename-*` does below — a single note's
`backlinks` still need the whole vault indexed, not just its own directory.
`parse` calls `parser::parse` directly; `check` spins up a full in-process
server and exercises the LSP lifecycle as a smoke test. The three
`rename-*` subcommands resolve config via `config::for_path(cwd, ..)` — the
current directory, not the target file, since a file argument would
otherwise narrow the index to just that file's own directory — reuse the
same `handlers::` computation the LSP `rename`/`willRenameFiles` handlers
use, and hand the resulting `WorkspaceEdit` to the Edit Applicator
(`edit::apply`) to write it to disk. `fix` resolves config the same way
`lint` does (a file target is used as-is, a directory target scopes to it),
walks every target note's links, and for each broken link or broken anchor
calls the same `handlers::compute_create_missing_file_fix`/
`handlers::compute_anchor_fix` functions the LSP "Create note"/"Change
anchor to..." code actions call — `knap fix` reuses those computations the
same way `rename-*` reuses the rename `compute_*` functions, picking the
anchor to fix via `handlers::suggest_anchor_fix` (skipping anything
ambiguous) since it has no cursor to let a human choose; for a broken link it
tries `handlers::suggest_link_fix`/`handlers::compute_link_fix` first
(repoint to the one unambiguous closest-matching existing note), falling
back to `compute_create_missing_file_fix` when no candidate is unambiguous.
The fix-selection loop itself lives in `cli::fix::plan_fixes`/`apply`
(`pub(crate)`), shared with `lint --fix` (below) so both apply the identical
unambiguous-only contract. No editor is needed for any of them.

`lint --suggest [N]` switches diagnostic computation from
`handlers::compute_diagnostics` to `handlers::compute_diagnostics_with_suggestions`,
which attaches up to `N` ranked candidates (same ranking `fix` uses to pick
its one unambiguous answer, exposed in full) to each `broken-link`/
`broken-anchor` diagnostic's `data` field. `lint --fix` runs
`cli::fix::plan_fixes`/`apply` over the whole target root before computing
the report, then rebuilds the index so the diagnostics shown reflect the
post-fix state — the one case where `lint` mutates files on disk; `--json`
output gains a `fixes_applied` field listing what was applied.

---

### Edit Applicator

`src/edit.rs`. The headless, in-process counterpart to what an LSP client
does upon `workspace/applyEdit`: given an already-computed `WorkspaceEdit`,
mutate real files on disk to match it.

**Contract:**

```
apply(edit: WorkspaceEdit) → usize  // files touched; errors propagate, no silent skips
```

Executes, in order:

1. `edit.changes` — plain per-file text edits (`HashMap<Uri, TextEdit[]>`,
   unordered across files by construction; order within one file is derived
   from each edit's range, not declaration order)
2. `edit.document_changes` — an ordered `Vec`, executed in list order, mixing
   `Edit` (a text edit, same application as above) and `Op` (a
   `ResourceOp::Create`/`Rename`/`Delete` filesystem operation)

Handlers already emit `document_changes` sequences that mix edits and
resource ops — the "create missing file" code action
(`handlers::handle_code_actions`) emits `[Op(Create), Edit(...)]` today, for
a real LSP client to execute. `apply_edit::apply` is the same execution
semantics, just run by knap itself instead of an editor, for commands where
no editor is in the loop.

Used only by headless CLI commands that mutate the workspace (`rename-file`,
`rename-heading`, `rename-tag` — v0.12; `fix` — v0.13). Never called by
`handlers.rs`, and never called by `knap lsp` — when a real editor is
connected, the editor applies its own edits, and this module doesn't run.

---

### Request Handlers

One handler per LSP capability. Each handler is a pure function of the form:

```
handle(params: LspParams, index: NoteIndex, config: Config) → LspResult
```

Handlers are stateless — all state lives in the Note Index; config is passed in
by the Protocol Handler.

| Handler          | LSP Method                        | Shipped |
| ---------------- | --------------------------------- | ------- |
| Completion       | `textDocument/completion`         | v0.1    |
| Definition       | `textDocument/definition`         | v0.1    |
| References       | `textDocument/references`         | v0.1    |
| Diagnostics      | `textDocument/publishDiagnostics` | v0.1    |
| WillRenameFiles  | `workspace/willRenameFiles`       | v0.2    |
| DocumentSymbols  | `textDocument/documentSymbol`     | v0.3    |
| WorkspaceSymbols | `workspace/symbol`                | v0.3    |
| PrepareRename    | `textDocument/prepareRename`      | v0.3    |
| Rename           | `textDocument/rename`             | v0.3    |
| CodeAction       | `textDocument/codeAction`         | v0.4    |
| CodeLens         | `textDocument/codeLens`           | v0.6    |
| FoldingRanges    | `textDocument/foldingRange`       | v0.9    |
| SelectionRange   | `textDocument/selectionRange`     | v0.9    |
| InlayHints       | `textDocument/inlayHint`          | v0.9    |

---

## Key Data Flows

### Startup

1. Client sends `initialize` → Protocol Handler resolves `workspaceFolders` and
   `initializationOptions` into a `Config` struct
2. Server responds to `initialize` with capability list
3. Client sends `initialized` → Protocol Handler registers file watchers with
   the client via `workspace/didRegisterCapability`
4. Note Index crawls all files under the configured roots, calls Parser on each,
   builds initial index
5. Server sends initial diagnostics for any broken links found

### User opens a Markdown link

1. Client sends `textDocument/completion` (triggered inside `[text](` path)
2. Completion Handler queries `index.all_notes()` for paths and frontmatter titles
3. Returns completion list; no filesystem I/O

### External file change (e.g. git checkout)

1. Client detects change via its own filesystem watcher, sends
   `workspace/didChangeWatchedFiles`
2. Protocol Handler forwards to Note Index
3. Note Index re-parses affected file(s), updates index
4. Diagnostics Handler re-evaluates and publishes updated diagnostics

---

## Boundaries and Invariants

- **Handlers compute `WorkspaceEdit`s; they never apply them.** All data
  access goes through the Note Index, and handlers never write to disk. Only
  two things ever realize an edit by writing files: the real editor, over
  LSP, when a session is live; and the Edit Applicator (`edit::apply`),
  in-process, for headless CLI commands that mutate the workspace when no
  editor is connected.
- **The Parser is stateless.** It has no knowledge of the rest of the workspace
  — link resolution is the Index's job.
- **The Transport Layer is LSP-agnostic.** It could serve any JSON-RPC protocol.
- **The client owns file change deduplication.** Open files are updated via
  `textDocument/didChange`; external changes arrive via
  `workspace/didChangeWatchedFiles`. The server never receives both for the same
  change.
