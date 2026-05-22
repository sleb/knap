# Knap — Architecture

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
│                   LSP Client (Editor)                │
└──────────────────────────────────────────────────────┘
                  │ JSON-RPC over stdio / TCP
┌──────────────────────────────────────────────────────┐
│                   Transport Layer                    │
└──────────────────────────────────────────────────────┘
                           │
┌──────────────────────────────────────────────────────┐
│                   Protocol Handler                   │
│      lifecycle · capability negotiation · routing    │
└──────────────────────────────────────────────────────┘
                  │                    │
         ┌────────┴────────┐  ┌────────┴────────┐
         │    Request      │  │   Note Index    │
         │    Handlers     │◄─┤                 │
         └─────────────────┘  └────────┬────────┘
                                       │
                              ┌────────┴───────┐
                              │    Markdown    │
                              │    Parser      │
                              └────────────────┘
```

---

## Configuration

Configuration is delivered via standard LSP mechanisms — no custom config
component is needed.

- **At startup:** the client passes user settings as `initializationOptions`
  inside the `initialize` request. This is how all major editors expose
  per-server config (VS Code `settings.json`, Neovim `lspconfig`, Helix
  `languages.toml`).

The Protocol Handler resolves `initializationOptions` into a plain `Config`
struct at startup. Configuration is fixed for the lifetime of the session —
`workspace/didChangeConfiguration` is not processed.

```
Config {
  index_roots: PathBuf[]       // workspace folders from the initialize request
  extensions: string[]         // default: ["md"]
  new_note_dir: Option<string> // inbox folder for Quick Fix "Create note"; relative to index_roots[0]
}
```

`workspaceFolders` comes from the `initialize` request itself and is not
user-configurable — it is whatever the editor has open.

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
  optional heading anchor _(v0.1 for wiki-links; superseded by standard links)_
- Extracting all headings with their level and text _(v0.3)_
- Extracting YAML frontmatter (title, tags, arbitrary keys) _(v0.1, extended v0.3)_

**Contract:**

```
parse(path: string, content: string) → Note
```

`Note` grows across releases. See the per-release design docs for the current
shape. The parser does not resolve links — it only records what is written in
the file.

The parser does not resolve links — it only records what is written in the file.

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
index(note: Note) → IndexDelta    // add or replace; returns affected paths for diagnostics
remove(path: string) → IndexDelta // delete; returns affected paths for diagnostics
```

**Contract (reads):**

```
resolve(source: Path, target: string) → ResolvedLink  // resolves target relative to source file
get_note(path: string) → Note | null
all_notes() → Note[]
links_to(path: string) → LocatedLink[]  // standard links from other notes pointing here
all_tags() → string[]
notes_by_tag(tag: string) → Note[]
add_attachment(path: PathBuf) → IndexDelta
remove_attachment(path: PathBuf) → IndexDelta
```

The index is the single source of truth. Request Handlers read from it
exclusively — they do not touch the filesystem directly.

---

### Debug CLI

A thin `src/cli.rs` module dispatched from `main.rs` when the first argument is
a known subcommand. Used during development to inspect component output without
a running editor. When no subcommand is given, `main.rs` falls through to normal
LSP server startup.

| Subcommand | Usage               | Available from |
| ---------- | ------------------- | -------------- |
| `parse`    | `knap parse <file>` | v0.1           |
| `index`    | `knap index <dir>`  | v0.1           |
| `check`    | `knap check`        | v0.2           |

The CLI shares the same library crate as the server — `cmd_parse` calls
`parser::parse` directly; `cmd_index` calls `index::build` directly; `cmd_check`
spins up a full in-process server and exercises the LSP lifecycle as a smoke
test. No editor is needed.

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

- **Handlers never touch the filesystem.** All data access goes through the Note
  Index.
- **The Parser is stateless.** It has no knowledge of the rest of the workspace
  — link resolution is the Index's job.
- **The Transport Layer is LSP-agnostic.** It could serve any JSON-RPC protocol.
- **The client owns file change deduplication.** Open files are updated via
  `textDocument/didChange`; external changes arrive via
  `workspace/didChangeWatchedFiles`. The server never receives both for the same
  change.
