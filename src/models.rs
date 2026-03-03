//! Data models for the application.
//!
//! This module contains the data structures used throughout the application.
//! These models are typically simplified versions of the underlying library models,
//! tailored for the CLI's display and formatting needs.

use atlas_local::models::State;
use semver::Version;
use serde::Serialize;

/// Deployment model representing a local MongoDB deployment.
///
/// This is a simplified representation of a deployment, containing only the fields
/// needed for CLI display and output formatting.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deployment {
    /// Name of the deployment.
    /// If the name is not set, the container ID is used.
    pub name: String,

    /// MongoDB version of the deployment.
    pub mongo_db_version: Version,

    /// State of the deployment.
    pub state: State,
}

/// Convert from the underlying library's deployment model to the CLI's deployment model.
///
/// This conversion extracts only the relevant fields and handles the case where
/// the deployment name might be `None` by falling back to the container ID.
impl From<atlas_local::models::Deployment> for Deployment {
    fn from(deployment: atlas_local::models::Deployment) -> Self {
        Deployment {
            name: deployment.name.unwrap_or(deployment.container_id),
            mongo_db_version: deployment.mongodb_version,
            state: deployment.state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_deployment_with_name() {
        let atlas_local_lib_deployment = atlas_local::models::Deployment {
            name: Some("test-deployment".to_string()),
            container_id: "test-container-id".to_string(),
            mongodb_version: Version::parse("8.2.2").unwrap(),
            state: atlas_local::models::State::Running,
            port_bindings: None,
            mongodb_type: atlas_local::models::MongodbType::Community,
            creation_source: None,
            local_seed_location: None,
            mongodb_initdb_database: None,
            mongodb_initdb_root_password_file: None,
            mongodb_initdb_root_password: None,
            mongodb_initdb_root_username_file: None,
            mongodb_initdb_root_username: None,
            mongodb_load_sample_data: None,
            mongot_log_file: None,
            runner_log_file: None,
            do_not_track: true,
            telemetry_base_url: None,
            voyage_api_key: None,
        };

        let expected = Deployment {
            name: "test-deployment".to_string(),
            mongo_db_version: Version::parse("8.2.2").unwrap(),
            state: State::Running,
        };
        let actual = Deployment::from(atlas_local_lib_deployment);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_from_deployment_without_name() {
        let atlas_local_lib_deployment = atlas_local::models::Deployment {
            name: None,
            container_id: "test-container-id".to_string(),
            mongodb_version: Version::parse("8.2.2").unwrap(),
            state: atlas_local::models::State::Paused,
            port_bindings: None,
            mongodb_type: atlas_local::models::MongodbType::Community,
            creation_source: None,
            local_seed_location: None,
            mongodb_initdb_database: None,
            mongodb_initdb_root_password_file: None,
            mongodb_initdb_root_password: None,
            mongodb_initdb_root_username_file: None,
            mongodb_initdb_root_username: None,
            mongodb_load_sample_data: None,
            mongot_log_file: None,
            runner_log_file: None,
            do_not_track: true,
            telemetry_base_url: None,
            voyage_api_key: None,
        };

        let expected = Deployment {
            name: "test-container-id".to_string(),
            mongo_db_version: Version::parse("8.2.2").unwrap(),
            state: State::Paused,
        };
        let actual = Deployment::from(atlas_local_lib_deployment);
        assert_eq!(actual, expected);
    }
}
