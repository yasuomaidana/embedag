//! Public domain traits and contracts for `embedag-core`.
//!
//! Traceability: `doc/04_Core_Domain_and_Entity_Contracts.md`,
//! `doc/15_Relational_Persistence_and_Schema_Engine.md`.

use std::collections::HashMap;
use std::fmt::Display;
use std::hash::Hash;

use chrono::{DateTime, Utc};

use crate::error::DagError;
use crate::model::{FeatureMap, MultimodalBlob, TagMap};
use crate::mutation::MutationDirective;

pub use crate::model::{EdgePayload, GraphTarget, ProvenanceKind};

/// Contract for turning domain data structures into multi-modal graph context.
///
/// Any application record seeking graph classification or entity resolution
/// implements `AsGraphContext`. It provides text for linguistic embedding,
/// continuous scalars, cyclical temporal dimensions, spatial coordinates,
/// categorical tags, and multimodal media references.
pub trait AsGraphContext: Send + Sync {
    /// Unique business key identifying this record within its source system.
    fn context_key(&self) -> String;

    /// Primary text representation for dense linguistic embeddings.
    fn text_repr(&self) -> String;

    /// Optional token weights for lexical/BM25 sparse retrieval.
    fn sparse_lexical_tokens(&self) -> Vec<(u32, f32)> {
        Vec::new()
    }

    /// Continuous numeric scalar dimensions (e.g. weight, volume, price).
    fn scalar_features(&self) -> FeatureMap {
        HashMap::new()
    }

    /// UTC timestamp for temporal analysis.
    fn timestamp_utc(&self) -> Option<DateTime<Utc>> {
        None
    }

    /// Optional geographic coordinates `(longitude, latitude)` in WGS84.
    fn geographic_coordinates(&self) -> Option<(f64, f64)> {
        None
    }

    /// Categorical tags or taxonomy breadcrumbs.
    fn categorical_tags(&self) -> TagMap {
        HashMap::new()
    }

    /// Non-textual multimodal media attachments (images, audio, video).
    fn multimodal_blobs(&self) -> Vec<MultimodalBlob> {
        Vec::new()
    }
}

/// Payload contract for nodes stored within the directed acyclic graph.
pub trait NodePayload: Clone + Send + Sync + 'static {
    /// The unique primary key type for node lookup (e.g., Uuid, String, i64).
    type Key: Eq + Hash + Clone + Send + Sync + Display;

    /// Returns the unique primary business key of this node.
    fn key(&self) -> &Self::Key;

    /// Returns the human-readable entity name or label.
    fn name(&self) -> &str;

    /// Returns the hierarchical entity level (e.g., "Category", "Product", "Variant").
    fn entity_level(&self) -> &str;

    /// Returns arbitrary structured metadata serialized as JSON.
    fn attributes(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
}

/// Object-safe dynamic dispatch contract for [`NodePayload`].
pub trait DynNodePayload: Send + Sync {
    /// Returns the string representation of the node's key.
    fn dyn_key(&self) -> String;

    /// Returns the human-readable entity name or label.
    fn dyn_name(&self) -> &str;

    /// Returns the hierarchical entity level.
    fn dyn_entity_level(&self) -> &str;

    /// Returns arbitrary structured metadata serialized as JSON.
    fn dyn_attributes(&self) -> serde_json::Value;
}

impl<T: NodePayload> DynNodePayload for T {
    fn dyn_key(&self) -> String {
        self.key().to_string()
    }

    fn dyn_name(&self) -> &str {
        self.name()
    }

    fn dyn_entity_level(&self) -> &str {
        self.entity_level()
    }

    fn dyn_attributes(&self) -> serde_json::Value {
        self.attributes()
    }
}

// ---------------------------------------------------------------------------
// Capability Sub-Traits for Relational & Native Storage Repositories
// ---------------------------------------------------------------------------

/// Read-only repository capability for graph lookups and local tree traversals.
#[async_trait::async_trait]
pub trait ReadOnlyGraphRepository<T: NodePayload>: Send + Sync {
    /// Fetches a node by its business key.
    async fn get_node(&self, key: &T::Key) -> Result<Option<T>, DagError>;

    /// Fetches immediate parents of a node.
    async fn get_parents(&self, key: &T::Key) -> Result<Vec<T>, DagError>;

    /// Fetches immediate children of a node.
    async fn get_children(&self, key: &T::Key) -> Result<Vec<T>, DagError>;
}

/// Vector similarity search capability against stored node embeddings.
#[async_trait::async_trait]
pub trait VectorSearchRepository<T: NodePayload>: Send + Sync {
    /// Performs approximate nearest neighbor vector search against indexed embeddings.
    async fn find_nearest_neighbors(
        &self,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(T, f32)>, DagError>;
}

/// Transactional mutation capability for applying atomic batches of graph directives.
#[async_trait::async_trait]
pub trait TransactionalGraphRepository<T: NodePayload>: Send + Sync {
    /// Atomically applies a batch of mutation directives.
    async fn apply_mutation_batch(
        &self,
        directives: &[MutationDirective<T>],
    ) -> Result<(), DagError>;
}

/// Master blanket trait combining all storage capabilities into a unified repository interface.
pub trait GraphRepository<T: NodePayload>:
    ReadOnlyGraphRepository<T> + VectorSearchRepository<T> + TransactionalGraphRepository<T>
{
}

impl<T: NodePayload, R> GraphRepository<T> for R where
    R: ReadOnlyGraphRepository<T>
        + VectorSearchRepository<T>
        + TransactionalGraphRepository<T>
        + ?Sized
{
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MinimalRecord {
        id: String,
        text: String,
    }

    impl AsGraphContext for MinimalRecord {
        fn context_key(&self) -> String {
            self.id.clone()
        }

        fn text_repr(&self) -> String {
            self.text.clone()
        }
    }

    #[test]
    fn test_default_as_graph_context_impl() {
        let record = MinimalRecord {
            id: "sku-100".to_string(),
            text: "Minimal product description".to_string(),
        };

        assert_eq!(record.context_key(), "sku-100");
        assert_eq!(record.text_repr(), "Minimal product description");
        assert!(record.sparse_lexical_tokens().is_empty());
        assert!(record.scalar_features().is_empty());
        assert!(record.timestamp_utc().is_none());
        assert!(record.geographic_coordinates().is_none());
        assert!(record.categorical_tags().is_empty());
        assert!(record.multimodal_blobs().is_empty());

        // Verify object safety
        let dyn_ctx: &dyn AsGraphContext = &record;
        assert_eq!(dyn_ctx.context_key(), "sku-100");
        assert_eq!(dyn_ctx.text_repr(), "Minimal product description");
    }

    #[derive(Clone, Debug, PartialEq)]
    struct MockNode {
        id: String,
        label: String,
        level: String,
        meta: serde_json::Value,
    }

    impl NodePayload for MockNode {
        type Key = String;

        fn key(&self) -> &Self::Key {
            &self.id
        }

        fn name(&self) -> &str {
            &self.label
        }

        fn entity_level(&self) -> &str {
            &self.level
        }

        fn attributes(&self) -> serde_json::Value {
            self.meta.clone()
        }
    }

    struct MockFullRepo;

    #[async_trait::async_trait]
    impl ReadOnlyGraphRepository<MockNode> for MockFullRepo {
        async fn get_node(&self, _key: &String) -> Result<Option<MockNode>, DagError> {
            Ok(None)
        }
        async fn get_parents(&self, _key: &String) -> Result<Vec<MockNode>, DagError> {
            Ok(vec![])
        }
        async fn get_children(&self, _key: &String) -> Result<Vec<MockNode>, DagError> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl VectorSearchRepository<MockNode> for MockFullRepo {
        async fn find_nearest_neighbors(
            &self,
            _embedding: &[f32],
            _limit: usize,
        ) -> Result<Vec<(MockNode, f32)>, DagError> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl TransactionalGraphRepository<MockNode> for MockFullRepo {
        async fn apply_mutation_batch(
            &self,
            _directives: &[MutationDirective<MockNode>],
        ) -> Result<(), DagError> {
            Ok(())
        }
    }

    #[test]
    fn test_sub_trait_object_safety() {
        let repo = MockFullRepo;

        // Verify that all sub-traits are object safe and can be held as &dyn Trait
        let _read_only: &dyn ReadOnlyGraphRepository<MockNode> = &repo;
        let _vector_search: &dyn VectorSearchRepository<MockNode> = &repo;
        let _transactional: &dyn TransactionalGraphRepository<MockNode> = &repo;
        let _master_repo: &dyn GraphRepository<MockNode> = &repo;
    }
}
