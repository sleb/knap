use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::{DiagnosticSeverity, PublishDiagnosticsParams};
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

    // Drain the server-initiated client/registerCapability request.
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

/// Send a dummy completion request and collect all `textDocument/publishDiagnostics`
/// notifications that arrive before its response. Because the server processes
/// messages in order, any diagnostics published by prior notifications will
/// already be in the channel before the response arrives.
fn sync_and_collect_diagnostics(
    client: &Connection,
    sync_id: i32,
) -> Vec<PublishDiagnosticsParams> {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(sync_id),
            method: "textDocument/completion".to_string(),
            params: json!({
                "textDocument": { "uri": "file:///sync" },
                "position": { "line": 0, "character": 0 }
            }),
        }))
        .unwrap();

    let mut all_diags = vec![];
    loop {
        match client.receiver.recv().unwrap() {
            Message::Response(r) if r.id == lsp_server::RequestId::from(sync_id) => break,
            Message::Notification(n) if n.method == "textDocument/publishDiagnostics" => {
                if let Ok(p) = serde_json::from_value::<PublishDiagnosticsParams>(n.params) {
                    all_diags.push(p);
                }
            }
            _ => {}
        }
    }
    all_diags
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Opening a file with a broken wiki-link publishes a WARNING diagnostic.
#[test]
fn broken_link_produces_warning() {
    let client = spawn_server();
    do_initialize(&client);

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/a.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[missing]]\n"
                }
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .find(|d| d.uri.as_str().ends_with("a.md"))
        .expect("no diagnostics published for a.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));

    do_shutdown(&client, 3);
}

/// Opening a file whose link resolves to an indexed note publishes no diagnostic.
#[test]
fn valid_link_no_diagnostic() {
    let client = spawn_server();
    do_initialize(&client);

    // Index the target first.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/b.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "# B\n"
                }
            }),
        }))
        .unwrap();

    // Open a file that links to b.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/links_to_b.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[b]]\n"
                }
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    // The last published diagnostics for links_to_b.md should be empty.
    let file_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("links_to_b.md"))
        .collect();
    let last = file_diags.last().expect("no diagnostics published for links_to_b.md");
    assert!(last.diagnostics.is_empty(), "expected no diagnostics, got {:?}", last.diagnostics);

    do_shutdown(&client, 3);
}

/// Two notes with the same stem produce an AMBIGUOUS warning on any link to that stem.
#[test]
fn ambiguous_link_produces_warning() {
    let client = spawn_server();
    do_initialize(&client);

    // Two notes with the same stem "dup".
    for uri in ["file:///tmp/knap_diag/sub1/dup.md", "file:///tmp/knap_diag/sub2/dup.md"] {
        client
            .sender
            .send(Message::Notification(Notification {
                method: "textDocument/didOpen".to_string(),
                params: json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": "markdown",
                        "version": 1,
                        "text": "# Dup\n"
                    }
                }),
            }))
            .unwrap();
    }

    // A note that links to the ambiguous stem.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/links_to_dup.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[dup]]\n"
                }
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("links_to_dup.md"))
        .collect();
    let last = file_diags.last().expect("no diagnostics published for links_to_dup.md");
    assert_eq!(last.diagnostics.len(), 1);
    assert_eq!(last.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));

    do_shutdown(&client, 3);
}

/// After the missing target is created, the diagnostic clears.
#[test]
fn diagnostic_clears_on_fix() {
    let target = std::env::temp_dir().join("knap_diag_fix_target.md");
    std::fs::write(&target, "# Target\n").unwrap();
    let target_uri = format!("file://{}", target.display());

    let client = spawn_server();
    do_initialize(&client);

    // Open a file with a broken link.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/wants_target.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[knap_diag_fix_target]]\n"
                }
            }),
        }))
        .unwrap();

    // Confirm the diagnostic is present.
    let diags = sync_and_collect_diagnostics(&client, 2);
    let broken = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("wants_target.md"))
        .last()
        .expect("no diagnostics for wants_target.md");
    assert_eq!(broken.diagnostics.len(), 1, "expected broken-link diagnostic before fix");

    // Simulate the watcher seeing the new file.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({ "changes": [{ "uri": target_uri, "type": 1 }] }),
        }))
        .unwrap();

    // The diagnostic for wants_target.md should now be empty.
    let diags = sync_and_collect_diagnostics(&client, 3);
    let cleared = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("wants_target.md"))
        .last()
        .expect("no diagnostics published for wants_target.md after fix");
    assert!(
        cleared.diagnostics.is_empty(),
        "expected diagnostic to clear after target created, got {:?}",
        cleared.diagnostics
    );

    let _ = std::fs::remove_file(&target);
    do_shutdown(&client, 4);
}

/// The diagnostic range covers only the stem text, not the `[[` `]]` brackets.
#[test]
fn diagnostic_range_is_stem_only() {
    let client = spawn_server();
    do_initialize(&client);

    // "[[my-stem]]" — stem starts at character 2, ends at character 9.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/range_test.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[my-stem]]\n"
                }
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .find(|d| d.uri.as_str().ends_with("range_test.md"))
        .expect("no diagnostics for range_test.md");
    assert_eq!(file_diags.diagnostics.len(), 1);

    let range = file_diags.diagnostics[0].range;
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 2, "stem should start after [[");
    assert_eq!(range.end.line, 0);
    assert_eq!(range.end.character, 9, "stem should end before ]]");

    do_shutdown(&client, 3);
}

/// A `[[image.png]]` link with no matching file in the index → warning.
#[test]
fn attachment_link_absent_diagnostic() {
    let client = spawn_server();
    do_initialize(&client);

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/has_png_link.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[missing_image.png]]\n"
                }
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .find(|d| d.uri.as_str().ends_with("has_png_link.md"))
        .expect("no diagnostics published for has_png_link.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(file_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));

    do_shutdown(&client, 3);
}

/// A `[[image.png]]` link clears once the file is registered as an attachment.
#[test]
fn attachment_link_present_no_diagnostic() {
    let client = spawn_server();
    do_initialize(&client);

    // Open a note with a broken attachment link.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/wants_png.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[logo.png]]\n"
                }
            }),
        }))
        .unwrap();

    // Simulate the watcher seeing a new attachment file.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({ "changes": [{ "uri": "file:///tmp/knap_diag/logo.png", "type": 1 }] }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("wants_png.md"))
        .collect();
    let last = file_diags.last().expect("no diagnostics published for wants_png.md");
    assert!(
        last.diagnostics.is_empty(),
        "expected no diagnostics after attachment registered, got {:?}",
        last.diagnostics
    );

    do_shutdown(&client, 3);
}

/// Broken note link message uses the new link-agnostic wording.
#[test]
fn diagnostic_message_broken() {
    let client = spawn_server();
    do_initialize(&client);

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/msg_broken.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[ghost]]\n"
                }
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags = diags
        .iter()
        .find(|d| d.uri.as_str().ends_with("msg_broken.md"))
        .expect("no diagnostics for msg_broken.md");
    assert_eq!(file_diags.diagnostics.len(), 1);
    assert_eq!(
        file_diags.diagnostics[0].message,
        "Link target not found: '[[ghost]]'"
    );

    do_shutdown(&client, 3);
}

/// Ambiguous stem message uses the new link-agnostic wording.
#[test]
fn diagnostic_message_ambiguous() {
    let client = spawn_server();
    do_initialize(&client);

    for uri in [
        "file:///tmp/knap_diag/msg_amb_dir1/shared.md",
        "file:///tmp/knap_diag/msg_amb_dir2/shared.md",
    ] {
        client
            .sender
            .send(Message::Notification(Notification {
                method: "textDocument/didOpen".to_string(),
                params: json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": "markdown",
                        "version": 1,
                        "text": "# Shared\n"
                    }
                }),
            }))
            .unwrap();
    }

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/msg_amb_linker.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[shared]]\n"
                }
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("msg_amb_linker.md"))
        .collect();
    let last = file_diags.last().expect("no diagnostics for msg_amb_linker.md");
    assert_eq!(last.diagnostics.len(), 1);
    assert!(
        last.diagnostics[0].message.starts_with("'[[shared]]' matches multiple files:"),
        "unexpected message: {}",
        last.diagnostics[0].message
    );

    do_shutdown(&client, 3);
}

/// `[[note#Nonexistent]]` where note exists but has no matching heading → Warning.
#[test]
fn broken_anchor_diagnostic() {
    let client = spawn_server();
    do_initialize(&client);

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/anchor_target.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "## Real Heading\n"
                }
            }),
        }))
        .unwrap();

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/anchor_src.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[anchor_target#Nonexistent]]\n"
                }
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> =
        diags.iter().filter(|d| d.uri.as_str().ends_with("anchor_src.md")).collect();
    let last = file_diags.last().expect("no diagnostics published for anchor_src.md");
    assert_eq!(last.diagnostics.len(), 1);
    assert_eq!(last.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(
        last.diagnostics[0].message,
        "Heading not found: '#Nonexistent' in '[[anchor_target#Nonexistent]]'"
    );

    do_shutdown(&client, 3);
}

/// `[[note#Real Heading]]` where the heading exists → no anchor diagnostic.
#[test]
fn valid_anchor_no_diagnostic() {
    let client = spawn_server();
    do_initialize(&client);

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/valid_anchor_target.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "## Real Heading\n"
                }
            }),
        }))
        .unwrap();

    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/valid_anchor_src.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[valid_anchor_target#Real Heading]]\n"
                }
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 2);
    let file_diags: Vec<_> =
        diags.iter().filter(|d| d.uri.as_str().ends_with("valid_anchor_src.md")).collect();
    let last = file_diags.last().expect("no diagnostics published for valid_anchor_src.md");
    assert!(
        last.diagnostics.is_empty(),
        "expected no diagnostic for valid anchor, got {:?}",
        last.diagnostics
    );

    do_shutdown(&client, 3);
}

/// Deleting a linked file publishes a diagnostic in the file that linked to it.
#[test]
fn cascade_on_delete() {
    let client = spawn_server();
    do_initialize(&client);

    // Open source and target so the link resolves.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/source.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "[[cascade_target]]\n"
                }
            }),
        }))
        .unwrap();
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": "file:///tmp/knap_diag/cascade_target.md",
                    "languageId": "markdown",
                    "version": 1,
                    "text": "# Target\n"
                }
            }),
        }))
        .unwrap();

    // Drain diagnostics from the two didOpen events.
    sync_and_collect_diagnostics(&client, 2);

    // Now delete the target.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({
                "changes": [{
                    "uri": "file:///tmp/knap_diag/cascade_target.md",
                    "type": 3  // Deleted
                }]
            }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 3);
    let source_diags = diags
        .iter()
        .filter(|d| d.uri.as_str().ends_with("source.md"))
        .last()
        .expect("no diagnostics published for source.md after cascade delete");
    assert_eq!(
        source_diags.diagnostics.len(),
        1,
        "expected broken-link warning after target deleted"
    );
    assert_eq!(source_diags.diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));

    do_shutdown(&client, 4);
}
