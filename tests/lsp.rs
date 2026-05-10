use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::{
    CompletionResponse, DiagnosticSeverity, GotoDefinitionResponse, Location,
    PublishDiagnosticsParams, TextDocumentSyncCapability, TextDocumentSyncKind,
};
use serde_json::json;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn spawn_server() -> Connection {
    let (server_conn, client_conn) = Connection::memory();
    thread::spawn(move || {
        knap::server::run(server_conn).expect("server error");
    });
    client_conn
}

fn do_initialize(client: &Connection) {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(1i32),
            method: "initialize".to_string(),
            params: json!({
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "0.0.1" }
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

    // Drain the server-initiated client/registerCapability request.
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

    let resp = recv_response(client, lsp_server::RequestId::from(request_id));
    assert!(resp.error.is_none(), "shutdown returned error: {:?}", resp.error);

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

fn did_open(client: &Connection, uri: &str, text: &str) {
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "markdown",
                    "version": 1,
                    "text": text
                }
            }),
        }))
        .unwrap();
}

/// Send a cheap request and collect all `textDocument/publishDiagnostics`
/// notifications that arrive before its response.
fn sync_and_collect_diagnostics(
    client: &Connection,
    sync_id: i32,
) -> Vec<PublishDiagnosticsParams> {
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

fn send_request(client: &Connection, id: i32, method: &str, params: serde_json::Value) {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(id),
            method: method.to_string(),
            params,
        }))
        .unwrap();
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Server advertises v0.1 capabilities: sync=Full, completion trigger `(`,
/// definition, references. Does not advertise v0.2+ capabilities.
#[test]
fn lifecycle_capabilities() {
    let client = spawn_server();

    // Send initialize directly so we can capture the raw InitializeResult.
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(1i32),
            method: "initialize".to_string(),
            params: json!({ "capabilities": {}, "clientInfo": {"name": "test"} }),
        }))
        .unwrap();

    let resp = recv_response(&client, lsp_server::RequestId::from(1i32));
    let result: lsp_types::InitializeResult =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let caps = &result.capabilities;

    assert!(
        matches!(
            caps.text_document_sync,
            Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
        ),
        "expected Full sync"
    );

    let trigger_chars = caps
        .completion_provider
        .as_ref()
        .and_then(|c| c.trigger_characters.as_ref())
        .expect("completion provider should be advertised");
    assert!(trigger_chars.contains(&"(".to_string()), "expected `(` as trigger character");

    assert!(caps.definition_provider.is_some(), "definition provider should be advertised");
    assert!(caps.references_provider.is_some(), "references provider should be advertised");

    // v0.2+ capabilities must NOT be present
    assert!(caps.hover_provider.is_none(), "hover should not be advertised in v0.1");
    assert!(caps.rename_provider.is_none(), "rename should not be advertised in v0.1");
    assert!(caps.code_lens_provider.is_none(), "code lens should not be advertised in v0.1");

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
    do_shutdown(&client, 2);
}

/// Opening a file with a broken Markdown link publishes a WARNING diagnostic.
#[test]
fn broken_link_produces_warning() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "[text](missing.md)\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .find(|d| d.uri.as_str().ends_with("a.md"))
        .expect("no diagnostics published for a.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert!(
        file_diags.diagnostics[0].message.contains("missing.md"),
        "unexpected message: {}",
        file_diags.diagnostics[0].message
    );

    do_shutdown(&client, 3);
}

/// A link that resolves to an indexed note produces no diagnostic.
#[test]
fn valid_link_no_warning() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "# B\n");
    did_open(&client, "file:///vault/a.md", "[link](b.md)\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> =
        diags.iter().filter(|d| d.uri.as_str().ends_with("a.md")).collect();
    let last = file_diags.last().expect("no diagnostics published for a.md");
    assert!(last.diagnostics.is_empty(), "expected no diagnostics, got {:?}", last.diagnostics);

    do_shutdown(&client, 3);
}

/// A link with a valid target but a missing anchor produces a WARNING.
#[test]
fn broken_anchor_produces_warning() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "## Real Heading\n");
    did_open(&client, "file:///vault/a.md", "[link](b.md#Missing)\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> =
        diags.iter().filter(|d| d.uri.as_str().ends_with("a.md")).collect();
    let last = file_diags.last().expect("no diagnostics published for a.md");
    assert_eq!(last.diagnostics.len(), 1);
    assert_eq!(last.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert!(
        last.diagnostics[0].message.contains("Missing"),
        "unexpected message: {}",
        last.diagnostics[0].message
    );

    do_shutdown(&client, 3);
}

/// A link with a valid target and a valid anchor produces no diagnostic.
#[test]
fn valid_anchor_no_warning() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "## Real Heading\n");
    did_open(&client, "file:///vault/a.md", "[link](b.md#Real Heading)\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> =
        diags.iter().filter(|d| d.uri.as_str().ends_with("a.md")).collect();
    let last = file_diags.last().expect("no diagnostics published for a.md");
    assert!(
        last.diagnostics.is_empty(),
        "expected no diagnostics for valid anchor, got {:?}",
        last.diagnostics
    );

    do_shutdown(&client, 3);
}

/// Broken attachment link clears once the file is registered via didChangeWatchedFiles.
/// (Uses a non-.md file so the server calls add_attachment rather than trying to
/// read the file from disk — the path is synthetic and does not exist on disk.)
#[test]
fn diagnostic_clears_when_target_created() {
    let client = spawn_server();
    do_initialize(&client);

    // Note links to an image that doesn't exist yet.
    did_open(&client, "file:///vault/a.md", "[img](logo.png)\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let broken = diags.iter().find(|d| d.uri.as_str().ends_with("a.md")).unwrap();
    assert_eq!(broken.diagnostics.len(), 1, "expected broken-link diagnostic before fix");

    // Simulate the watcher seeing the attachment created (non-.md → add_attachment,
    // no disk read required).
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({ "changes": [{ "uri": "file:///vault/logo.png", "type": 1 }] }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 3);
    let cleared = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .last()
        .expect("no diagnostics for a.md after fix");
    assert!(
        cleared.diagnostics.is_empty(),
        "expected diagnostic to clear after attachment created, got {:?}",
        cleared.diagnostics
    );

    do_shutdown(&client, 4);
}

/// Completion after `](` returns notes with relative-path insert_text values.
#[test]
fn completion_returns_relative_paths() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "");
    // "[link](" → cursor at position 7 (right after `(`)
    did_open(&client, "file:///vault/a.md", "[link](");

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 0, "character": 7 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: Option<CompletionResponse> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    let items = match result.expect("expected completion result") {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };

    assert!(!items.is_empty(), "expected at least one completion item");
    let b_item = items.iter().find(|i| i.insert_text.as_deref() == Some("b.md"));
    assert!(b_item.is_some(), "expected an item with insert_text = \"b.md\"");

    do_shutdown(&client, 3);
}

/// Go-to-definition on `[text](b.md)` navigates to the top of b.md.
#[test]
fn definition_navigates_to_target() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "# B\n");
    // "[link](b.md)" — cursor at (0, 3) is inside the link span
    did_open(&client, "file:///vault/a.md", "[link](b.md)\n");

    send_request(
        &client,
        2,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 0, "character": 3 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: Option<GotoDefinitionResponse> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    let loc = match result.expect("expected definition result") {
        GotoDefinitionResponse::Scalar(loc) => loc,
        GotoDefinitionResponse::Array(locs) if locs.len() == 1 => locs.into_iter().next().unwrap(),
        other => panic!("unexpected response shape: {:?}", other),
    };

    assert!(loc.uri.as_str().ends_with("b.md"), "expected navigation to b.md");
    assert_eq!(loc.range.start.line, 0);

    do_shutdown(&client, 3);
}

/// workspace/willRenameFiles returns edits that rewrite incoming links in other notes.
#[test]
fn test_will_rename_incoming() {
    let client = spawn_server();
    do_initialize(&client);

    // a.md links to b.md; rename b.md → c.md should rewrite a.md's link target.
    // b.md must be indexed first so the reverse link (a.md → b.md) is tracked.
    did_open(&client, "file:///vault/b.md", "# B\n");
    did_open(&client, "file:///vault/a.md", "[link](b.md)\n");

    send_request(
        &client,
        2,
        "workspace/willRenameFiles",
        json!({
            "files": [{ "oldUri": "file:///vault/b.md", "newUri": "file:///vault/c.md" }]
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    let edits = result["changes"]["file:///vault/a.md"].as_array().expect("expected edits for a.md");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["newText"], "c.md");

    do_shutdown(&client, 3);
}

/// workspace/willRenameFiles rewrites outgoing links inside the renamed note.
#[test]
fn test_will_rename_outgoing() {
    let client = spawn_server();
    do_initialize(&client);

    // sub/a.md links to ../b.md (= /vault/b.md); rename sub/a.md → a.md should
    // rewrite that link to b.md (relative from new location).
    did_open(&client, "file:///vault/sub/a.md", "[link](../b.md)\n");

    send_request(
        &client,
        2,
        "workspace/willRenameFiles",
        json!({
            "files": [{
                "oldUri": "file:///vault/sub/a.md",
                "newUri": "file:///vault/a.md"
            }]
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    let edits = result["changes"]["file:///vault/sub/a.md"]
        .as_array()
        .expect("expected edits for sub/a.md");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["newText"], "b.md");

    do_shutdown(&client, 3);
}

/// workspace/willRenameFiles returns an empty WorkspaceEdit for a file with no links.
#[test]
fn test_will_rename_no_links() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/c.md", "# No links here\n");

    send_request(
        &client,
        2,
        "workspace/willRenameFiles",
        json!({
            "files": [{ "oldUri": "file:///vault/c.md", "newUri": "file:///vault/d.md" }]
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    let is_empty = result["changes"]
        .as_object()
        .map(|o| o.is_empty())
        .unwrap_or(true);
    assert!(is_empty, "expected empty changes for unlinked file, got {result}");

    do_shutdown(&client, 3);
}

/// A link to a non-Markdown file that is already tracked produces no diagnostic.
#[test]
fn test_attachment_no_diagnostic() {
    let client = spawn_server();
    do_initialize(&client);

    // Register the attachment before the note is opened.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({ "changes": [{ "uri": "file:///vault/logo.png", "type": 1 }] }),
        }))
        .unwrap();

    did_open(&client, "file:///vault/a.md", "[img](logo.png)\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let is_clean = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .last()
        .map(|d| d.diagnostics.is_empty())
        .unwrap_or(true);
    assert!(is_clean, "expected no diagnostic for registered attachment");

    do_shutdown(&client, 3);
}

/// Deleting a tracked attachment produces a broken-link diagnostic on referencing notes.
#[test]
fn test_attachment_deleted_adds_diagnostic() {
    let client = spawn_server();
    do_initialize(&client);

    // Register attachment, then open a note linking to it — should be clean.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({ "changes": [{ "uri": "file:///vault/logo.png", "type": 1 }] }),
        }))
        .unwrap();

    did_open(&client, "file:///vault/a.md", "[img](logo.png)\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let before = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .last()
        .map(|d| d.diagnostics.is_empty())
        .unwrap_or(true);
    assert!(before, "expected no diagnostic before deletion");

    // Delete the attachment.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({ "changes": [{ "uri": "file:///vault/logo.png", "type": 3 }] }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 3);
    let after = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .last()
        .expect("expected diagnostics published for a.md after deletion");
    assert_eq!(
        after.diagnostics.len(),
        1,
        "expected a broken-link diagnostic after attachment deleted"
    );

    do_shutdown(&client, 4);
}

/// Completion at `](` returns items for non-Markdown workspace files.
#[test]
fn test_completion_includes_attachment() {
    let client = spawn_server();
    do_initialize(&client);

    // Register an attachment.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({ "changes": [{ "uri": "file:///vault/logo.png", "type": 1 }] }),
        }))
        .unwrap();

    did_open(&client, "file:///vault/a.md", "[img](");

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 0, "character": 6 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: Option<CompletionResponse> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    let items = match result.expect("expected completion result") {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };

    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        labels.contains(&"logo.png"),
        "expected logo.png in completion items, got {labels:?}"
    );

    do_shutdown(&client, 3);
}

/// Find-references at the top of b.md returns the location of the link in a.md.
#[test]
fn references_returns_backlinks() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "");
    did_open(&client, "file:///vault/a.md", "[link](b.md)\n");

    // Request references for b.md (cursor at (0,0) — no link at that position,
    // so the handler falls back to returning all backlinks of b.md).
    send_request(
        &client,
        2,
        "textDocument/references",
        json!({
            "textDocument": { "uri": "file:///vault/b.md" },
            "position": { "line": 0, "character": 0 },
            "context": { "includeDeclaration": false }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let locs: Option<Vec<Location>> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let locs = locs.unwrap_or_default();

    assert_eq!(locs.len(), 1, "expected 1 backlink from a.md");
    assert!(locs[0].uri.as_str().ends_with("a.md"), "expected backlink to come from a.md");

    do_shutdown(&client, 3);
}
