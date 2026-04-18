use std::path::{Path, PathBuf};
use std::thread;

use crate::index::{self, ResolvedLink};
use crate::parser;

pub fn cmd_parse(args: &[String]) -> anyhow::Result<()> {
    let path = args.first().ok_or_else(|| anyhow::anyhow!("usage: knap parse <file>"))?;
    let path = Path::new(path);
    let content = std::fs::read_to_string(path)?;
    let note = parser::parse(path, &content);

    println!("path:  {}", note.path.display());
    println!("stem:  {}", note.stem);

    match &note.frontmatter {
        None => println!("title: (no frontmatter)"),
        Some(fm) => match &fm.title {
            None => println!("title: (none)"),
            Some(t) => println!("title: {t}"),
        },
    }

    if note.wiki_links.is_empty() {
        println!("links: none");
    } else {
        println!("links: {}", note.wiki_links.len());
        for link in &note.wiki_links {
            let r = &link.range;
            let ir = &link.inner_range;
            let anchor_str = match &link.anchor {
                Some(a) => format!("  anchor: #{a}"),
                None => String::new(),
            };
            println!(
                "  [[{}]]  {}:{}\u{2013}{}:{}  (inner: {}:{}\u{2013}{}:{}){}",
                link.stem,
                r.start.line,
                r.start.character,
                r.end.line,
                r.end.character,
                ir.start.line,
                ir.start.character,
                ir.end.line,
                ir.end.character,
                anchor_str,
            );
        }
    }

    if note.headings.is_empty() {
        println!("headings: none");
    } else {
        println!("headings: {}", note.headings.len());
        for h in &note.headings {
            let r = &h.range;
            let tr = &h.text_range;
            println!(
                "  h{}  \"{}\"  {}:{}\u{2013}{}:{}  (text: {}:{}\u{2013}{}:{})",
                h.level,
                h.text,
                r.start.line,
                r.start.character,
                r.end.line,
                r.end.character,
                tr.start.line,
                tr.start.character,
                tr.end.line,
                tr.end.character,
            );
        }
    }

    if note.md_links.is_empty() {
        println!("md_links: none");
    } else {
        println!("md_links: {}", note.md_links.len());
        for link in &note.md_links {
            let r = &link.range;
            let kind = if link.is_image { "image" } else { "link" };
            println!(
                "  [{kind}]  \"{}\"  →  {}  {}:{}\u{2013}{}:{}",
                link.text,
                link.target,
                r.start.line,
                r.start.character,
                r.end.line,
                r.end.character,
            );
        }
    }

    Ok(())
}

pub fn cmd_index(args: &[String]) -> anyhow::Result<()> {
    let dir = args.first().ok_or_else(|| anyhow::anyhow!("usage: knap index <dir>"))?;
    let root = PathBuf::from(dir);

    let (idx, _) = index::build(&[root], &["md"]);

    let mut notes: Vec<_> = idx.all_notes().collect();
    notes.sort_by(|a, b| a.path.cmp(&b.path));

    println!("{} note(s) indexed", notes.len());

    for note in notes {
        println!();
        println!("{}  (stem: {})", note.path.display(), note.stem);

        if note.wiki_links.is_empty() {
            println!("  links: none");
        } else {
            for link in &note.wiki_links {
                let status = match idx.resolve(&link.stem) {
                    ResolvedLink::Found(p) => format!("→ {}", p.display()),
                    ResolvedLink::Ambiguous(_) => "ambiguous".to_string(),
                    ResolvedLink::Broken => "broken".to_string(),
                };
                println!("  [[{}]]  {}", link.stem, status);
            }
        }

        let incoming = idx.links_to(&note.path);
        if !incoming.is_empty() {
            println!("  referenced by:");
            for l in incoming {
                println!("    {}", l.source_path.display());
            }
        }
    }

    Ok(())
}

/// In-process LSP smoke test: acts as a minimal LSP client against the server
/// running in a background thread. Verifies the full lifecycle without needing
/// a real editor.
///
/// Exit code is non-zero if any check fails, so this can be used in CI.
pub fn cmd_check() -> anyhow::Result<()> {
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
                }
            }),
        }))
        .expect("send");

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
        .map(|ts| ts.contains(&"[".to_string()))
        .unwrap_or(false);
    check!("completion trigger", trigger_ok, r#""[""#);

    check!("definition provider", caps.definition_provider.is_some(), "advertised");
    check!("references provider", caps.references_provider.is_some(), "advertised");

    // ── initialized → registerCapability ─────────────────────────────────────

    client_conn
        .sender
        .send(Message::Notification(Notification {
            method: "initialized".to_string(),
            params: json!({}),
        }))
        .expect("send");

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

    // ── stub requests ─────────────────────────────────────────────────────────

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
            }))
            .expect("send");

        let resp = recv_response(&client_conn)?;
        let null_result =
            resp.error.is_none() && resp.result == Some(serde_json::Value::Null);
        check!(method, null_result, "null (stubbed)");
    }

    // ── unknown method ────────────────────────────────────────────────────────

    client_conn
        .sender
        .send(Message::Request(Request {
            id: lsp_server::RequestId::from(5i32),
            method: "workspace/unknownMethod".to_string(),
            params: json!({}),
        }))
        .expect("send");

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
        }))
        .expect("send");

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
    loop {
        match conn.receiver.recv()? {
            lsp_server::Message::Response(r) => return Ok(r),
            lsp_server::Message::Request(_) => {}
            lsp_server::Message::Notification(_) => {}
        }
    }
}
