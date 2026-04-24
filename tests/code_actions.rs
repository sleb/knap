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

fn code_action(client: &Connection, request_id: i32, uri: &str, line: u32, character: u32) -> lsp_server::Response {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(request_id),
            method: "textDocument/codeAction".to_string(),
            params: json!({
                "textDocument": { "uri": uri },
                "range": {
                    "start": { "line": line, "character": character },
                    "end":   { "line": line, "character": character }
                },
                "context": { "diagnostics": [] }
            }),
        }))
        .unwrap();
    recv_response(client, lsp_server::RequestId::from(request_id))
}

/// Cursor on a valid resolved link → empty code action list.
#[test]
fn no_action_on_valid_link() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/b.md", "");
    did_open(&client, "file:///vault/a.md", "[[b]]");

    // Drain any diagnostics emitted by the didOpen events.
    let resp = code_action(&client, 2, "file:///vault/a.md", 0, 2);

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let actions: Vec<serde_json::Value> =
        serde_json::from_value(resp.result.unwrap_or(json!([]))).unwrap();
    assert!(actions.is_empty(), "expected no actions, got: {actions:?}");

    do_shutdown(&client, 3);
}
