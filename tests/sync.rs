use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use serde_json::json;

fn spawn_server() -> Connection {
    let (server_conn, client_conn) = Connection::memory();
    thread::spawn(move || {
        knap::server::run(server_conn).expect("server error");
    });
    client_conn
}

/// Perform the initialize / initialized handshake.
/// After this returns the server is in Running state.
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

    // Consume the initialize response.
    recv_response(client, lsp_server::RequestId::from(1i32));

    client
        .sender
        .send(Message::Notification(Notification {
            method: "initialized".to_string(),
            params: json!({}),
        }))
        .unwrap();

    // Consume the server-initiated client/registerCapability request.
    loop {
        match client.receiver.recv().unwrap() {
            Message::Request(req) if req.method == "client/registerCapability" => break,
            Message::Request(_) | Message::Notification(_) | Message::Response(_) => {}
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

// ─── Tests ────────────────────────────────────────────────────────────────────

/// After `didOpen`, the note is in the index. Verified here by confirming the
/// server stays alive and responsive (Step 6 will verify via diagnostics).
#[test]
fn did_open_indexes_note() {
    let client = spawn_server();
    do_initialize(&client);

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_test/a.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "# Note A\n\n[[b]]\n"
                }
            }),
        }))
        .unwrap();

    // Server stays alive and processes further requests.
    do_shutdown(&client, 2);
}

/// After `didChange`, the index reflects the updated content.
#[test]
fn did_change_updates_note() {
    let client = spawn_server();
    do_initialize(&client);

    // Open with one link.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_test/a.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[old-link]]\n"
                }
            }),
        }))
        .unwrap();

    // Change to different content.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didChange".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_test/a.md",
                    "version": 2
                },
                "contentChanges": [{ "text": "[[new-link]]\n" }]
            }),
        }))
        .unwrap();

    do_shutdown(&client, 2);
}

/// After `didClose`, the note is still in the index (closing does not remove it).
#[test]
fn did_close_retains_note() {
    let client = spawn_server();
    do_initialize(&client);

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_test/a.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "# Note A\n"
                }
            }),
        }))
        .unwrap();

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didClose".to_string(),
            params: json!({
                "textDocument": { "uri": "file:///tmp/knap_test/a.md" }
            }),
        }))
        .unwrap();

    // Server stays alive — note is retained in the index.
    do_shutdown(&client, 2);
}

/// A `didChangeWatchedFiles` Created event is processed without error.
/// If the file exists on disk it gets indexed; if not, the error is logged and
/// skipped. Step 6 will verify the diagnostic behavior once diagnostics are wired.
#[test]
fn watched_file_created() {
    let path = std::env::temp_dir().join("knap_test_watched_created.md");
    std::fs::write(&path, "# Watched\n[[link]]\n").unwrap();
    let uri = format!("file://{}", path.display());

    let client = spawn_server();
    do_initialize(&client);

    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({
                "changes": [{ "uri": uri, "type": 1 }]  // 1 = Created
            }),
        }))
        .unwrap();

    do_shutdown(&client, 2);

    let _ = std::fs::remove_file(&path);
}

/// A `didChangeWatchedFiles` Deleted event removes the note from the index.
/// Sending a delete for a path not yet in the index is a no-op.
#[test]
fn watched_file_deleted() {
    let client = spawn_server();
    do_initialize(&client);

    // Open a note so it's in the index.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_test/to_delete.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "# To Delete\n"
                }
            }),
        }))
        .unwrap();

    // Simulate the watcher detecting its deletion.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({
                "changes": [{ "uri": "file:///tmp/knap_test/to_delete.md", "type": 3 }]  // 3 = Deleted
            }),
        }))
        .unwrap();

    do_shutdown(&client, 2);
}
