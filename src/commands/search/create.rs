use std::{
    fmt::Display,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use mongodb::{
    Client,
    bson::{self, doc},
};
use serde::Serialize;
use tracing::debug;

use crate::{
    args,
    commands::{
        CommandWithOutput,
        search::file_definition::SearchIndexCreateRequest,
        with_mongodb::{TryFromWithMongodbClient, TryToGetMongodbClientError},
    },
    dependencies::{
        CreateSearchIndexModel, FileReader, MongoDbSearchIndexStatus, SearchIndexCreator,
        SearchIndexStatusGetter, SearchIndexType, TokioFs,
    },
    interaction::{InputPrompt, Interaction, SpinnerInteraction},
};

// Interaction dependencies for the delete command
pub trait CreateInteraction: InputPrompt + SpinnerInteraction + Send + Sync {}
impl<T: InputPrompt + SpinnerInteraction + Send + Sync> CreateInteraction for T {}

// MongoDB dependencies for the create command
pub trait MongoDbClient: SearchIndexCreator + SearchIndexStatusGetter + Send + Sync {}
impl<T: SearchIndexCreator + SearchIndexStatusGetter + Send + Sync> MongoDbClient for T {}

pub struct Create {
    watch_interval: Duration,
    watch: bool,
    definition_source: IndexDefinitionSource,

    interaction: Box<dyn CreateInteraction>,
    file_reader: Box<dyn FileReader + Send + Sync>,
    mongodb_client: Result<Box<dyn MongoDbClient + Send + Sync>, TryToGetMongodbClientError>,
}

enum IndexDefinitionSource {
    Flags(IndexDefinitionSourceFlags),
    File(PathBuf),
}

struct IndexDefinitionSourceFlags {
    index_name: Option<String>,
    database_name: Option<String>,
    collection: Option<String>,
}

impl TryFromWithMongodbClient<args::search::Create> for Create {
    fn try_from_with_mongodb(
        args: args::search::Create,
        client_result: Result<Client, TryToGetMongodbClientError>,
    ) -> Result<Self> {
        // Convert the arguments into an index definition source.
        // If a file is provided, use it as the index definition source.
        // Otherwise, use the flags to create the index definition source.
        // When a flag is not provided, we will prompt the user for the missing information.
        let definition_source = match args.file {
            Some(file) => IndexDefinitionSource::File(PathBuf::from(file)),
            None => IndexDefinitionSource::Flags(IndexDefinitionSourceFlags {
                index_name: args.index_name,
                database_name: args.database_name,
                collection: args.collection,
            }),
        };

        Ok(Self {
            watch: args.watch,
            watch_interval: Duration::from_secs(1),

            definition_source,

            interaction: Box::new(Interaction::new()),
            file_reader: Box::new(TokioFs::new()),
            // It is possible that the mongodb client is not created successfully, so we need to return a result.
            // We'll handle the result in the `execute` method. This way we can return a `Failed` result if the mongodb client is not created successfully.
            mongodb_client: client_result
                .map(|client| Box::new(client) as Box<dyn MongoDbClient + Send + Sync>),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CreateResult {
    Created { search_index_id: String },
    Failed { error: String },
}

impl Display for CreateResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created { search_index_id } => {
                write!(f, "Search index created with ID: {search_index_id}")
            }
            Self::Failed { error } => write!(f, "Creating index failed: {}", error),
        }
    }
}

#[async_trait]
impl CommandWithOutput for Create {
    type Output = CreateResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        // Build the index definition
        debug!("building index definition");
        let create_search_index_model_result = match &self.definition_source {
            IndexDefinitionSource::Flags(flags) => {
                self.build_index_definition_from_flags(flags).await
            }
            IndexDefinitionSource::File(file) => {
                self.create_search_index_model_from_file(file).await
            }
        };
        debug!(
            ?create_search_index_model_result,
            "create search index model result"
        );

        // If the index definition is not built successfully, return a failed result.
        let create_search_index_model = match create_search_index_model_result {
            Ok(create_search_index_model) => create_search_index_model,
            Err(e) => {
                return Ok(CreateResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        debug!("verifying mongodb client");

        // Get the mongodb client, if it is not available, return a failed result.
        let mongodb_client = match &self.mongodb_client {
            Ok(client) => client,
            Err(e) => {
                return Ok(CreateResult::Failed {
                    error: e.to_string(),
                });
            }
        };

        debug!("mongodb client available, creating search index");

        // Create the search index.
        let search_index_id = match mongodb_client
            .create_search_index(create_search_index_model.clone())
            .await
        {
            Ok(search_index_id) => search_index_id,
            Err(e) => {
                return Ok(CreateResult::Failed {
                    error: format!("failed to create search index: {e}"),
                });
            }
        };

        debug!(search_index_id, "search index created");

        if self.watch {
            debug!("watching enabled, watching search index");

            let _watch_spinner_handle = self
                .interaction
                .start_spinner("Building search index...".to_string())?;

            loop {
                match mongodb_client
                    .get_search_index_status(
                        create_search_index_model.database_name.clone(),
                        create_search_index_model.collection_name.clone(),
                        search_index_id.clone(),
                    )
                    .await
                {
                    Err(e) => {
                        return Ok(CreateResult::Failed {
                            error: format!(
                                "failed to get search index status while watching the search index: {e}"
                            ),
                        });
                    }
                    Ok(None) => {
                        return Ok(CreateResult::Failed {
                            error: "failed to get search index status while watching the search index, the search index does not exist".to_string(),
                        });
                    }
                    Ok(Some(status)) => match status {
                        MongoDbSearchIndexStatus::Ready => break,
                        MongoDbSearchIndexStatus::DoesNotExist
                        | MongoDbSearchIndexStatus::Deleting
                        | MongoDbSearchIndexStatus::Failed => {
                            return Ok(CreateResult::Failed {
                                error: format!(
                                    "failed to get search index status while watching the search index, the search index is not ready: {status}"
                                ),
                            });
                        }
                        MongoDbSearchIndexStatus::Pending
                        | MongoDbSearchIndexStatus::Building
                        | MongoDbSearchIndexStatus::Stale => {
                            tokio::time::sleep(self.watch_interval).await;
                        }
                    },
                }
            }

            debug!("watch loop exited, search index is ready");
        } else {
            debug!("watching disabled, skipping watch");
        }

        Ok(CreateResult::Created { search_index_id })
    }
}

impl Create {
    async fn create_search_index_model_from_file(
        &self,
        path: &Path,
    ) -> Result<CreateSearchIndexModel> {
        let file = self
            .file_reader
            .read_to_string(path)
            .await
            .with_context(|| format!("failed to read file at path: {path:?}"))?;
        let index_definition: SearchIndexCreateRequest = serde_json::from_str(&file)
            .map_err(|e| anyhow!("failed to parse file as search index create request: {e}"))?;
        let create_search_index_model = CreateSearchIndexModel {
            database_name: index_definition.database,
            collection_name: index_definition.collection_name,
            definition: index_definition
                .definition
                .and_then(|value| bson::to_document(&value).ok())
                .unwrap_or_default(),
            name: Some(index_definition.name),
            index_type: index_definition.index_type,
        };

        Ok(create_search_index_model)
    }

    async fn build_index_definition_from_flags(
        &self,
        flags: &IndexDefinitionSourceFlags,
    ) -> Result<CreateSearchIndexModel> {
        // Prompt the user for the missing fields if they are not provided.
        let index_name = self
            .interaction
            .prompt_if_none(flags.index_name.as_deref(), "Search Index Name?")?;

        let database_name = self
            .interaction
            .prompt_if_none(flags.database_name.as_deref(), "Database?")?;

        let collection_name = self
            .interaction
            .prompt_if_none(flags.collection.as_deref(), "Collection?")?;

        let create_search_index_model = CreateSearchIndexModel {
            database_name,
            collection_name,
            definition: doc! {
                "analyzer": "lucene.standard",
                "searchAnalyzer": "lucene.standard",
                "mappings": {
                    "dynamic": true,
                },
            },
            name: Some(index_name),
            index_type: Some(SearchIndexType::Search),
        };

        Ok(create_search_index_model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependencies::mocks::{MockMongoDB, MockTokioFs};
    use crate::interaction::mocks::MockInteraction;
    use crate::interaction::{InputPromptResult, SpinnerHandle};
    use std::path::PathBuf;
    use std::time::Duration;

    // ============================================================================
    // Test Helpers
    // ============================================================================

    fn create_spinner_handle() -> SpinnerHandle {
        SpinnerHandle::new(Box::new(|| {}))
    }

    fn create_command_from_flags(
        index_name: Option<String>,
        database_name: Option<String>,
        collection: Option<String>,
        watch: bool,
        interaction: MockInteraction,
        file_reader: MockTokioFs,
        mongodb_client: Result<MockMongoDB, TryToGetMongodbClientError>,
    ) -> Create {
        Create {
            watch_interval: Duration::from_millis(10),
            watch,
            definition_source: IndexDefinitionSource::Flags(IndexDefinitionSourceFlags {
                index_name,
                database_name,
                collection,
            }),
            interaction: Box::new(interaction),
            file_reader: Box::new(file_reader),
            mongodb_client: mongodb_client
                .map(|client| Box::new(client) as Box<dyn MongoDbClient + Send + Sync>),
        }
    }

    fn create_command_from_file(
        file_path: PathBuf,
        watch: bool,
        interaction: MockInteraction,
        file_reader: MockTokioFs,
        mongodb_client: Result<MockMongoDB, TryToGetMongodbClientError>,
    ) -> Create {
        Create {
            watch_interval: Duration::from_millis(10),
            watch,
            definition_source: IndexDefinitionSource::File(file_path),
            interaction: Box::new(interaction),
            file_reader: Box::new(file_reader),
            mongodb_client: mongodb_client
                .map(|client| Box::new(client) as Box<dyn MongoDbClient + Send + Sync>),
        }
    }

    // ============================================================================
    // Happy Path Tests - From Flags
    // ============================================================================

    #[tokio::test]
    async fn test_create_from_flags_all_provided_no_watch() {
        let mock_interaction = MockInteraction::new();
        let mock_file_reader = MockTokioFs::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .withf(|model| {
                model.database_name == "test_db"
                    && model.collection_name == "test_collection"
                    && model.name == Some("test_index".to_string())
            })
            .return_once(|_| Ok("index-123".to_string()));

        let mut cmd = create_command_from_flags(
            Some("test_index".to_string()),
            Some("test_db".to_string()),
            Some("test_collection".to_string()),
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(
            result,
            CreateResult::Created {
                search_index_id: "index-123".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_create_from_flags_prompts_for_missing_fields() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(3)
            .returning(|options| match options.message.as_str() {
                "Search Index Name?" => Ok(InputPromptResult::Input("prompted_index".to_string())),
                "Database?" => Ok(InputPromptResult::Input("prompted_db".to_string())),
                "Collection?" => Ok(InputPromptResult::Input("prompted_col".to_string())),
                _ => panic!("Unexpected prompt: {}", options.message),
            });

        let mock_file_reader = MockTokioFs::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .withf(|model| {
                model.database_name == "prompted_db"
                    && model.collection_name == "prompted_col"
                    && model.name == Some("prompted_index".to_string())
            })
            .return_once(|_| Ok("index-456".to_string()));

        let mut cmd = create_command_from_flags(
            None,
            None,
            None,
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(
            result,
            CreateResult::Created {
                search_index_id: "index-456".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_create_from_flags_with_watch_becomes_ready() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mock_file_reader = MockTokioFs::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .return_once(|_| Ok("index-789".to_string()));

        // Simulate: Building -> Ready
        let status_sequence = std::sync::atomic::AtomicUsize::new(0);
        mock_mongodb
            .expect_get_search_index_status()
            .times(2)
            .returning(move |_, _, _| {
                let count = status_sequence.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match count {
                    0 => Ok(Some(MongoDbSearchIndexStatus::Building)),
                    _ => Ok(Some(MongoDbSearchIndexStatus::Ready)),
                }
            });

        let mut cmd = create_command_from_flags(
            Some("test_index".to_string()),
            Some("test_db".to_string()),
            Some("test_collection".to_string()),
            true,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(
            result,
            CreateResult::Created {
                search_index_id: "index-789".to_string()
            }
        );
    }

    // ============================================================================
    // Happy Path Tests - From File
    // ============================================================================

    #[tokio::test]
    async fn test_create_from_file_no_watch() {
        let mock_interaction = MockInteraction::new();

        let mut mock_file_reader = MockTokioFs::new();
        mock_file_reader.expect_read_to_string().return_once(|_| {
            Ok(r#"{
                    "collectionName": "file_collection",
                    "database": "file_db",
                    "name": "file_index",
                    "type": "search"
                }"#
            .to_string())
        });

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .withf(|model| {
                model.database_name == "file_db"
                    && model.collection_name == "file_collection"
                    && model.name == Some("file_index".to_string())
            })
            .return_once(|_| Ok("file-index-id".to_string()));

        let mut cmd = create_command_from_file(
            PathBuf::from("/path/to/index.json"),
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(
            result,
            CreateResult::Created {
                search_index_id: "file-index-id".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_create_from_file_with_definition() {
        let mock_interaction = MockInteraction::new();

        let mut mock_file_reader = MockTokioFs::new();
        mock_file_reader.expect_read_to_string().return_once(|_| {
            Ok(r#"{
                    "collectionName": "col",
                    "database": "db",
                    "name": "idx",
                    "definition": {"mappings": {"dynamic": false}}
                }"#
            .to_string())
        });

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .withf(|model| {
                // Check that the definition contains the expected nested structure
                model.definition.get_document("mappings").is_ok()
                    && model
                        .definition
                        .get_document("mappings")
                        .unwrap()
                        .get_bool("dynamic")
                        == Ok(false)
            })
            .return_once(|_| Ok("def-index-id".to_string()));

        let mut cmd = create_command_from_file(
            PathBuf::from("/path/to/index.json"),
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(
            result,
            CreateResult::Created {
                search_index_id: "def-index-id".to_string()
            }
        );
    }

    // ============================================================================
    // Error Path Tests - File Reading
    // ============================================================================

    #[tokio::test]
    async fn test_create_from_file_read_error() {
        let mock_interaction = MockInteraction::new();

        let mut mock_file_reader = MockTokioFs::new();
        mock_file_reader
            .expect_read_to_string()
            .return_once(|_| Err(anyhow!("file not found")));

        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command_from_file(
            PathBuf::from("/nonexistent.json"),
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("failed to read file"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_create_from_file_invalid_json() {
        let mock_interaction = MockInteraction::new();

        let mut mock_file_reader = MockTokioFs::new();
        mock_file_reader
            .expect_read_to_string()
            .return_once(|_| Ok("{ invalid json".to_string()));

        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command_from_file(
            PathBuf::from("/invalid.json"),
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("failed to parse file"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    // ============================================================================
    // Error Path Tests - MongoDB Client
    // ============================================================================

    #[tokio::test]
    async fn test_create_mongodb_client_unavailable() {
        let mock_interaction = MockInteraction::new();
        let mock_file_reader = MockTokioFs::new();

        let mut cmd = create_command_from_flags(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            false,
            mock_interaction,
            mock_file_reader,
            Err(TryToGetMongodbClientError::ConnectingToDocker(anyhow!(
                "Docker not running"
            ))),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("Docker"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_create_search_index_creation_fails() {
        let mock_interaction = MockInteraction::new();
        let mock_file_reader = MockTokioFs::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .return_once(|_| Err(anyhow!("duplicate index name")));

        let mut cmd = create_command_from_flags(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("failed to create search index"));
                assert!(error.contains("duplicate index name"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    // ============================================================================
    // Error Path Tests - User Cancellation
    // ============================================================================

    #[tokio::test]
    async fn test_create_user_cancels_index_name_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .return_once(|_| Ok(InputPromptResult::Canceled));

        let mock_file_reader = MockTokioFs::new();
        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command_from_flags(
            None,
            Some("db".to_string()),
            Some("col".to_string()),
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("user canceled"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_create_user_cancels_database_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(2)
            .returning(|options| match options.message.as_str() {
                "Search Index Name?" => Ok(InputPromptResult::Input("idx".to_string())),
                "Database?" => Ok(InputPromptResult::Canceled),
                _ => panic!("Unexpected prompt"),
            });

        let mock_file_reader = MockTokioFs::new();
        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command_from_flags(
            None,
            None,
            Some("col".to_string()),
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("user canceled"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_create_user_cancels_collection_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_input()
            .times(3)
            .returning(|options| match options.message.as_str() {
                "Search Index Name?" => Ok(InputPromptResult::Input("idx".to_string())),
                "Database?" => Ok(InputPromptResult::Input("db".to_string())),
                "Collection?" => Ok(InputPromptResult::Canceled),
                _ => panic!("Unexpected prompt"),
            });

        let mock_file_reader = MockTokioFs::new();
        let mock_mongodb = MockMongoDB::new();

        let mut cmd = create_command_from_flags(
            None,
            None,
            None,
            false,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("user canceled"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    // ============================================================================
    // Watch Mode Error Tests
    // ============================================================================

    #[tokio::test]
    async fn test_create_watch_status_query_fails() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mock_file_reader = MockTokioFs::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .return_once(|_| Ok("idx-id".to_string()));
        mock_mongodb
            .expect_get_search_index_status()
            .return_once(|_, _, _| Err(anyhow!("connection lost")));

        let mut cmd = create_command_from_flags(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            true,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("failed to get search index status"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_create_watch_index_does_not_exist() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mock_file_reader = MockTokioFs::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .return_once(|_| Ok("idx-id".to_string()));
        mock_mongodb
            .expect_get_search_index_status()
            .return_once(|_, _, _| Ok(None));

        let mut cmd = create_command_from_flags(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            true,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("does not exist"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_create_watch_index_enters_failed_state() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mock_file_reader = MockTokioFs::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .return_once(|_| Ok("idx-id".to_string()));
        mock_mongodb
            .expect_get_search_index_status()
            .return_once(|_, _, _| Ok(Some(MongoDbSearchIndexStatus::Failed)));

        let mut cmd = create_command_from_flags(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            true,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("not ready"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_create_watch_index_enters_deleting_state() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mock_file_reader = MockTokioFs::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .return_once(|_| Ok("idx-id".to_string()));
        mock_mongodb
            .expect_get_search_index_status()
            .return_once(|_, _, _| Ok(Some(MongoDbSearchIndexStatus::Deleting)));

        let mut cmd = create_command_from_flags(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            true,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        match result {
            CreateResult::Failed { error } => {
                assert!(error.contains("not ready"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_create_watch_pending_then_ready() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_start_spinner()
            .return_once(|_| Ok(create_spinner_handle()));

        let mock_file_reader = MockTokioFs::new();

        let mut mock_mongodb = MockMongoDB::new();
        mock_mongodb
            .expect_create_search_index()
            .return_once(|_| Ok("idx-id".to_string()));

        let status_sequence = std::sync::atomic::AtomicUsize::new(0);
        mock_mongodb
            .expect_get_search_index_status()
            .times(3)
            .returning(move |_, _, _| {
                let count = status_sequence.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match count {
                    0 => Ok(Some(MongoDbSearchIndexStatus::Pending)),
                    1 => Ok(Some(MongoDbSearchIndexStatus::Stale)),
                    _ => Ok(Some(MongoDbSearchIndexStatus::Ready)),
                }
            });

        let mut cmd = create_command_from_flags(
            Some("idx".to_string()),
            Some("db".to_string()),
            Some("col".to_string()),
            true,
            mock_interaction,
            mock_file_reader,
            Ok(mock_mongodb),
        );

        let result = cmd.execute().await.expect("execute should succeed");

        assert_eq!(
            result,
            CreateResult::Created {
                search_index_id: "idx-id".to_string()
            }
        );
    }

    // ============================================================================
    // Display Tests
    // ============================================================================

    #[test]
    fn test_create_result_display_created() {
        let result = CreateResult::Created {
            search_index_id: "abc-123".to_string(),
        };
        let output = format!("{}", result);
        assert!(output.contains("abc-123"));
        assert!(output.contains("created"));
    }

    #[test]
    fn test_create_result_display_failed() {
        let result = CreateResult::Failed {
            error: "something went wrong".to_string(),
        };
        let output = format!("{}", result);
        assert!(output.contains("failed"));
        assert!(output.contains("something went wrong"));
    }
}
