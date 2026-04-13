use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::CompletionItem;
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

fn request_completion(client: &Connection, request_id: i32, uri: &str, line: u32, character: u32) -> Vec<CompletionItem> {
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
    let result = resp.result.unwrap_or(json!([]));
    serde_json::from_value(result).expect("deserialize completion items")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Cursor immediately after `[[` triggers completion with one item per indexed note.
#[test]
fn completion_after_double_bracket() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_comp/alpha.md", "# Alpha\n");
    // Document requesting completion: content is "[[", cursor at (0, 2).
    open_note(&client, "file:///tmp/knap_comp/cursor.md", "[[");

    let items = request_completion(&client, 2, "file:///tmp/knap_comp/cursor.md", 0, 2);
    assert!(!items.is_empty(), "expected completion items after [[");

    do_shutdown(&client, 3);
}

/// Cursor after a single `[` does not trigger completion.
#[test]
fn completion_after_single_bracket() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_comp/single.md", "[");

    let items = request_completion(&client, 2, "file:///tmp/knap_comp/single.md", 0, 1);
    assert!(items.is_empty(), "expected no completion items after single [");

    do_shutdown(&client, 3);
}

/// With three notes in the index, completion returns exactly three items.
#[test]
fn completion_includes_all_notes() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_comp/note1.md", "# One\n");
    open_note(&client, "file:///tmp/knap_comp/note2.md", "# Two\n");
    open_note(&client, "file:///tmp/knap_comp/note3.md", "# Three\n");
    // Document requesting completion — itself a 4th note.
    open_note(&client, "file:///tmp/knap_comp/query.md", "[[");

    let items = request_completion(&client, 2, "file:///tmp/knap_comp/query.md", 0, 2);
    assert_eq!(items.len(), 4, "expected 4 items (3 notes + query.md itself), got {}", items.len());

    do_shutdown(&client, 3);
}

/// Every completion item has kind `File` (17).
#[test]
fn completion_item_is_file_kind() {
    use lsp_types::CompletionItemKind;

    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_comp/kind_target.md", "# Target\n");
    open_note(&client, "file:///tmp/knap_comp/kind_cursor.md", "[[");

    let items = request_completion(&client, 2, "file:///tmp/knap_comp/kind_cursor.md", 0, 2);
    assert!(!items.is_empty(), "expected at least one completion item");
    for item in &items {
        assert_eq!(
            item.kind,
            Some(CompletionItemKind::FILE),
            "expected FILE kind, got {:?}",
            item.kind
        );
    }

    do_shutdown(&client, 3);
}
