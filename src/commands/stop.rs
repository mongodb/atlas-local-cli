use std::fmt::Display;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use atlas_local::{Client, models::State};
use bollard::Docker;
use serde::Serialize;
use tracing::{debug, trace};

use crate::{
    args,
    commands::CommandWithOutput,
    dependencies::{DeploymentGetDeployment, DeploymentStopper},
    interaction::{Interaction, SpinnerInteraction},
};

// Stop dependencies for the stop command
pub trait StopDeploymentManagement: DeploymentGetDeployment + DeploymentStopper {}
impl<T: DeploymentGetDeployment + DeploymentStopper> StopDeploymentManagement for T {}

pub struct Stop {
    deployment_name: String,

    interaction: Box<dyn SpinnerInteraction + Send>,
    deployment_management: Box<dyn StopDeploymentManagement + Send>,
}

impl TryFrom<args::Stop> for Stop {
    type Error = anyhow::Error;

    fn try_from(args: args::Stop) -> Result<Self> {
        Ok(Self {
            deployment_name: args.deployment_name,

            interaction: Box::new(Interaction::new()),
            deployment_management: Box::new(Client::new(
                Docker::connect_with_defaults().context("connecting to Docker")?,
            )),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum StopResult {
    Stopped {
        deployment_name: String,
    },
    Failed {
        deployment_name: String,
        error: String,
    },
}

impl Display for StopResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped { deployment_name } => {
                write!(f, "Deployment '{}' stopped", deployment_name)
            }
            Self::Failed {
                deployment_name,
                error,
            } => {
                write!(
                    f,
                    "Stopping deployment '{}' failed: {}",
                    deployment_name, error
                )
            }
        }
    }
}

#[async_trait]
impl CommandWithOutput for Stop {
    type Output = StopResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        trace!(
            deployment_name=?self.deployment_name,
        "executing stop command");

        // Start the spinner
        // When spinner goes out of scope, the spinner will be stopped
        let _spinner = self
            .interaction
            .start_spinner("Stopping deployment...".to_string())?;

        debug!("searching for deployment '{}'", self.deployment_name);

        // Get the deployment
        // If the deployment is not found, return a failed result
        let deployment = match self
            .deployment_management
            .get_deployment(&self.deployment_name)
            .await
        {
            Ok(deployment) => deployment,
            Err(err) => match err {
                atlas_local::GetDeploymentError::ContainerInspect(error) => {
                    return Ok(StopResult::Failed {
                        deployment_name: self.deployment_name.clone(),
                        error: error.to_string(),
                    });
                }
                atlas_local::GetDeploymentError::IntoDeployment(e) => {
                    bail!("Failed to get deployment, into deployment error: {}", e)
                }
            },
        };

        debug!(?deployment, "deployment found");

        match deployment.state {
            State::Running | State::Restarting | State::Paused => {
                // stop the deployment when it's running or restarting
                debug!(state=?deployment.state, "stopping deployment");

                self.deployment_management
                    .stop(&self.deployment_name)
                    .await?;

                debug!("deployment stopped");
            }
            State::Created | State::Exited => {
                // deployment is already stopped
                debug!(state=?deployment.state, "deployment is already stopped, no action needed");
            }
            State::Dead => {
                debug!(state=?deployment.state, "deployment is dead, returning failed result");

                return Ok(StopResult::Failed {
                    deployment_name: self.deployment_name.clone(),
                    error: "Deployment is dead".to_string(),
                });
            }
            State::Removing => {
                debug!(state=?deployment.state, "deployment is in removing state, returning failed result");

                return Ok(StopResult::Failed {
                    deployment_name: self.deployment_name.clone(),
                    error: "Deployment is in removing state".to_string(),
                });
            }
        }

        Ok(StopResult::Stopped {
            deployment_name: self.deployment_name.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependencies::mocks::MockDocker;
    use crate::interaction::SpinnerHandle;
    use crate::interaction::mocks::MockInteraction;
    use atlas_local::{
        GetDeploymentError,
        client::StopDeploymentError,
        models::{Deployment as AtlasDeployment, IntoDeploymentError},
    };
    use bollard::errors::Error as BollardError;
    use semver::Version;
    use std::io;

    fn create_spinner_handle() -> SpinnerHandle {
        SpinnerHandle::new(Box::new(|| {}))
    }

    fn create_deployment(name: &str, state: State) -> AtlasDeployment {
        AtlasDeployment {
            name: Some(name.to_string()),
            container_id: format!("container-{}", name),
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

    // State-based tests

    #[tokio::test]
    async fn test_stop_from_running_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Running)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_stop()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StopResult::Stopped {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_stop_from_restarting_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| {
                Ok(create_deployment(
                    &deployment_name_for_get,
                    State::Restarting,
                ))
            });

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_stop()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StopResult::Stopped {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_stop_from_paused_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Paused)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_stop()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StopResult::Stopped {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_stop_from_created_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Created)));

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StopResult::Stopped {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_stop_from_exited_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Exited)));

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StopResult::Stopped {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_stop_from_dead_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Dead)));

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StopResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "Deployment is dead".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_stop_from_removing_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Removing)));

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StopResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "Deployment is in removing state".to_string()
            }
        );
    }

    // Error handling tests

    #[tokio::test]
    async fn test_stop_get_deployment_container_inspect_error() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

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

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StopResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "container not found".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_stop_get_deployment_into_deployment_error() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

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

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command.execute().await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to get deployment, into deployment error")
        );
    }

    #[tokio::test]
    async fn test_stop_stop_error() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Stopping deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Running)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_stop()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| {
                Err(StopDeploymentError::ContainerStop(
                    "failed to stop".to_string(),
                ))
            });

        let mut stop_command = Stop {
            deployment_name: deployment_name.clone(),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = stop_command.execute().await;

        assert!(result.is_err());
    }
}
