use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dependencies::SearchIndexType;

/// Request to create an Atlas Search index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexCreateRequest {
    /// Label that identifies the collection to create an Atlas Search index in.
    pub collection_name: String,
    /// Label that identifies the database that contains the collection to create an Atlas Search index in.
    pub database: String,
    /// Label that identifies this index. Within each namespace, names of all indexes in the namespace must be unique.
    pub name: String,
    /// Type of the index. The default type is search.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub index_type: Option<SearchIndexType>,
    /// The index definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<Value>,
}
