# Protocol Handler

Owns the LSP session lifecycle, resolves configuration, and routes every inbound message to the right handler or index operation.

---

## Dependencies

```toml
lsp-server = "0.10"
lsp-types  = "0.97"
serde_json = "1"
anyhow     = "1"
```

---

## Server state

The handler enforces a simple lifecycle. Requests received in the wrong state return a JSON-RPC error.

```
Uninitialized ──► Running ──► ShuttingDown
```

- `Uninitialized`: only `initialize` is accepted
- `Running`: all requests and notifications are accepted
- `ShuttingDown`: only `exit` is accepted; all other requests return `InvalidRequest`

---

## Config

Resolved once from `initialize`. Configuration is fixed for the session —
`workspace/didChangeConfiguration` is not processed.

```rust
struct Config {
    /// Workspace folders from the initialize request.
    index_roots: Vec<PathBuf>,
    /// File extensions treated as notes. Default: ["md"]. Raw form, kept
    /// for tests — path_filter is the actual authority.
    extensions: Vec<String>,
    /// Inbox folder for Quick Fix "Create note"; relative to index_roots[0].
    new_note_dir: Option<String>,
    /// Frontmatter key/value constraints. Default: empty (no validation).
    frontmatter_schema: FrontmatterSchema,
    /// Glob patterns left out of indexing entirely. Default: []. Raw form,
    /// kept for tests — path_filter is the actual authority.
    exclude: Vec<String>,
    /// Compiled exclude/index authority, built once by `finalize` from
    /// `exclude` and `extensions`. See `should_index`/`is_note` below.
    path_filter: PathFilter,
}
```

`Config` is built by `config::for_lsp`, the same loader shared with the
`knap lint`/`knap index` CLI commands (see `docs/ARCHITECTURE.md` §
Configuration for the full module). `index_roots` is set directly from
`params.workspace_folders` at init time. `extensions` and the other fields
come from `initializationOptions`, layered over an optional `knap.toml` at
`index_roots[0]` (the editor value wins where present). If
`initializationOptions` cannot be deserialized (e.g. a typo in the editor's
LSP config), a `warn!()` is logged and that field defaults — the server
still starts rather than rejecting the session. A malformed `knap.toml`, by
contrast, fails `initialize` outright, since it's a file the user wrote
themselves.

---

## Initialisation sequence

### `initialize` request

1. Extract `InitializeParams` from the request
2. Compute `Config` from `params.workspace_folders` and `params.initialization_options`
3. Respond with `InitializeResult` advertising capabilities:

```rust
ServerCapabilities {
    text_document_sync: Some(TextDocumentSyncCapability::Kind(
        TextDocumentSyncKind::FULL,
    )),
    completion_provider: Some(CompletionOptions {
        trigger_characters: Some(vec!["(".to_string(), "#".to_string(), "/".to_string(), "-".to_string()]),
        ..Default::default()
    }),
    code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
    code_lens_provider: Some(CodeLensOptions { resolve_provider: Some(false) }),
    definition_provider: Some(OneOf::Left(true)),
    references_provider: Some(OneOf::Left(true)),
    document_symbol_provider: Some(OneOf::Left(true)),
    workspace_symbol_provider: Some(OneOf::Left(true)),
    rename_provider: Some(OneOf::Right(RenameOptions {
        prepare_provider: Some(true),
        work_done_progress_options: Default::default(),
    })),
    folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
    selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
    inlay_hint_provider: Some(OneOf::Left(true)),
    workspace: Some(WorkspaceServerCapabilities {
        file_operations: Some(WorkspaceFileOperationsServerCapabilities {
            will_rename: Some(FileOperationRegistrationOptions {
                filters: vec![FileOperationFilter {
                    scheme: Some("file".to_string()),
                    pattern: FileOperationPattern {
                        glob: "**/*".to_string(),
                        ..Default::default()
                    },
                }],
            }),
            ..Default::default()
        }),
        ..Default::default()
    }),
    ..Default::default()
}
```

`FULL` sync means the client sends the complete document content on every change.

### `initialized` notification

1. Register the file watcher with the client:

```rust
let registration = Registration {
    id: "file-watcher".to_string(),
    method: DidChangeWatchedFiles::METHOD.to_string(),
    register_options: Some(serde_json::to_value(
        DidChangeWatchedFilesRegistrationOptions {
            watchers: config.extensions.iter().map(|ext| FileSystemWatcher {
                glob_pattern: GlobPattern::String(format!("**/*.{ext}")),
                kind: None, // all events: create, change, delete
            }).collect(),
        }
    )?),
};
connection.sender.send(Message::Request(Request::new(
    next_request_id(),
    RegisterCapability::METHOD.to_string(),
    RegistrationParams { registrations: vec![registration] },
)))?;
```

2. Crawl all files in `config.index_roots`, parse each, populate the `NoteIndex`
3. Publish initial diagnostics for any broken links found during the crawl

---

## Main loop

```rust
for msg in &connection.receiver {
    match msg {
        Message::Request(req) => {
            if connection.handle_shutdown(&req)? {
                break;
            }
            dispatch_request(req, &connection, &index, &config);
        }
        Message::Notification(notif) => {
            dispatch_notification(notif, &connection, &mut index, &config);
        }
        Message::Response(_) => {
            // responses to our own outbound requests (e.g. register capability)
            // ignored in v0.1
        }
    }
}
```

`connection.handle_shutdown` responds to `shutdown` and returns `true` on `exit`, breaking the loop.

---

## Request routing

```rust
fn dispatch_request(req: Request, ...) {
    match req.method.as_str() {
        Completion::METHOD              => handle_completion(req, ...),
        GotoDefinition::METHOD          => handle_definition(req, ...),
        References::METHOD              => handle_references(req, ...),
        "workspace/willRenameFiles"     => handle_will_rename_files(req, ...),
        "textDocument/documentSymbol"   => handle_document_symbols(req, ...),
        "workspace/symbol"              => handle_workspace_symbols(req, ...),
        "textDocument/prepareRename"    => handle_prepare_rename(req, ...),
        "textDocument/rename"           => handle_rename(req, ...),
        "textDocument/codeAction"       => handle_code_actions(req, ...),
        "textDocument/codeLens"         => handle_code_lens(req, ...),
        "textDocument/foldingRange"     => handle_folding_ranges(req, ...),
        "textDocument/selectionRange"   => handle_selection_range(req, ...),
        "textDocument/inlayHint"        => handle_inlay_hints(req, ...),
        _                               => respond_with_null(req, ...),
    }
}
```

Unknown methods return a null result (not an error) — this is the correct LSP behaviour for unimplemented optional capabilities.

## Notification routing

```rust
fn dispatch_notification(notif: Notification, ...) {
    match notif.method.as_str() {
        DidOpenTextDocument::METHOD         => on_did_open(notif, ...),
        DidChangeTextDocument::METHOD       => on_did_change(notif, ...),
        DidCloseTextDocument::METHOD        => {}  // no-op: on-disk version already indexed
        DidChangeWatchedFiles::METHOD       => on_did_change_watched_files(notif, ...),
        _                                   => {}  // ignore unknown notifications
    }
}
```

---

## Document sync handlers

These handlers feed the Note Index. After each index update they trigger diagnostic republishing for any affected files (see [handlers.md](handlers.md)).

### `textDocument/didOpen`

```
params → path from URI → config.should_index(path)? no → return
       → parse document content → index.index(note)
       → extend affected with register_ancestor_dirs(path, config.index_roots, index)
       → publish_diagnostics(affected)
```

### `textDocument/didChange`

```
params → path from URI → config.should_index(path)? no → return
       → parse full content from params.content_changes[0].text
       → index.index(note)
       → extend affected with register_ancestor_dirs(path, config.index_roots, index)
       → publish_diagnostics(affected)
```

`FULL` sync guarantees `content_changes` has exactly one entry with the full text.

### `textDocument/didClose`

No index update. The on-disk version was already indexed; closing a file doesn't invalidate it.

### `workspace/didChangeWatchedFiles`

```
for each FileEvent in params.changes:
    config.should_index(path)? no → skip this event
    config.is_note(path)?
        note:
            Created | Changed → read file from disk → parse → index.index(note)
                                 → extend affected with register_ancestor_dirs(path, config.index_roots, index)
            Deleted            → index.remove(path)
        attachment:
            Created → index.add_attachment(path)
                       → extend affected with register_ancestor_dirs(path, config.index_roots, index)
            Deleted → index.remove_attachment(path)
            Changed → no-op
→ publish_diagnostics(all affected files)
```

`config.should_index`/`config.is_note` are the same `PathFilter`-backed checks
`index::build`'s crawl uses, so a path excluded from the initial index (via
the hardcoded `.git`/`node_modules`/`target` prune or `knap.toml`'s `exclude`)
stays excluded across the whole live session, not just on startup.

### `register_ancestor_dirs` (v0.17)

Directories created after startup become visible without a restart. Called
after every note/attachment `Created` or `Changed` above, it climbs from
`path`'s parent directory upward, registering each not-yet-known ancestor
via `index.add_dir(..)`. Climbing stops at the first already-known ancestor,
or at the matching configured root — so a single new file never walks
arbitrarily far up the filesystem. If `path` isn't under any configured
root, it returns an empty set without panicking. Its return value (the
union of every affected path from each `add_dir` call) is folded into the
call site's `affected_paths` before `publish_diagnostics` runs, so a
directory link that was broken because its target didn't exist yet clears
in the same round-trip as the file that created it.
