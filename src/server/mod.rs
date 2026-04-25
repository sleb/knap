// LSP lifecycle, message loop, request/notification routing.
// See docs/design/components/protocol-handler.md

use std::path::PathBuf;

use anyhow::Result;
use log::{debug, info, warn};
use crossbeam_channel::Sender;
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CodeActionOptions, CodeActionParams, CodeActionProviderCapability,
    CompletionOptions, CompletionParams, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidOpenTextDocumentParams, DidChangeWatchedFilesRegistrationOptions,
    DocumentSymbolParams, FileChangeType, FileOperationFilter, FileOperationPattern,
    FileOperationRegistrationOptions, FileSystemWatcher, GlobPattern, GotoDefinitionParams,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, OneOf,
    ReferenceParams, Registration, RegistrationParams, RelativePattern, RenameFilesParams,
    RenameOptions, RenameParams, ServerCapabilities, ServerInfo, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
    WorkspaceFileOperationsServerCapabilities, WorkspaceServerCapabilities, WorkspaceSymbolParams,
};

use crate::handlers::{self, uri_to_path};
use crate::index::{self, NoteIndex};
use crate::parser;

struct Config {
    index_roots: Vec<PathBuf>,
    extensions: Vec<String>,
    attachments_dir: Option<PathBuf>,
    new_note_dir: Option<PathBuf>,
}

/// Mirrors the shape of `initializationOptions` sent by the editor.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct InitOptions {
    extensions: Option<Vec<String>>,
    attachments_dir: Option<String>,
    new_note_dir: Option<String>,
}

impl Config {
    fn from_params(params: &InitializeParams) -> Self {
        let index_roots = params
            .workspace_folders
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter_map(|folder| {
                url::Url::parse(folder.uri.as_str())
                    .ok()?
                    .to_file_path()
                    .ok()
            })
            .collect();

        let opts: InitOptions = params
            .initialization_options
            .as_ref()
            .map(|v| {
                serde_json::from_value::<InitOptions>(v.clone()).unwrap_or_else(|e| {
                    warn!("initializationOptions parse error: {e}; using defaults");
                    InitOptions::default()
                })
            })
            .unwrap_or_default();

        Config {
            index_roots,
            extensions: opts.extensions.unwrap_or_else(|| vec!["md".to_string()]),
            attachments_dir: opts.attachments_dir.map(PathBuf::from),
            new_note_dir: opts.new_note_dir.map(PathBuf::from),
        }
    }
}

#[cfg(test)]
mod tests;


pub fn run(connection: Connection) -> Result<()> {
    info!(
        "knap {} starting ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::current_exe()
            .ok()
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("unknown path")
    );

    let (init_id, init_params_raw) = connection.initialize_start()?;
    let init_params: InitializeParams = serde_json::from_value(init_params_raw)?;

    let client_name = init_params
        .client_info
        .as_ref()
        .map(|c| c.name.as_str())
        .unwrap_or("unknown");
    let client_version = init_params
        .client_info
        .as_ref()
        .and_then(|c| c.version.as_deref())
        .unwrap_or("unknown");
    info!("initialize: client={} version={}", client_name, client_version);

    let config = Config::from_params(&init_params);

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::FULL,
        )),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["[".to_string()]),
            ..Default::default()
        }),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![lsp_types::CodeActionKind::QUICKFIX]),
            resolve_provider: Some(false),
            ..Default::default()
        })),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        workspace: Some(WorkspaceServerCapabilities {
            file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                will_rename: Some(FileOperationRegistrationOptions {
                    filters: vec![FileOperationFilter {
                        scheme: Some("file".to_string()),
                        pattern: FileOperationPattern {
                            glob: "**".to_string(),
                            ..Default::default()
                        },
                    }],
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let init_result = InitializeResult {
        capabilities,
        server_info: Some(ServerInfo {
            name: "knap".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    };

    connection.initialize_finish(init_id, serde_json::to_value(init_result)?)?;

    // `initialized` has been consumed by initialize_finish at this point.
    info!(
        "initialized: registering file watcher, crawling {} roots",
        config.index_roots.len()
    );
    let mut next_request_id: i32 = 1;
    register_file_watcher(&connection, &config, &mut next_request_id)?;

    let exts: Vec<&str> = config.extensions.iter().map(|s| s.as_str()).collect();
    let (mut index, initial_delta) = index::build(&config.index_roots, &exts);
    info!("index ready: {} notes", index.all_notes().count());
    handlers::publish_diagnostics(&initial_delta.affected_paths, &index, &connection.sender);

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                debug!("request: method={} id={:?}", req.method, req.id);
                if connection.handle_shutdown(&req)? {
                    info!("shutdown requested");
                    break;
                }
                dispatch_request(req, &connection, &index, &config)?;
            }
            Message::Notification(notif) => {
                debug!("notification: method={}", notif.method);
                dispatch_notification(notif, &mut index, &connection.sender, &config.extensions);
            }
            Message::Response(_) => {
                // Responses to our outbound requests (e.g. registerCapability) — ignored.
            }
        }
    }

    info!("exiting");
    Ok(())
}

fn register_file_watcher(
    connection: &Connection,
    config: &Config,
    next_id: &mut i32,
) -> Result<()> {
    let id = *next_id;
    *next_id += 1;

    let mut watchers: Vec<FileSystemWatcher> = config
        .extensions
        .iter()
        .map(|ext| {
            debug!("registering file watcher for extension: {ext}");
            FileSystemWatcher {
                glob_pattern: GlobPattern::String(format!("**/*.{ext}")),
                kind: None,
            }
        })
        .collect();

    // If an attachments directory is configured, watch all files inside it.
    if let Some(ref attachments_dir) = config.attachments_dir {
        for root in &config.index_roots {
            let base_url = url::Url::from_file_path(root.join(attachments_dir))
                .expect("attachment dir path must be convertible to a URL");
            let base_uri: Uri = base_url
                .as_str()
                .parse()
                .expect("attachment dir URL must be a valid URI");
            debug!("registering attachment watcher for: {base_url}");
            watchers.push(FileSystemWatcher {
                glob_pattern: GlobPattern::Relative(RelativePattern {
                    base_uri: OneOf::Right(base_uri),
                    pattern: "**/*".to_string(),
                }),
                kind: None,
            });
        }
    }

    let watcher_registration = Registration {
        id: "file-watcher".to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(serde_json::to_value(
            DidChangeWatchedFilesRegistrationOptions { watchers },
        )?),
    };

    let rename_registration = Registration {
        id: "will-rename-files".to_string(),
        method: "workspace/willRenameFiles".to_string(),
        register_options: Some(serde_json::to_value(FileOperationRegistrationOptions {
            filters: vec![FileOperationFilter {
                scheme: Some("file".to_string()),
                pattern: FileOperationPattern {
                    glob: "**".to_string(),
                    ..Default::default()
                },
            }],
        })?),
    };

    connection.sender.send(Message::Request(Request {
        id: lsp_server::RequestId::from(id),
        method: "client/registerCapability".to_string(),
        params: serde_json::to_value(RegistrationParams {
            registrations: vec![watcher_registration, rename_registration],
        })?,
    }))?;

    Ok(())
}

fn dispatch_request(req: Request, connection: &Connection, index: &NoteIndex, config: &Config) -> Result<()> {
    match req.method.as_str() {
        "textDocument/codeAction" => {
            let actions = match serde_json::from_value::<CodeActionParams>(req.params) {
                Ok(params) => {
                    let new_note_dir = config.new_note_dir.as_ref().and_then(|rel| {
                        let doc_path = handlers::uri_to_path(&params.text_document.uri)?;
                        let root = config.index_roots.iter().find(|r| doc_path.starts_with(r))?;
                        Some(root.join(rel))
                    });
                    handlers::handle_code_action(params, index, new_note_dir.as_deref())
                }
                Err(e) => {
                    warn!("codeAction: bad params: {e}");
                    vec![]
                }
            };
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, actions)))?;
        }
        "textDocument/completion" => {
            let items = serde_json::from_value::<CompletionParams>(req.params)
                .ok()
                .map(|params| handlers::handle_completion(params, index));
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, items)))?;
        }
        "textDocument/hover" => {
            let hover = serde_json::from_value::<HoverParams>(req.params)
                .ok()
                .and_then(|params| handlers::handle_hover(params, index));
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, hover)))?;
        }
        "textDocument/definition" => {
            let response = serde_json::from_value::<GotoDefinitionParams>(req.params)
                .ok()
                .and_then(|params| handlers::handle_definition(params, index));
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, response)))?;
        }
        "textDocument/references" => {
            let locations = serde_json::from_value::<ReferenceParams>(req.params)
                .ok()
                .map(|params| handlers::handle_references(params, index));
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, locations)))?;
        }
        "textDocument/prepareRename" => {
            let response = serde_json::from_value::<TextDocumentPositionParams>(req.params)
                .ok()
                .and_then(|params| handlers::handle_prepare_rename(params, index));
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, response)))?;
        }
        "textDocument/rename" => {
            let edit = serde_json::from_value::<RenameParams>(req.params)
                .ok()
                .and_then(|params| handlers::handle_rename(params, index));
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, edit)))?;
        }
        "workspace/symbol" => {
            let symbols = serde_json::from_value::<WorkspaceSymbolParams>(req.params)
                .map(|params| handlers::handle_workspace_symbols(params, index))
                .unwrap_or_default();
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, symbols)))?;
        }
        "textDocument/documentSymbol" => {
            let response = serde_json::from_value::<DocumentSymbolParams>(req.params)
                .map(|params| handlers::handle_document_symbols(params, index))
                .unwrap_or(lsp_types::DocumentSymbolResponse::Nested(vec![]));
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, response)))?;
        }
        "workspace/willRenameFiles" => {
            let edit = serde_json::from_value::<RenameFilesParams>(req.params)
                .map(|params| handlers::handle_will_rename_files(params, index))
                .unwrap_or_default();
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, edit)))?;
        }
        _ => {
            // Unknown methods return null (not an error) per LSP spec.
            connection
                .sender
                .send(Message::Response(Response::new_ok(req.id, ())))?;
        }
    }
    Ok(())
}

fn dispatch_notification(
    notif: Notification,
    index: &mut NoteIndex,
    sender: &Sender<Message>,
    extensions: &[String],
) {
    match notif.method.as_str() {
        "textDocument/didOpen" => on_did_open(notif, index, sender),
        "textDocument/didChange" => on_did_change(notif, index, sender),
        "textDocument/didClose" => {} // no-op: on-disk version already indexed
        "workspace/didChangeWatchedFiles" => {
            on_did_change_watched_files(notif, index, sender, extensions)
        }
        _ => {}
    }
}

fn on_did_open(notif: Notification, index: &mut NoteIndex, sender: &Sender<Message>) {
    let params: DidOpenTextDocumentParams = match serde_json::from_value(notif.params) {
        Ok(p) => p,
        Err(e) => {
            warn!("didOpen: bad params: {e}");
            return;
        }
    };
    let Some(path) = uri_to_path(&params.text_document.uri) else { return; };
    let note = parser::parse(&path, &params.text_document.text);
    let delta = index.index(note);
    handlers::publish_diagnostics(&delta.affected_paths, index, sender);
}

fn on_did_change(notif: Notification, index: &mut NoteIndex, sender: &Sender<Message>) {
    let params: DidChangeTextDocumentParams = match serde_json::from_value(notif.params) {
        Ok(p) => p,
        Err(e) => {
            warn!("didChange: bad params: {e}");
            return;
        }
    };
    let content = match params.content_changes.into_iter().next() {
        Some(c) => c.text,
        None => {
            warn!("didChange: no content changes");
            return;
        }
    };
    let Some(path) = uri_to_path(&params.text_document.uri) else { return; };
    let note = parser::parse(&path, &content);
    let delta = index.index(note);
    handlers::publish_diagnostics(&delta.affected_paths, index, sender);
}

fn on_did_change_watched_files(
    notif: Notification,
    index: &mut NoteIndex,
    sender: &Sender<Message>,
    extensions: &[String],
) {
    let params: DidChangeWatchedFilesParams = match serde_json::from_value(notif.params) {
        Ok(p) => p,
        Err(e) => {
            warn!("didChangeWatchedFiles: bad params: {e}");
            return;
        }
    };
    for event in params.changes {
        let Some(path) = uri_to_path(&event.uri) else { continue; };
        let is_note = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| extensions.iter().any(|e| e == ext))
            .unwrap_or(false);

        if is_note {
            if event.typ == FileChangeType::CREATED || event.typ == FileChangeType::CHANGED {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        let delta = index.index(parser::parse(&path, &content));
                        handlers::publish_diagnostics(&delta.affected_paths, index, sender);
                    }
                    Err(e) => {
                        warn!("cannot read {}: {e}", path.display());
                    }
                }
            } else if event.typ == FileChangeType::DELETED {
                let delta = index.remove(&path);
                handlers::publish_diagnostics(&delta.affected_paths, index, sender);
            }
        } else {
            // Non-note file (attachment): update by_filename only.
            if event.typ == FileChangeType::CREATED {
                let delta = index.add_attachment(path);
                handlers::publish_diagnostics(&delta.affected_paths, index, sender);
            } else if event.typ == FileChangeType::DELETED {
                let delta = index.remove_attachment(&path);
                handlers::publish_diagnostics(&delta.affected_paths, index, sender);
            }
            // Changed → no-op for attachments
        }
    }
}
