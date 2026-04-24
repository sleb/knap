use std::thread;

use lsp_server::{Connection, Message, Notification, Request, Response};
use serde_json::{json, Value};

/// Run the server in a background thread; return the client-side connection.
fn spawn_server() -> Connection {
    let (server_conn, client_conn) = Connection::memory();
    thread::spawn(move || {
        knap::server::run(server_conn).expect("server error");
    });
    client_conn
}

/// Perform the initialize / initialized handshake. Returns the parsed InitializeResult.
/// After this returns the server is in Running state and may have already sent a
/// client/registerCapability request into the channel.
fn do_initialize(client: &Connection) -> lsp_types::InitializeResult {
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

    let resp = recv_response(client, lsp_server::RequestId::from(1i32));
    let init_result: lsp_types::InitializeResult =
        serde_json::from_value(resp.result.unwrap()).unwrap();

    client
        .sender
        .send(Message::Notification(Notification {
            method: "initialized".to_string(),
            params: json!({}),
        }))
        .unwrap();

    init_result
}

/// Perform a clean shutdown: send shutdown request, wait for null response, send exit.
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

    // Server may have already exited; ignore send failure.
    let _ = client.sender.send(Message::Notification(Notification {
        method: "exit".to_string(),
        params: json!(null),
    }));
}

/// Receive the response matching `expected_id`, skipping server-initiated requests
/// (e.g. client/registerCapability) that arrive on the same channel.
fn recv_response(client: &Connection, expected_id: lsp_server::RequestId) -> Response {
    loop {
        match client.receiver.recv().expect("channel closed unexpectedly") {
            Message::Response(r) if r.id == expected_id => return r,
            Message::Response(r) => panic!("unexpected response id {:?}", r.id),
            Message::Request(_) => {} // server-initiated request — skip
            Message::Notification(_) => {} // server notification — skip
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn initialize_shutdown() {
    let client = spawn_server();
    do_initialize(&client);
    do_shutdown(&client, 2);
}

#[test]
fn capabilities_advertised() {
    let client = spawn_server();
    let init_result = do_initialize(&client);

    let caps = init_result.capabilities;

    let completion = caps.completion_provider.expect("completion_provider missing");
    let triggers = completion.trigger_characters.expect("no trigger_characters");
    assert!(
        triggers.contains(&"[".to_string()),
        "expected '[' trigger, got {:?}",
        triggers
    );

    assert!(caps.definition_provider.is_some(), "definition_provider missing");
    assert!(caps.references_provider.is_some(), "references_provider missing");
    assert!(caps.code_action_provider.is_some(), "code_action_provider missing");

    do_shutdown(&client, 2);
}

#[test]
fn unknown_request_returns_null() {
    let client = spawn_server();
    do_initialize(&client);

    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(2i32),
            method: "unknownMethod/doSomething".to_string(),
            params: json!({}),
        }))
        .unwrap();

    let resp = recv_response(&client, lsp_server::RequestId::from(2i32));
    assert!(resp.error.is_none(), "expected null result, got error: {:?}", resp.error);
    assert_eq!(resp.result, Some(Value::Null));

    do_shutdown(&client, 3);
}
