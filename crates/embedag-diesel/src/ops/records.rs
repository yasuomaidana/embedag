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

impl NodeUpsertRecord {
    /// A node with empty attributes and no embedding.
    pub fn new(id: Uuid, business_key: &str, entity_level: &str, name: &str) -> Self {
        Self {
            id,
            business_key: business_key.into(),
            entity_level: entity_level.into(),
            name: name.into(),
            attributes: serde_json::json!({}),
            embedding: None,
        }
    }
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

impl EdgeUpsertRecord {
    /// An edge from `parent_id` to `child_id`.
    pub fn new(
        parent_id: Uuid,
        child_id: Uuid,
        relationship_type: &str,
        confidence: f32,
        provenance: &str,
    ) -> Self {
        Self {
            parent_id,
            child_id,
            relationship_type: relationship_type.into(),
            confidence,
            provenance: provenance.into(),
        }
    }
}

/// A chain of nodes, each the parent of the next, with fresh ids: one node per `levels` entry
/// (`business key`, `entity level`, `name`) and one edge per consecutive pair.
pub fn lineage_records(
    levels: &[(&str, &str, &str)],
    relationship_type: &str,
    provenance: &str,
) -> (Vec<NodeUpsertRecord>, Vec<EdgeUpsertRecord>) {
    let nodes: Vec<NodeUpsertRecord> = levels
        .iter()
        .map(|(key, level, name)| NodeUpsertRecord::new(Uuid::new_v4(), key, level, name))
        .collect();
    let edges = nodes
        .windows(2)
        .map(|pair| {
            EdgeUpsertRecord::new(pair[0].id, pair[1].id, relationship_type, 1.0, provenance)
        })
        .collect();
    (nodes, edges)
}

/// Like [`lineage_records`], with a last edge from the final node back to the first: a cycle.
pub fn cycle_records(
    levels: &[(&str, &str, &str)],
    relationship_type: &str,
    provenance: &str,
) -> (Vec<NodeUpsertRecord>, Vec<EdgeUpsertRecord>) {
    let (nodes, mut edges) = lineage_records(levels, relationship_type, provenance);
    if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {
        edges.push(EdgeUpsertRecord::new(
            last.id,
            first.id,
            relationship_type,
            1.0,
            provenance,
        ));
    }
    (nodes, edges)
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

impl MutationDirectiveRecord {
    /// The edge an `AttachParent` directive writes; `None` for every other directive.
    pub fn attached_edge(&self) -> Option<EdgeUpsertRecord> {
        match self {
            Self::AttachParent {
                parent_id,
                child_id,
                relationship_type,
                confidence,
                provenance,
            } => Some(EdgeUpsertRecord {
                parent_id: *parent_id,
                child_id: *child_id,
                relationship_type: relationship_type.clone(),
                confidence: *confidence,
                provenance: provenance.clone(),
            }),
            _ => None,
        }
    }
}
