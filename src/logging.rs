use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Setup logging.
/// By default, it will only show logs from our crate at the info level.
///
/// This function sets up logging for the application.
/// The log level can be overridden by setting the `ATLAS_LOCAL_LOG` environment variable.
/// If the `ATLAS_LOCAL_LOG_ALL` environment variable is set, it will show logs from all crates at the specified level.
pub fn setup_logging() {
    // Get the log level from the environment variable.
    let log_level = std::env::var("ATLAS_LOCAL_LOG").unwrap_or_else(|_| "info".to_string());

    // Check if we should show logs from all crates.
    let show_all_logs = std::env::var("ATLAS_LOCAL_LOG_ALL").is_ok();

    // Build the filter for the logging.
    // This will ether be "log_level" or "atlas_local=log_level".
    let filter = if show_all_logs {
        // Show logs from all crates at the specified level
        log_level
    } else {
        // Only show logs from our crate at the specified level
        format!("atlas_local={}", log_level)
    };

    // Initialize the logging using the filter built above.
    tracing_subscriber::registry()
        .with(fmt::layer().compact().with_target(false))
        .with(EnvFilter::new(filter))
        .init();
}
