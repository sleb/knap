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

`main.rs` collapses to logging setup plus `knap::cli::run()` — it doesn't know
about `Connection` directly. `knap::cli` (`src/cli/mod.rs`) owns clap parsing
and dispatch to one subcommand module each: `lsp`, `lint`, `index`, `parse`,
`rename` (`rename-file`/`rename-heading`/`rename-tag`), `check`, `version`
(see [ARCHITECTURE.md](../../ARCHITECTURE.md) § CLI). A
subcommand is required — clap exits non-zero with usage text on bare `knap`,
there is no argument-free fallback to the LSP server. `src/cli/lsp.rs` is the
only module that touches `Connection`; it owns the transport setup and hands
the connection off to the Protocol Handler (`knap::server::run`, in
`src/server/mod.rs`):

```rust
// src/main.rs
fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().filter_or("KNAP_LOG", "info"),
    )
    .init();

    knap::cli::run()
}

// src/cli/lsp.rs
pub fn run() -> anyhow::Result<()> {
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
