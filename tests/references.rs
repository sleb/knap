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

fn request_references(
    client: &Connection,
    request_id: i32,
    uri: &str,
    line: u32,
    character: u32,
) -> Vec<Location> {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(request_id),
            method: "textDocument/references".to_string(),
            params: json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
                "context": { "includeDeclaration": false }
            }),
        }))
        .unwrap();

    let resp = recv_response(client, lsp_server::RequestId::from(request_id));
    assert!(resp.error.is_none(), "references returned error: {:?}", resp.error);
    let result = resp.result.unwrap_or(json!([]));
    serde_json::from_value(result).expect("deserialize Vec<Location>")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Cursor on a valid wiki-link returns Locations for all files that link to the target.
#[test]
fn references_on_valid_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_ref/target.md", "# Target\n");
    // "[[target]]" — full range (0,0)..(0,10)
    open_note(&client, "file:///tmp/knap_ref/source.md", "[[target]]\n");

    // Cursor at (0, 3) — inside "target"
    let locs = request_references(&client, 2, "file:///tmp/knap_ref/source.md", 0, 3);
    assert_eq!(locs.len(), 1, "expected one reference");
    assert!(
        locs[0].uri.as_str().ends_with("source.md"),
        "expected source.md in reference, got {}",
        locs[0].uri.as_str()
    );

    do_shutdown(&client, 3);
}

/// Cursor on a broken wiki-link returns an empty list.
#[test]
fn references_on_broken_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_ref/broken_src.md", "[[nonexistent]]\n");

    let locs = request_references(&client, 2, "file:///tmp/knap_ref/broken_src.md", 0, 4);
    assert!(locs.is_empty(), "expected empty list for a broken link");

    do_shutdown(&client, 3);
}

/// Cursor not on any wiki-link returns an empty list.
#[test]
fn references_off_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_ref/prose.md", "Just some prose\n");

    // Cursor at (0, 5) — middle of plain text
    let locs = request_references(&client, 2, "file:///tmp/knap_ref/prose.md", 0, 5);
    assert!(locs.is_empty(), "expected empty list when cursor is not on a wiki-link");

    do_shutdown(&client, 3);
}

/// Cursor on an ambiguous wiki-link returns an empty list.
#[test]
fn references_on_ambiguous_link() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_ref/sub1/dup.md", "# Dup\n");
    open_note(&client, "file:///tmp/knap_ref/sub2/dup.md", "# Dup\n");
    open_note(&client, "file:///tmp/knap_ref/ambig_src.md", "[[dup]]\n");

    // Cursor at (0, 3) — inside "dup"
    let locs = request_references(&client, 2, "file:///tmp/knap_ref/ambig_src.md", 0, 3);
    assert!(locs.is_empty(), "expected empty list for an ambiguous link");

    do_shutdown(&client, 3);
}
