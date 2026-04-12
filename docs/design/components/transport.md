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

`main.rs` is the only file that knows about `Connection`. It owns the transport setup and hands the connection off to the Protocol Handler.

```rust
fn main() -> anyhow::Result<()> {
    let (connection, io_threads) = Connection::stdio();
    protocol_handler::run(connection)?;
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
