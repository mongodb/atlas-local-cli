//! Command to list all search indexes for a local deployment.
//!
//! This module implements the `search indexes list` command which retrieves and displays
//! all Atlas Search indexes for a specified database and collection in a local deployment.

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
    dependencies::{SearchIndex, SearchIndexLister},
    interaction::{InputPrompt, InputPromptOptions, InputPromptResult, Interaction},
    table::Table,
};

/// Default search index type when none is specified.
const DEFAULT_INDEX_TYPE: &str = "search";

// Interaction dependencies for the list command.
pub trait ListInteraction: InputPrompt + Send + Sync {}
impl<T: InputPrompt + Send + Sync> ListInteraction for T {}

// MongoDB dependencies for the list command.
pub trait MongoDbClient: SearchIndexLister + Send + Sync {}
impl<T: SearchIndexLister + Send + Sync> MongoDbClient for T {}

/// Command to list all search indexes for a local deployment.
pub struct List {
    database_name: Option<String>,
    collection: Option<String>,

    interaction: Box<dyn ListInteraction>,
    mongodb_client: Result<Box<dyn MongoDbClient + Send + Sync>, TryToGetMongodbClientError>,
}

impl TryFromWithMongodbClient<args::search::List> for List {
    fn try_from_with_mongodb(
        args: args::search::List,
        client_result: Result<Client, TryToGetMongodbClientError>,
    ) -> Result<Self> {
        Ok(Self {
            database_name: args.database_name,
            collection: args.collection,

            interaction: Box::new(Interaction::new()),
            mongodb_client: client_result
                .map(|client| Box::new(client) as Box<dyn MongoDbClient + Send + Sync>),
        })
    }
}

/// Result of the list command.
///
/// We're using a newtype pattern to wrap the vector of search indexes.
/// This allows us to implement traits like [`Display`] and table conversion on the result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ListResult {
    Success { indexes: Vec<SearchIndex> },
    Failed { error: String },
}

/// Convert the list result into a table for display.
///
/// The table follows the same format as the Go CLI, with columns for
/// ID, NAME, DATABASE, COLLECTION, STATUS, and TYPE.
impl From<&ListResult> for Table {
    fn from(value: &ListResult) -> Self {
        match value {
            ListResult::Success { indexes } => Table::from_iter(
                indexes,
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
            ListResult::Failed { .. } => Table::new(vec![], vec![]),
        }
    }
}

/// Format the list result for display.
impl Display for ListResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ListResult::Success { .. } => Table::from(self).fmt(f),
            ListResult::Failed { error } => write!(f, "Listing indexes failed: {}", error),
        }
    }
}

#[async_trait]
impl CommandWithOutput for List {
    type Output = ListResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        // Prompt for database name if not provided.
        let database_name = match self
            .prompt_if_none(&self.database_name.clone(), "Database?")
            .await
        {
            Ok(name) => name,
            Err(e) => {
                return Ok(ListResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        // Prompt for collection name if not provided.
        let collection_name = match self
            .prompt_if_none(&self.collection.clone(), "Collection?")
            .await
        {
            Ok(name) => name,
            Err(e) => {
                return Ok(ListResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        // Get the mongodb client, if it is not available, return a failed result.
        let mongodb_client = match &self.mongodb_client {
            Ok(client) => client,
            Err(e) => {
                return Ok(ListResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        // List the search indexes.
        match mongodb_client
            .list_search_indexes(database_name, collection_name)
            .await
        {
            Ok(indexes) => Ok(ListResult::Success { indexes }),
            Err(e) => Ok(ListResult::Failed {
                error: format!("failed to list search indexes: {e}"),
            }),
        }
    }
}

impl List {
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
        database_name: Option<String>,
        collection: Option<String>,
        interaction: MockInteraction,
        mongodb_client: Result<MockMongoDB, TryToGetMongodbClientError>,
    ) -> List {
        List {
            database_name,
            collection,
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
    async fn test_list_with_all_args_provided() {
        let mock_interaction = MockInteraction::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_list_search_indexes()
            .withf(|db, col| db == "test_db" && col == "test_collection")
            .return_once(|_, _| Ok(vec![create_sample_search_index("idx-1", "my_index")]));

        let mut cmd = create_command(
            Some("test_db".to_string()),
            Some("test_collection".to_string()),
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            ListResult::Success { indexes } => {
                assert_eq!(indexes.len(), 1);
                assert_eq!(indexes[0].index_id, "idx-1");
                assert_eq!(indexes[0].name, "my_index");
            }
            ListResult::Failed { error } => panic!("Expected success, got error: {}", error),
        }
    }

    #[tokio::test]
    async fn test_list_prompts_for_missing_database() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(1)
            .returning(|options| {
                assert_eq!(options.message, "Database?");
                Ok(InputPromptResult::Input("prompted_db".to_string()))
            });

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_list_search_indexes()
            .withf(|db, col| db == "prompted_db" && col == "test_collection")
            .return_once(|_, _| Ok(vec![]));

        let mut cmd = create_command(
            None,
            Some("test_collection".to_string()),
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            ListResult::Success { indexes } => assert!(indexes.is_empty()),
            ListResult::Failed { error } => panic!("Expected success, got error: {}", error),
        }
    }

    #[tokio::test]
    async fn test_list_prompts_for_missing_collection() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(1)
            .returning(|options| {
                assert_eq!(options.message, "Collection?");
                Ok(InputPromptResult::Input("prompted_col".to_string()))
            });

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_list_search_indexes()
            .withf(|db, col| db == "test_db" && col == "prompted_col")
            .return_once(|_, _| Ok(vec![]));

        let mut cmd = create_command(
            Some("test_db".to_string()),
            None,
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            ListResult::Success { indexes } => assert!(indexes.is_empty()),
            ListResult::Failed { error } => panic!("Expected success, got error: {}", error),
        }
    }

    #[tokio::test]
    async fn test_list_prompts_for_both_missing() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(2)
            .returning(|options| match options.message.as_str() {
                "Database?" => Ok(InputPromptResult::Input("prompted_db".to_string())),
                "Collection?" => Ok(InputPromptResult::Input("prompted_col".to_string())),
                _ => panic!("Unexpected prompt: {}", options.message),
            });

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_list_search_indexes()
            .withf(|db, col| db == "prompted_db" && col == "prompted_col")
            .return_once(|_, _| Ok(vec![]));

        let mut cmd = create_command(None, None, mock_interaction, Ok(mock_mongodb));

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            ListResult::Success { indexes } => assert!(indexes.is_empty()),
            ListResult::Failed { error } => panic!("Expected success, got error: {}", error),
        }
    }

    #[tokio::test]
    async fn test_list_multiple_indexes() {
        let mock_interaction = MockInteraction::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_list_search_indexes()
            .return_once(|_, _| {
                Ok(vec![
                    create_sample_search_index("idx-1", "index_a"),
                    create_sample_search_index("idx-2", "index_b"),
                    create_sample_search_index("idx-3", "index_c"),
                ])
            });

        let mut cmd = create_command(
            Some("db".to_string()),
            Some("col".to_string()),
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            ListResult::Success { indexes } => {
                assert_eq!(indexes.len(), 3);
            }
            ListResult::Failed { error } => panic!("Expected success, got error: {}", error),
        }
    }

    // ============================================================================
    // Error Path Tests
    // ============================================================================

    #[tokio::test]
    async fn test_list_mongodb_client_unavailable() {
        let mock_interaction = MockInteraction::new();

        let mut cmd = create_command(
            Some("db".to_string()),
            Some("col".to_string()),
            mock_interaction,
            Err(TryToGetMongodbClientError::ConnectingToDocker(anyhow!(
                "Docker not running"
            ))),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            ListResult::Failed { error } => {
                assert!(error.contains("Docker"));
            }
            ListResult::Success { .. } => panic!("Expected failure"),
        }
    }

    #[tokio::test]
    async fn test_list_mongodb_query_fails() {
        let mock_interaction = MockInteraction::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_list_search_indexes()
            .return_once(|_, _| Err(anyhow!("connection lost")));

        let mut cmd = create_command(
            Some("db".to_string()),
            Some("col".to_string()),
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            ListResult::Failed { error } => {
                assert!(error.contains("failed to list search indexes"));
            }
            ListResult::Success { .. } => panic!("Expected failure"),
        }
    }

    #[tokio::test]
    async fn test_list_user_cancels_database_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .return_once(|_| Ok(InputPromptResult::Canceled));

        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command(
            None,
            Some("col".to_string()),
            mock_interaction,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            ListResult::Failed { error } => {
                assert!(error.contains("user canceled"));
            }
            ListResult::Success { .. } => panic!("Expected failure"),
        }
    }

    #[tokio::test]
    async fn test_list_user_cancels_collection_prompt() {
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

        let mut cmd = create_command(None, None, mock_interaction, Ok(mock_mongodb));

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            ListResult::Failed { error } => {
                assert!(error.contains("user canceled"));
            }
            ListResult::Success { .. } => panic!("Expected failure"),
        }
    }

    // ============================================================================
    // Display Tests
    // ============================================================================

    #[test]
    fn test_list_result_display_success_with_indexes() {
        let result = ListResult::Success {
            indexes: vec![create_sample_search_index("idx-1", "my_index")],
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
    fn test_list_result_display_success_empty() {
        let result = ListResult::Success { indexes: vec![] };
        let output = format!("{}", result);
        // Should have headers but no data rows
        assert!(output.contains("ID"));
        assert!(output.contains("NAME"));
    }

    #[test]
    fn test_list_result_display_failed() {
        let result = ListResult::Failed {
            error: "something went wrong".to_string(),
        };
        let output = format!("{}", result);
        assert!(output.contains("failed"));
        assert!(output.contains("something went wrong"));
    }

    #[test]
    fn test_list_result_default_index_type() {
        let mut index = create_sample_search_index("idx-1", "my_index");
        index.index_type = None;

        let result = ListResult::Success {
            indexes: vec![index],
        };
        let output = format!("{}", result);
        // Should show default type "search"
        assert!(output.contains("search"));
    }
}
