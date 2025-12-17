//! Command to list all local deployments.
//!
//! This module implements the `list` command which retrieves and displays all local MongoDB deployments.
//! The command uses dependency injection to allow for easy testing and mocking of external dependencies.

use std::fmt::Display;

use anyhow::{Context, Result};
use async_trait::async_trait;
use atlas_local::Client;
use bollard::Docker;
use serde::Serialize;

use crate::{
    args, commands::CommandWithOutput, dependencies::DeploymentLister, models::Deployment,
    table::Table,
};

/// Command to list all local deployments.
///
/// This command retrieves all local MongoDB deployments and formats them for display.
/// It uses a [`DeploymentLister`] trait to abstract away the actual deployment retrieval logic,
/// making it easy to test and mock.
pub struct List {
    deployment_lister: Box<dyn DeploymentLister + Send>,
}

/// Convert CLI arguments to command with default dependencies injected.
///
/// This implementation creates a new `List` command with the default `atlas_local::Client`
/// as the deployment lister. The client connects to Docker using Unix socket defaults.
impl TryFrom<args::List> for List {
    type Error = anyhow::Error;

    fn try_from(_: args::List) -> std::result::Result<Self, Self::Error> {
        Ok(List {
            deployment_lister: Box::new(Client::new(
                Docker::connect_with_unix_defaults().context("connecting to Docker")?,
            )),
        })
    }
}

/// Result of the list command.
///
/// We're using a newtype pattern to wrap the vector of deployments.
/// This allows us to implement traits like [`Display`] and table conversion on the result
/// without implementing them directly on `Vec<Deployment>`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListResult(Vec<Deployment>);

/// Convert the list result into a table for display.
///
/// This implementation allows the list result to be converted into a [`Table`] structure.
/// The table follows the same format as the tables printed using the Atlas CLI, with columns
/// for NAME, MDB VER (MongoDB version), and STATE.
impl From<&ListResult> for Table {
    fn from(value: &ListResult) -> Self {
        Table::from_iter(
            &value.0,
            &[
                ("NAME", |d| d.name.clone()),
                ("MDB VER", |d| d.mongo_db_version.to_string()),
                ("STATE", |d| d.state.to_string()),
            ],
        )
    }
}

/// Format the list result for display.
///
/// This implementation forwards the formatting to the table implementation,
/// which handles the actual table formatting logic.
impl Display for ListResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Table::from(self).fmt(f)
    }
}

/// Execute the list command and return the result.
///
/// This implementation calls the deployment lister to retrieve all local deployments,
/// converts them into the application's [`Deployment`] model, and wraps them in a [`ListResult`].
#[async_trait]
impl CommandWithOutput for List {
    type Output = ListResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        Ok(ListResult(
            self.deployment_lister
                .list()
                .await?
                .into_iter()
                .map(Deployment::from)
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use atlas_local::models::State;
    use semver::Version;

    use super::*;
    use crate::dependencies::MockDocker;

    #[tokio::test]
    async fn test_list_command() {
        let mut deployment_lister = MockDocker::new();
        deployment_lister.expect_list().return_once(|| {
            Ok(vec![atlas_local::models::Deployment {
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
            }])
        });

        let mut list_command = List {
            deployment_lister: Box::new(deployment_lister),
        };

        let result = list_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ListResult(vec![Deployment {
                name: "test-deployment".to_string(),
                mongo_db_version: Version::parse("8.2.2").unwrap(),
                state: State::Running,
            }])
        );
    }
}
