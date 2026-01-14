use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Setup logging.
/// By default, it will only show logs from our crate at the info level.
///
/// This function sets up logging for the application.
/// The log level can be overridden by setting the `ATLAS_LOCAL_LOG` environment variable.
/// If the `ATLAS_LOCAL_LOG_ALL` environment variable is set, it will show logs from all crates at the specified level.
pub fn setup_logging(enable_debug: bool) {
    // Based on the enable_debug flag, set up the logging filter.
    // If enable_debug is true, set the log level to debug and only show logs from this crate.
    // If enable_debug is false, get the log settings from the environment variables
    let (log_level, show_all_logs) = if enable_debug {
        // Set the log level to debug and only show logs from this crate.
        ("debug".to_string(), false)
    } else {
        // Get the log level from the environment variable.
        (
            std::env::var("ATLAS_LOCAL_LOG").unwrap_or_else(|_| "info".to_string()),
            std::env::var("ATLAS_LOCAL_LOG_ALL").is_ok(),
        )
    };

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
