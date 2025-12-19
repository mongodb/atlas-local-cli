use std::fmt::Display;

use anyhow::{Context, Result};
use async_trait::async_trait;
use atlas_local::{Client, DeleteDeploymentError};
use bollard::Docker;
use serde::Serialize;

use crate::{
    args,
    commands::CommandWithOutput,
    dependencies::DeploymentDeleter,
    interaction::{
        ConfirmationPrompt, ConfirmationPromptOptions, ConfirmationPromptResult, Interaction,
        SpinnerInteraction,
    },
};

// Interaction dependencies for the delete command
pub trait DeleteInteraction: ConfirmationPrompt + SpinnerInteraction + Send {}
impl<T: ConfirmationPrompt + SpinnerInteraction + Send> DeleteInteraction for T {}

pub struct Delete {
    deployment_name: String,
    force: bool,

    interaction: Box<dyn DeleteInteraction>,
    deployment_deleter: Box<dyn DeploymentDeleter + Send>,
}

impl TryFrom<args::Delete> for Delete {
    type Error = anyhow::Error;

    fn try_from(args: args::Delete) -> Result<Self> {
        Ok(Self {
            deployment_name: args.deployment_name,
            force: args.force,

            interaction: Box::new(Interaction::new()),
            deployment_deleter: Box::new(Client::new(
                Docker::connect_with_defaults().context("connecting to Docker")?,
            )),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum DeleteResult {
    Deleted {
        deployment_name: String,
    },
    Failed {
        deployment_name: String,
        error: String,
    },
    Canceled {
        deployment_name: String,
    },
}

impl Display for DeleteResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deleted { deployment_name } => {
                write!(f, "Deployment '{}' deleted", deployment_name)
            }
            Self::Failed {
                deployment_name,
                error,
            } => write!(
                f,
                "Deleting deployment '{}' failed: {}",
                deployment_name, error
            ),
            Self::Canceled { .. } => write!(f, "Deployment not deleted"),
        }
    }
}

#[async_trait]
impl CommandWithOutput for Delete {
    type Output = DeleteResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        if !self.force {
            let confirmation = self
                .interaction
                .confirm(
                    ConfirmationPromptOptions::builder()
                        .pre_confirmation_help_text("This operation will delete the deployment, and all of its data. This action cannot be undone.")
                        .message(&format!("Are you sure you want to terminate '{}'?", self.deployment_name))
                        .default(false)
                        .build())
                .context("confirming deletion")?;

            if matches!(
                confirmation,
                ConfirmationPromptResult::No | ConfirmationPromptResult::Canceled
            ) {
                // Operation cancelled by user.
                return Ok(DeleteResult::Canceled {
                    deployment_name: self.deployment_name.clone(),
                });
            }
        }

        // Start the spinner and delete the deployment
        // When _spinner goes out of scope, the spinner will be stopped
        let _spinner = self
            .interaction
            .start_spinner("Deleting deployment...".to_string())?;

        // Delete the deployment and handle the errors
        if let Err(e) = self.deployment_deleter.delete(&self.deployment_name).await {
            // Convert the error to a more user-friendly error
            // This type of error is not an "error", it's one of the possible outcomes of the operation
            return Ok(DeleteResult::Failed {
                deployment_name: self.deployment_name.clone(),
                error: match e {
                    DeleteDeploymentError::GetDeployment(..) => "deployment not found".to_string(),
                    DeleteDeploymentError::ContainerStop(..) => {
                        "failed to stop the container".to_string()
                    }
                    DeleteDeploymentError::ContainerRemove(..) => {
                        "failed to delete the container".to_string()
                    }
                },
            });
        }

        Ok(DeleteResult::Deleted {
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
    use anyhow::anyhow;
    use bollard::errors::Error as BollardError;
    use std::io;

    fn create_spinner_handle() -> SpinnerHandle {
        SpinnerHandle::new(Box::new(|| {}))
    }

    #[tokio::test]
    async fn test_delete_force_false_user_confirms() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_confirm()
            .return_once(|_| Ok(ConfirmationPromptResult::Yes));

        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Deleting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deleter = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        mock_deleter
            .expect_delete()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let mut delete_command = Delete {
            deployment_name: deployment_name.clone(),
            force: false,
            interaction: Box::new(mock_interaction),
            deployment_deleter: Box::new(mock_deleter),
        };

        let result = delete_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Deleted {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_delete_force_true() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        // When force is true, confirm should not be called
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Deleting deployment...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deleter = MockDocker::new();
        let deployment_name_clone = deployment_name.clone();
        mock_deleter
            .expect_delete()
            .withf(move |name| name == &deployment_name_clone)
            .return_once(|_| Ok(()));

        let mut delete_command = Delete {
            deployment_name: deployment_name.clone(),
            force: true,
            interaction: Box::new(mock_interaction),
            deployment_deleter: Box::new(mock_deleter),
        };

        let result = delete_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Deleted {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_delete_force_false_user_cancels_no() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_confirm()
            .return_once(|_| Ok(ConfirmationPromptResult::No));

        let mut delete_command = Delete {
            deployment_name: deployment_name.clone(),
            force: false,
            interaction: Box::new(mock_interaction),
            deployment_deleter: Box::new(MockDocker::new()),
        };

        let result = delete_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Canceled {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_delete_force_false_user_cancels_canceled() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_confirm()
            .return_once(|_| Ok(ConfirmationPromptResult::Canceled));

        let mut delete_command = Delete {
            deployment_name: deployment_name.clone(),
            force: false,
            interaction: Box::new(mock_interaction),
            deployment_deleter: Box::new(MockDocker::new()),
        };

        let result = delete_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Canceled {
                deployment_name: deployment_name.clone()
            }
        );
    }

    #[tokio::test]
    async fn test_delete_force_false_input_failed() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_confirm()
            .return_once(|_| Err(anyhow!("input error")));

        let mut delete_command = Delete {
            deployment_name: deployment_name.clone(),
            force: false,
            interaction: Box::new(mock_interaction),
            deployment_deleter: Box::new(MockDocker::new()),
        };

        let result = delete_command.execute().await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("confirming deletion")
        );
    }

    #[tokio::test]
    async fn test_delete_force_true_get_deployment_failed() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deleter = MockDocker::new();
        mock_deleter.expect_delete().return_once(|_| {
            Err(DeleteDeploymentError::GetDeployment(
                atlas_local::GetDeploymentError::from(BollardError::from(io::Error::new(
                    io::ErrorKind::NotFound,
                    "deployment not found",
                ))),
            ))
        });

        let mut delete_command = Delete {
            deployment_name: deployment_name.clone(),
            force: true,
            interaction: Box::new(mock_interaction),
            deployment_deleter: Box::new(mock_deleter),
        };

        let result = delete_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "deployment not found".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_delete_force_true_stop_failed() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deleter = MockDocker::new();
        mock_deleter.expect_delete().return_once(|_| {
            Err(DeleteDeploymentError::ContainerStop(BollardError::from(
                io::Error::new(io::ErrorKind::Other, "failed to stop"),
            )))
        });

        let mut delete_command = Delete {
            deployment_name: deployment_name.clone(),
            force: true,
            interaction: Box::new(mock_interaction),
            deployment_deleter: Box::new(mock_deleter),
        };

        let result = delete_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "failed to stop the container".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_delete_force_true_remove_failed() {
        let deployment_name = "test-deployment".to_string();

        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_deleter = MockDocker::new();
        mock_deleter.expect_delete().return_once(|_| {
            Err(DeleteDeploymentError::ContainerRemove(BollardError::from(
                io::Error::new(io::ErrorKind::Other, "failed to remove"),
            )))
        });

        let mut delete_command = Delete {
            deployment_name: deployment_name.clone(),
            force: true,
            interaction: Box::new(mock_interaction),
            deployment_deleter: Box::new(mock_deleter),
        };

        let result = delete_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Failed {
                deployment_name: deployment_name.clone(),
                error: "failed to delete the container".to_string()
            }
        );
    }
}
