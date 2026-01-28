use std::fmt::Display;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::{Client, SearchIndexModel, bson::Document};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

/// A search index as returned by list_search_indexes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchIndex {
    /// The unique identifier of the search index.
    #[serde(rename = "id")]
    pub index_id: String,
    /// The name of the search index.
    pub name: String,
    /// The name of the database containing this index.
    pub database: String,
    /// The name of the collection containing this index.
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    /// The status of the search index.
    pub status: MongoDbSearchIndexStatus,
    /// The type of the search index (e.g., "search" or "vectorSearch").
    #[serde(rename = "type")]
    pub index_type: Option<String>,
}

/// Trait for listing search indexes.
#[async_trait]
pub trait SearchIndexLister {
    async fn list_search_indexes(
        &self,
        database_name: String,
        collection_name: String,
    ) -> Result<Vec<SearchIndex>>;
}

/// Trait for deleting a search index by name.
#[async_trait]
pub trait SearchIndexDeleter {
    async fn delete_search_index(
        &self,
        database_name: String,
        collection_name: String,
        index_name: String,
    ) -> Result<()>;
}

// Dependency to create search indexes
#[async_trait]
pub trait SearchIndexCreator {
    async fn create_search_index(&self, model: CreateSearchIndexModel) -> Result<String>;
}

#[async_trait]
pub trait SearchIndexStatusGetter {
    async fn get_search_index_status(
        &self,
        database_name: String,
        collection_name: String,
        index_name: String,
    ) -> Result<Option<MongoDbSearchIndexStatus>>;
}

/// The status of a MongoDB Search Index, as returned by `$listSearchIndexes`.
/// See: https://www.mongodb.com/docs/manual/reference/operator/aggregation/listSearchIndexes/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MongoDbSearchIndexStatus {
    /// The index is being built or re-built after an edit.
    Building,
    /// The index does not exist.
    DoesNotExist,
    /// The index is being deleted.
    Deleting,
    /// The index build failed (e.g. invalid definition).
    Failed,
    /// The index is pending: MongoDB has not yet started building it.
    Pending,
    /// The index is ready and can support queries.
    Ready,
    /// The index is queryable but has stopped replicating data; may return stale results.
    Stale,
}

impl Display for MongoDbSearchIndexStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Building => write!(f, "building"),
            Self::DoesNotExist => write!(f, "does not exist"),
            Self::Deleting => write!(f, "deleting"),
            Self::Failed => write!(f, "failed"),
            Self::Pending => write!(f, "pending"),
            Self::Ready => write!(f, "ready"),
            Self::Stale => write!(f, "stale"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateSearchIndexModel {
    pub database_name: String,
    pub collection_name: String,
    pub definition: Document,
    pub name: Option<String>,
    pub index_type: Option<SearchIndexType>,
}

/// Specifies the type of search index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchIndexType {
    /// A regular search index.
    Search,
    /// A vector search index.
    VectorSearch,
    /// An unknown type of search index.
    #[serde(untagged)]
    Other(String),
}

impl From<SearchIndexType> for mongodb::SearchIndexType {
    fn from(value: SearchIndexType) -> Self {
        match value {
            SearchIndexType::Search => mongodb::SearchIndexType::Search,
            SearchIndexType::VectorSearch => mongodb::SearchIndexType::VectorSearch,
            SearchIndexType::Other(s) => mongodb::SearchIndexType::Other(s),
        }
    }
}

#[async_trait]
impl SearchIndexCreator for Client {
    async fn create_search_index(&self, model: CreateSearchIndexModel) -> Result<String> {
        self.database(&model.database_name)
            .collection::<()>(&model.collection_name)
            .create_search_index(
                SearchIndexModel::builder()
                    .name(model.name)
                    .index_type(
                        model
                            .index_type
                            .map(Into::into)
                            .unwrap_or(mongodb::SearchIndexType::Search),
                    )
                    .definition(model.definition)
                    .build(),
            )
            .await
            .map_err(mongodb_error_to_user_friendly_error)
    }
}

fn mongodb_error_to_user_friendly_error(e: mongodb::error::Error) -> anyhow::Error {
    use mongodb::error::ErrorKind;

    match e.kind.as_ref() {
        ErrorKind::Command(command_error) => anyhow!("{}", command_error.message),
        _error_without_friendly_equivalent => e.into(),
    }
}

#[async_trait]
impl SearchIndexStatusGetter for Client {
    async fn get_search_index_status(
        &self,
        database_name: String,
        collection_name: String,
        index_name: String,
    ) -> Result<Option<MongoDbSearchIndexStatus>> {
        let search_indexes = self
            .database(&database_name)
            .collection::<()>(&collection_name)
            .list_search_indexes()
            .await
            .context("listing search indexes")?;

        // Example data from logs:
        // 2026-01-26T16:26:46.062803Z TRACE search index doc search_index_doc=Ok(Document({"id": String("696a5ea625551143afc54aa3"), "name": String("*"), "type": String("search"), "status": String("BUILDING"), "queryable": Boolean(false), "latestVersion": Int32(0), "latestDefinition": Document({"analyzer": String("lucene.standard"), "searchAnalyzer": String("lucene.standard"), "mappings": Document({"dynamic": Boolean(true), "fields": Document({})})})}))
        // 2026-01-26T16:26:46.062975Z TRACE search index doc search_index_doc=Ok(Document({"id": String("696a5fbc25551143afc54aa6"), "name": String("sfdjkladsfqewuio"), "type": String("search"), "status": String("BUILDING"), "queryable": Boolean(false), "latestVersion": Int32(0), "latestDefinition": Document({"analyzer": String("lucene.standard"), "searchAnalyzer": String("lucene.standard"), "mappings": Document({"dynamic": Boolean(true), "fields": Document({})})})}))
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct SearchIndexDefinition {
            #[serde(rename = "id")]
            pub index_id: String,
            pub name: String,
            #[serde(rename = "type")]
            pub index_type: String,
            pub status: MongoDbSearchIndexStatus,
            pub queryable: bool,
            #[serde(rename = "latestDefinition")]
            pub latest_definition: Document,
            #[serde(rename = "latestVersion")]
            pub latest_version: i32,
        }

        let search_index_definitions = search_indexes
            .with_type::<SearchIndexDefinition>()
            .try_collect::<Vec<_>>()
            .await
            .context("collecting search index definitions")?;
        trace!(
            ?search_index_definitions,
            database_name, collection_name, "all search index definitions"
        );

        let search_index = search_index_definitions
            .iter()
            .find(|index| index.name == index_name)
            .context("finding search index")?;
        trace!(
            ?search_index,
            "search index definition for index name: {index_name}"
        );

        Ok(Some(search_index.status))
    }
}

#[async_trait]
impl SearchIndexLister for Client {
    async fn list_search_indexes(
        &self,
        database_name: String,
        collection_name: String,
    ) -> Result<Vec<SearchIndex>> {
        debug!(database_name, collection_name, "listing search indexes");

        let search_indexes = self
            .database(&database_name)
            .collection::<()>(&collection_name)
            .list_search_indexes()
            .await
            .context("listing search indexes")?;

        // Internal struct to deserialize the raw search index document.
        // See: https://www.mongodb.com/docs/manual/reference/operator/aggregation/listSearchIndexes/
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct RawSearchIndex {
            #[serde(rename = "id")]
            index_id: String,
            name: String,
            #[serde(rename = "type")]
            index_type: Option<String>,
            status: MongoDbSearchIndexStatus,
        }

        let raw_search_indexes = search_indexes
            .with_type::<RawSearchIndex>()
            .try_collect::<Vec<_>>()
            .await
            .context("collecting search index definitions")?;

        trace!(?raw_search_indexes, "raw search indexes");

        // Convert raw search indexes to SearchIndex, adding database and collection info.
        let search_indexes = raw_search_indexes
            .into_iter()
            .map(|raw| SearchIndex {
                index_id: raw.index_id,
                name: raw.name,
                database: database_name.clone(),
                collection_name: collection_name.clone(),
                status: raw.status,
                index_type: raw.index_type,
            })
            .collect();

        Ok(search_indexes)
    }
}

#[async_trait]
impl SearchIndexDeleter for Client {
    async fn delete_search_index(
        &self,
        database_name: String,
        collection_name: String,
        index_name: String,
    ) -> Result<()> {
        debug!(
            database_name,
            collection_name, index_name, "deleting search index"
        );

        self.database(&database_name)
            .collection::<()>(&collection_name)
            .drop_search_index(index_name)
            .await
            .map_err(mongodb_error_to_user_friendly_error)
    }
}

#[cfg(test)]
pub mod mocks {
    use super::*;
    use mockall::mock;

    mock! {
        pub MongoDB {}

        #[async_trait]
        impl SearchIndexCreator for MongoDB {
            async fn create_search_index(&self, model: CreateSearchIndexModel) -> Result<String>;
        }

        #[async_trait]
        impl SearchIndexStatusGetter for MongoDB {
            async fn get_search_index_status(
                &self,
                database_name: String,
                collection_name: String,
                index_name: String,
            ) -> Result<Option<MongoDbSearchIndexStatus>>;
        }

        #[async_trait]
        impl SearchIndexLister for MongoDB {
            async fn list_search_indexes(
                &self,
                database_name: String,
                collection_name: String,
            ) -> Result<Vec<SearchIndex>>;
        }

        #[async_trait]
        impl SearchIndexDeleter for MongoDB {
            async fn delete_search_index(
                &self,
                database_name: String,
                collection_name: String,
                index_name: String,
            ) -> Result<()>;
        }
    }
}
