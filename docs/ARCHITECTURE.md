# Knap — Architecture

High-level component design. Each component is described by its responsibility and the contracts it exposes or depends on. Per-feature implementation details live in release-level design docs.

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

Configuration is delivered via standard LSP mechanisms — no custom config component is needed.

- **At startup:** the client passes user settings as `initializationOptions` inside the `initialize` request. This is how all major editors expose per-server config (VS Code `settings.json`, Neovim `lspconfig`, Helix `languages.toml`).
- **At runtime:** the client sends `workspace/didChangeConfiguration` if settings change while the server is running.

The Protocol Handler resolves both into a plain `Config` struct that is passed to components that need it:

```
Config {
  noteRoot?: string       // optional subdirectory to restrict indexing (e.g. "docs/")
                          // when absent, all workspaceFolders are indexed
  extensions: string[]    // default: [".md"]
  linkResolution: "stem" | "path"   // default: "stem"
}
```

`workspaceFolders` comes from the `initialize` request itself and is not user-configurable — it is whatever the editor has open.

---

## File Change Notifications

The server does not run its own filesystem watcher. Instead, it uses the LSP-native `workspace/didChangeWatchedFiles` mechanism:

- At `initialized`, the server registers interest in its configured extensions via `workspace/didRegisterCapability`
- The client delivers `workspace/didChangeWatchedFiles` notifications for external changes (e.g. git checkouts, files edited outside the editor)
- The client does **not** send `workspace/didChangeWatchedFiles` for files currently open in the editor — those are managed exclusively by `textDocument/didChange`

This means deduplication is handled by the client. Both notification types converge on the same Note Index update interface, with no risk of double-indexing an open file.

---

## Components

### Transport Layer

Owns the wire protocol. Reads and writes JSON-RPC 2.0 messages over stdio (default) or TCP.

**Responsibilities:**

- Framing: Content-Length header encoding/decoding
- Serialising and deserialising JSON-RPC request/response/notification envelopes
- Forwarding decoded messages to the Protocol Handler
- Writing encoded responses back to the client

**Does not** know anything about LSP semantics — it only handles bytes and JSON.

---

### Protocol Handler

The server's front door. Owns the LSP session lifecycle and routes every incoming message to the right handler.

**Responsibilities:**

- Managing the `initialize` / `initialized` / `shutdown` / `exit` lifecycle
- Resolving `workspaceFolders` and `initializationOptions` from `initialize` into a `Config` struct
- Updating `Config` on `workspace/didChangeConfiguration` and triggering a re-index when index-affecting fields change
- Registering file watchers with the client via `workspace/didRegisterCapability` at `initialized`
- Advertising server capabilities during `initialize` based on what handlers are registered
- Routing `textDocument/*` and `workspace/*` requests to Request Handlers
- Routing `textDocument/didOpen`, `didChange`, `didClose` and `workspace/didChangeWatchedFiles` notifications to the Note Index
- Sending `textDocument/publishDiagnostics` notifications on behalf of handlers
- Error handling: returning well-formed JSON-RPC error responses

**Contract (inbound):** receives decoded LSP messages  
**Contract (outbound):** calls into Request Handlers and Note Index, passing `Config` as needed

---

### Markdown Parser

Parses a single Markdown file and returns a structured `Note`. Stateless and pure — given the same source text it always returns the same result.

**Responsibilities (full target state — fields added per release):**

- Extracting `[[wiki-links]]` with position, target stem, optional alias, optional heading anchor *(v0.1)*
- Extracting all headings with their level and text *(v0.3)*
- Extracting YAML frontmatter (title, tags, arbitrary keys) *(v0.4)*
- Extracting standard Markdown links and images with position *(v0.4)*

**Contract:**

```
parse(path: string, content: string) → Note
```

`Note` grows across releases. In v0.1 it carries only `path`, `stem`, `wikiLinks`, and `content` (raw source). Fields for headings, frontmatter, and standard links are added in later releases. See the per-release design docs for the current shape.

The parser does not resolve links — it only records what is written in the file.

---

### Note Index

The server's central knowledge base. Maintains a live, queryable model of all notes in the workspace.

**Responsibilities:**

- Building the initial index by parsing all files under the configured roots on startup
- Accepting incremental updates (note added, changed, deleted) from the Protocol Handler
- Resolving `[[link]]` stems to file paths according to the configured `linkResolution` strategy
- Detecting broken links (no matching file) and ambiguous stems (multiple matching files)
- Maintaining a reverse index: for each file, which files link to it (backlinks)

**Contract (writes):**

```
index(note: Note) → IndexDelta    // add or replace; returns affected paths for diagnostics
remove(path: string) → IndexDelta // delete; returns affected paths for diagnostics
```

**Contract (reads):**

```
resolve(stem: string) → ResolvedLink   // stem → path, or broken/ambiguous
getNote(path: string) → Note | null
getAllNotes() → Note[]
getBacklinks(path: string) → WikiLink[]  // links from other notes pointing here
getAllTags() → string[]
getNotesByTag(tag: string) → Note[]
```

The index is the single source of truth. Request Handlers read from it exclusively — they do not touch the filesystem directly.

---

### Request Handlers

One handler per LSP capability. Each handler is a pure function of the form:

```
handle(params: LspParams, index: NoteIndex, config: Config) → LspResult
```

Handlers are stateless — all state lives in the Note Index; config is passed in by the Protocol Handler.

| Handler          | LSP Method                        | Releases               |
| ---------------- | --------------------------------- | ---------------------- |
| Completion       | `textDocument/completion`         | v0.1, v0.2, v0.4, v0.5 |
| Definition       | `textDocument/definition`         | v0.1, v0.3, v0.5       |
| References       | `textDocument/references`         | v0.1, v0.5             |
| Diagnostics      | `textDocument/publishDiagnostics` | v0.1, v0.2, v0.3       |
| Rename           | `workspace/willRenameFiles`       | v0.2                   |
| DocumentSymbols  | `textDocument/documentSymbol`     | v0.3                   |
| WorkspaceSymbols | `workspace/symbol`                | v0.3                   |
| Hover            | `textDocument/hover`              | v0.4                   |
| CodeAction       | `textDocument/codeAction`         | v0.6                   |

---

## Key Data Flows

### Startup

1. Client sends `initialize` → Protocol Handler resolves `workspaceFolders` and `initializationOptions` into a `Config` struct
2. Server responds to `initialize` with capability list
3. Client sends `initialized` → Protocol Handler registers file watchers with the client via `workspace/didRegisterCapability`
4. Note Index crawls all files under the configured roots, calls Parser on each, builds initial index
5. Server sends initial diagnostics for any broken links found

### User types `[[`

1. Client sends `textDocument/completion`
2. Completion Handler queries `index.getAllNotes()` for stems/titles
3. Returns completion list; no filesystem I/O

### File renamed in editor

1. Client sends `workspace/willRenameFiles`
2. Rename Handler queries `index.getBacklinks(oldPath)` to find all linking notes
3. Returns a `WorkspaceEdit` with text edits for each linking note
4. Client applies edits; sends `textDocument/didChange` for affected files
5. Note Index updates affected notes

### External file change (e.g. git checkout)

1. Client detects change via its own filesystem watcher, sends `workspace/didChangeWatchedFiles`
2. Protocol Handler forwards to Note Index
3. Note Index re-parses affected file(s), updates index
4. Diagnostics Handler re-evaluates and publishes updated diagnostics

---

## Boundaries and Invariants

- **Handlers never touch the filesystem.** All data access goes through the Note Index.
- **The Parser is stateless.** It has no knowledge of the rest of the workspace — link resolution is the Index's job.
- **The Transport Layer is LSP-agnostic.** It could serve any JSON-RPC protocol.
- **Config changes trigger a full re-index.** Changes to `noteRoot` or `extensions` (via `workspace/didChangeConfiguration`) invalidate the entire index. Changes to `workspaceFolders` (via `workspace/didChangeWorkspaceFolders`) do the same.
- **The client owns file change deduplication.** Open files are updated via `textDocument/didChange`; external changes arrive via `workspace/didChangeWatchedFiles`. The server never receives both for the same change.
  </thinking>
