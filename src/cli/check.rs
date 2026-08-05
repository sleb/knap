use std::thread;

/// In-process LSP smoke test: acts as a minimal LSP client against the server
/// running in a background thread. Verifies the full lifecycle without needing
/// a real editor.
///
/// Exit code is non-zero if any check fails, so this can be used in CI.
pub fn run() -> anyhow::Result<()> {
    use lsp_server::{Connection, Message, Notification, Request};
    use lsp_types::{
        DidChangeWatchedFilesRegistrationOptions, GlobPattern, RegistrationParams,
        TextDocumentSyncCapability, TextDocumentSyncKind,
    };
    use serde_json::json;

    println!("knap check");
    println!();

    let (server_conn, client_conn) = Connection::memory();
    thread::spawn(move || {
        if let Err(e) = crate::server::run(server_conn) {
            eprintln!("[server] {e}");
        }
    });

    let mut pass: usize = 0;
    let mut fail: usize = 0;

    macro_rules! check {
        ($label:expr, $cond:expr, $detail:expr) => {{
            let detail = $detail;
            if $cond {
                println!("  [ok]   {}: {}", $label, detail);
                pass += 1;
            } else {
                println!("  [FAIL] {}: {}", $label, detail);
                fail += 1;
            }
        }};
    }

    // ── initialize ────────────────────────────────────────────────────────────
    // A real `workspaceFolders` entry (the cwd) is required so the server has
    // a root to register a file watcher against — without one, the
    // registerCapability check below is guaranteed to fail regardless of
    // whether watcher registration actually works.

    let cwd = std::env::current_dir()?;
    let cwd_uri = url::Url::from_file_path(&cwd)
        .map_err(|_| anyhow::anyhow!("cwd is not a valid file:// URI: {}", cwd.display()))?;

    client_conn
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(1i32),
            method: "initialize".to_string(),
            params: json!({
                "capabilities": {},
                "clientInfo": {
                    "name": "knap-check",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "workspaceFolders": [{
                    "uri": cwd_uri.as_str(),
                    "name": "knap-check"
                }]
            }),
        }))?;

    let init_resp = recv_response(&client_conn)?;
    check!("initialize", init_resp.error.is_none(), "ok");

    let init_result: lsp_types::InitializeResult =
        serde_json::from_value(init_resp.result.unwrap_or_default())?;
    let caps = &init_result.capabilities;

    if let Some(info) = &init_result.server_info {
        let version = info.version.as_deref().unwrap_or("?");
        println!("         server: {} {}", info.name, version);
    }

    let sync_full = matches!(
        &caps.text_document_sync,
        Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
    );
    check!("textDocumentSync", sync_full, "Full");

    let trigger_ok = caps
        .completion_provider
        .as_ref()
        .and_then(|c| c.trigger_characters.as_ref())
        .map(|ts| ts.contains(&"(".to_string()))
        .unwrap_or(false);
    check!("completion trigger", trigger_ok, r#""(""#);

    check!("definition provider", caps.definition_provider.is_some(), "advertised");
    check!("references provider", caps.references_provider.is_some(), "advertised");

    // ── initialized → registerCapability ─────────────────────────────────────

    client_conn
        .sender
        .send(Message::Notification(Notification {
            method: "initialized".to_string(),
            params: json!({}),
        }))?;

    // Server should immediately send client/registerCapability.
    match client_conn.receiver.recv()? {
        Message::Request(req) if req.method == "client/registerCapability" => {
            let params: RegistrationParams = serde_json::from_value(req.params)?;
            let globs: Vec<String> = params
                .registrations
                .iter()
                .filter_map(|r| r.register_options.clone())
                .filter_map(|v| {
                    serde_json::from_value::<DidChangeWatchedFilesRegistrationOptions>(v).ok()
                })
                .flat_map(|o| o.watchers)
                .map(|w| match w.glob_pattern {
                    GlobPattern::String(s) => s,
                    _ => "?".to_string(),
                })
                .collect();
            check!(
                "registerCapability",
                !globs.is_empty(),
                format!("watchers: {}", globs.join(", "))
            );
        }
        msg => {
            println!("  [FAIL] registerCapability: unexpected {:?}", msg);
            fail += 1;
        }
    }

    // ── graceful-degradation requests ────────────────────────────────────────
    // These send `{}` as params (invalid for all three methods) to verify that
    // deserialization failure is handled gracefully rather than panicking.
    // Handler correctness is covered by the integration tests in tests/.

    for (id, method) in [
        (2i32, "textDocument/completion"),
        (3i32, "textDocument/definition"),
        (4i32, "textDocument/references"),
    ] {
        client_conn
            .sender
            .send(Message::Request(Request {
                id: lsp_server::RequestId::from(id),
                method: method.to_string(),
                params: json!({}),
            }))?;

        let resp = recv_response(&client_conn)?;
        let null_result =
            resp.error.is_none() && resp.result == Some(serde_json::Value::Null);
        check!(method, null_result, "null (graceful on bad params)");
    }

    // ── unknown method ────────────────────────────────────────────────────────

    client_conn
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(5i32),
            method: "workspace/unknownMethod".to_string(),
            params: json!({}),
        }))?;

    let unk = recv_response(&client_conn)?;
    check!(
        "unknown method",
        unk.error.is_none() && unk.result == Some(serde_json::Value::Null),
        "null (not an error)"
    );

    // ── shutdown ──────────────────────────────────────────────────────────────

    client_conn
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(6i32),
            method: "shutdown".to_string(),
            params: json!(null),
        }))?;

    let shut = recv_response(&client_conn)?;
    check!("shutdown", shut.error.is_none(), "clean");

    let _ = client_conn.sender.send(Message::Notification(Notification {
        method: "exit".to_string(),
        params: json!(null),
    }));

    // ── summary ───────────────────────────────────────────────────────────────

    println!();
    println!("{} passed, {} failed", pass, fail);

    if fail > 0 {
        anyhow::bail!("{} check(s) failed", fail);
    }

    Ok(())
}

/// Receive the next Response from `conn`, skipping server-initiated Requests
/// (e.g. client/registerCapability) or Notifications that arrive on the channel.
fn recv_response(conn: &lsp_server::Connection) -> anyhow::Result<lsp_server::Response> {
    let mut skipped = 0u32;
    loop {
        match conn.receiver.recv()? {
            lsp_server::Message::Response(r) => return Ok(r),
            lsp_server::Message::Request(_) | lsp_server::Message::Notification(_) => {
                skipped += 1;
                if skipped > 32 {
                    anyhow::bail!("recv_response: server sent >32 non-response messages");
                }
            }
        }
    }
}
