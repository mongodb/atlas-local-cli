use std::{collections::HashMap, fmt::Display, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use atlas_local::{
    Client, GetDeploymentError,
    client::WatchDeploymentError,
    models::{State, WatchOptions},
};
use bollard::Docker;
use serde::Serialize;
use tracing::debug;

use crate::{
    args::{self, ConnectWith},
    commands::{
        CommandWithOutput,
        connectors::{Compass, Connector, DeploymentParams, Mongosh, VsCode},
    },
    dependencies::{
        DeploymentGetConnectionString, DeploymentGetDeployment, DeploymentStarter,
        DeploymentUnpauser, DeploymentWaiter,
    },
    interaction::{
        Interaction, MultiStepSpinnerInteraction, MultiStepSpinnerOutcome, MultiStepSpinnerStep,
    },
};

const DEFAULT_WAIT_FOR_HEALTHY_TIMEOUT: Duration = Duration::from_secs(60);

// Dependencies for the connect command
pub trait ConnectDeploymentManagement:
    DeploymentGetConnectionString
    + DeploymentGetDeployment
    + DeploymentStarter
    + DeploymentUnpauser
    + DeploymentWaiter
    + Send
    + Sync
{
}
impl<
    T: DeploymentGetConnectionString
        + DeploymentGetDeployment
        + DeploymentStarter
        + DeploymentUnpauser
        + DeploymentWaiter
        + Send
        + Sync,
> ConnectDeploymentManagement for T
{
}

// Interaction dependencies for the connect command
pub trait ConnectInteraction: MultiStepSpinnerInteraction + Send + Sync {}
impl<T: MultiStepSpinnerInteraction + Send + Sync> ConnectInteraction for T {}

pub struct Connect {
    deployment_name: String,
    connector: ConnectWith,

    interaction: Box<dyn ConnectInteraction>,
    deployment_inspector: Box<dyn ConnectDeploymentManagement>,
    connectors: HashMap<ConnectWith, Box<dyn Connector + Send + Sync>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ConnectResult {
    Success {
        // When the connector is `ConnectionString`, the connection string is returned.
        // Otherwise, it is `None`.
        #[serde(skip_serializing_if = "Option::is_none")]
        connection_string: Option<String>,
    },
    Failed {
        error: String,
    },
}

impl Display for ConnectResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success { connection_string } => match connection_string {
                Some(connection_string) => write!(
                    f,
                    "The connection string for the deployment is: {}",
                    connection_string
                ),
                None => write!(f, "Finished successfully connecting to deployment"),
            },
            Self::Failed { error } => write!(f, "Failed to connect to deployment: {}", error),
        }
    }
}

impl TryFrom<args::Connect> for Connect {
    type Error = anyhow::Error;

    fn try_from(args: args::Connect) -> Result<Self> {
        Ok(Self {
            deployment_name: args.deployment_name,
            connector: args.connector,
            interaction: Box::new(Interaction::new()),
            deployment_inspector: Box::new(Client::new(Docker::connect_with_defaults()?)),
            connectors: HashMap::from([
                (
                    ConnectWith::Compass,
                    Box::new(Compass::new()) as Box<dyn Connector + Send + Sync>,
                ),
                (ConnectWith::Mongosh, Box::new(Mongosh::new())),
                (ConnectWith::VsCode, Box::new(VsCode::new())),
            ]),
        })
    }
}

#[async_trait]
impl CommandWithOutput for Connect {
    type Output = ConnectResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        // Try to execute the inner command
        // Wrap the error in a ConnectResult
        // - Actual errors are propagated
        // - Failed errors are wrapped in a ConnectResult::Failed, these are expected errors that are not actual errors
        match self.execute_inner().await {
            Ok(result) => Ok(result),
            Err(ConnectInnerError::Failed(error)) => Ok(ConnectResult::Failed { error }),
            Err(ConnectInnerError::ActualError(error)) => Err(error),
        }
    }
}

enum ConnectInnerError {
    Failed(String),
    ActualError(anyhow::Error),
}

impl Connect {
    async fn execute_inner(&mut self) -> Result<ConnectResult, ConnectInnerError> {
        // Get the deployment
        let deployment = self
            .deployment_inspector
            .get_deployment(&self.deployment_name)
            .await
            .map_err(|e| match e {
                GetDeploymentError::ContainerInspect(_) => ConnectInnerError::Failed(format!(
                    "Container {} does not exist",
                    self.deployment_name
                )),
                GetDeploymentError::IntoDeployment(e) => ConnectInnerError::ActualError(e.into()),
            })?;

        // Start/unpause the deployment if needed, or error on bad states
        self.start_deployment_if_needed(deployment.state).await?;

        // Get the connection string
        let connection_string = self
            .deployment_inspector
            .get_connection_string(deployment.container_id)
            .await
            .map_err(|e| ConnectInnerError::ActualError(e.into()))?;

        // If the connector is `ConnectionString`, return the connection string
        if self.connector == ConnectWith::ConnectionString {
            return Ok(ConnectResult::Success {
                connection_string: Some(connection_string),
            });
        }

        // Get the connector, if this fails, return an actual error
        let connector = self
            .connectors
            .get(&self.connector)
            .context("Connector not found")
            .map_err(ConnectInnerError::ActualError)?;

        // If the connector is not available, return a failed error
        if !connector.is_available().await {
            return Ok(ConnectResult::Failed {
                error: format!(
                    "{} is not installed",
                    match self.connector {
                        ConnectWith::Compass => "Compass",
                        ConnectWith::Mongosh => "Mongosh",
                        ConnectWith::VsCode => "VsCode",
                        ConnectWith::ConnectionString => unreachable!(),
                    }
                ),
            });
        }

        // Launch the connector
        connector
            .launch(&DeploymentParams::new(
                deployment.name.as_deref().unwrap_or_default(),
                &connection_string,
            ))
            .await
            .map_err(ConnectInnerError::ActualError)?;

        Ok(ConnectResult::Success {
            connection_string: None,
        })
    }

    async fn start_deployment_if_needed(&self, state: State) -> Result<(), ConnectInnerError> {
        // Determine what action to take based on state
        let action = match state {
            State::Created | State::Exited => Some(StartAction::Start),
            State::Paused => Some(StartAction::Unpause),
            State::Running | State::Restarting => None,
            State::Dead => {
                debug!(?state, "deployment is dead, returning failed result");
                return Err(ConnectInnerError::Failed("Deployment is dead".to_string()));
            }
            State::Removing => {
                debug!(
                    ?state,
                    "deployment is in removing state, returning failed result"
                );
                return Err(ConnectInnerError::Failed(
                    "Deployment is in removing state".to_string(),
                ));
            }
        };

        // If no action needed, deployment is already running
        let Some(action) = action else {
            debug!(
                ?state,
                "deployment is already running or restarting, no action needed"
            );
            return Ok(());
        };

        // Create spinner with 2 steps
        let mut spinner = self
            .interaction
            .start_multi_step_spinner(vec![
                MultiStepSpinnerStep::new(match action {
                    StartAction::Start => "Starting deployment...",
                    StartAction::Unpause => "Unpausing deployment...",
                }),
                MultiStepSpinnerStep::new("Waiting for deployment to become healthy..."),
            ])
            .map_err(ConnectInnerError::ActualError)?;

        // Step 1: Start or unpause the deployment
        let step1_result = match action {
            StartAction::Start => {
                debug!(?state, "starting deployment before connecting");
                self.deployment_inspector
                    .start(&self.deployment_name)
                    .await
                    .map_err(|e| ConnectInnerError::ActualError(e.into()))
            }
            StartAction::Unpause => {
                debug!(?state, "unpausing deployment before connecting");
                self.deployment_inspector
                    .unpause(&self.deployment_name)
                    .await
                    .map_err(|e| ConnectInnerError::ActualError(e.into()))
            }
        };

        if let Err(e) = step1_result {
            let _ = spinner.set_step_outcome(0, MultiStepSpinnerOutcome::Failure);
            let _ = spinner.set_step_outcome(1, MultiStepSpinnerOutcome::Skipped);
            return Err(e);
        }

        debug!("deployment started/unpaused");
        let _ = spinner.set_step_outcome(0, MultiStepSpinnerOutcome::Success);

        // Step 2: Wait for the deployment to become healthy
        // Paused deployments always start as unhealthy, so we allow unhealthy initial state
        let can_start_unhealthy = matches!(action, StartAction::Unpause);

        debug!(can_start_unhealthy, "waiting for healthy deployment");

        let wait_result = self
            .deployment_inspector
            .wait_for_healthy_deployment(
                &self.deployment_name,
                WatchOptions::builder()
                    .allow_unhealthy_initial_state(can_start_unhealthy)
                    .timeout_duration(DEFAULT_WAIT_FOR_HEALTHY_TIMEOUT)
                    .build(),
            )
            .await;

        match wait_result {
            Ok(()) => {
                debug!("deployment is healthy");
                let _ = spinner.set_step_outcome(1, MultiStepSpinnerOutcome::Success);
                Ok(())
            }
            Err(WatchDeploymentError::Timeout { .. }) => {
                let _ = spinner.set_step_outcome(1, MultiStepSpinnerOutcome::Failure);
                Err(ConnectInnerError::Failed(
                    "Waiting for deployment to become healthy timed out".to_string(),
                ))
            }
            Err(WatchDeploymentError::UnhealthyDeployment { .. }) => {
                let _ = spinner.set_step_outcome(1, MultiStepSpinnerOutcome::Failure);
                Err(ConnectInnerError::Failed(
                    "Deployment became unhealthy".to_string(),
                ))
            }
            Err(e) => {
                let _ = spinner.set_step_outcome(1, MultiStepSpinnerOutcome::Failure);
                Err(ConnectInnerError::ActualError(anyhow::anyhow!(
                    "Failed to wait for healthy deployment: {}",
                    e
                )))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum StartAction {
    Start,
    Unpause,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependencies::mocks::MockDocker;
    use crate::interaction::MultiStepSpinner;
    use crate::interaction::mocks::MockInteraction;
    use atlas_local::{
        GetDeploymentError,
        client::{StartDeploymentError, UnpauseDeploymentError, WatchDeploymentError},
        models::{Deployment as AtlasDeployment, IntoDeploymentError},
    };
    use bollard::errors::Error as BollardError;
    use bollard::secret::HealthStatusEnum;
    use mockall::mock;
    use semver::Version;
    use std::io;

    mock! {
        pub Connector {}

        #[async_trait]
        impl Connector for Connector {
            async fn is_available(&self) -> bool;
            async fn launch(&self, params: &DeploymentParams) -> Result<()>;
        }
    }

    // Mock for MultiStepSpinner
    struct MockMultiStepSpinner;
    impl MultiStepSpinner for MockMultiStepSpinner {
        fn set_step_outcome(
            &mut self,
            _step: usize,
            _outcome: MultiStepSpinnerOutcome,
        ) -> Result<()> {
            Ok(())
        }
    }

    fn create_mock_interaction() -> MockInteraction {
        let mut mock = MockInteraction::new();
        mock.expect_start_multi_step_spinner()
            .returning(|_| Ok(Box::new(MockMultiStepSpinner)));
        mock
    }

    fn create_deployment_with_state(
        name: &str,
        container_id: &str,
        state: State,
    ) -> AtlasDeployment {
        AtlasDeployment {
            name: Some(name.to_string()),
            container_id: container_id.to_string(),
            mongodb_version: Version::parse("8.2.2").unwrap(),
            state,
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
        }
    }

    fn create_deployment(name: &str, container_id: &str) -> AtlasDeployment {
        create_deployment_with_state(name, container_id, State::Running)
    }

    #[tokio::test]
    async fn test_connect_with_connection_string() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Success {
                connection_string: Some(connection_string)
            }
        );
    }

    #[tokio::test]
    async fn test_connect_with_compass_success() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| true);
        mock_connector.expect_launch().returning(|_| Ok(()));

        let mut connectors = HashMap::new();
        connectors.insert(
            ConnectWith::Compass,
            Box::new(mock_connector) as Box<dyn Connector + Send + Sync>,
        );

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::Compass,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors,
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Success {
                connection_string: None
            }
        );
    }

    #[tokio::test]
    async fn test_connect_with_mongosh_success() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| true);
        mock_connector.expect_launch().returning(|_| Ok(()));

        let mut connectors = HashMap::new();
        connectors.insert(
            ConnectWith::Mongosh,
            Box::new(mock_connector) as Box<dyn Connector + Send + Sync>,
        );

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::Mongosh,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors,
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Success {
                connection_string: None
            }
        );
    }

    #[tokio::test]
    async fn test_connect_with_vscode_success() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| true);
        mock_connector.expect_launch().returning(|_| Ok(()));

        let mut connectors = HashMap::new();
        connectors.insert(
            ConnectWith::VsCode,
            Box::new(mock_connector) as Box<dyn Connector + Send + Sync>,
        );

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::VsCode,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors,
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Success {
                connection_string: None
            }
        );
    }

    #[tokio::test]
    async fn test_connect_deployment_not_found() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| {
                Err(GetDeploymentError::ContainerInspect(BollardError::from(
                    io::Error::new(io::ErrorKind::NotFound, "container not found"),
                )))
            });

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Failed {
                error: format!("Container {} does not exist", deployment_name)
            }
        );
    }

    #[tokio::test]
    async fn test_connect_compass_not_available() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| false);

        let mut connectors = HashMap::new();
        connectors.insert(
            ConnectWith::Compass,
            Box::new(mock_connector) as Box<dyn Connector + Send + Sync>,
        );

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::Compass,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors,
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Failed {
                error: "Compass is not installed".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_connect_mongosh_not_available() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| false);

        let mut connectors = HashMap::new();
        connectors.insert(
            ConnectWith::Mongosh,
            Box::new(mock_connector) as Box<dyn Connector + Send + Sync>,
        );

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::Mongosh,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors,
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Failed {
                error: "Mongosh is not installed".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_connect_vscode_not_available() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| false);

        let mut connectors = HashMap::new();
        connectors.insert(
            ConnectWith::VsCode,
            Box::new(mock_connector) as Box<dyn Connector + Send + Sync>,
        );

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::VsCode,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors,
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Failed {
                error: "VsCode is not installed".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_connect_get_deployment_into_deployment_error() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| {
                Err(GetDeploymentError::IntoDeployment(
                    IntoDeploymentError::MissingContainerID,
                ))
            });

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_connect_get_connection_string_error() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(|_| {
                Err(atlas_local::GetConnectionStringError::GetDeployment(
                    GetDeploymentError::ContainerInspect(BollardError::from(io::Error::new(
                        io::ErrorKind::Other,
                        "failed to get connection string",
                    ))),
                ))
            });

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_connect_connector_not_found() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::Compass,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(), // Empty connectors map
        };

        let result = connect_command.execute().await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Connector not found")
        );
    }

    #[tokio::test]
    async fn test_connect_launch_error() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_create,
                    &container_id_for_create,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| true);
        mock_connector
            .expect_launch()
            .returning(|_| Err(anyhow::anyhow!("failed to launch")));

        let mut connectors = HashMap::new();
        connectors.insert(
            ConnectWith::Compass,
            Box::new(mock_connector) as Box<dyn Connector + Send + Sync>,
        );

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::Compass,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors,
        };

        let result = connect_command.execute().await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to launch"));
    }

    #[tokio::test]
    async fn test_connect_deployment_with_no_name() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut deployment = create_deployment(&deployment_name, &container_id);
        deployment.name = None; // Deployment without a name

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let container_id_clone = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(deployment));

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_clone)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| true);
        mock_connector.expect_launch().returning(|_| Ok(()));

        let mut connectors = HashMap::new();
        connectors.insert(
            ConnectWith::Compass,
            Box::new(mock_connector) as Box<dyn Connector + Send + Sync>,
        );

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::Compass,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors,
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Success {
                connection_string: None
            }
        );
    }

    // State-based tests - deployment starting/unpausing

    #[tokio::test]
    async fn test_connect_from_created_state_starts_deployment() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Created,
                ))
            });

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone && !options.allow_unhealthy_initial_state
            })
            .return_once(|_, _| Ok(()));

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Success {
                connection_string: Some(connection_string)
            }
        );
    }

    #[tokio::test]
    async fn test_connect_from_exited_state_starts_deployment() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Exited,
                ))
            });

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone && !options.allow_unhealthy_initial_state
            })
            .return_once(|_, _| Ok(()));

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Success {
                connection_string: Some(connection_string)
            }
        );
    }

    #[tokio::test]
    async fn test_connect_from_paused_state_unpauses_deployment() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Paused,
                ))
            });

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_unpause()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        // Paused deployments allow unhealthy initial state
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone && options.allow_unhealthy_initial_state
            })
            .return_once(|_, _| Ok(()));

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Success {
                connection_string: Some(connection_string)
            }
        );
    }

    #[tokio::test]
    async fn test_connect_from_restarting_state_succeeds() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();
        let connection_string = "mongodb://localhost:27017".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        let container_id_for_connection = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Restarting,
                ))
            });

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .withf(move |id| id == &container_id_for_connection)
            .return_once(move |_| Ok(connection_string_clone.clone()));

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Success {
                connection_string: Some(connection_string)
            }
        );
    }

    #[tokio::test]
    async fn test_connect_from_dead_state_fails() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Dead,
                ))
            });

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Failed {
                error: "Deployment is dead".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_connect_from_removing_state_fails() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Removing,
                ))
            });

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Failed {
                error: "Deployment is in removing state".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_connect_start_error() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Created,
                ))
            });

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| {
                Err(StartDeploymentError::ContainerStart(
                    "failed to start".to_string(),
                ))
            });

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_connect_unpause_error() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Paused,
                ))
            });

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_unpause()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| {
                Err(UnpauseDeploymentError::ContainerUnpause(
                    "failed to unpause".to_string(),
                ))
            });

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_connect_wait_for_healthy_timeout() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Created,
                ))
            });

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, _options| name == &deployment_name_clone)
            .return_once(|name, _| {
                Err(WatchDeploymentError::Timeout {
                    deployment_name: name.to_string(),
                })
            });

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Failed {
                error: "Waiting for deployment to become healthy timed out".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_connect_wait_for_healthy_unhealthy() {
        let deployment_name = "test-deployment".to_string();
        let container_id = "test-container-id".to_string();

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        let deployment_name_for_create = deployment_name.clone();
        let container_id_for_create = container_id.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment_with_state(
                    &deployment_name_for_create,
                    &container_id_for_create,
                    State::Created,
                ))
            });

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, _options| name == &deployment_name_clone)
            .return_once(|name, _| {
                Err(WatchDeploymentError::UnhealthyDeployment {
                    deployment_name: name.to_string(),
                    status: HealthStatusEnum::UNHEALTHY,
                })
            });

        let mut connect_command = Connect {
            deployment_name: deployment_name.clone(),
            connector: ConnectWith::ConnectionString,
            interaction: Box::new(create_mock_interaction()),
            deployment_inspector: Box::new(mock_deployment_management),
            connectors: HashMap::new(),
        };

        let result = connect_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            ConnectResult::Failed {
                error: "Deployment became unhealthy".to_string()
            }
        );
    }
}
