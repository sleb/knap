// v0.16 integration tests — `exclude` (`knap.toml` field + `--exclude`
// flag) over the CLI process boundary and the full in-process LSP message
// loop. See docs/design/releases/v0.16/exclude-paths/{design,plan}.md.

use std::process::Command;
use std::thread;

use lsp_server::{Connection, Message, Notification, Request};
use lsp_types::PublishDiagnosticsParams;
use serde_json::json;

// ─── CLI helpers (mirrors tests/cli.rs conventions) ────────────────────────────

fn knap() -> Command {
    Command::new(env!("CARGO_BIN_EXE_knap"))
}

// ─── LSP helpers (mirrors tests/lsp.rs conventions) ────────────────────────────

fn spawn_server() -> Connection {
    let (server_conn, client_conn) = Connection::memory();
    thread::spawn(move || {
        knap::server::run(server_conn).expect("server error");
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
/// `tests/lsp.rs` uses to drain diagnostics published as a side effect of
/// an earlier notification (didOpen/didChange/initialize's own crawl).
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
fn lint_excludes_configured_directory() {
    let output = knap()
        .args(["lint", "tests/fixtures/exclude_config", "--json"])
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
        "fixtures/broken.md's broken link leaked past the knap.toml exclude: {stdout}"
    );
}

#[test]
fn lint_exclude_flag_adds_to_config() {
    // Without --exclude: knap.toml's own `exclude = ["fixtures/**"]` covers
    // fixtures/broken.md, but other/broken2.md is still flagged — proves
    // the fixture actually has a violation for --exclude to suppress below,
    // not that the directory was already clean.
    let without_flag = knap()
        .args(["lint", "tests/fixtures/exclude_flag", "--json"])
        .output()
        .expect("failed to run knap");
    assert!(
        !without_flag.status.success(),
        "expected other/broken2.md to be flagged without --exclude"
    );

    // With --exclude "other/**": both knap.toml's pattern and the flag's
    // pattern apply, so nothing is left to flag.
    let with_flag = knap()
        .args([
            "lint",
            "tests/fixtures/exclude_flag",
            "--exclude",
            "other/**",
            "--json",
        ])
        .output()
        .expect("failed to run knap");
    assert!(
        with_flag.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&with_flag.stdout)
    );
    let stdout = String::from_utf8_lossy(&with_flag.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout was not JSON");
    assert_eq!(
        value["problem_count"].as_u64(),
        Some(0),
        "stdout was: {stdout}"
    );
}

#[test]
fn index_json_omits_excluded_notes() {
    let output = knap()
        .args(["index", "tests/fixtures/exclude_index", "--json"])
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
    assert_eq!(
        notes.len(),
        1,
        "expected only note.md, fixtures/hidden.md should be excluded: {notes:?}"
    );
    assert!(
        notes
            .iter()
            .all(|n| !n["path"].as_str().unwrap_or("").ends_with("hidden.md")),
        "excluded note.md leaked into the index: {notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n["path"].as_str().unwrap_or("").ends_with("note.md")),
        "the non-excluded note.md is missing: {notes:?}"
    );
}

// ─── LSP test ───────────────────────────────────────────────────────────────

/// An in-process LSP session started against a vault whose `knap.toml`
/// excludes `fixtures/**` never publishes diagnostics for the excluded
/// file's URI — not on the initial crawl, and not over the course of the
/// session as other (non-excluded) notes are opened and edited.
///
/// As of docs/design/releases/v0.16/path-filter-authority/design.md,
/// `on_did_open`/`on_did_change`/`on_did_change_watched_files` each guard on
/// `config.should_index` too, so an excluded path is now also rejected if a
/// client somehow sends a notification for it directly (see
/// `lsp_did_change_watched_files_on_excluded_path_is_ignored` below for that
/// guard's coverage). This test still never sends `didOpen`/`didChange` for
/// the excluded file itself — a real editor wouldn't, since it's hidden from
/// completions/workspace symbols/Go to Definition — and instead verifies the
/// excluded file stays invisible across an ordinary editing session of the
/// *rest* of the vault.
#[test]
fn lsp_initialize_applies_knap_toml_exclude() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("knap.toml"), "exclude = [\"fixtures/**\"]\n")
        .expect("write knap.toml");
    std::fs::write(dir.path().join("note.md"), "# Note\n").expect("write note.md");
    std::fs::create_dir(dir.path().join("fixtures")).expect("create fixtures dir");
    std::fs::write(
        dir.path().join("fixtures").join("broken.md"),
        "[broken link](missing.md)\n",
    )
    .expect("write fixtures/broken.md");

    let workspace_uri = url::Url::from_file_path(dir.path())
        .expect("valid file URL")
        .to_string();
    let note_uri = format!("{workspace_uri}/note.md");

    let client = spawn_server();
    do_initialize_with_workspace(&client, &workspace_uri);

    // Initial crawl: fixtures/broken.md is excluded, so it's never indexed
    // and never appears in any publishDiagnostics notification.
    let diags = sync_and_collect_diagnostics(&client, 2);
    assert!(
        diags.iter().all(|d| !d.uri.as_str().ends_with("broken.md")),
        "excluded file should never receive diagnostics after initial crawl: {diags:?}"
    );

    // Open and edit a real, non-excluded note — an ordinary editing
    // session elsewhere in the vault must not surface the excluded file.
    did_open(&client, &note_uri, "# Note\n");
    let diags = sync_and_collect_diagnostics(&client, 3);
    assert!(
        diags.iter().all(|d| !d.uri.as_str().ends_with("broken.md")),
        "excluded file should never receive diagnostics after didOpen elsewhere: {diags:?}"
    );

    did_change(&client, &note_uri, "# Note\n\nSome edited prose.\n");
    let diags = sync_and_collect_diagnostics(&client, 4);
    assert!(
        diags.iter().all(|d| !d.uri.as_str().ends_with("broken.md")),
        "excluded file should never receive diagnostics after didChange elsewhere: {diags:?}"
    );

    do_shutdown(&client, 5);
}

/// Reproduction for issue #68 step (b): a `didChangeWatchedFiles` event for
/// a path under `knap.toml`'s `exclude` (simulating `git checkout`, a
/// formatter, or `sed -i` touching a watched file outside the editor) must
/// not re-admit that path into the live index. Today it does, because
/// `on_did_change_watched_files` only prunes hardcoded skip-dir names
/// (`.git`, `node_modules`, `target`) via `should_skip_path` — it never
/// consults `config.exclude` at all.
#[test]
fn lsp_did_change_watched_files_on_excluded_path_is_ignored() {
    // `tempfile::tempdir()` itself produces a dot-prefixed directory name
    // (e.g. `.tmpXXXXXX`) on some platforms, and that directory is an
    // ancestor component of every path underneath it — which would trip
    // the hardcoded dotfile skip-dir check for a reason unrelated to what
    // this test verifies. Use an explicit non-dot prefix so the full path
    // has no dot-prefixed component anywhere.
    let dir = tempfile::Builder::new()
        .prefix("knap-exclude-test-")
        .tempdir()
        .expect("failed to create temp dir");
    let dir = dir.path();
    std::fs::write(dir.join("knap.toml"), "exclude = [\"fixtures/**\"]\n")
        .expect("write knap.toml");
    std::fs::write(dir.join("note.md"), "# Note\n").expect("write note.md");
    std::fs::create_dir(dir.join("fixtures")).expect("create fixtures dir");
    std::fs::write(
        dir.join("fixtures").join("broken.md"),
        "[broken link](missing.md)\n",
    )
    .expect("write fixtures/broken.md");

    let workspace_uri = url::Url::from_file_path(dir)
        .expect("valid file URL")
        .to_string();
    let broken_uri = format!("{workspace_uri}/fixtures/broken.md");

    let client = spawn_server();
    do_initialize_with_workspace(&client, &workspace_uri);

    // Initial crawl: fixtures/broken.md is excluded, so it's never indexed.
    let diags = sync_and_collect_diagnostics(&client, 2);
    assert!(
        diags.iter().all(|d| !d.uri.as_str().ends_with("broken.md")),
        "excluded file should never receive diagnostics after initial crawl: {diags:?}"
    );

    // Simulate something outside the editor (git checkout, a formatter,
    // sed -i) touching the excluded file — the watcher reports it as
    // Created.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({ "changes": [{ "uri": broken_uri, "type": 1 }] }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 3);
    assert!(
        diags.iter().all(|d| !d.uri.as_str().ends_with("broken.md")),
        "excluded file should never receive diagnostics after a \
         didChangeWatchedFiles event for it: {diags:?}"
    );

    do_shutdown(&client, 4);
}

/// A client that sends `didOpen` directly for a path under `exclude` (e.g. a
/// user manually opening an excluded file) must not have it indexed —
/// `on_did_open`'s `config.should_index` guard rejects it before it ever
/// reaches `index.index`.
#[test]
fn lsp_did_open_on_excluded_file_is_not_indexed() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("knap.toml"), "exclude = [\"fixtures/**\"]\n")
        .expect("write knap.toml");
    std::fs::write(dir.path().join("note.md"), "# Note\n").expect("write note.md");
    std::fs::create_dir(dir.path().join("fixtures")).expect("create fixtures dir");
    std::fs::write(
        dir.path().join("fixtures").join("broken.md"),
        "[broken link](missing.md)\n",
    )
    .expect("write fixtures/broken.md");

    let workspace_uri = url::Url::from_file_path(dir.path())
        .expect("valid file URL")
        .to_string();
    let broken_uri = format!("{workspace_uri}/fixtures/broken.md");

    let client = spawn_server();
    do_initialize_with_workspace(&client, &workspace_uri);

    // Drain the initial crawl's diagnostics before the assertion below.
    sync_and_collect_diagnostics(&client, 2);

    // Open the excluded file directly — it must not be indexed, so its own
    // broken link never produces a diagnostic for it.
    did_open(&client, &broken_uri, "[broken link](missing.md)\n");
    let diags = sync_and_collect_diagnostics(&client, 3);
    assert!(
        diags.iter().all(|d| !d.uri.as_str().ends_with("broken.md")),
        "excluded file should never receive diagnostics after a direct \
         didOpen for it: {diags:?}"
    );

    do_shutdown(&client, 4);
}

/// A client that sends `didChange` directly for a path under `exclude` must
/// not have it indexed — `on_did_change`'s `config.should_index` guard
/// rejects it before it ever reaches `index.index`.
#[test]
fn lsp_did_change_on_excluded_file_is_not_indexed() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("knap.toml"), "exclude = [\"fixtures/**\"]\n")
        .expect("write knap.toml");
    std::fs::write(dir.path().join("note.md"), "# Note\n").expect("write note.md");
    std::fs::create_dir(dir.path().join("fixtures")).expect("create fixtures dir");
    std::fs::write(
        dir.path().join("fixtures").join("broken.md"),
        "[broken link](missing.md)\n",
    )
    .expect("write fixtures/broken.md");

    let workspace_uri = url::Url::from_file_path(dir.path())
        .expect("valid file URL")
        .to_string();
    let broken_uri = format!("{workspace_uri}/fixtures/broken.md");

    let client = spawn_server();
    do_initialize_with_workspace(&client, &workspace_uri);

    // Drain the initial crawl's diagnostics before the assertion below.
    sync_and_collect_diagnostics(&client, 2);

    // Send didChange directly for the excluded file without a prior
    // didOpen — the guard must reject it regardless.
    did_change(&client, &broken_uri, "[broken link](missing.md)\n");
    let diags = sync_and_collect_diagnostics(&client, 3);
    assert!(
        diags.iter().all(|d| !d.uri.as_str().ends_with("broken.md")),
        "excluded file should never receive diagnostics after a direct \
         didChange for it: {diags:?}"
    );

    do_shutdown(&client, 4);
}

/// A `didChangeWatchedFiles` event for a path that is *not* excluded — a
/// sibling of an excluded directory — must still be indexed normally. This
/// is the converse of `lsp_did_change_watched_files_on_excluded_path_is_ignored`:
/// proves the `should_index` guard doesn't over-exclude paths that merely
/// share a parent with an excluded one.
#[test]
fn lsp_did_change_watched_files_admits_non_excluded_sibling() {
    let dir = tempfile::Builder::new()
        .prefix("knap-exclude-test-")
        .tempdir()
        .expect("failed to create temp dir");
    let dir = dir.path();
    std::fs::write(dir.join("knap.toml"), "exclude = [\"fixtures/**\"]\n")
        .expect("write knap.toml");
    std::fs::write(dir.join("note.md"), "# Note\n").expect("write note.md");
    std::fs::create_dir(dir.join("fixtures")).expect("create fixtures dir");
    std::fs::write(
        dir.join("fixtures").join("broken.md"),
        "[broken link](missing.md)\n",
    )
    .expect("write fixtures/broken.md");
    // A new, non-excluded sibling file created after the initial crawl.
    std::fs::write(
        dir.join("new-note.md"),
        "[broken link](also-missing.md)\n",
    )
    .expect("write new-note.md");

    let workspace_uri = url::Url::from_file_path(dir)
        .expect("valid file URL")
        .to_string();
    let new_note_uri = format!("{workspace_uri}/new-note.md");

    let client = spawn_server();
    do_initialize_with_workspace(&client, &workspace_uri);

    // Initial crawl: new-note.md was written before initialize, so it's
    // already indexed with its broken link flagged. Drain those diagnostics.
    sync_and_collect_diagnostics(&client, 2);

    // Simulate the watcher reporting new-note.md as Changed (e.g. an
    // external editor saving it) — a non-excluded path must still be
    // indexed and diagnosed normally.
    client
        .sender
        .send(Message::Notification(Notification {
            method: "workspace/didChangeWatchedFiles".to_string(),
            params: json!({ "changes": [{ "uri": new_note_uri, "type": 2 }] }),
        }))
        .unwrap();

    let diags = sync_and_collect_diagnostics(&client, 3);
    assert!(
        diags
            .iter()
            .any(|d| d.uri.as_str().ends_with("new-note.md") && !d.diagnostics.is_empty()),
        "non-excluded sibling should still be indexed and diagnosed after \
         a didChangeWatchedFiles event: {diags:?}"
    );

    do_shutdown(&client, 4);
}
