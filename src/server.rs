// Step 4: LSP lifecycle, message loop, request/notification routing.
// See docs/design/components/protocol-handler.md

use std::path::PathBuf;

use anyhow::Result;
use log::{debug, info};
use lsp_server::{Connection, Message, Request, Response};
use lsp_types::{
    CompletionOptions, DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern,
    InitializeParams, InitializeResult, OneOf, Registration, RegistrationParams, ServerCapabilities,
    ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
};

struct Config {
    index_roots: Vec<PathBuf>,
    extensions: Vec<String>,
}

impl Config {
    fn from_params(_params: &InitializeParams) -> Self {
        // Workspace folder → PathBuf conversion wired in Step 5.
        Config {
            index_roots: Vec::new(),
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
                // Document sync notifications wired in Step 5.
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
