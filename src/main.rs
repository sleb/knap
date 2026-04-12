fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().filter_or("KNAP_LOG", "info"),
    )
    .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "parse" {
        return knap::cli::cmd_parse(&args[2..]);
    }
    if args.len() >= 2 && args[1] == "index" {
        return knap::cli::cmd_index(&args[2..]);
    }
    if args.len() >= 2 && args[1] == "check" {
        return knap::cli::cmd_check();
    }

    let (connection, io_threads) = lsp_server::Connection::stdio();
    knap::server::run(connection)?;
    io_threads.join()?;
    Ok(())
}
