fn main() -> anyhow::Result<()> {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    knap::server::run(connection)?;
    io_threads.join()?;
    Ok(())
}
