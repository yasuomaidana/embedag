//! Data structures and mapping between embedag domain contracts and SurrealDB records.

use embedag_core::traits::NodePayload;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use surrealdb::types::{RecordId, SurrealValue};

/// Record representation of an embedag node in SurrealDB.
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct SurrealNodeRecord {
    /// SurrealDB record identifier.
    pub id: Option<RecordId>,
    /// Primary business key.
    pub business_key: String,
    /// Domain entity level (e.g., category, item, invoice).
    pub entity_level: String,
    /// Display or entity name.
    pub name: String,
    /// Arbitrary dynamic attributes as JSON.
    pub attributes: serde_json::Value,
    /// Optional dense vector embedding.
    pub embedding: Option<Vec<f32>>,
    /// Serialized payload JSON string for lossless deserialization.
    pub payload_json: String,
}

impl SurrealNodeRecord {
    /// Converts a domain node payload into a SurrealDB record.
    pub fn from_node<T: NodePayload + Serialize>(node: &T) -> Result<Self, serde_json::Error> {
        let key = node.key().to_string();
        let payload_json = serde_json::to_string(node)?;
        Ok(Self {
            id: None,
            business_key: key,
            entity_level: node.entity_level().to_string(),
            name: node.name().to_string(),
            attributes: node.attributes(),
            embedding: None,
            payload_json,
        })
    }

    /// Converts a domain node payload with explicit embedding.
    pub fn from_node_with_embedding<T: NodePayload + Serialize>(
        node: &T,
        embedding: Option<Vec<f32>>,
    ) -> Result<Self, serde_json::Error> {
        let key = node.key().to_string();
        let payload_json = serde_json::to_string(node)?;
        Ok(Self {
            id: None,
            business_key: key,
            entity_level: node.entity_level().to_string(),
            name: node.name().to_string(),
            attributes: node.attributes(),
            embedding,
            payload_json,
        })
    }

    /// Reconstructs the domain node payload from this record.
    pub fn to_node<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.payload_json)
    }
}

embedag_core::edge_record! {
    /// Record representation of an embedag graph edge in SurrealDB.
    #[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
    pub struct SurrealEdgeRecord;
}
