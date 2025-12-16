use atlas_local::models::State;
use semver::Version;
use serde::Serialize;

/// Deployment model
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deployment {
    pub name: String,
    pub mongo_db_version: Version,
    pub state: State,
}

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
