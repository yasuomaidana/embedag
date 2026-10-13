//! Bidirectional mapping between embedag domain contracts and OxiRS RDF terms/quads.

use std::fmt::Display;

use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use oxirs_core::model::{GraphName, Literal, NamedNode, Object, Predicate, Quad, Subject};
use serde::{Serialize, de::DeserializeOwned};

use crate::config::OxiRsTdbConfig;
use crate::error::OxiRsStorageError;

/// Core RDF predicate IRIs used by the OxiRS TDB2 storage engine.
#[derive(Debug, Clone)]
pub struct SchemaVocab {
    pub rdf_type: NamedNode,
    pub rdfs_label: NamedNode,
    pub business_key: NamedNode,
    pub entity_level: NamedNode,
    pub attributes: NamedNode,
    pub payload: NamedNode,
    pub edge_type: NamedNode,
    pub parent_rel: NamedNode,
    pub child_rel: NamedNode,
    pub relationship: NamedNode,
    pub confidence: NamedNode,
    pub provenance: NamedNode,
    pub schema_ns: String,
}

impl SchemaVocab {
    /// Creates a new schema vocabulary configured with the target schema namespace.
    pub fn new(schema_ns: &str) -> Result<Self, OxiRsStorageError> {
        let ns = if schema_ns.ends_with('#') || schema_ns.ends_with('/') {
            schema_ns.to_string()
        } else {
            format!("{schema_ns}#")
        };

        Ok(Self {
            rdf_type: NamedNode::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
                .map_err(|e| OxiRsStorageError::InvalidIri("rdf:type".into(), e.to_string()))?,
            rdfs_label: NamedNode::new("http://www.w3.org/2000/01/rdf-schema#label")
                .map_err(|e| OxiRsStorageError::InvalidIri("rdfs:label".into(), e.to_string()))?,
            business_key: NamedNode::new(format!("{ns}businessKey"))
                .map_err(|e| OxiRsStorageError::InvalidIri("businessKey".into(), e.to_string()))?,
            entity_level: NamedNode::new(format!("{ns}entityLevel"))
                .map_err(|e| OxiRsStorageError::InvalidIri("entityLevel".into(), e.to_string()))?,
            attributes: NamedNode::new(format!("{ns}attributes"))
                .map_err(|e| OxiRsStorageError::InvalidIri("attributes".into(), e.to_string()))?,
            payload: NamedNode::new(format!("{ns}payload"))
                .map_err(|e| OxiRsStorageError::InvalidIri("payload".into(), e.to_string()))?,
            edge_type: NamedNode::new(format!("{ns}Edge"))
                .map_err(|e| OxiRsStorageError::InvalidIri("Edge".into(), e.to_string()))?,
            parent_rel: NamedNode::new(format!("{ns}parent"))
                .map_err(|e| OxiRsStorageError::InvalidIri("parent".into(), e.to_string()))?,
            child_rel: NamedNode::new(format!("{ns}child"))
                .map_err(|e| OxiRsStorageError::InvalidIri("child".into(), e.to_string()))?,
            relationship: NamedNode::new(format!("{ns}relationship"))
                .map_err(|e| OxiRsStorageError::InvalidIri("relationship".into(), e.to_string()))?,
            confidence: NamedNode::new(format!("{ns}confidence"))
                .map_err(|e| OxiRsStorageError::InvalidIri("confidence".into(), e.to_string()))?,
            provenance: NamedNode::new(format!("{ns}provenance"))
                .map_err(|e| OxiRsStorageError::InvalidIri("provenance".into(), e.to_string()))?,
            schema_ns: ns,
        })
    }

    /// Constructs a custom relationship predicate IRI from the relationship name.
    pub fn custom_relation(&self, rel_name: &str) -> Result<NamedNode, OxiRsStorageError> {
        let clean_name = sanitize_iri_token(rel_name);
        NamedNode::new(format!("{}{}", self.schema_ns, clean_name))
            .map_err(|e| OxiRsStorageError::InvalidIri(rel_name.into(), e.to_string()))
    }
}

/// Helper to sanitize arbitrary string tokens for valid IRI composition.
pub fn sanitize_iri_token(token: &str) -> String {
    token
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Creates a `NamedNode` IRI for a node business key.
pub fn make_node_iri(base_iri: &str, key: &str) -> Result<NamedNode, OxiRsStorageError> {
    let clean_key = sanitize_iri_token(key);
    let full_iri = if base_iri.ends_with('/') || base_iri.ends_with('#') || base_iri.ends_with(':')
    {
        format!("{base_iri}{clean_key}")
    } else {
        format!("{base_iri}/{clean_key}")
    };

    NamedNode::new(full_iri)
        .map_err(|e| OxiRsStorageError::InvalidIri(key.to_string(), e.to_string()))
}

/// Creates a `GraphName` based on the given configuration.
pub fn make_graph_name(config: &OxiRsTdbConfig) -> Result<GraphName, OxiRsStorageError> {
    if let Some(ref graph_uri) = config.named_graph {
        let node = NamedNode::new(graph_uri)
            .map_err(|e| OxiRsStorageError::InvalidIri(graph_uri.clone(), e.to_string()))?;
        Ok(GraphName::NamedNode(node))
    } else {
        Ok(GraphName::DefaultGraph)
    }
}

/// Converts a domain node `T: NodePayload` into a set of RDF quads for OxiRS storage.
pub fn node_to_quads<T: NodePayload + Serialize>(
    config: &OxiRsTdbConfig,
    vocab: &SchemaVocab,
    node: &T,
) -> Result<Vec<Quad>, OxiRsStorageError> {
    let node_iri = make_node_iri(&config.base_iri, &node.key().to_string())?;
    let subject = Subject::NamedNode(node_iri);
    let graph = make_graph_name(config)?;

    let mut quads = Vec::with_capacity(6);

    // 1. rdf:type <schema_ns>type/<entity_level>
    let level_iri = NamedNode::new(format!(
        "{}type/{}",
        vocab.schema_ns,
        sanitize_iri_token(node.entity_level())
    ))
    .map_err(|e| OxiRsStorageError::InvalidIri(node.entity_level().into(), e.to_string()))?;
    quads.push(Quad::new(
        subject.clone(),
        Predicate::NamedNode(vocab.rdf_type.clone()),
        Object::NamedNode(level_iri),
        graph.clone(),
    ));

    // 2. rdfs:label Literal
    quads.push(Quad::new(
        subject.clone(),
        Predicate::NamedNode(vocab.rdfs_label.clone()),
        Object::Literal(Literal::new(node.name())),
        graph.clone(),
    ));

    // 3. embedag:businessKey Literal
    quads.push(Quad::new(
        subject.clone(),
        Predicate::NamedNode(vocab.business_key.clone()),
        Object::Literal(Literal::new(node.key().to_string())),
        graph.clone(),
    ));

    // 4. embedag:entityLevel Literal
    quads.push(Quad::new(
        subject.clone(),
        Predicate::NamedNode(vocab.entity_level.clone()),
        Object::Literal(Literal::new(node.entity_level())),
        graph.clone(),
    ));

    // 5. embedag:attributes Literal (serialized JSON)
    quads.push(Quad::new(
        subject.clone(),
        Predicate::NamedNode(vocab.attributes.clone()),
        Object::Literal(Literal::new(node.attributes().to_string())),
        graph.clone(),
    ));

    // 6. embedag:payload Literal (exact JSON serialized node payload for 100% loss-less reconstruction)
    let payload_json = serde_json::to_string(node)?;
    quads.push(Quad::new(
        subject,
        Predicate::NamedNode(vocab.payload.clone()),
        Object::Literal(Literal::new(payload_json)),
        graph,
    ));

    Ok(quads)
}

/// Converts a directed edge from parent to child into RDF quads for OxiRS storage.
pub fn edge_to_quads<K: Display>(
    config: &OxiRsTdbConfig,
    vocab: &SchemaVocab,
    parent: &K,
    child: &K,
    edge: &EdgePayload,
) -> Result<Vec<Quad>, OxiRsStorageError> {
    let parent_key = parent.to_string();
    let child_key = child.to_string();

    let parent_iri = make_node_iri(&config.base_iri, &parent_key)?;
    let child_iri = make_node_iri(&config.base_iri, &child_key)?;
    let graph = make_graph_name(config)?;

    let mut quads = Vec::with_capacity(7);

    // 1. Direct relationship quad: parent -> custom_rel -> child
    let rel_pred = vocab.custom_relation(&edge.relationship)?;
    quads.push(Quad::new(
        Subject::NamedNode(parent_iri.clone()),
        Predicate::NamedNode(rel_pred),
        Object::NamedNode(child_iri.clone()),
        graph.clone(),
    ));

    // 2. Reified Edge metadata entity
    let edge_iri_str = format!(
        "{}edge/{}_{}_{}",
        config.base_iri,
        sanitize_iri_token(&parent_key),
        sanitize_iri_token(&edge.relationship),
        sanitize_iri_token(&child_key)
    );
    let edge_iri = NamedNode::new(edge_iri_str)
        .map_err(|e| OxiRsStorageError::InvalidIri("edge_iri".into(), e.to_string()))?;
    let edge_subj = Subject::NamedNode(edge_iri);

    // edge a embedag:Edge
    quads.push(Quad::new(
        edge_subj.clone(),
        Predicate::NamedNode(vocab.rdf_type.clone()),
        Object::NamedNode(vocab.edge_type.clone()),
        graph.clone(),
    ));

    // edge embedag:parent parent_iri
    quads.push(Quad::new(
        edge_subj.clone(),
        Predicate::NamedNode(vocab.parent_rel.clone()),
        Object::NamedNode(parent_iri),
        graph.clone(),
    ));

    // edge embedag:child child_iri
    quads.push(Quad::new(
        edge_subj.clone(),
        Predicate::NamedNode(vocab.child_rel.clone()),
        Object::NamedNode(child_iri),
        graph.clone(),
    ));

    // edge embedag:relationship relationship
    quads.push(Quad::new(
        edge_subj.clone(),
        Predicate::NamedNode(vocab.relationship.clone()),
        Object::Literal(Literal::new(&edge.relationship)),
        graph.clone(),
    ));

    // edge embedag:confidence confidence
    quads.push(Quad::new(
        edge_subj.clone(),
        Predicate::NamedNode(vocab.confidence.clone()),
        Object::Literal(Literal::new(format!("{:.4}", edge.confidence))),
        graph.clone(),
    ));

    // edge embedag:provenance provenance
    let prov_str = match edge.provenance {
        ProvenanceKind::DeterministicRule => "DeterministicRule",
        ProvenanceKind::DenseSimilarityMatch => "DenseSimilarityMatch",
        ProvenanceKind::LlmConstrainedInference => "LlmConstrainedInference",
        ProvenanceKind::HumanVerified => "HumanVerified",
    };
    quads.push(Quad::new(
        edge_subj,
        Predicate::NamedNode(vocab.provenance.clone()),
        Object::Literal(Literal::new(prov_str)),
        graph,
    ));

    Ok(quads)
}

/// Reconstructs a node `T` from a payload literal or raw RDF quad terms.
pub fn reconstruct_node<T: DeserializeOwned>(
    quads: &[Quad],
    vocab: &SchemaVocab,
) -> Result<Option<T>, OxiRsStorageError> {
    // Look for exact embedag:payload literal first
    for quad in quads {
        if let Predicate::NamedNode(pred) = quad.predicate()
            && pred == &vocab.payload
            && let Object::Literal(lit) = quad.object()
        {
            let parsed: T = serde_json::from_str(lit.value())?;
            return Ok(Some(parsed));
        }
    }

    Ok(None)
}
