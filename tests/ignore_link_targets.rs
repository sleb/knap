// v0.20 integration tests — `ignore-link-targets` frontmatter key,
// `knap.toml`'s `ignore_link_targets` field, and the `--ignore-link-target`
// flag, over the CLI process boundary and the full in-process LSP message
// loop. See docs/design/releases/v0.20/ignore-link-targets/plan.md Step 4.

use std::process::Command;
use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::PublishDiagnosticsParams;
use serde_json::json;

// ─── CLI helpers (mirrors tests/exclude.rs conventions) ────────────────────

fn knap() -> Command {
    Command::new(env!("CARGO_BIN_EXE_knap"))
}

// ─── LSP helpers (mirrors tests/exclude.rs conventions) ────────────────────

fn spawn_server() -> Connection {
    let (server_conn, client_conn) = Connection::memory();
    thread::spawn(move || {
        knap::server_run(server_conn).expect("server error");
    });
    client_conn
}

fn do_initialize_with_workspace(client: &Connection, workspace_uri: &str) {
    client
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(1i32),
            method: "initialize".to_string(),
            params: json!({
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "0.0.1" },
                "workspaceFolders": [{ "uri": workspace_uri, "name": "vault" }]
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
    assert!(
        resp.response_result.is_ok(),
        "shutdown returned error: {:?}",
        resp.response_result
    );

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

fn did_open(client: &Connection, uri: &str, text: &str) {
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didOpen".to_string(),
            params: json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "markdown",
                    "version": 1,
                    "text": text
                }
            }),
        }))
        .unwrap();
}

fn did_change(client: &Connection, uri: &str, text: &str) {
    client
        .sender
        .send(Message::Notification(Notification {
            method: "textDocument/didChange".to_string(),
            params: json!({
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": text }]
            }),
        }))
        .unwrap();
}

/// Send a cheap request and collect all `textDocument/publishDiagnostics`
/// notifications that arrive before its response — the same technique
/// `tests/lsp.rs`/`tests/exclude.rs` use to drain diagnostics published as a
/// side effect of an earlier notification (didOpen/didChange/initialize's
/// own crawl).
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

// ─── CLI tests ──────────────────────────────────────────────────────────────

#[test]
fn lint_frontmatter_ignore_link_targets_suppresses_diagnostic() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(
        dir.path().join("note.md"),
        "---\nignore-link-targets: [missing/x.md]\n---\n[link](missing/x.md)\n",
    )
    .expect("write note.md");

    let output = knap()
        .args(["lint", ".", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    assert_eq!(
        value["problem_count"].as_u64(),
        Some(0),
        "frontmatter ignore-link-targets should suppress the diagnostic: {stdout}"
    );
}

#[test]
fn lint_knap_toml_ignore_link_targets_suppresses_across_docs() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(
        dir.path().join("knap.toml"),
        "ignore_link_targets = [\"sibling/**\"]\n",
    )
    .expect("write knap.toml");
    std::fs::write(dir.path().join("a.md"), "[link](sibling/x.md)\n").expect("write a.md");
    std::fs::write(dir.path().join("b.md"), "[link](sibling/y.md)\n").expect("write b.md");

    let output = knap()
        .args(["lint", ".", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    assert_eq!(
        value["problem_count"].as_u64(),
        Some(0),
        "knap.toml ignore_link_targets should suppress the diagnostic in both docs: {stdout}"
    );
}

#[test]
fn lint_ignore_link_target_flag_adds_to_config() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(
        dir.path().join("knap.toml"),
        "ignore_link_targets = [\"ignored/**\"]\n",
    )
    .expect("write knap.toml");
    std::fs::write(
        dir.path().join("note.md"),
        "[ignored](ignored/x.md)\n\n[flagged](flagged/y.md)\n",
    )
    .expect("write note.md");

    // Without --ignore-link-target: knap.toml's own `ignore_link_targets`
    // covers `ignored/x.md`, but `flagged/y.md` is still flagged — proves
    // the fixture actually has a violation for --ignore-link-target to
    // suppress below, not that the directory was already clean.
    let without_flag = knap()
        .args(["lint", ".", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        !without_flag.status.success(),
        "expected flagged/y.md to be reported without --ignore-link-target"
    );
    let without_stdout = String::from_utf8_lossy(&without_flag.stdout);
    let without_value: serde_json::Value =
        serde_json::from_str(&without_stdout).expect("stdout was not JSON");
    assert_eq!(
        without_value["problem_count"].as_u64(),
        Some(1),
        "stdout was: {without_stdout}"
    );

    // With --ignore-link-target "flagged/**": both knap.toml's pattern and
    // the flag's pattern apply, so nothing is left to flag.
    let with_flag = knap()
        .args(["lint", ".", "--ignore-link-target", "flagged/**", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        with_flag.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&with_flag.stdout)
    );
    let with_stdout = String::from_utf8_lossy(&with_flag.stdout);
    let with_value: serde_json::Value =
        serde_json::from_str(&with_stdout).expect("stdout was not JSON");
    assert_eq!(
        with_value["problem_count"].as_u64(),
        Some(0),
        "stdout was: {with_stdout}"
    );
}

#[test]
fn lint_ignore_link_targets_does_not_affect_other_broken_links() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(
        dir.path().join("knap.toml"),
        "ignore_link_targets = [\"ignored/**\"]\n",
    )
    .expect("write knap.toml");
    std::fs::write(
        dir.path().join("note.md"),
        "[ignored](ignored/x.md)\n\n[really broken](nowhere.md)\n",
    )
    .expect("write note.md");

    let output = knap()
        .args(["lint", ".", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        !output.status.success(),
        "expected nowhere.md to be flagged"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    assert_eq!(
        value["problem_count"].as_u64(),
        Some(1),
        "only the genuinely broken link should be reported: {stdout}"
    );
    let messages: Vec<&str> = value["diagnostics"]
        .as_array()
        .expect("diagnostics present")
        .iter()
        .flat_map(|f| f["diagnostics"].as_array().expect("diagnostics present"))
        .map(|d| d["message"].as_str().expect("message present"))
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("nowhere.md")),
        "messages were: {messages:?}"
    );
    assert!(
        messages.iter().all(|m| !m.contains("ignored/x.md")),
        "messages were: {messages:?}"
    );
}

#[test]
fn index_json_still_reports_ignored_link_as_unresolved() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(
        dir.path().join("knap.toml"),
        "ignore_link_targets = [\"ignored/**\"]\n",
    )
    .expect("write knap.toml");
    std::fs::write(dir.path().join("note.md"), "[ignored](ignored/x.md)\n").expect("write note.md");

    let output = knap()
        .args(["index", ".", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run knap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");

    let notes = value["notes"].as_array().expect("notes present");
    let note = notes
        .iter()
        .find(|n| n["path"].as_str().unwrap_or("").ends_with("note.md"))
        .expect("note.md present");
    let links = note["links"].as_array().expect("links present");
    let ignored_link = links
        .iter()
        .find(|l| l["target"].as_str() == Some("ignored/x.md"))
        .expect("ignored/x.md link present");
    assert!(
        ignored_link["resolved"].is_null(),
        "ignoring a link target is diagnostics-only — the index should still \
         report it as unresolved: {ignored_link:?}"
    );
}

// ─── LSP test ───────────────────────────────────────────────────────────────

/// An in-process LSP session started against a vault whose `knap.toml` sets
/// `ignore_link_targets` never publishes a `broken-link` diagnostic for a
/// matching target — not on the initial crawl, and not over the course of
/// the session as the note is edited.
#[test]
fn lsp_initialize_applies_knap_toml_ignore_link_targets() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(
        dir.path().join("knap.toml"),
        "ignore_link_targets = [\"ignored/**\"]\n",
    )
    .expect("write knap.toml");
    std::fs::write(dir.path().join("note.md"), "[ignored](ignored/x.md)\n").expect("write note.md");

    let workspace_uri = url::Url::from_file_path(dir.path())
        .expect("valid file URL")
        .to_string();
    let note_uri = format!("{workspace_uri}/note.md");

    let client = spawn_server();
    do_initialize_with_workspace(&client, &workspace_uri);

    // Initial crawl: the link matches ignore_link_targets, so no
    // broken-link diagnostic is published for it.
    let diags = sync_and_collect_diagnostics(&client, 2);
    assert!(
        diags.iter().all(|d| d
            .diagnostics
            .iter()
            .all(|diag| !diag.message.contains("ignored/x.md"))),
        "ignored target should never receive a diagnostic after initial crawl: {diags:?}"
    );

    // Re-open and re-save the note — the suppression must hold across the
    // rest of the session, not just the initial crawl.
    did_open(&client, &note_uri, "[ignored](ignored/x.md)\n");
    let diags = sync_and_collect_diagnostics(&client, 3);
    assert!(
        diags.iter().all(|d| d
            .diagnostics
            .iter()
            .all(|diag| !diag.message.contains("ignored/x.md"))),
        "ignored target should never receive a diagnostic after didOpen: {diags:?}"
    );

    did_change(
        &client,
        &note_uri,
        "[ignored](ignored/x.md)\n\nSome edited prose.\n",
    );
    let diags = sync_and_collect_diagnostics(&client, 4);
    assert!(
        diags.iter().all(|d| d
            .diagnostics
            .iter()
            .all(|diag| !diag.message.contains("ignored/x.md"))),
        "ignored target should never receive a diagnostic after didChange: {diags:?}"
    );

    do_shutdown(&client, 5);
}
