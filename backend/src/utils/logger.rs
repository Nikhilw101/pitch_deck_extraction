pub fn init() {
    // Default to INFO so API hit/endpoint logs are visible in backend terminal.
    // You can still override with RUST_LOG.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
