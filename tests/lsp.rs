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
    assert!(caps.code_lens_provider.is_none(), "code lens should not be advertised");

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
