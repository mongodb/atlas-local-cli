//! Command to delete a search index from a local deployment.
//!
//! This module implements the `search indexes delete` command which deletes
//! a specified search index from a local deployment.

use std::fmt::Display;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use mongodb::Client;
use serde::Serialize;

use crate::{
    args,
    commands::{
        CommandWithOutput,
        with_mongodb::{TryFromWithMongodbClient, TryToGetMongodbClientError},
    },
    dependencies::SearchIndexDeleter,
    interaction::{
        ConfirmationPrompt, ConfirmationPromptOptions, ConfirmationPromptResult, InputPrompt,
        InputPromptOptions, InputPromptResult, Interaction, SpinnerInteraction,
    },
};

// Interaction dependencies for the delete command.
pub trait DeleteInteraction:
    ConfirmationPrompt + InputPrompt + SpinnerInteraction + Send + Sync
{
}
impl<T: ConfirmationPrompt + InputPrompt + SpinnerInteraction + Send + Sync> DeleteInteraction
    for T
{
}

// MongoDB dependencies for the delete command.
pub trait MongoDbClient: SearchIndexDeleter + Send + Sync {}
impl<T: SearchIndexDeleter + Send + Sync> MongoDbClient for T {}

/// Command to delete a search index from a local deployment.
pub struct Delete {
    index_name: Option<String>,
    database_name: Option<String>,
    collection: Option<String>,
    force: bool,

    interaction: Box<dyn DeleteInteraction>,
    mongodb_client: Result<Box<dyn MongoDbClient + Send + Sync>, TryToGetMongodbClientError>,
}

impl TryFromWithMongodbClient<args::search::Delete> for Delete {
    fn try_from_with_mongodb(
        args: args::search::Delete,
        client_result: Result<Client, TryToGetMongodbClientError>,
    ) -> Result<Self> {
        Ok(Self {
            index_name: args.index_name,
            database_name: args.database_name,
            collection: args.collection,
            force: args.force,

            interaction: Box::new(Interaction::new()),
            mongodb_client: client_result
                .map(|client| Box::new(client) as Box<dyn MongoDbClient + Send + Sync>),
        })
    }
}

/// Result of the delete command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum DeleteResult {
    Deleted { index_name: String },
    Failed { error: String },
    Canceled,
}

impl Display for DeleteResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deleted { index_name } => write!(f, "Index '{}' deleted", index_name),
            Self::Failed { error } => write!(f, "Index not deleted: {}", error),
            Self::Canceled => write!(f, "Index not deleted"),
        }
    }
}

#[async_trait]
impl CommandWithOutput for Delete {
    type Output = DeleteResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        // Prompt for database name if not provided.
        let database_name = match self.prompt_if_none(&self.database_name.clone(), "Database?") {
            Ok(name) => name,
            Err(e) => {
                return Ok(DeleteResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        // Prompt for collection name if not provided.
        let collection_name = match self.prompt_if_none(&self.collection.clone(), "Collection?") {
            Ok(name) => name,
            Err(e) => {
                return Ok(DeleteResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        // Prompt for index name if not provided.
        let index_name = match self.prompt_if_none(&self.index_name.clone(), "Search Index Name?") {
            Ok(name) => name,
            Err(e) => {
                return Ok(DeleteResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        // Ask for confirmation if not forced.
        if !self.force {
            let confirmation = self
                .interaction
                .confirm(
                    ConfirmationPromptOptions::builder()
                        .message(format!(
                            "Are you sure you want to delete search index '{}'?",
                            index_name
                        ))
                        .default(false)
                        .build(),
                )
                .context("confirming deletion")?;

            if matches!(
                confirmation,
                ConfirmationPromptResult::No | ConfirmationPromptResult::Canceled
            ) {
                return Ok(DeleteResult::Canceled);
            }
        }

        // Get the mongodb client.
        let mongodb_client = match &self.mongodb_client {
            Ok(client) => client,
            Err(e) => {
                return Ok(DeleteResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        // Start spinner while deleting the index.
        let _spinner = self
            .interaction
            .start_spinner("Deleting search index...".to_string())?;

        // Delete the index by name.
        if let Err(e) = mongodb_client
            .delete_search_index(database_name, collection_name, index_name.clone())
            .await
        {
            return Ok(DeleteResult::Failed {
                error: format!("failed to delete search index: {e}"),
            });
        }

        Ok(DeleteResult::Deleted { index_name })
    }
}

impl Delete {
    fn prompt_if_none(&self, field: &Option<String>, prompt: &str) -> Result<String> {
        match field {
            Some(value) => Ok(value.clone()),
            None => {
                match self.interaction.input(
                    InputPromptOptions::builder()
                        .message(prompt.to_string())
                        .build(),
                )? {
                    InputPromptResult::Input(value) => Ok(value),
                    InputPromptResult::Canceled => Err(anyhow!("user canceled the prompt")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependencies::mocks::MockMongoDB;
    use crate::interaction::SpinnerHandle;
    use crate::interaction::mocks::MockInteraction;

    // ============================================================================
    // Test Helpers
    // ============================================================================

    fn create_spinner_handle() -> SpinnerHandle {
        SpinnerHandle::new(Box::new(|| {}))
    }

    fn create_command(
        index_name: Option<String>,
        database_name: Option<String>,
        collection: Option<String>,
        force: bool,
        interaction: MockInteraction,
        mongodb_client: Result<MockMongoDB, TryToGetMongodbClientError>,
    ) -> Delete {
        Delete {
            index_name,
            database_name,
            collection,
            force,
            interaction: Box::new(interaction),
            mongodb_client: mongodb_client
                .map(|client| Box::new(client) as Box<dyn MongoDbClient + Send + Sync>),
        }
    }

    // ============================================================================
    // Happy Path Tests
    // ============================================================================

    #[tokio::test]
    async fn test_delete_with_force_all_args_provided() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .withf(|msg| msg == "Deleting search index...")
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_delete_search_index()
            .withf(|db, col, name| {
                db == "test_db" && col == "test_collection" && name == "my_index"
            })
            .return_once(|_, _, _| Ok(()));

        let mut cmd = create_command(
            Some("my_index".to_string()),
            Some("test_db".to_string()),
            Some("test_collection".to_string()),
            true,
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Deleted {
                index_name: "my_index".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_delete_with_confirmation_yes() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_confirm()
            .return_once(|_| Ok(ConfirmationPromptResult::Yes));
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_delete_search_index()
            .return_once(|_, _, _| Ok(()));

        let mut cmd = create_command(
            Some("index_name".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            false,
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Deleted {
                index_name: "index_name".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_delete_prompts_for_all_missing_fields() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(3)
            .returning(|options| match options.message.as_str() {
                "Database?" => Ok(InputPromptResult::Input("prompted_db".to_string())),
                "Collection?" => Ok(InputPromptResult::Input("prompted_col".to_string())),
                "Search Index Name?" => Ok(InputPromptResult::Input("prompted_name".to_string())),
                _ => panic!("Unexpected prompt: {}", options.message),
            });
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_delete_search_index()
            .withf(|db, col, name| {
                db == "prompted_db" && col == "prompted_col" && name == "prompted_name"
            })
            .return_once(|_, _, _| Ok(()));

        let mut cmd = create_command(None, None, None, true, mock_interaction, Ok(mock_mongodb));

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(
            result,
            DeleteResult::Deleted {
                index_name: "prompted_name".to_string()
            }
        );
    }

    // ============================================================================
    // Cancellation Tests
    // ============================================================================

    #[tokio::test]
    async fn test_delete_user_cancels_confirmation_no() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_confirm()
            .return_once(|_| Ok(ConfirmationPromptResult::No));

        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            false,
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(result, DeleteResult::Canceled);
    }

    #[tokio::test]
    async fn test_delete_user_cancels_confirmation_canceled() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_confirm()
            .return_once(|_| Ok(ConfirmationPromptResult::Canceled));

        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            false,
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(result, DeleteResult::Canceled);
    }

    #[tokio::test]
    async fn test_delete_user_cancels_database_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .return_once(|_| Ok(InputPromptResult::Canceled));

        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command(
            Some("idx".to_string()),
            None,
            Some("col".to_string()),
            true,
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DeleteResult::Failed { error } => {
                assert!(error.contains("user canceled"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_delete_user_cancels_collection_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(2)
            .returning(|options| match options.message.as_str() {
                "Database?" => Ok(InputPromptResult::Input("db".to_string())),
                "Collection?" => Ok(InputPromptResult::Canceled),
                _ => panic!("Unexpected prompt"),
            });

        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command(
            Some("idx".to_string()),
            None,
            None,
            true,
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DeleteResult::Failed { error } => {
                assert!(error.contains("user canceled"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_delete_user_cancels_index_name_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(3)
            .returning(|options| match options.message.as_str() {
                "Database?" => Ok(InputPromptResult::Input("db".to_string())),
                "Collection?" => Ok(InputPromptResult::Input("col".to_string())),
                "Search Index Name?" => Ok(InputPromptResult::Canceled),
                _ => panic!("Unexpected prompt"),
            });

        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command(None, None, None, true, mock_interaction, Ok(mock_mongodb));

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DeleteResult::Failed { error } => {
                assert!(error.contains("user canceled"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    // ============================================================================
    // Error Path Tests
    // ============================================================================

    #[tokio::test]
    async fn test_delete_mongodb_client_unavailable() {
        let mock_interaction = MockInteraction::new();

        let mut cmd = create_command(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            true,
            mock_interaction,
            Err(TryToGetMongodbClientError::ConnectingToDocker(
                anyhow::anyhow!("Docker not running"),
            )),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DeleteResult::Failed { error } => {
                assert!(error.contains("Docker"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_delete_deletion_fails() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_delete_search_index()
            .return_once(|_, _, _| Err(anyhow::anyhow!("permission denied")));

        let mut cmd = create_command(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            true,
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DeleteResult::Failed { error } => {
                assert!(error.contains("failed to delete search index"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    // ============================================================================
    // Display Tests
    // ============================================================================

    #[test]
    fn test_delete_result_display_deleted() {
        let result = DeleteResult::Deleted {
            index_name: "my_index".to_string(),
        };
        let output = format!("{}", result);
        assert!(output.contains("my_index"));
        assert!(output.contains("deleted"));
    }

    #[test]
    fn test_delete_result_display_failed() {
        let result = DeleteResult::Failed {
            error: "something went wrong".to_string(),
        };
        let output = format!("{}", result);
        assert!(output.contains("not deleted"));
        assert!(output.contains("something went wrong"));
    }

    #[test]
    fn test_delete_result_display_canceled() {
        let result = DeleteResult::Canceled;
        let output = format!("{}", result);
        assert!(output.contains("not deleted"));
    }
}
