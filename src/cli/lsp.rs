pub fn run() -> anyhow::Result<()> {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    crate::server::run(connection)?;
    io_threads.join()?;
    Ok(())
}
