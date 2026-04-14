use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::{PublishDiagnosticsParams, WorkspaceEdit};
use serde_json::json;

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

fn open_note(client: &Connection, uri: &str, content: &str) {
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

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Full round-trip: send `willRenameFiles`, receive `WorkspaceEdit` with the
/// correct text edit rewriting the backlink stem.
#[test]
fn will_rename_returns_workspace_edit() {
    let client = spawn_server();
    do_initialize(&client);

    // Index the rename target and the file that links to it.
    open_note(&client, "file:///tmp/knap_rename/target.md", "# Target\n");
    open_note(&client, "file:///tmp/knap_rename/source.md", "[[target]]\n");

    // Send willRenameFiles: target.md → renamed.md
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(2i32),
            method: "workspace/willRenameFiles".to_string(),
            params: json!({
                "files": [{
                    "oldUri": "file:///tmp/knap_rename/target.md",
                    "newUri": "file:///tmp/knap_rename/renamed.md"
                }]
            }),
        }))
        .unwrap();

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    assert!(resp.error.is_none(), "willRenameFiles returned error: {:?}", resp.error);

    let edit: WorkspaceEdit =
        serde_json::from_value(resp.result.expect("expected result")).expect("deserialize WorkspaceEdit");

    let changes = edit.changes.expect("expected changes in WorkspaceEdit");
    let source_uri: lsp_types::Uri = "file:///tmp/knap_rename/source.md"
        .parse()
        .expect("valid URI");
    let edits = changes.get(&source_uri).expect("expected edits for source.md");
    assert_eq!(edits.len(), 1, "expected exactly one TextEdit");
    assert_eq!(edits[0].new_text, "renamed", "expected stem rewritten to 'renamed'");

    do_shutdown(&client, 3);
}

/// After the rename edit is applied and the index updated via `didChange` and
/// `didChangeWatchedFiles`, diagnostics on source.md clear (no broken link).
#[test]
fn index_consistent_after_rename() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_rename2/target.md", "# Target\n");
    open_note(&client, "file:///tmp/knap_rename2/source.md", "[[target]]\n");

    // Drain initial diagnostics.
    sync_and_collect_diagnostics(&client, 2);

    // Simulate applying the WorkspaceEdit: source.md now contains [[moved]].
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didChange".to_string(),
            params: json!({
                "textDocument": { "uri": "file:///tmp/knap_rename2/source.md", "version": 2 },
                "contentChanges": [{ "text": "[[moved]]\n" }]
            }),
        }))
        .unwrap();

    // Simulate the watcher: target.md deleted, moved.md created.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({
                "changes": [
                    { "uri": "file:///tmp/knap_rename2/target.md", "type": 3 },
                    { "uri": "file:///tmp/knap_rename2/moved.md",  "type": 1 }
                ]
            }),
        }))
        .unwrap();

    // The watcher CREATED event reads moved.md from disk — it doesn't exist in
    // this test, so we open it explicitly to register it in the index.
    open_note(&client, "file:///tmp/knap_rename2/moved.md", "# Moved\n");

    let diags = sync_and_collect_diagnostics(&client, 3);

    // source.md should now have no broken-link diagnostic (links to "moved").
    let source_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("source.md"))
        .collect();
    if let Some(last) = source_diags.last() {
        assert!(
            last.diagnostics.is_empty(),
            "expected no diagnostics after rename, got {:?}",
            last.diagnostics
        );
    }
    // If no diagnostics were published for source.md at all, that's also fine
    // (it means the server correctly found no issues).

    do_shutdown(&client, 4);
}

/// Renaming a file that has no backlinks returns an empty WorkspaceEdit.
#[test]
fn will_rename_no_backlinks() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_rename3/standalone.md", "# Standalone\n");

    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(2i32),
            method: "workspace/willRenameFiles".to_string(),
            params: json!({
                "files": [{
                    "oldUri": "file:///tmp/knap_rename3/standalone.md",
                    "newUri": "file:///tmp/knap_rename3/other.md"
                }]
            }),
        }))
        .unwrap();

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    assert!(resp.error.is_none(), "willRenameFiles returned error: {:?}", resp.error);

    let edit: WorkspaceEdit =
        serde_json::from_value(resp.result.expect("expected result")).expect("deserialize WorkspaceEdit");

    let is_empty = edit.changes.as_ref().map(|c| c.is_empty()).unwrap_or(true);
    assert!(is_empty, "expected empty WorkspaceEdit for a file with no backlinks");

    do_shutdown(&client, 3);
}

/// A note with an aliased link `[[old|alias]]` — only the stem range is rewritten.
#[test]
fn will_rename_aliased_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_rename4/target.md", "# Target\n");
    open_note(&client, "file:///tmp/knap_rename4/doc.md", "[[target|display text]]\n");

    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(2i32),
            method: "workspace/willRenameFiles".to_string(),
            params: json!({
                "files": [{
                    "oldUri": "file:///tmp/knap_rename4/target.md",
                    "newUri": "file:///tmp/knap_rename4/new-target.md"
                }]
            }),
        }))
        .unwrap();

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    let edit: WorkspaceEdit =
        serde_json::from_value(resp.result.expect("expected result")).expect("deserialize WorkspaceEdit");

    let changes = edit.changes.expect("expected changes");
    let doc_uri: lsp_types::Uri = "file:///tmp/knap_rename4/doc.md".parse().expect("valid URI");
    let edits = changes.get(&doc_uri).expect("expected edits for doc.md");
    assert_eq!(edits.len(), 1);
    // new_text is the new stem only — alias is preserved by applying to inner_range
    assert_eq!(edits[0].new_text, "new-target");
    // inner_range covers "target" (chars 2–8 on line 0)
    assert_eq!(edits[0].range.start.character, 2);
    assert_eq!(edits[0].range.end.character, 8);

    do_shutdown(&client, 3);
}
