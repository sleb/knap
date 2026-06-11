use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::{
    CompletionItemKind, CompletionResponse, CompletionTextEdit, DiagnosticSeverity,
    GotoDefinitionResponse, Location, OneOf, PublishDiagnosticsParams, TextDocumentSyncCapability,
    TextDocumentSyncKind,
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

/// Server advertises core capabilities: sync=Full, completion trigger `(`,
/// definition, references, rename (with prepareRename). Does not advertise
/// hover, code_lens, or other unimplemented capabilities.
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

    // rename must be advertised with prepare support
    let rename_opts = caps.rename_provider.as_ref().expect("rename provider should be advertised");
    assert!(
        matches!(rename_opts, OneOf::Right(lsp_types::RenameOptions { prepare_provider: Some(true), .. })),
        "rename provider should have prepare_provider=true"
    );

    // unimplemented capabilities must NOT be present
    assert!(caps.hover_provider.is_none(), "hover should not be advertised");

    // code lens must be advertised (v0.6)
    assert!(caps.code_lens_provider.is_some(), "code lens should be advertised");

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
    // b.md is a sibling of a.md — appears as a FILE item with text_edit new_text "b.md"
    let b_item = items.iter().find(|i| match i.text_edit.as_ref() {
        Some(CompletionTextEdit::Edit(te)) => te.new_text == "b.md",
        _ => false,
    });
    assert!(b_item.is_some(), "expected an item with text_edit new_text = \"b.md\"");

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

// ─── v0.3 Integration tests ───────────────────────────────────────────────────

/// `textDocument/documentSymbol` returns one flat symbol per heading in document order.
#[test]
fn test_document_symbols_lists_headings() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "# Title\n\n## Section\n\n### Sub\n");

    send_request(
        &client,
        2,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": "file:///vault/a.md" } }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let symbols = result.as_array().expect("expected array of symbols");

    assert_eq!(symbols.len(), 3, "expected 3 heading symbols");
    let names: Vec<&str> = symbols.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Title"), "expected Title");
    assert!(names.contains(&"Section"), "expected Section");
    assert!(names.contains(&"Sub"), "expected Sub");

    do_shutdown(&client, 3);
}

/// File with no headings returns an empty flat list, not null.
#[test]
fn test_document_symbols_empty_for_no_headings() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "Just prose, no headings.\n");

    send_request(
        &client,
        2,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": "file:///vault/a.md" } }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let symbols = result.as_array().expect("expected empty array, not null");
    assert!(symbols.is_empty(), "expected no symbols for headingless file");

    do_shutdown(&client, 3);
}

/// `workspace/symbol` with a query string returns only headings whose text
/// contains the query (case-insensitive).
#[test]
fn test_workspace_symbols_query() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "# Introduction\n\n## Summary\n");
    did_open(&client, "file:///vault/b.md", "# Conclusion\n");

    send_request(&client, 2, "workspace/symbol", json!({ "query": "intro" }));

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let symbols = result.as_array().expect("expected array");

    assert_eq!(symbols.len(), 1, "expected only Introduction");
    assert_eq!(symbols[0]["name"].as_str().unwrap(), "Introduction");

    do_shutdown(&client, 3);
}

/// `workspace/symbol` with an empty query returns every heading from all indexed notes.
#[test]
fn test_workspace_symbols_empty_query() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "# H1\n\n## H2\n");
    did_open(&client, "file:///vault/b.md", "# H3\n");

    send_request(&client, 2, "workspace/symbol", json!({ "query": "" }));

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let symbols = result.as_array().expect("expected array");

    assert_eq!(symbols.len(), 3, "expected all 3 headings for empty query");

    do_shutdown(&client, 3);
}

/// `textDocument/prepareRename` on a heading line returns a non-null
/// RangeWithPlaceholder containing the heading text.
#[test]
fn test_prepare_rename_on_heading() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "## My Heading\n");

    send_request(
        &client,
        2,
        "textDocument/prepareRename",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 0, "character": 5 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    assert!(!result.is_null(), "expected non-null prepareRename for heading");
    assert!(result.get("range").is_some(), "expected 'range' field");
    assert_eq!(
        result["placeholder"].as_str(),
        Some("My Heading"),
        "placeholder should be the heading text"
    );

    do_shutdown(&client, 3);
}

/// `textDocument/prepareRename` on a prose line returns null.
#[test]
fn test_prepare_rename_off_heading() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "## Heading\n\nSome prose here.\n");

    send_request(
        &client,
        2,
        "textDocument/prepareRename",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 2, "character": 5 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    assert!(result.is_null(), "expected null prepareRename for prose line");

    do_shutdown(&client, 3);
}

/// `textDocument/rename` on a heading rewrites slug anchors in other files.
#[test]
fn test_rename_heading_updates_anchor_links() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "## Old Heading\n");
    did_open(&client, "file:///vault/a.md", "[link](b.md#old-heading)\n");

    send_request(
        &client,
        2,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": "file:///vault/b.md" },
            "position": { "line": 0, "character": 5 },
            "newName": "New Heading"
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    let b_edits = result["changes"]["file:///vault/b.md"]
        .as_array()
        .expect("expected edits for b.md");
    assert_eq!(b_edits.len(), 1);
    assert_eq!(b_edits[0]["newText"].as_str(), Some("New Heading"), "heading text should be updated");

    let a_edits = result["changes"]["file:///vault/a.md"]
        .as_array()
        .expect("expected edits for a.md");
    assert_eq!(a_edits.len(), 1);
    assert_eq!(a_edits[0]["newText"].as_str(), Some("new-heading"), "anchor slug should be updated");

    do_shutdown(&client, 3);
}

/// `textDocument/rename` on a heading rewrites anchor-only self-links within
/// the same file.
#[test]
fn test_rename_heading_updates_self_links() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "## Old Heading\n\n[jump](#old-heading)\n");

    send_request(
        &client,
        2,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 0, "character": 5 },
            "newName": "New Heading"
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    let a_edits = result["changes"]["file:///vault/a.md"]
        .as_array()
        .expect("expected edits for a.md");
    assert_eq!(a_edits.len(), 2, "expected 2 edits: heading text + self-link anchor");

    let new_texts: Vec<&str> =
        a_edits.iter().map(|e| e["newText"].as_str().unwrap()).collect();
    assert!(new_texts.contains(&"New Heading"), "expected heading text edit");
    assert!(new_texts.contains(&"new-heading"), "expected slug edit for self-link");

    do_shutdown(&client, 3);
}

/// `](file.md#` triggers anchor completion; items carry heading-text labels
/// and GFM slug insert_text values.
#[test]
fn test_anchor_completion() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "## My Section\n\n## Another Part\n");
    // cursor is right after `#` — character 12 in "[link](b.md#"
    did_open(&client, "file:///vault/a.md", "[link](b.md#");

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 0, "character": 12 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: Option<CompletionResponse> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let items = match result.expect("expected completion result") {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };

    assert_eq!(items.len(), 2, "expected 2 heading completions");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"My Section"), "expected 'My Section' label");
    assert!(labels.contains(&"Another Part"), "expected 'Another Part' label");

    let my_section = items.iter().find(|i| i.label == "My Section").unwrap();
    assert_eq!(
        my_section.insert_text.as_deref(),
        Some("my-section"),
        "insert_text should be GFM slug"
    );

    do_shutdown(&client, 3);
}

// ─── v0.3.1 Integration tests ────────────────────────────────────────────────

/// `textDocument/completion` at `](` returns FOLDER items for subdirectories
/// and FILE items for same-level siblings — not a flat list of all paths.
#[test]
fn test_dir_completion_initial() {
    let client = spawn_server();
    do_initialize(&client);

    // Workspace: vault/sub/b.md (in subdirectory) + vault/c.md (sibling of a.md)
    did_open(&client, "file:///vault/sub/b.md", "# B\n");
    did_open(&client, "file:///vault/c.md", "# C\n");
    // a.md: cursor right after `(` at character 7
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

    // FOLDER item for the sub/ directory must be present
    let folder_item = items.iter().find(|i| i.kind == Some(CompletionItemKind::FOLDER));
    assert!(folder_item.is_some(), "expected a FOLDER item for sub/");
    let folder = folder_item.unwrap();
    assert_eq!(folder.label, "sub/", "folder label should be 'sub/'");
    let folder_new_text = match folder.text_edit.as_ref() {
        Some(CompletionTextEdit::Edit(te)) => &te.new_text,
        _ => panic!("folder item should have a text_edit"),
    };
    assert_eq!(folder_new_text, "sub/", "folder new_text should be 'sub/'");

    // FILE item for the sibling c.md must be present
    let c_item = items
        .iter()
        .find(|i| matches!(i.text_edit.as_ref(), Some(CompletionTextEdit::Edit(te)) if te.new_text == "c.md"));
    assert!(c_item.is_some(), "expected a FILE item for c.md");
    assert_eq!(c_item.unwrap().kind, Some(CompletionItemKind::FILE));

    // sub/b.md also appears as a global FILE item so the user can jump directly
    let flat_item = items
        .iter()
        .find(|i| matches!(i.text_edit.as_ref(), Some(CompletionTextEdit::Edit(te)) if te.new_text == "sub/b.md"));
    assert!(
        flat_item.is_some(),
        "sub/b.md should appear as a global FILE item alongside the sub/ FOLDER"
    );

    do_shutdown(&client, 3);
}

/// Completion triggered with `/` after a subdir name (`](sub/`) returns children
/// of that subdirectory; new_text replaces the entire typed segment.
#[test]
fn test_dir_completion_retrigger_slash() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/sub/b.md", "# B\n");
    did_open(&client, "file:///vault/c.md", "# C\n");
    // a.md: cursor right after `sub/` at character 11
    did_open(&client, "file:///vault/a.md", "[link](sub/");

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 0, "character": 11 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: Option<CompletionResponse> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let items = match result.expect("expected completion result") {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };

    // sub/b.md should appear; new_text replaces the whole typed `sub/`
    let b_item = items
        .iter()
        .find(|i| matches!(i.text_edit.as_ref(), Some(CompletionTextEdit::Edit(te)) if te.new_text == "sub/b.md"));
    assert!(
        b_item.is_some(),
        "expected sub/b.md as a completion item when cursor is after 'sub/'; got: {:?}",
        items.iter().map(|i| &i.label).collect::<Vec<_>>()
    );
    assert_eq!(b_item.unwrap().kind, Some(CompletionItemKind::FILE));

    do_shutdown(&client, 3);
}

// ─── v0.1 tests (kept below) ─────────────────────────────────────────────────

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

// ─── Issue #2 regression test ─────────────────────────────────────────────────

/// prepareRename must return a non-null result even when the file was never sent
/// via didOpen (i.e. it is absent from the NoteIndex at the time of the request).
///
/// Scenario: server starts with no workspace_folders (empty index), no didOpen
/// is sent, and prepareRename is fired directly against an on-disk file.
#[test]
fn prepare_rename_without_did_open() {
    let path = std::env::temp_dir().join("knap_integ_pr_no_open.md");
    std::fs::write(&path, "# My Heading\n\nsome prose\n").expect("write temp file");
    let uri = format!("file://{}", path.display());

    let client = spawn_server();

    // Initialize with no workspace_folders → empty index at startup.
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(1i32),
            method: "initialize".to_string(),
            params: json!({
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "0.0.0" }
            }),
        }))
        .unwrap();
    recv_response(&client, lsp_server::RequestId::from(1i32));
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

    // No didOpen — file is absent from the index.
    send_request(
        &client,
        2,
        "textDocument/prepareRename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 5 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    std::fs::remove_file(&path).ok();

    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    assert!(!result.is_null(), "prepareRename should return non-null for an on-disk file even without didOpen");
    assert_eq!(
        result["placeholder"].as_str(),
        Some("My Heading"),
        "placeholder should be the heading text"
    );

    do_shutdown(&client, 3);
}

// ─── v0.4 Integration tests ───────────────────────────────────────────────────

fn do_initialize_with_options(client: &Connection, workspace_uri: &str, options: serde_json::Value) {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(1i32),
            method: "initialize".to_string(),
            params: json!({
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "0.0.1" },
                "workspaceFolders": [{ "uri": workspace_uri, "name": "vault" }],
                "initializationOptions": options
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

/// Cursor on a broken link returns a `CreateFile` code action.
#[test]
fn test_code_action_create_note() {
    let client = spawn_server();
    do_initialize(&client);

    // "[link](missing.md)" — cursor at (0, 3) is inside the link
    did_open(&client, "file:///vault/a.md", "[link](missing.md)\n");

    send_request(
        &client,
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 0, "character": 3 } },
            "context": { "diagnostics": [] }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let actions: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();

    assert_eq!(actions.len(), 1, "expected one code action for broken link");

    let action = &actions[0];
    let create_uri = action["edit"]["documentChanges"][0]["uri"].as_str();
    assert!(
        create_uri.map(|u| u.ends_with("missing.md")).unwrap_or(false),
        "expected CreateFile URI ending in missing.md, got: {action}"
    );

    do_shutdown(&client, 3);
}

/// With `newNoteDir` configured, `CreateFile` URI points into the inbox folder.
#[test]
fn test_code_action_create_note_in_new_note_dir() {
    let client = spawn_server();
    do_initialize_with_options(
        &client,
        "file:///vault",
        json!({ "newNoteDir": "inbox" }),
    );

    did_open(&client, "file:///vault/a.md", "[link](missing.md)\n");

    send_request(
        &client,
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 0, "character": 3 } },
            "context": { "diagnostics": [] }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let actions: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();

    assert_eq!(actions.len(), 1);
    let create_uri = actions[0]["edit"]["documentChanges"][0]["uri"].as_str().unwrap_or("");
    assert!(
        create_uri.ends_with("/vault/inbox/missing.md"),
        "expected CreateFile in inbox folder, got: {create_uri}"
    );

    do_shutdown(&client, 3);
}

/// Broken anchor with headings in target → actions with correct TextEdit.
#[test]
fn test_code_action_fix_anchor() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "# Introduction\n");
    // "[link](b.md#wrong)" — cursor at (0, 3)
    did_open(&client, "file:///vault/a.md", "[link](b.md#wrong)\n");

    send_request(
        &client,
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 0, "character": 3 } },
            "context": { "diagnostics": [] }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let actions: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();

    assert_eq!(actions.len(), 1, "expected one fix-anchor action");

    let new_text = actions[0]["edit"]["changes"]["file:///vault/a.md"][0]["newText"]
        .as_str()
        .unwrap_or("");
    assert_eq!(new_text, "introduction", "expected slug of 'Introduction'");

    do_shutdown(&client, 3);
}

/// Cursor on a valid link returns an empty list (not null).
#[test]
fn test_code_action_empty_for_valid_link() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "");
    did_open(&client, "file:///vault/a.md", "[link](b.md)\n");

    send_request(
        &client,
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 0, "character": 3 } },
            "context": { "diagnostics": [] }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result = resp.result.unwrap_or_default();
    let actions: Vec<serde_json::Value> = serde_json::from_value(result).unwrap_or_default();
    assert!(actions.is_empty(), "expected empty list for valid link, got {actions:?}");

    do_shutdown(&client, 3);
}

/// Cursor not on a link returns an empty list.
#[test]
fn test_code_action_empty_off_link() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "[link](missing.md) prose\n");

    send_request(
        &client,
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "range": { "start": { "line": 0, "character": 22 }, "end": { "line": 0, "character": 22 } },
            "context": { "diagnostics": [] }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let actions: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();
    assert!(actions.is_empty(), "expected empty list when cursor is off any link");

    do_shutdown(&client, 3);
}

// ─── Code Lens (v0.6) ─────────────────────────────────────────────────────────

/// File with two inbound links → one lens with the correct count and command.
#[test]
fn test_code_lens_backlinks() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/target.md", "# Target\n");
    did_open(&client, "file:///vault/a.md", "[link](target.md)\n");
    did_open(&client, "file:///vault/b.md", "[link](target.md)\n");

    send_request(
        &client,
        2,
        "textDocument/codeLens",
        serde_json::json!({
            "textDocument": { "uri": "file:///vault/target.md" }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let lenses: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();

    assert_eq!(lenses.len(), 1, "expected exactly one code lens");
    let title = lenses[0]["command"]["title"].as_str().unwrap_or("");
    assert_eq!(title, "↑ 2 backlinks");
    assert_eq!(lenses[0]["command"]["command"].as_str().unwrap_or(""), "editor.action.showReferences");

    let locations = lenses[0]["command"]["arguments"][2].as_array().unwrap();
    assert_eq!(locations.len(), 2);

    do_shutdown(&client, 3);
}

/// Orphan file → empty array, no lens.
#[test]
fn test_code_lens_no_backlinks() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/orphan.md", "# Orphan\n");

    send_request(
        &client,
        2,
        "textDocument/codeLens",
        serde_json::json!({
            "textDocument": { "uri": "file:///vault/orphan.md" }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let lenses: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();

    assert!(lenses.is_empty(), "orphan note should return empty lens array");

    do_shutdown(&client, 3);
}

// ─── v0.7 Integration tests ───────────────────────────────────────────────────

/// `textDocument/definition` on `[text](#section)` navigates to the matching
/// heading in the same file.
#[test]
fn test_same_file_anchor_definition() {
    let client = spawn_server();
    do_initialize(&client);

    // Line 0: "## Section", Line 2: "[text](#section)"
    did_open(&client, "file:///vault/a.md", "## Section\n\n[text](#section)\n");

    send_request(
        &client,
        2,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 2, "character": 3 }
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
    assert!(loc.uri.as_str().ends_with("a.md"), "should navigate within the same file");
    assert_eq!(loc.range.start.line, 0, "should land on the heading line");

    do_shutdown(&client, 3);
}

/// `textDocument/definition` on `[text](#missing)` returns the top of the same file.
#[test]
fn test_same_file_anchor_definition_missing() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "## Section\n\n[text](#missing)\n");

    send_request(
        &client,
        2,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 2, "character": 3 }
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
    assert!(loc.uri.as_str().ends_with("a.md"));
    assert_eq!(loc.range.start.line, 0);
    assert_eq!(loc.range.start.character, 0, "missing anchor falls back to top of file");

    do_shutdown(&client, 3);
}

/// Opening a file with `[text](#missing)` (no matching heading) publishes a WARNING.
#[test]
fn test_same_file_anchor_broken_diagnostic() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "## Real\n\n[text](#missing)\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .last()
        .expect("expected diagnostics for a.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert!(
        file_diags.diagnostics[0].message.contains("#missing"),
        "message should contain '#missing', got: {}",
        file_diags.diagnostics[0].message
    );

    do_shutdown(&client, 3);
}

/// Opening a file with `[text](#existing)` where a matching heading exists → no diagnostic.
#[test]
fn test_same_file_anchor_valid_no_diagnostic() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "## Existing\n\n[text](#existing)\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> =
        diags.iter().filter(|d| d.uri.as_str().ends_with("a.md")).collect();
    let last = file_diags.last().expect("expected diagnostics published for a.md");
    assert!(
        last.diagnostics.is_empty(),
        "expected no diagnostics for valid bare anchor, got {:?}",
        last.diagnostics
    );

    do_shutdown(&client, 3);
}

/// `textDocument/completion` at `[see](#` returns headings from the current file.
#[test]
fn test_same_file_anchor_completion() {
    let client = spawn_server();
    do_initialize(&client);

    // Line 0: "## My Section", Line 1: "## Another", Line 2: "", Line 3: "[see](#"
    did_open(&client, "file:///vault/a.md", "## My Section\n## Another\n\n[see](#");

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 3, "character": 7 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: Option<CompletionResponse> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let items = match result.expect("expected completion result") {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };

    assert_eq!(items.len(), 2, "expected 2 heading completions from current file");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"My Section"));
    assert!(labels.contains(&"Another"));

    do_shutdown(&client, 3);
}

/// `textDocument/references` on a heading line returns the bare anchor link
/// pointing to it within the same file.
#[test]
fn test_same_file_anchor_references_on_heading() {
    let client = spawn_server();
    do_initialize(&client);

    // Line 0: "## Introduction", Line 2: "[go to intro](#introduction)"
    did_open(&client, "file:///vault/a.md", "## Introduction\n\n[go to intro](#introduction)\n");

    send_request(
        &client,
        2,
        "textDocument/references",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 0, "character": 3 },
            "context": { "includeDeclaration": false }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let locs: Option<Vec<Location>> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let locs = locs.unwrap_or_default();

    assert_eq!(locs.len(), 1, "expected 1 reference: the bare anchor link");
    assert!(locs[0].uri.as_str().ends_with("a.md"));

    do_shutdown(&client, 3);
}

/// After a new linking note is opened, the lens count reflects the updated index.
#[test]
fn test_code_lens_updates_after_index_change() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/target.md", "# Target\n");
    did_open(&client, "file:///vault/a.md", "[link](target.md)\n");

    // Initial state: 1 backlink.
    send_request(
        &client,
        2,
        "textDocument/codeLens",
        serde_json::json!({ "textDocument": { "uri": "file:///vault/target.md" } }),
    );
    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let lenses: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();
    assert_eq!(lenses[0]["command"]["title"].as_str().unwrap_or(""), "↑ 1 backlink");

    // Add a second linking note.
    did_open(&client, "file:///vault/b.md", "[link](target.md)\n");

    // Updated state: 2 backlinks.
    send_request(
        &client,
        3,
        "textDocument/codeLens",
        serde_json::json!({ "textDocument": { "uri": "file:///vault/target.md" } }),
    );
    let resp = recv_response(&client, lsp_server::RequestId::from(3i32));
    let lenses: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();
    assert_eq!(lenses[0]["command"]["title"].as_str().unwrap_or(""), "↑ 2 backlinks");

    do_shutdown(&client, 4);
}

// ─── Schema (v0.8) ────────────────────────────────────────────────────────────

/// Schema key completions: blank frontmatter line with schema configured → FIELD items.
#[test]
fn test_schema_key_completion() {
    let client = spawn_server();
    do_initialize_with_options(
        &client,
        "file:///vault",
        json!({
            "frontmatterSchema": {
                "fields": {
                    "status": { "values": ["draft", "published"], "required": true },
                    "type": { "values": ["note", "meeting"] }
                }
            }
        }),
    );

    // blank frontmatter line; cursor at line 1, char 0
    did_open(&client, "file:///vault/a.md", "---\n\n---\n");

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 1, "character": 0 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: Option<CompletionResponse> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let items = match result.expect("expected completion result") {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };

    assert!(!items.is_empty(), "expected schema key items");
    assert!(items.iter().all(|i| i.kind == Some(CompletionItemKind::FIELD)));
    assert!(items.iter().any(|i| i.label == "status"));
    assert!(items.iter().any(|i| i.label == "type"));

    do_shutdown(&client, 3);
}

/// Schema value completions: `status: ` position with enum schema → VALUE items.
#[test]
fn test_schema_value_completion() {
    let client = spawn_server();
    do_initialize_with_options(
        &client,
        "file:///vault",
        json!({
            "frontmatterSchema": {
                "fields": {
                    "status": { "values": ["draft", "published"] }
                }
            }
        }),
    );

    // "status: " — cursor at char 8 (right after the space)
    did_open(&client, "file:///vault/a.md", "---\nstatus: \n---\n");

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 1, "character": 8 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: Option<CompletionResponse> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();
    let items = match result.expect("expected completion result") {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };

    assert_eq!(items.len(), 2, "expected exactly two enum values");
    assert!(items.iter().all(|i| i.kind == Some(CompletionItemKind::VALUE)));
    assert!(items.iter().any(|i| i.label == "draft"));
    assert!(items.iter().any(|i| i.label == "published"));

    do_shutdown(&client, 3);
}

/// Required key missing from frontmatter → WARNING diagnostic at (0,0).
#[test]
fn test_schema_required_key_missing_diagnostic() {
    let client = spawn_server();
    do_initialize_with_options(
        &client,
        "file:///vault",
        json!({
            "frontmatterSchema": {
                "fields": {
                    "status": { "required": true }
                }
            }
        }),
    );

    did_open(&client, "file:///vault/a.md", "---\ntitle: My Note\n---\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .last()
        .expect("no diagnostics published for a.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert!(
        file_diags.diagnostics[0].message.contains("status"),
        "unexpected message: {}",
        file_diags.diagnostics[0].message
    );

    do_shutdown(&client, 3);
}

/// Value not in enum → WARNING diagnostic.
#[test]
fn test_schema_invalid_value_diagnostic() {
    let client = spawn_server();
    do_initialize_with_options(
        &client,
        "file:///vault",
        json!({
            "frontmatterSchema": {
                "fields": {
                    "status": { "values": ["draft", "published"] }
                }
            }
        }),
    );

    did_open(&client, "file:///vault/a.md", "---\nstatus: Draft\n---\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .last()
        .expect("no diagnostics published for a.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert!(
        file_diags.diagnostics[0].message.contains("Draft"),
        "unexpected message: {}",
        file_diags.diagnostics[0].message
    );

    do_shutdown(&client, 3);
}

/// Note satisfying all schema rules → no schema warnings.
#[test]
fn test_schema_valid_note_no_diagnostic() {
    let client = spawn_server();
    do_initialize_with_options(
        &client,
        "file:///vault",
        json!({
            "frontmatterSchema": {
                "fields": {
                    "status": { "values": ["draft", "published"], "required": true }
                }
            }
        }),
    );

    did_open(&client, "file:///vault/a.md", "---\nstatus: draft\n---\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .collect();
    let last = file_diags.last().expect("no diagnostics published for a.md");
    assert!(
        last.diagnostics.is_empty(),
        "valid note should have no warnings, got {:?}",
        last.diagnostics
    );

    do_shutdown(&client, 3);
}

/// `requireFrontmatter: true` + note without frontmatter block → WARNING at (0,0).
#[test]
fn test_schema_require_frontmatter_warns_on_missing_block() {
    let client = spawn_server();
    do_initialize_with_options(
        &client,
        "file:///vault",
        json!({
            "frontmatterSchema": {
                "fields": {
                    "status": { "required": true }
                },
                "requireFrontmatter": true
            }
        }),
    );

    did_open(&client, "file:///vault/a.md", "just prose, no frontmatter\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .last()
        .expect("no diagnostics published for a.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(
        file_diags.diagnostics[0].range.start,
        lsp_types::Position { line: 0, character: 0 }
    );

    do_shutdown(&client, 3);
}

/// `warnOnUnknownKeys: true` + note with unrecognised key → WARNING on that key.
#[test]
fn test_schema_warn_unknown_keys() {
    let client = spawn_server();
    do_initialize_with_options(
        &client,
        "file:///vault",
        json!({
            "frontmatterSchema": {
                "fields": {
                    "status": {}
                },
                "warnOnUnknownKeys": true
            }
        }),
    );

    did_open(&client, "file:///vault/a.md", "---\nfoobar: x\n---\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .last()
        .expect("no diagnostics published for a.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert!(
        file_diags.diagnostics[0].message.contains("foobar"),
        "unexpected message: {}",
        file_diags.diagnostics[0].message
    );

    do_shutdown(&client, 3);
}

/// No schema configured → no extra diagnostics for any frontmatter content.
#[test]
fn test_no_schema_no_extra_diagnostics() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/a.md", "---\narbitrary: value\nother: thing\n---\n");

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("a.md"))
        .collect();
    let last = file_diags.last().expect("no diagnostics published for a.md");
    assert!(
        last.diagnostics.is_empty(),
        "no schema should produce no diagnostics, got {:?}",
        last.diagnostics
    );

    do_shutdown(&client, 3);
}

// ─── v0.9 Integration tests ───────────────────────────────────────────────────

/// `textDocument/foldingRange` returns fold regions for headings and fenced blocks.
#[test]
fn folding_ranges_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    // Line 0: ## First, Line 1: content, Line 2: ## Second, Line 3: ``` (fence start),
    // Line 4: code, Line 5: ``` (fence end)
    did_open(
        &client,
        "file:///vault/a.md",
        "## First\ncontent\n## Second\n```\ncode\n```\n",
    );

    send_request(
        &client,
        2,
        "textDocument/foldingRange",
        serde_json::json!({ "textDocument": { "uri": "file:///vault/a.md" } }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let ranges: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();

    assert!(!ranges.is_empty(), "expected at least one fold region");
    // ## First (line 0) ends at line 1 (before ## Second at line 2)
    let first = ranges.iter().find(|r| r["startLine"] == 0);
    assert!(first.is_some(), "expected a fold starting at line 0");
    assert_eq!(first.unwrap()["endLine"], 1, "## First section should end at line 1");
    // Code fence (line 3..5) should appear
    let fence = ranges.iter().find(|r| r["startLine"] == 3);
    assert!(fence.is_some(), "expected a fold for the code fence at line 3");
    assert_eq!(fence.unwrap()["endLine"], 5, "code fence should end at line 5");

    do_shutdown(&client, 3);
}

/// `textDocument/selectionRange` returns a correct selection chain for a given cursor.
#[test]
fn selection_range_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    // Line 0: ## Heading, Line 1: (blank), Line 2: hello world
    did_open(&client, "file:///vault/a.md", "## Heading\n\nhello world\n");

    send_request(
        &client,
        2,
        "textDocument/selectionRange",
        serde_json::json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "positions": [{ "line": 2, "character": 2 }]
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();

    assert_eq!(result.len(), 1, "one position → one selection range chain");
    // Outermost range must start at (0, 0)
    let mut node = &result[0];
    loop {
        if node.get("parent").map(|p| !p.is_null()).unwrap_or(false) {
            node = &node["parent"];
        } else {
            break;
        }
    }
    assert_eq!(node["range"]["start"]["line"], 0, "outermost range must start at line 0");
    assert_eq!(node["range"]["start"]["character"], 0);

    do_shutdown(&client, 3);
}

/// `textDocument/inlayHint` returns hints for links whose targets have a `title:`.
#[test]
fn inlay_hints_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "---\ntitle: My Note\n---\n");
    did_open(&client, "file:///vault/a.md", "[link](b.md)\n");

    send_request(
        &client,
        2,
        "textDocument/inlayHint",
        serde_json::json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 9999, "character": 9999 }
            }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let hints: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();

    assert_eq!(hints.len(), 1, "expected one inlay hint for the titled link");
    let label = hints[0]["label"].as_str().unwrap_or("");
    assert_eq!(label, "-> My Note");

    do_shutdown(&client, 3);
}

/// `textDocument/codeLens` returns heading lenses alongside the backlinks lens.
#[test]
fn code_lens_heading_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    // a.md has a heading "## Target" that b.md links to via bare anchor from the same file,
    // and c.md links via cross-file anchor.
    did_open(&client, "file:///vault/a.md", "## Target\ncontent\n[same](#target)\n");
    did_open(&client, "file:///vault/b.md", "[link](a.md#target)\n");

    send_request(
        &client,
        2,
        "textDocument/codeLens",
        serde_json::json!({ "textDocument": { "uri": "file:///vault/a.md" } }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let lenses: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap_or_default();

    // There should be a heading lens for "## Target" (2 anchor links total)
    let heading_lens = lenses.iter().find(|l| {
        l["command"]["title"]
            .as_str()
            .map(|t| t.contains("anchor"))
            .unwrap_or(false)
    });
    assert!(heading_lens.is_some(), "expected a heading anchor lens; got: {lenses:?}");
    let title = heading_lens.unwrap()["command"]["title"].as_str().unwrap_or("");
    assert!(title.contains('2'), "heading lens should count 2 anchor links; got: {title}");

    do_shutdown(&client, 3);
}

// ─── v0.10 Integration tests ─────────────────────────────────────────────────

/// `textDocument/prepareRename` on a frontmatter tag returns the tag's range
/// and name as placeholder.
#[test]
fn prepare_rename_tag_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    // Line 0: ---
    // Line 1: tags: rust
    // Line 2: ---
    did_open(&client, "file:///vault/a.md", "---\ntags: rust\n---\n");

    send_request(
        &client,
        2,
        "textDocument/prepareRename",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 1, "character": 7 }
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    assert!(!result.is_null(), "expected non-null prepareRename for tag");
    assert!(result.get("range").is_some(), "expected 'range' field");
    assert_eq!(
        result["placeholder"].as_str(),
        Some("rust"),
        "placeholder should be the tag name"
    );

    do_shutdown(&client, 3);
}

/// `textDocument/rename` on a frontmatter tag returns a workspace edit
/// covering every file that carries that tag.
#[test]
fn rename_tag_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    // Two notes each carrying the tag "rust".
    // Line 0: ---
    // Line 1: tags: rust
    // Line 2: ---
    did_open(&client, "file:///vault/a.md", "---\ntags: rust\n---\n");
    did_open(&client, "file:///vault/b.md", "---\ntags: rust\n---\n");

    send_request(
        &client,
        2,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": "file:///vault/a.md" },
            "position": { "line": 1, "character": 7 },
            "newName": "systems"
        }),
    );

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let result: serde_json::Value =
        serde_json::from_value(resp.result.unwrap_or_default()).unwrap();

    let a_edits = result["changes"]["file:///vault/a.md"]
        .as_array()
        .expect("expected edits for a.md");
    assert_eq!(a_edits.len(), 1);
    assert_eq!(a_edits[0]["newText"].as_str(), Some("systems"));

    let b_edits = result["changes"]["file:///vault/b.md"]
        .as_array()
        .expect("expected edits for b.md");
    assert_eq!(b_edits.len(), 1);
    assert_eq!(b_edits[0]["newText"].as_str(), Some("systems"));

    do_shutdown(&client, 3);
}
