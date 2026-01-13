use std::{collections::HashMap, fmt::Display};

use anyhow::{Context, Result};
use async_trait::async_trait;
use atlas_local::{Client, GetDeploymentError};
use bollard::Docker;
use serde::Serialize;

use crate::{
    args::{self, ConnectWith},
    commands::{
        CommandWithOutput,
        connectors::{Compass, Connector, DeploymentParams, Mongosh, VsCode},
    },
    dependencies::{DeploymentGetConnectionString, DeploymentGetDeployment},
};

// Start dependencies for the start command
pub trait ConnectDeploymentManagement:
    DeploymentGetConnectionString + DeploymentGetDeployment + Send
{
}
impl<T: DeploymentGetConnectionString + DeploymentGetDeployment + Send> ConnectDeploymentManagement
    for T
{
}

pub struct Connect {
    deployment_name: String,
    connector: ConnectWith,

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependencies::mocks::MockDocker;
    use atlas_local::{
        GetDeploymentError,
        models::{Deployment as AtlasDeployment, IntoDeploymentError},
    };
    use bollard::errors::Error as BollardError;
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

    fn create_deployment(name: &str, container_id: &str) -> AtlasDeployment {
        AtlasDeployment {
            name: Some(name.to_string()),
            container_id: container_id.to_string(),
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
        }
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
}
