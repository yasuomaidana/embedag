//! Data structures and mapping between embedag domain contracts and Neo4j node/edge properties.

use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Property representation of an embedag node in Neo4j.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neo4jNodeRecord {
    /// Business key used as unique primary key in Neo4j graph.
    pub business_key: String,
    /// Entity level (e.g. Category, Variant).
    pub entity_level: String,
    /// Display name.
    pub name: String,
    /// Serialized JSON attributes string.
    pub attributes_json: String,
    /// Optional dense vector embedding.
    pub embedding: Option<Vec<f32>>,
    /// Lossless serialized payload JSON.
    pub payload_json: String,
}

impl Neo4jNodeRecord {
    /// Converts a domain node payload into a Neo4j property set.
    pub fn from_node<T: NodePayload + Serialize>(node: &T) -> Result<Self, serde_json::Error> {
        let key = node.key().to_string();
        let payload_json = serde_json::to_string(node)?;
        let attributes_json = serde_json::to_string(&node.attributes())?;
        Ok(Self {
            business_key: key,
            entity_level: node.entity_level().to_string(),
            name: node.name().to_string(),
            attributes_json,
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
        let attributes_json = serde_json::to_string(&node.attributes())?;
        Ok(Self {
            business_key: key,
            entity_level: node.entity_level().to_string(),
            name: node.name().to_string(),
            attributes_json,
            embedding,
            payload_json,
        })
    }

    /// Reconstructs the domain node payload from this record.
    pub fn to_node<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.payload_json)
    }
}

/// Property representation of a Neo4j graph relationship.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neo4jEdgeRecord {
    /// Relationship type label (e.g. `CONTAINS`, `SUB_CATEGORY`).
    pub relationship: String,
    /// Traversal confidence score.
    pub confidence: f32,
    /// Provenance origin label.
    pub provenance: String,
    /// Creation timestamp in RFC 3339.
    pub created_at: String,
}

impl Neo4jEdgeRecord {
    /// Converts an [`EdgePayload`] into relationship properties for Cypher queries.
    pub fn from_edge(edge: &EdgePayload) -> Self {
        let provenance_str = match &edge.provenance {
            ProvenanceKind::DeterministicRule => "deterministic_rule".to_string(),
            ProvenanceKind::DenseSimilarityMatch => "dense_similarity_match".to_string(),
            ProvenanceKind::LlmConstrainedInference => "llm_constrained_inference".to_string(),
            ProvenanceKind::HumanVerified => "human_verified".to_string(),
        };

        Self {
            relationship: edge.relationship.clone(),
            confidence: edge.confidence,
            provenance: provenance_str,
            created_at: edge.created_at.to_rfc3339(),
        }
    }
}
