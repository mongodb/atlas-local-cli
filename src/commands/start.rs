use std::{fmt::Display, time::Duration};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use atlas_local::{
    Client,
    client::WatchDeploymentError,
    models::{State, WatchOptions},
};
use bollard::Docker;
use serde::Serialize;
use tracing::{debug, info, trace};

use crate::{
    args,
    commands::CommandWithOutput,
    dependencies::{
        DeploymentGetDeployment, DeploymentStarter, DeploymentUnpauser, DeploymentWaiter,
    },
    interaction::{Interaction, SpinnerInteraction},
};

// Start dependencies for the start command
pub trait StartDeploymentManagement:
    DeploymentStarter + DeploymentGetDeployment + DeploymentUnpauser + DeploymentWaiter
{
}
impl<T: DeploymentStarter + DeploymentGetDeployment + DeploymentUnpauser + DeploymentWaiter>
    StartDeploymentManagement for T
{
}

pub struct Start {
    deployment_name: String,

    wait_for_healthy: bool,
    wait_for_healthy_timeout: Duration,

    interaction: Box<dyn SpinnerInteraction + Send>,
    deployment_management: Box<dyn StartDeploymentManagement + Send>,
}

impl TryFrom<args::Start> for Start {
    type Error = anyhow::Error;

    fn try_from(args: args::Start) -> Result<Self> {
        Ok(Self {
            deployment_name: args.deployment_name,
            wait_for_healthy: args.wait_for_healthy,
            wait_for_healthy_timeout: args.wait_for_healthy_timeout,

            interaction: Box::new(Interaction::new()),
            deployment_management: Box::new(Client::new(
                Docker::connect_with_defaults().context("connecting to Docker")?,
            )),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum StartResult {
    Started {
        deployment_name: String,
    },
    Failed {
        deployment_name: String,
        error: String,
    },
}

impl Display for StartResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Started { deployment_name } => {
                write!(f, "Deployment '{}' started", deployment_name)
            }
            Self::Failed {
                deployment_name,
                error,
            } => {
                write!(
                    f,
                    "Starting deployment '{}' failed: {}",
                    deployment_name, error
                )
            }
        }
    }
}

#[async_trait]
impl CommandWithOutput for Start {
    type Output = StartResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        trace!(
            deployment_name=?self.deployment_name,
            wait_for_healthy=self.wait_for_healthy,
            wait_for_healthy_timeout=?self.wait_for_healthy_timeout,
        "executing start command");

        // Start the spinner
        // When start_spinner goes out of scope, the spinner will be stopped (when the command returns early)
        // Or when start_spinner gets dropped later in the code (before waiting for healthy deployment)
        let start_spinner = self
            .interaction
            .start_spinner("Starting deployment...".to_string())?;

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
                    return Ok(StartResult::Failed {
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
            State::Created | State::Exited => {
                // start the deployment when the container is not running (created or exited)
                debug!(state=?deployment.state, "starting deployment");

                self.deployment_management
                    .start(&self.deployment_name)
                    .await?;

                debug!("deployment started");
            }
            State::Paused => {
                // unpause the deployment
                debug!(state=?deployment.state, "unpausing deployment");

                self.deployment_management
                    .unpause(&self.deployment_name)
                    .await?;

                debug!("deployment unpaused");
            }
            State::Running | State::Restarting => {
                // return that the deployment is already running or restarting
                // we don't need to do anything
                debug!(state=?deployment.state, "deployment is already running or restarting, no action needed");
            }
            State::Dead => {
                debug!(state=?deployment.state, "deployment is dead, returning failed result");

                return Ok(StartResult::Failed {
                    deployment_name: self.deployment_name.clone(),
                    error: "Deployment is dead".to_string(),
                });
            }
            State::Removing => {
                debug!(state=?deployment.state, "deployment is in removing state, returning failed result");

                return Ok(StartResult::Failed {
                    deployment_name: self.deployment_name.clone(),
                    error: "Deployment is in removing state".to_string(),
                });
            }
        }

        // Drop the start spinner, this stops the spinner from spinning
        drop(start_spinner);

        if !self.wait_for_healthy {
            info!(
                "deployment started, did not wait for healthy deployment (--wait-for-healthy=false)"
            );

            return Ok(StartResult::Started {
                deployment_name: self.deployment_name.clone(),
            });
        }

        // Start the wait for health deployment spinner
        // When wait_for_healthy_spinner goes out of scope, the spinner will be stopped
        let _wait_for_healthy_deployment_spinner = self
            .interaction
            .start_spinner("Waiting for deployment to become healthy...".to_string())?;

        // now wait till the deployment is healthy
        let allow_unhealthy_initial_state = match deployment.state {
            // for these deployments we go from created to running and healthy/unhealthy
            State::Created | State::Exited | State::Restarting => false,
            // for these deployments we go to healthy
            State::Paused => true,
            // for these deployments we should be healthy
            State::Running => false,
            // these states should never be encountered
            // but we return false to be safe in case of some race condition (state changing between last calls)
            State::Dead | State::Removing => false,
        };

        debug!(
            allow_unhealthy_initial_state,
            "waiting for healthy deployment"
        );

        // wait till the deployment is healthy
        // If the deployment is not healthy within the timeout, return a failed result
        if let Err(err) = self
            .deployment_management
            .wait_for_healthy_deployment(
                &self.deployment_name,
                WatchOptions::builder()
                    .allow_unhealthy_initial_state(allow_unhealthy_initial_state)
                    .timeout_duration(self.wait_for_healthy_timeout)
                    .build(),
            )
            .await
        {
            match err {
                WatchDeploymentError::Timeout { deployment_name: _ } => {
                    return Ok(StartResult::Failed {
                        deployment_name: self.deployment_name.clone(),
                        error: "Waiting for deployment to become healthy timed out".to_string(),
                    });
                }
                WatchDeploymentError::UnhealthyDeployment {
                    deployment_name, ..
                } => {
                    return Ok(StartResult::Failed {
                        deployment_name: deployment_name.clone(),
                        error: "Deployment became unhealthy".to_string(),
                    });
                }
                // Other cases should never happen and are unexpected errors
                e => bail!("Failed to wait for healthy deployment: {}", e),
            }
        }

        Ok(StartResult::Started {
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
        client::{StartDeploymentError, UnpauseDeploymentError, WatchDeploymentError},
        models::{Deployment as AtlasDeployment, IntoDeploymentError},
    };
    use bollard::errors::Error as BollardError;
    use bollard::secret::HealthStatusEnum;
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

    // State-based tests (wait_for_healthy=false)

    #[tokio::test]
    async fn test_start_from_created_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Created)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_exited_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Exited)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_paused_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
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
            .expect_unpause()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_running_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Running)));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_restarting_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
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

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_dead_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Dead)));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "Deployment is dead".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_removing_state() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Removing)));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "Deployment is in removing state".to_string()
            }
        );
    }

    // State-based tests (wait_for_healthy=true)

    #[tokio::test]
    async fn test_start_from_created_wait_for_healthy() {
        let deployment_name = "test-deployment".to_string();
        let timeout = Duration::from_secs(30);

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Waiting for deployment to become healthy...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Created)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        let timeout_clone = timeout;
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone
                    && !options.allow_unhealthy_initial_state
                    && options.timeout_duration == Some(timeout_clone)
            })
            .return_once(|_, _| Ok(()));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: true,
            wait_for_healthy_timeout: timeout,
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_exited_wait_for_healthy() {
        let deployment_name = "test-deployment".to_string();
        let timeout = Duration::from_secs(30);

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Waiting for deployment to become healthy...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Exited)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        let timeout_clone = timeout;
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone
                    && !options.allow_unhealthy_initial_state
                    && options.timeout_duration == Some(timeout_clone)
            })
            .return_once(|_, _| Ok(()));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: true,
            wait_for_healthy_timeout: timeout,
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_paused_wait_for_healthy() {
        let deployment_name = "test-deployment".to_string();
        let timeout = Duration::from_secs(30);

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Waiting for deployment to become healthy...")
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
            .expect_unpause()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        let timeout_clone = timeout;
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone
                    && options.allow_unhealthy_initial_state
                    && options.timeout_duration == Some(timeout_clone)
            })
            .return_once(|_, _| Ok(()));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: true,
            wait_for_healthy_timeout: timeout,
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_running_wait_for_healthy() {
        let deployment_name = "test-deployment".to_string();
        let timeout = Duration::from_secs(30);

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Waiting for deployment to become healthy...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Running)));

        let deployment_name_clone = deployment_name.clone();
        let timeout_clone = timeout;
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone
                    && !options.allow_unhealthy_initial_state
                    && options.timeout_duration == Some(timeout_clone)
            })
            .return_once(|_, _| Ok(()));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: true,
            wait_for_healthy_timeout: timeout,
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_start_from_restarting_wait_for_healthy() {
        let deployment_name = "test-deployment".to_string();
        let timeout = Duration::from_secs(30);

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Waiting for deployment to become healthy...")
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
        let timeout_clone = timeout;
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone
                    && !options.allow_unhealthy_initial_state
                    && options.timeout_duration == Some(timeout_clone)
            })
            .return_once(|_, _| Ok(()));

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: true,
            wait_for_healthy_timeout: timeout,
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Started {
                deployment_name: deployment_name.clone()
            }
        );
    }

    // Error handling tests

    #[tokio::test]
    async fn test_start_get_deployment_container_inspect_error() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
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

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "container not found".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_start_get_deployment_into_deployment_error() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
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

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command.execute().await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to get deployment, into deployment error")
        );
    }

    #[tokio::test]
    async fn test_start_start_error() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Created)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| {
                Err(StartDeploymentError::ContainerStart(
                    "failed to start".to_string(),
                ))
            });

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_unpause_error() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
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
            .expect_unpause()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| {
                Err(UnpauseDeploymentError::ContainerUnpause(
                    "failed to unpause".to_string(),
                ))
            });

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: false,
            wait_for_healthy_timeout: Duration::from_secs(30),
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_wait_timeout() {
        let deployment_name = "test-deployment".to_string();
        let timeout = Duration::from_secs(30);

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Waiting for deployment to become healthy...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Created)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        let timeout_clone = timeout;
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone
                    && !options.allow_unhealthy_initial_state
                    && options.timeout_duration == Some(timeout_clone)
            })
            .return_once(|name, _| {
                Err(WatchDeploymentError::Timeout {
                    deployment_name: name.to_string(),
                })
            });

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: true,
            wait_for_healthy_timeout: timeout,
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "Waiting for deployment to become healthy timed out".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_start_wait_unhealthy() {
        let deployment_name = "test-deployment".to_string();
        let timeout = Duration::from_secs(30);

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Waiting for deployment to become healthy...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Created)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        let timeout_clone = timeout;
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone
                    && !options.allow_unhealthy_initial_state
                    && options.timeout_duration == Some(timeout_clone)
            })
            .return_once(|name, _| {
                Err(WatchDeploymentError::UnhealthyDeployment {
                    deployment_name: name.to_string(),
                    status: HealthStatusEnum::UNHEALTHY,
                })
            });

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: true,
            wait_for_healthy_timeout: timeout,
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            StartResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "Deployment became unhealthy".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_start_wait_unexpected_error() {
        let deployment_name = "test-deployment".to_string();
        let timeout = Duration::from_secs(30);

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Starting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Waiting for deployment to become healthy...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deployment_management = MockDocker::new();
        let deployment_name_for_get = deployment_name.clone();
        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_get_deployment()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(move |_| Ok(create_deployment(&deployment_name_for_get, State::Created)));

        let deployment_name_clone = deployment_name.clone();
        mock_deployment_management
            .expect_start()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let deployment_name_clone = deployment_name.clone();
        let timeout_clone = timeout;
        mock_deployment_management
            .expect_wait_for_healthy_deployment()
            .withf(move |name, options| {
                name == &deployment_name_clone
                    && !options.allow_unhealthy_initial_state
                    && options.timeout_duration == Some(timeout_clone)
            })
            .return_once(|_, _| {
                Err(WatchDeploymentError::ContainerInspect(BollardError::from(
                    io::Error::new(io::ErrorKind::Other, "unexpected error"),
                )))
            });

        let mut start_command = Start {
            deployment_name: deployment_name.clone(),
            wait_for_healthy: true,
            wait_for_healthy_timeout: timeout,
            interaction: Box::new(mock_interaction),
            deployment_management: Box::new(mock_deployment_management),
        };

        let result = start_command.execute().await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to wait for healthy deployment")
        );
    }
}
