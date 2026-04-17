use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::DocumentSymbol;
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

fn request_document_symbols(
    client: &Connection,
    request_id: i32,
    uri: &str,
) -> Vec<DocumentSymbol> {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(request_id),
            method: "textDocument/documentSymbol".to_string(),
            params: json!({ "textDocument": { "uri": uri } }),
        }))
        .unwrap();

    let resp = recv_response(client, lsp_server::RequestId::from(request_id));
    assert!(resp.error.is_none(), "documentSymbol returned error: {:?}", resp.error);
    let result = resp.result.unwrap_or(json!([]));
    serde_json::from_value(result).expect("deserialize Vec<DocumentSymbol>")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Request for a file with 3 headings → correct headings returned by the server.
#[test]
fn document_symbols_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(
        &client,
        "file:///tmp/knap_sym/outline.md",
        "# Title\n\n## Section\n\nSome prose.\n\n### Subsection\n",
    );

    let symbols = request_document_symbols(&client, 2, "file:///tmp/knap_sym/outline.md");
    assert_eq!(symbols.len(), 3, "expected 3 symbols, got {:?}", symbols);
    assert_eq!(symbols[0].name, "Title");
    assert_eq!(symbols[1].name, "Section");
    assert_eq!(symbols[2].name, "Subsection");

    do_shutdown(&client, 3);
}
