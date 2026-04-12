mod handlers;
mod index;
mod parser;
mod server;

fn main() -> anyhow::Result<()> {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    server::run(connection)?;
    io_threads.join()?;
    Ok(())
}
