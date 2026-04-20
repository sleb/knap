use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::Hover;
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

fn request_hover(
    client: &Connection,
    request_id: i32,
    uri: &str,
    line: u32,
    character: u32,
) -> Option<Hover> {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(request_id),
            method: "textDocument/hover".to_string(),
            params: json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        }))
        .unwrap();

    let resp = recv_response(client, lsp_server::RequestId::from(request_id));
    assert!(resp.error.is_none(), "hover returned error: {:?}", resp.error);
    let value = resp.result.unwrap_or(json!(null));
    if value.is_null() {
        None
    } else {
        Some(serde_json::from_value(value).expect("deserialize Hover"))
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Hover on `[[note]]` → MarkupContent containing the note's title and body.
#[test]
fn hover_wiki_link_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(
        &client,
        "file:///tmp/knap_hover/target.md",
        "---\ntitle: Target Note\n---\nThis is the body.\n",
    );
    // cursor.md links to target; `[[target]]` spans cols 0–10 on line 0.
    open_note(&client, "file:///tmp/knap_hover/cursor.md", "[[target]]");

    let hover = request_hover(&client, 2, "file:///tmp/knap_hover/cursor.md", 0, 3)
        .expect("expected a hover result");

    let lsp_types::HoverContents::Markup(mc) = hover.contents else {
        panic!("expected Markup hover contents");
    };
    assert_eq!(mc.kind, lsp_types::MarkupKind::Markdown);
    assert!(mc.value.contains("**Target Note**"), "title missing: {}", mc.value);
    assert!(mc.value.contains("This is the body."), "body missing: {}", mc.value);

    do_shutdown(&client, 3);
}

/// Hover on plain text (not on any link) → null response.
#[test]
fn hover_no_link_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(&client, "file:///tmp/knap_hover/plain.md", "just plain text");

    let result = request_hover(&client, 2, "file:///tmp/knap_hover/plain.md", 0, 5);
    assert!(result.is_none(), "expected null hover on plain text");

    do_shutdown(&client, 3);
}

/// Hover on a local Markdown link `[text](./other.md)` → note preview.
#[test]
fn hover_md_link_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    open_note(
        &client,
        "file:///tmp/knap_hover2/other.md",
        "---\ntitle: Linked Note\n---\nLinked body content.\n",
    );
    // "[text](./other.md)" — cursor at (0, 5) is inside the link span.
    open_note(&client, "file:///tmp/knap_hover2/cursor.md", "[text](./other.md)");

    let hover = request_hover(&client, 2, "file:///tmp/knap_hover2/cursor.md", 0, 5)
        .expect("expected hover for local md link");

    let lsp_types::HoverContents::Markup(mc) = hover.contents else {
        panic!("expected Markup hover contents");
    };
    assert!(mc.value.contains("**Linked Note**"), "title missing: {}", mc.value);
    assert!(mc.value.contains("Linked body content."), "body missing: {}", mc.value);

    do_shutdown(&client, 3);
}

/// Hover on an external URL link `[text](https://…)` → formatted link string.
#[test]
fn hover_external_url_round_trip() {
    let client = spawn_server();
    do_initialize(&client);

    // "[visit](https://example.com)" — cursor at (0, 5) is inside the link.
    open_note(&client, "file:///tmp/knap_hover2/ext.md", "[visit](https://example.com)");

    let hover = request_hover(&client, 2, "file:///tmp/knap_hover2/ext.md", 0, 5)
        .expect("expected hover for external URL");

    let lsp_types::HoverContents::Markup(mc) = hover.contents else {
        panic!("expected Markup hover contents");
    };
    assert_eq!(mc.value, "[visit](https://example.com)");

    do_shutdown(&client, 3);
}
