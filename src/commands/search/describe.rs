//! Command to describe a search index for a local deployment.
//!
//! This module implements the `search indexes describe` command which retrieves and displays
//! details about a specific Atlas Search index by its ID in a local deployment.

use std::fmt::Display;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use mongodb::Client;
use serde::Serialize;

use crate::{
    args,
    commands::{
        CommandWithOutput,
        with_mongodb::{TryFromWithMongodbClient, TryToGetMongodbClientError},
    },
    dependencies::{SearchIndex, SearchIndexDescriber},
    interaction::{InputPrompt, InputPromptOptions, InputPromptResult, Interaction},
    table::Table,
};

/// Default search index type when none is specified.
const DEFAULT_INDEX_TYPE: &str = "search";

// Interaction dependencies for the describe command.
pub trait DescribeInteraction: InputPrompt + Send + Sync {}
impl<T: InputPrompt + Send + Sync> DescribeInteraction for T {}

// MongoDB dependencies for the describe command.
pub trait MongoDbClient: SearchIndexDescriber + Send + Sync {}
impl<T: SearchIndexDescriber + Send + Sync> MongoDbClient for T {}

/// Command to describe a search index for a local deployment.
pub struct Describe {
    index_id: Option<String>,

    interaction: Box<dyn DescribeInteraction>,
    mongodb_client: Result<Box<dyn MongoDbClient + Send + Sync>, TryToGetMongodbClientError>,
}

impl TryFromWithMongodbClient<args::search::Describe> for Describe {
    fn try_from_with_mongodb(
        args: args::search::Describe,
        client_result: Result<Client, TryToGetMongodbClientError>,
    ) -> Result<Self> {
        Ok(Self {
            index_id: args.index_id,

            interaction: Box::new(Interaction::new()),
            mongodb_client: client_result
                .map(|client| Box::new(client) as Box<dyn MongoDbClient + Send + Sync>),
        })
    }
}

/// Result of the describe command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum DescribeResult {
    Success { index: SearchIndex },
    Failed { error: String },
}

/// Convert the describe result into a table for display.
///
/// The table follows the same format as the Go CLI, with columns for
/// ID, NAME, DATABASE, COLLECTION, STATUS, and TYPE.
impl From<&DescribeResult> for Table {
    fn from(value: &DescribeResult) -> Self {
        match value {
            DescribeResult::Success { index } => Table::from_iter(
                std::iter::once(index),
                &[
                    ("ID", |i: &SearchIndex| i.index_id.clone()),
                    ("NAME", |i: &SearchIndex| i.name.clone()),
                    ("DATABASE", |i: &SearchIndex| i.database.clone()),
                    ("COLLECTION", |i: &SearchIndex| i.collection_name.clone()),
                    ("STATUS", |i: &SearchIndex| {
                        i.status.to_string().to_uppercase()
                    }),
                    ("TYPE", |i: &SearchIndex| {
                        i.index_type
                            .clone()
                            .unwrap_or_else(|| DEFAULT_INDEX_TYPE.to_string())
                    }),
                ],
            ),
            DescribeResult::Failed { .. } => Table::new(vec![], vec![]),
        }
    }
}

/// Format the describe result for display.
impl Display for DescribeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DescribeResult::Success { .. } => Table::from(self).fmt(f),
            DescribeResult::Failed { error } => write!(f, "Describing index failed: {}", error),
        }
    }
}

#[async_trait]
impl CommandWithOutput for Describe {
    type Output = DescribeResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        // Prompt for index ID if not provided.
        let index_id = match self
            .prompt_if_none(&self.index_id.clone(), "Search Index ID?")
            .await
        {
            Ok(id) => id,
            Err(e) => {
                return Ok(DescribeResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        // Get the mongodb client, if it is not available, return a failed result.
        let mongodb_client = match &self.mongodb_client {
            Ok(client) => client,
            Err(e) => {
                return Ok(DescribeResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        // Describe the search index.
        match mongodb_client.describe_search_index(index_id.clone()).await {
            Ok(Some(index)) => Ok(DescribeResult::Success { index }),
            Ok(None) => Ok(DescribeResult::Failed {
                error: format!("search index with ID '{}' not found", index_id),
            }),
            Err(e) => Ok(DescribeResult::Failed {
                error: format!("failed to describe search index: {e}"),
            }),
        }
    }
}

impl Describe {
    async fn prompt_if_none(&self, field: &Option<String>, prompt: &str) -> Result<String> {
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
    use crate::dependencies::MongoDbSearchIndexStatus;
    use crate::dependencies::mocks::MockMongoDB;
    use crate::interaction::mocks::MockInteraction;

    // ============================================================================
    // Test Helpers
    // ============================================================================

    fn create_command(
        index_id: Option<String>,
        interaction: MockInteraction,
        mongodb_client: Result<MockMongoDB, TryToGetMongodbClientError>,
    ) -> Describe {
        Describe {
            index_id,
            interaction: Box::new(interaction),
            mongodb_client: mongodb_client
                .map(|client| Box::new(client) as Box<dyn MongoDbClient + Send + Sync>),
        }
    }

    fn create_sample_search_index(id: &str, name: &str) -> SearchIndex {
        SearchIndex {
            index_id: id.to_string(),
            name: name.to_string(),
            database: "test_db".to_string(),
            collection_name: "test_collection".to_string(),
            status: MongoDbSearchIndexStatus::Ready,
            index_type: Some("search".to_string()),
        }
    }

    // ============================================================================
    // Happy Path Tests
    // ============================================================================

    #[tokio::test]
    async fn test_describe_with_index_id_provided() {
        let mock_interaction = MockInteraction::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_describe_search_index()
            .withf(|id| id == "idx-123")
            .return_once(|_| Ok(Some(create_sample_search_index("idx-123", "my_index"))));

        let mut cmd = create_command(
            Some("idx-123".to_string()),
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DescribeResult::Success { index } => {
                assert_eq!(index.index_id, "idx-123");
                assert_eq!(index.name, "my_index");
            }
            DescribeResult::Failed { error } => panic!("Expected success, got error: {}", error),
        }
    }

    #[tokio::test]
    async fn test_describe_prompts_for_missing_index_id() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(1)
            .returning(|options| {
                assert_eq!(options.message, "Search Index ID?");
                Ok(InputPromptResult::Input("prompted-id".to_string()))
            });

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_describe_search_index()
            .withf(|id| id == "prompted-id")
            .return_once(|_| Ok(Some(create_sample_search_index("prompted-id", "my_index"))));

        let mut cmd = create_command(None, mock_interaction, Ok(mock_mongodb));

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DescribeResult::Success { index } => {
                assert_eq!(index.index_id, "prompted-id");
            }
            DescribeResult::Failed { error } => panic!("Expected success, got error: {}", error),
        }
    }

    // ============================================================================
    // Error Path Tests
    // ============================================================================

    #[tokio::test]
    async fn test_describe_index_not_found() {
        let mock_interaction = MockInteraction::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_describe_search_index()
            .return_once(|_| Ok(None));

        let mut cmd = create_command(
            Some("nonexistent-id".to_string()),
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DescribeResult::Failed { error } => {
                assert!(error.contains("not found"));
                assert!(error.contains("nonexistent-id"));
            }
            DescribeResult::Success { .. } => panic!("Expected failure"),
        }
    }

    #[tokio::test]
    async fn test_describe_mongodb_client_unavailable() {
        let mock_interaction = MockInteraction::new();

        let mut cmd = create_command(
            Some("idx-123".to_string()),
            mock_interaction,
            Err(TryToGetMongodbClientError::ConnectingToDocker(anyhow!(
                "Docker not running"
            ))),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DescribeResult::Failed { error } => {
                assert!(error.contains("Docker"));
            }
            DescribeResult::Success { .. } => panic!("Expected failure"),
        }
    }

    #[tokio::test]
    async fn test_describe_mongodb_query_fails() {
        let mock_interaction = MockInteraction::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_describe_search_index()
            .return_once(|_| Err(anyhow!("connection lost")));

        let mut cmd = create_command(
            Some("idx-123".to_string()),
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DescribeResult::Failed { error } => {
                assert!(error.contains("failed to describe search index"));
            }
            DescribeResult::Success { .. } => panic!("Expected failure"),
        }
    }

    #[tokio::test]
    async fn test_describe_user_cancels_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .return_once(|_| Ok(InputPromptResult::Canceled));

        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command(None, mock_interaction, Ok(mock_mongodb));

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            DescribeResult::Failed { error } => {
                assert!(error.contains("user canceled"));
            }
            DescribeResult::Success { .. } => panic!("Expected failure"),
        }
    }

    // ============================================================================
    // Display Tests
    // ============================================================================

    #[test]
    fn test_describe_result_display_success() {
        let result = DescribeResult::Success {
            index: create_sample_search_index("idx-1", "my_index"),
        };
        let output = format!("{}", result);
        assert!(output.contains("idx-1"));
        assert!(output.contains("my_index"));
        assert!(output.contains("test_db"));
        assert!(output.contains("test_collection"));
        assert!(output.contains("READY"));
        assert!(output.contains("search"));
    }

    #[test]
    fn test_describe_result_display_failed() {
        let result = DescribeResult::Failed {
            error: "something went wrong".to_string(),
        };
        let output = format!("{}", result);
        assert!(output.contains("failed"));
        assert!(output.contains("something went wrong"));
    }

    #[test]
    fn test_describe_result_default_index_type() {
        let mut index = create_sample_search_index("idx-1", "my_index");
        index.index_type = None;

        let result = DescribeResult::Success { index };
        let output = format!("{}", result);
        // Should show default type "search"
        assert!(output.contains("search"));
    }
}
