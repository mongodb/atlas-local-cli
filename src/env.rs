//! Environment variable names used by the application.

/// When set to "true", use the preview MongoDB version for new deployments.
/// Cannot be used together with the `--mdbVersion` flag.
pub const MONGODB_ATLAS_LOCAL_PREVIEW: &str = "MONGODB_ATLAS_LOCAL_PREVIEW";

/// Optional API key for Voyage embeddings.
/// When set, it is passed to the deployment for use during index building.
pub const MONGODB_ATLAS_LOCAL_VOYAGE_API_KEY: &str = "MONGODB_ATLAS_LOCAL_VOYAGE_API_KEY";

/// Overrides the log level (e.g. "debug", "info", "warn", "error").
/// Default is "info" when unset.
pub const ATLAS_LOCAL_LOG: &str = "ATLAS_LOCAL_LOG";

/// When set, show logs from all crates at the level specified by
/// `ATLAS_LOCAL_LOG`. When unset, only this crate's logs are shown.
pub const ATLAS_LOCAL_LOG_ALL: &str = "ATLAS_LOCAL_LOG_ALL";
