# Transport Layer

Handles JSON-RPC framing over stdio. In v0.1 this is entirely delegated to `lsp-server` — we own none of the framing code.

---

## What lsp-server provides

`lsp_server::Connection` reads Content-Length–framed JSON-RPC messages from stdin and writes responses to stdout. It exposes two typed channels:

```rust
connection.receiver: Receiver<Message>   // inbound
connection.sender:   Sender<Message>     // outbound
```

`Message` is an enum:

```rust
enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}
```

We never touch stdin/stdout directly. All I/O goes through `Connection`.

---

## Entry point

`main.rs` is the only file that knows about `Connection`. Before setting up
transport, it checks `argv[1]` against the Debug CLI subcommands
(`parse`/`index`/`check`/`version` — see [ARCHITECTURE.md](../../ARCHITECTURE.md))
and dispatches to `knap::cli` if one matches, returning early. Only when no
subcommand is given does it fall through to normal LSP server startup, owning
the transport setup and handing the connection off to the Protocol Handler
(`knap::server::run`, in `src/server/mod.rs`):

```rust
fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().filter_or("KNAP_LOG", "info"),
    )
    .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "parse" {
        return knap::cli::cmd_parse(&args[2..]);
    }
    // ... "index", "check", "version" dispatch similarly ...

    let (connection, io_threads) = Connection::stdio();
    knap::server::run(connection)?;
    io_threads.join()?;
    Ok(())
}
```

`io_threads.join()` blocks until the background I/O threads finish draining after the connection closes — required for clean shutdown.

---

## Sending messages

Outbound messages are sent via `connection.sender`. The Protocol Handler calls this directly; handlers return values rather than writing to the sender themselves.

Responses to requests:

```rust
connection.sender.send(Message::Response(Response::new_ok(id, result)))?;
```

Server-initiated notifications (e.g. `publishDiagnostics`):

```rust
connection.sender.send(Message::Notification(Notification::new(
    PublishDiagnostics::METHOD.to_string(),
    params,
)))?;
```

---

## Error handling

Transport errors (broken pipe, malformed framing) propagate as `anyhow::Error` and terminate the process. There is no recovery path — if the transport fails, the editor has already closed the connection.
