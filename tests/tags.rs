use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::{CompletionItem, Location};
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

fn request_completion(
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
    assert!(resp.error.is_none(), "completion error: {:?}", resp.error);
    serde_json::from_value(resp.result.unwrap_or(json!([]))).expect("deserialize completion items")
}

/// Request definition and return the raw JSON result (may be null, object, or array).
fn request_definition_raw(
    client: &Connection,
    request_id: i32,
    uri: &str,
    line: u32,
    character: u32,
) -> serde_json::Value {
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
    assert!(resp.error.is_none(), "definition error: {:?}", resp.error);
    resp.result.unwrap_or(json!(null))
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
    assert!(resp.error.is_none(), "references error: {:?}", resp.error);
    serde_json::from_value(resp.result.unwrap_or(json!([]))).expect("deserialize locations")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Completion triggered inside `tags: [` returns the workspace's known tags.
#[test]
fn tag_completion_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    // Two notes that define tags in the workspace.
    open_note(
        &client,
        "file:///tmp/knap_tags/note_a.md",
        "---\ntags: [rust, lsp]\n---\n# Note A\n",
    );
    open_note(
        &client,
        "file:///tmp/knap_tags/note_b.md",
        "---\ntags: [tools]\n---\n# Note B\n",
    );
    // cursor.md has a tags: [ line where completion is triggered.
    // line 1 is `tags: [`, cursor at character 8 (just after `[`).
    open_note(
        &client,
        "file:///tmp/knap_tags/cursor.md",
        "---\ntags: [\n---\n",
    );

    let items = request_completion(&client, 2, "file:///tmp/knap_tags/cursor.md", 1, 8);
    assert!(!items.is_empty(), "expected tag completions");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"rust"), "expected 'rust': {:?}", labels);
    assert!(labels.contains(&"lsp"), "expected 'lsp': {:?}", labels);
    assert!(labels.contains(&"tools"), "expected 'tools': {:?}", labels);

    do_shutdown(&client, 3);
}

/// `textDocument/definition` on a tag value returns an array of all files
/// that carry that tag, each pointing to the tag's range.
#[test]
fn tag_definition_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    // note_a and note_b both have the `rust` tag; note_c does not.
    // note_a: "---\ntags: [rust]\n---\n"
    //          line 1: tags: [rust]
    //          'rust' starts at col 8 on line 1
    open_note(
        &client,
        "file:///tmp/knap_tagdef/note_a.md",
        "---\ntags: [rust]\n---\n",
    );
    open_note(
        &client,
        "file:///tmp/knap_tagdef/note_b.md",
        "---\ntags: [rust, lsp]\n---\n",
    );
    open_note(
        &client,
        "file:///tmp/knap_tagdef/note_c.md",
        "---\ntags: [lsp]\n---\n",
    );

    // Cursor on 'rust' in note_a.md: line 1, char 8
    let result = request_definition_raw(&client, 2, "file:///tmp/knap_tagdef/note_a.md", 1, 8);

    assert!(result.is_array(), "expected JSON array for tag definition, got: {result}");
    let locs: Vec<Location> =
        serde_json::from_value(result).expect("deserialize location array");
    assert_eq!(locs.len(), 2, "expected 2 notes with 'rust' tag: {:?}", locs);
    let uris: Vec<&str> = locs.iter().map(|l| l.uri.as_str()).collect();
    assert!(uris.iter().any(|u| u.ends_with("note_a.md")));
    assert!(uris.iter().any(|u| u.ends_with("note_b.md")));

    do_shutdown(&client, 3);
}

/// `textDocument/references` on a tag returns the same set as definition.
#[test]
fn tag_references_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(
        &client,
        "file:///tmp/knap_tagref/note_a.md",
        "---\ntags: [rust]\n---\n",
    );
    open_note(
        &client,
        "file:///tmp/knap_tagref/note_b.md",
        "---\ntags: [rust]\n---\n",
    );

    // Cursor on 'rust' in note_a.md: line 1, char 8
    let locs = request_references(&client, 2, "file:///tmp/knap_tagref/note_a.md", 1, 8);
    assert_eq!(locs.len(), 2, "expected references for both notes with 'rust'");
    let uris: Vec<&str> = locs.iter().map(|l| l.uri.as_str()).collect();
    assert!(uris.iter().any(|u| u.ends_with("note_a.md")));
    assert!(uris.iter().any(|u| u.ends_with("note_b.md")));

    do_shutdown(&client, 3);
}

/// Definition on body text (not a tag or wiki-link) returns null.
#[test]
fn tag_definition_no_tag_at_pos() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(
        &client,
        "file:///tmp/knap_tagmiss/note.md",
        "---\ntags: [rust]\n---\nJust prose here.\n",
    );

    // Cursor on "prose" in body (line 3, char 5) — not a tag or wiki-link
    let result = request_definition_raw(&client, 2, "file:///tmp/knap_tagmiss/note.md", 3, 5);
    assert!(result.is_null(), "expected null for body text, got: {result}");

    do_shutdown(&client, 3);
}
