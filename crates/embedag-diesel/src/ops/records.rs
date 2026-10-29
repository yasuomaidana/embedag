use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Node record for batch relational upserts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeUpsertRecord {
    pub id: Uuid,
    pub business_key: String,
    pub entity_level: String,
    pub name: String,
    pub attributes: serde_json::Value,
    pub embedding: Option<Vec<f32>>,
}

/// Directed edge record for batch relational upserts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeUpsertRecord {
    pub parent_id: Uuid,
    pub child_id: Uuid,
    pub relationship_type: String,
    pub confidence: f32,
    pub provenance: String,
}

/// Dynamic mutation directive record to be applied against the relational store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MutationDirectiveRecord {
    InsertNode(NodeUpsertRecord),
    AttachParent {
        parent_id: Uuid,
        child_id: Uuid,
        relationship_type: String,
        confidence: f32,
        provenance: String,
    },
    UpdateNodePayload {
        id: Uuid,
        updated_attributes: serde_json::Value,
    },
    RetargetEdge {
        child_id: Uuid,
        old_parent_id: Uuid,
        new_parent_id: Uuid,
        relationship_type: String,
        confidence: f32,
    },
}
