use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::Location;
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

fn request_definition(
    client: &Connection,
    request_id: i32,
    uri: &str,
    line: u32,
    character: u32,
) -> Option<Location> {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(request_id),
            method: "textDocument/definition".to_string(),
            params: json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        }))
        .unwrap();

    let resp = recv_response(client, lsp_server::RequestId::from(request_id));
    assert!(resp.error.is_none(), "definition returned error: {:?}", resp.error);
    let result = resp.result.unwrap_or(json!(null));
    serde_json::from_value(result).expect("deserialize Location")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Cursor on a valid wiki-link returns a Location pointing to the target note.
#[test]
fn definition_on_valid_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_def/target.md", "# Target\n");
    // "[[target]]" — [[  at 0, target at 2..8, ]] at 8
    open_note(&client, "file:///tmp/knap_def/source.md", "[[target]]\n");

    // Cursor at (0, 3) — inside "target"
    let loc = request_definition(&client, 2, "file:///tmp/knap_def/source.md", 0, 3);
    let loc = loc.expect("expected a Location for a valid link");
    assert!(
        loc.uri.as_str().ends_with("target.md"),
        "expected uri ending with target.md, got {}",
        loc.uri.as_str()
    );

    do_shutdown(&client, 3);
}

/// Cursor on a broken wiki-link returns null (no location).
#[test]
fn definition_on_broken_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_def/broken_src.md", "[[nonexistent]]\n");

    let loc = request_definition(&client, 2, "file:///tmp/knap_def/broken_src.md", 0, 4);
    assert!(loc.is_none(), "expected null for a broken link");

    do_shutdown(&client, 3);
}

/// Cursor not on any wiki-link returns null.
#[test]
fn definition_off_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_def/prose.md", "Just some prose\n");

    // Cursor at (0, 5) — middle of plain text
    let loc = request_definition(&client, 2, "file:///tmp/knap_def/prose.md", 0, 5);
    assert!(loc.is_none(), "expected null when cursor is not on a wiki-link");

    do_shutdown(&client, 3);
}

/// Cursor on a link with a heading anchor ([[note#section]]) resolves to the note.
#[test]
fn definition_on_anchor_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_def/anchored.md", "# Anchored\n");
    // "[[anchored#intro]]" — cursor at (0, 4) inside "anchored"
    open_note(&client, "file:///tmp/knap_def/anchor_src.md", "[[anchored#intro]]\n");

    let loc = request_definition(&client, 2, "file:///tmp/knap_def/anchor_src.md", 0, 4);
    let loc = loc.expect("expected a Location for an anchor link");
    assert!(
        loc.uri.as_str().ends_with("anchored.md"),
        "expected uri ending with anchored.md, got {}",
        loc.uri.as_str()
    );

    do_shutdown(&client, 3);
}

/// Cursor on a link with a display alias ([[note|Alias]]) resolves to the note.
#[test]
fn definition_on_aliased_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_def/aliased.md", "# Aliased\n");
    // "[[aliased|My Alias]]" — cursor at (0, 3) inside "aliased"
    open_note(&client, "file:///tmp/knap_def/alias_src.md", "[[aliased|My Alias]]\n");

    let loc = request_definition(&client, 2, "file:///tmp/knap_def/alias_src.md", 0, 3);
    let loc = loc.expect("expected a Location for an aliased link");
    assert!(
        loc.uri.as_str().ends_with("aliased.md"),
        "expected uri ending with aliased.md, got {}",
        loc.uri.as_str()
    );

    do_shutdown(&client, 3);
}

/// Cursor on an ambiguous wiki-link (multiple notes with same stem) returns null.
#[test]
fn definition_on_ambiguous_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_def/sub1/dup.md", "# Dup\n");
    open_note(&client, "file:///tmp/knap_def/sub2/dup.md", "# Dup\n");
    open_note(&client, "file:///tmp/knap_def/ambig_src.md", "[[dup]]\n");

    // Cursor at (0, 3) — inside "dup"
    let loc = request_definition(&client, 2, "file:///tmp/knap_def/ambig_src.md", 0, 3);
    assert!(loc.is_none(), "expected null for an ambiguous link");

    do_shutdown(&client, 3);
}
