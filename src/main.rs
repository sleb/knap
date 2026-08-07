fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().filter_or("KNAP_LOG", "info")).init();

    knap::cli::run()
}
