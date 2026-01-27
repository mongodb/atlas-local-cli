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
    interaction::{
        InputPrompt, InputPromptOptions, InputPromptResult, Interaction, SpinnerInteraction,
    },
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
            .prompt_if_none(&flags.index_name, "Search Index Name?")
            .await?;

        let database_name = self
            .prompt_if_none(&flags.database_name, "Database?")
            .await?;

        let collection_name = self
            .prompt_if_none(&flags.collection, "Collection?")
            .await?;

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
