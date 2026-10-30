//! Core value objects: edges, provenance, classification targets and multimodal blobs.
//!
//! Traceability: `doc/04_Core_Domain_and_Entity_Contracts.md` §2.3,
//! `doc/05_In_Memory_DAG_and_Arena_Topology.md` §2.1,
//! `doc/08_Neural_and_Multimodal_Embeddings.md` §4.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DagError;

/// Continuous scalar feature dimensions keyed by feature name (e.g. `"weight_grams"`).
pub type FeatureMap = HashMap<String, f64>;

/// Categorical tags keyed by tag name (e.g. `"packaging"` -> `"bar"`).
pub type TagMap = HashMap<String, String>;

/// Origin of a graph relationship, used to grade trust in the edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    /// Produced by an exact, deterministic rule (alias table, explicit key match).
    DeterministicRule,
    /// Produced by dense/hybrid similarity matching (Tier 2).
    DenseSimilarityMatch,
    /// Produced by constrained LLM inference (Tier 3).
    LlmConstrainedInference,
    /// Confirmed or authored by a human reviewer.
    HumanVerified,
}

impl ProvenanceKind {
    /// The `snake_case` label stores and wire formats use for this provenance.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeterministicRule => "deterministic_rule",
            Self::DenseSimilarityMatch => "dense_similarity_match",
            Self::LlmConstrainedInference => "llm_constrained_inference",
            Self::HumanVerified => "human_verified",
        }
    }

    /// Returns `true` for algorithmic/human provenance that does not depend on a probabilistic model.
    #[must_use]
    pub const fn is_deterministic(self) -> bool {
        matches!(self, Self::DeterministicRule | Self::HumanVerified)
    }
}

/// Typed, provenance-carrying payload attached to every directed edge (`parent -> child`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgePayload {
    /// Relationship label, e.g. `"is_a"`, `"belongs_to"`, `"packaged_as"`.
    pub relationship: String,
    /// Confidence in `[0.0, 1.0]`.
    pub confidence: f32,
    /// Origin of the relationship.
    pub provenance: ProvenanceKind,
    /// Creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
}

impl EdgePayload {
    /// Creates a validated edge payload stamped with the current UTC time.
    ///
    /// # Errors
    /// Returns [`DagError::InvalidConfidence`] when `confidence` is `NaN` or outside `[0.0, 1.0]`.
    pub fn new(
        relationship: impl Into<String>,
        confidence: f32,
        provenance: ProvenanceKind,
    ) -> Result<Self, DagError> {
        Self::with_created_at(relationship, confidence, provenance, Utc::now())
    }

    /// Creates a validated edge payload with an explicit creation timestamp (deterministic tests, replays).
    ///
    /// # Errors
    /// Returns [`DagError::InvalidConfidence`] when `confidence` is `NaN` or outside `[0.0, 1.0]``.
    pub fn with_created_at(
        relationship: impl Into<String>,
        confidence: f32,
        provenance: ProvenanceKind,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DagError> {
        let edge = Self {
            relationship: relationship.into(),
            confidence,
            provenance,
            created_at,
        };
        edge.validate()?;
        Ok(edge)
    }

    /// Validates the confidence invariant.
    ///
    /// # Errors
    /// Returns [`DagError::InvalidConfidence`] when `confidence` is `NaN` or outside `[0.0, 1.0]`.
    pub fn validate(&self) -> Result<(), DagError> {
        if (0.0..=1.0).contains(&self.confidence) {
            Ok(())
        } else {
            Err(DagError::InvalidConfidence(self.confidence))
        }
    }

    /// Returns `true` when the edge is algorithmically extracted with full confidence
    /// (`EXTRACTED (1.0)` in evidence packaging), `false` when it is model-inferred.
    #[must_use]
    pub fn is_extracted(&self) -> bool {
        self.provenance.is_deterministic() && self.confidence >= 1.0
    }
}

/// Reference to non-textual media attached to a record (image, audio, video).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MultimodalBlob {
    /// MIME type, e.g. `"image/jpeg"`.
    pub mime_type: String,
    /// Storage location (S3 / filesystem URI).
    pub storage_uri: String,
    /// Optional precomputed, L2-normalized dense embedding of the media.
    pub precomputed_vector: Vec<f32>,
}

impl MultimodalBlob {
    /// Creates a blob reference without a precomputed vector.
    #[must_use]
    pub fn new(mime_type: impl Into<String>, storage_uri: impl Into<String>) -> Self {
        Self {
            mime_type: mime_type.into(),
            storage_uri: storage_uri.into(),
            precomputed_vector: Vec::new(),
        }
    }
}

/// Placement decision produced by a graph classification engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphTarget<K> {
    /// Exact match with an existing node key.
    ExactMatch(K),
    /// Fuzzy candidate match with a similarity score in `[0.0, 1.0]`.
    FuzzyCandidate {
        /// Candidate node key.
        key: K,
        /// Similarity score.
        similarity: f32,
    },
    /// Placement as a child of an existing parent with relationship semantics.
    NewChildOf {
        /// Parent node key.
        parent_key: K,
        /// Relationship label.
        relationship: String,
    },
    /// Placement as a child of several existing parents.
    NewMultiParentChild {
        /// `(parent key, relationship)` pairs.
        parent_keys: Vec<(K, String)>,
    },
    /// Creation of a brand new root node.
    NewRoot,
    /// Ambiguous match requiring human arbitration or LLM fallback.
    Ambiguous(Vec<(K, f32)>),
}

impl<K> GraphTarget<K> {
    /// Returns `true` when the target creates a new node.
    #[must_use]
    pub const fn creates_node(&self) -> bool {
        matches!(
            self,
            Self::NewChildOf { .. } | Self::NewMultiParentChild { .. } | Self::NewRoot
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).single().unwrap()
    }

    #[test]
    fn test_edge_payload_provenance_equality() {
        let a = EdgePayload::with_created_at(
            "is_a",
            0.9,
            ProvenanceKind::LlmConstrainedInference,
            fixed_time(),
        )
        .unwrap();
        let b = EdgePayload::with_created_at(
            "is_a",
            0.9,
            ProvenanceKind::LlmConstrainedInference,
            fixed_time(),
        )
        .unwrap();
        let c =
            EdgePayload::with_created_at("is_a", 0.9, ProvenanceKind::HumanVerified, fixed_time())
                .unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c, "provenance participates in equality");

        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"provenance\":\"llm_constrained_inference\""));
        let back: EdgePayload = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back, "serialization round-trip preserves equality");
    }

    #[test]
    fn test_edge_payload_rejects_invalid_confidence() {
        for bad in [-0.01_f32, 1.01, f32::NAN, f32::INFINITY] {
            let err = EdgePayload::new("is_a", bad, ProvenanceKind::DeterministicRule).unwrap_err();
            assert!(matches!(err, DagError::InvalidConfidence(_)));
        }
        assert!(EdgePayload::new("is_a", 0.0, ProvenanceKind::DeterministicRule).is_ok());
        assert!(EdgePayload::new("is_a", 1.0, ProvenanceKind::DeterministicRule).is_ok());
    }

    #[test]
    fn test_edge_extracted_vs_inferred() {
        let extracted = EdgePayload::new("is_a", 1.0, ProvenanceKind::DeterministicRule).unwrap();
        let inferred =
            EdgePayload::new("is_a", 0.8, ProvenanceKind::LlmConstrainedInference).unwrap();
        assert!(extracted.is_extracted());
        assert!(!inferred.is_extracted());
    }

    #[test]
    fn test_graph_target_creates_node() {
        assert!(GraphTarget::<String>::NewRoot.creates_node());
        assert!(!GraphTarget::ExactMatch("a".to_string()).creates_node());
        assert!(!GraphTarget::Ambiguous(vec![("a".to_string(), 0.5)]).creates_node());
    }

    #[test]
    fn test_graph_target_serialization_roundtrip() {
        let target = GraphTarget::NewChildOf {
            parent_key: "parent-1".to_string(),
            relationship: "is_a".to_string(),
        };
        let serialized = serde_json::to_string(&target).unwrap();
        let deserialized: GraphTarget<String> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(target, deserialized);
    }

    #[test]
    fn test_multimodal_blob_roundtrip() {
        let blob = MultimodalBlob::new("image/jpeg", "s3://bucket/a.jpg");
        let json = serde_json::to_string(&blob).unwrap();
        assert_eq!(serde_json::from_str::<MultimodalBlob>(&json).unwrap(), blob);
        assert!(blob.precomputed_vector.is_empty());
    }
}
