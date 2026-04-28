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

fn request_code_lens(client: &Connection, request_id: i32, uri: &str) -> Vec<serde_json::Value> {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(request_id),
            method: "textDocument/codeLens".to_string(),
            params: json!({ "textDocument": { "uri": uri } }),
        }))
        .unwrap();
    let resp = recv_response(client, lsp_server::RequestId::from(request_id));
    assert!(resp.error.is_none(), "codeLens returned error: {:?}", resp.error);
    serde_json::from_value(resp.result.unwrap_or(json!([]))).unwrap()
}

/// Note with 2 inbound links → one lens with the correct plural title.
#[test]
fn code_lens_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/target.md", "# Target\n");
    did_open(&client, "file:///vault/a.md", "[[target]]\n");
    did_open(&client, "file:///vault/b.md", "[[target]]\n");

    let lenses = request_code_lens(&client, 2, "file:///vault/target.md");

    assert_eq!(lenses.len(), 1, "expected exactly one lens");
    assert_eq!(lenses[0]["command"]["title"], "↑ 2 backlinks");

    do_shutdown(&client, 3);
}

/// Note with no inbound links → one lens showing "↑ 0 backlinks" (feature is visibly active).
#[test]
fn code_lens_zero_backlinks_shown() {
    let client = spawn_server();
    do_initialize(&client);

    did_open(&client, "file:///vault/orphan.md", "# Orphan\n");

    let lenses = request_code_lens(&client, 2, "file:///vault/orphan.md");

    assert_eq!(lenses.len(), 1, "expected exactly one lens even with zero backlinks");
    assert_eq!(lenses[0]["command"]["title"], "↑ 0 backlinks");

    do_shutdown(&client, 3);
}
