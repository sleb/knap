use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::{CompletionItem, CompletionItemKind, DiagnosticSeverity, PublishDiagnosticsParams};
use serde_json::json;

fn spawn_server() -> Connection {
    let (server_conn, client_conn) = Connection::memory();
    thread::spawn(move || {
        knap::server::run(server_conn).expect("server error");
    });
    client_conn
}

fn do_initialize(client: &Connection, schema: serde_json::Value) {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(1i32),
            method: "initialize".to_string(),
            params: json!({
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "0.0.1" },
                "initializationOptions": { "frontmatterSchema": schema }
            }),
        }))
        .unwrap();

    recv_response(client, lsp_server::RequestId::from(1i32));

    client
        .sender
        .send(Message::Notification(Notification {
            method: "initialized".to_string(),
            params: json!({}),
        }))
        .unwrap();

    loop {
        match client.receiver.recv().unwrap() {
            Message::Request(req) if req.method == "client/registerCapability" => break,
            _ => {}
        }
    }
}

fn do_shutdown(client: &Connection, request_id: i32) {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(request_id),
            method: "shutdown".to_string(),
            params: json!(null),
        }))
        .unwrap();
    recv_response(client, lsp_server::RequestId::from(request_id));
    let _ = client.sender.send(Message::Notification(Notification {
        method: "exit".to_string(),
        params: json!(null),
    }));
}

fn recv_response(client: &Connection, expected_id: lsp_server::RequestId) -> lsp_server::Response {
    loop {
        match client.receiver.recv().expect("channel closed unexpectedly") {
            Message::Response(r) if r.id == expected_id => return r,
            Message::Response(r) => panic!("unexpected response id {:?}", r.id),
            Message::Request(_) | Message::Notification(_) => {}
        }
    }
}

fn did_open(client: &Connection, uri: &str, content: &str) {
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "markdown",
                    "version": 1,
                    "text": content
                }
            }),
        }))
        .unwrap();
}

fn request_completions(
    client: &Connection,
    request_id: i32,
    uri: &str,
    line: u32,
    character: u32,
) -> Vec<CompletionItem> {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(request_id),
            method: "textDocument/completion".to_string(),
            params: json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        }))
        .unwrap();
    let resp = recv_response(client, lsp_server::RequestId::from(request_id));
    assert!(resp.error.is_none(), "completion returned error: {:?}", resp.error);
    serde_json::from_value(resp.result.unwrap_or(json!([]))).unwrap()
}

/// Sync helper: sends a dummy completion request and collects all
/// `textDocument/publishDiagnostics` notifications that arrive before its
/// response. Since the server processes messages in order, any diagnostics
/// triggered by earlier notifications are already in the channel.
fn collect_diagnostics(client: &Connection, sync_id: i32) -> Vec<PublishDiagnosticsParams> {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(sync_id),
            method: "textDocument/completion".to_string(),
            params: json!({
                "textDocument": { "uri": "file:///sync" },
                "position": { "line": 0, "character": 0 }
            }),
        }))
        .unwrap();

    let mut all_diags = vec![];
    loop {
        match client.receiver.recv().unwrap() {
            Message::Response(r) if r.id == lsp_server::RequestId::from(sync_id) => break,
            Message::Notification(n) if n.method == "textDocument/publishDiagnostics" => {
                if let Ok(p) = serde_json::from_value::<PublishDiagnosticsParams>(n.params) {
                    all_diags.push(p);
                }
            }
            _ => {}
        }
    }
    all_diags
}

/// Cursor after `status: ` in frontmatter → enum value completions over LSP.
#[test]
fn schema_value_completion_round_trip() {
    let client = spawn_server();
    do_initialize(&client, json!({
        "properties": {
            "status": { "enum": ["draft", "review", "published"] }
        }
    }));

    // line 0: ---   line 1: status:    line 2: ---
    did_open(&client, "file:///vault/note.md", "---\nstatus: \n---\n");

    // cursor at line 1, col 8 — after "status: "
    let items = request_completions(&client, 2, "file:///vault/note.md", 1, 8);
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"draft"), "expected 'draft': {labels:?}");
    assert!(labels.contains(&"review"), "expected 'review': {labels:?}");
    assert!(labels.contains(&"published"), "expected 'published': {labels:?}");
    assert!(
        items.iter().all(|i| i.kind == Some(CompletionItemKind::VALUE)),
        "all items should be VALUE kind"
    );

    do_shutdown(&client, 3);
}

/// Blank line inside frontmatter → schema key completions over LSP.
#[test]
fn schema_key_completion_round_trip() {
    let client = spawn_server();
    do_initialize(&client, json!({
        "properties": {
            "status": { "enum": ["draft"] },
            "author": {}
        }
    }));

    // line 0: ---   line 1: (blank)   line 2: ---
    did_open(&client, "file:///vault/note.md", "---\n\n---\n");

    let items = request_completions(&client, 2, "file:///vault/note.md", 1, 0);
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"status"), "expected 'status': {labels:?}");
    assert!(labels.contains(&"author"), "expected 'author': {labels:?}");
    assert!(
        items.iter().all(|i| i.kind == Some(CompletionItemKind::PROPERTY)),
        "all items should be PROPERTY kind"
    );

    do_shutdown(&client, 3);
}

/// Invalid enum value → Warning diagnostic published after didOpen.
#[test]
fn schema_diag_invalid_value_round_trip() {
    let client = spawn_server();
    do_initialize(&client, json!({
        "properties": {
            "status": { "enum": ["draft", "published"] }
        }
    }));

    did_open(&client, "file:///vault/note.md", "---\nstatus: oops\n---\n");

    let diags = collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .find(|d| d.uri.as_str().ends_with("note.md"))
        .expect("no diagnostics for note.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert!(
        file_diags.diagnostics[0].message.contains("Invalid value"),
        "{}",
        file_diags.diagnostics[0].message
    );

    do_shutdown(&client, 3);
}

/// Missing required key → Warning diagnostic published after didOpen.
#[test]
fn schema_diag_missing_required_round_trip() {
    let client = spawn_server();
    do_initialize(&client, json!({
        "properties": {
            "status": { "enum": ["draft"] },
            "author": {}
        },
        "required": ["status"]
    }));

    // Note has `author` but no `status`.
    did_open(&client, "file:///vault/note.md", "---\nauthor: alice\n---\n");

    let diags = collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .find(|d| d.uri.as_str().ends_with("note.md"))
        .expect("no diagnostics for note.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert!(
        file_diags.diagnostics[0].message.contains("Missing required"),
        "{}",
        file_diags.diagnostics[0].message
    );

    do_shutdown(&client, 3);
}
