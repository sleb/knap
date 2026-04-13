// LSP lifecycle, message loop, request/notification routing.
// See docs/design/components/protocol-handler.md

use std::path::PathBuf;

use anyhow::Result;
use log::{debug, info, warn};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CompletionOptions, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidOpenTextDocumentParams, DidChangeWatchedFilesRegistrationOptions,
    FileChangeType, FileSystemWatcher, GlobPattern, InitializeParams, InitializeResult, OneOf,
    Registration, RegistrationParams, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};

use crate::handlers::uri_to_path;
use crate::index::{self, NoteIndex};
use crate::parser;

struct Config {
    index_roots: Vec<PathBuf>,
    extensions: Vec<String>,
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

        Config {
            index_roots,
            extensions: vec!["md".to_string()],
        }
    }
}

pub fn run(connection: Connection) -> Result<()> {
    info!("knap starting");

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
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
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
    let (mut index, _initial_delta) = index::build(&config.index_roots, &exts);
    info!("index ready: {} notes", index.all_notes().count());
    // _initial_delta will be used for initial diagnostics in Step 6.

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                debug!("request: method={} id={:?}", req.method, req.id);
                if connection.handle_shutdown(&req)? {
                    info!("shutdown requested");
                    break;
                }
                dispatch_request(req, &connection)?;
            }
            Message::Notification(notif) => {
                debug!("notification: method={}", notif.method);
                dispatch_notification(notif, &mut index);
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

    let watchers = config
        .extensions
        .iter()
        .map(|ext| FileSystemWatcher {
            glob_pattern: GlobPattern::String(format!("**/*.{ext}")),
            kind: None,
        })
        .collect();

    let registration = Registration {
        id: "file-watcher".to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(serde_json::to_value(
            DidChangeWatchedFilesRegistrationOptions { watchers },
        )?),
    };

    connection.sender.send(Message::Request(Request {
        id: lsp_server::RequestId::from(id),
        method: "client/registerCapability".to_string(),
        params: serde_json::to_value(RegistrationParams {
            registrations: vec![registration],
        })?,
    }))?;

    Ok(())
}

fn dispatch_request(req: Request, connection: &Connection) -> Result<()> {
    // All v0.1 request methods return null until implemented in Steps 7–9.
    // Unknown methods also return null (not an error) per LSP spec.
    connection
        .sender
        .send(Message::Response(Response::new_ok(req.id, ())))?;
    Ok(())
}

fn dispatch_notification(notif: Notification, index: &mut NoteIndex) {
    match notif.method.as_str() {
        "textDocument/didOpen" => on_did_open(notif, index),
        "textDocument/didChange" => on_did_change(notif, index),
        "textDocument/didClose" => {} // no-op: on-disk version already indexed
        "workspace/didChangeWatchedFiles" => on_did_change_watched_files(notif, index),
        _ => {}
    }
}

fn on_did_open(notif: Notification, index: &mut NoteIndex) {
    let params: DidOpenTextDocumentParams = match serde_json::from_value(notif.params) {
        Ok(p) => p,
        Err(e) => {
            warn!("didOpen: bad params: {e}");
            return;
        }
    };
    let path = uri_to_path(&params.text_document.uri);
    let note = parser::parse(&path, &params.text_document.text);
    index.index(note);
}

fn on_did_change(notif: Notification, index: &mut NoteIndex) {
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
    let path = uri_to_path(&params.text_document.uri);
    let note = parser::parse(&path, &content);
    index.index(note);
}

fn on_did_change_watched_files(notif: Notification, index: &mut NoteIndex) {
    let params: DidChangeWatchedFilesParams = match serde_json::from_value(notif.params) {
        Ok(p) => p,
        Err(e) => {
            warn!("didChangeWatchedFiles: bad params: {e}");
            return;
        }
    };
    for event in params.changes {
        let path = uri_to_path(&event.uri);
        if event.typ == FileChangeType::CREATED || event.typ == FileChangeType::CHANGED {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    index.index(parser::parse(&path, &content));
                }
                Err(e) => {
                    warn!("cannot read {}: {e}", path.display());
                }
            }
        } else if event.typ == FileChangeType::DELETED {
            index.remove(&path);
        }
    }
}
