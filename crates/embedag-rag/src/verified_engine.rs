//! Pure-Rust verified retrieval engine for embedag.
//!
//! Replaces the former `oxirag` pipeline (relicensed to BUSL-1.1) with the in-house building blocks:
//! - Retrieval: [`SparseLexicalIndex`] (token overlap plus trigram fuzzy matching).
//! - Fusion: [`RrfRanker`] over the ranked lists that are available.
//! - Verification: an answer is composed only from retrieved chunks and cites each one as `[n]`;
//!   a query that no chunk supports fails with [`RagError::VerificationFailure`] instead of guessing.

use crate::expansion::ExpandedSubgraph;
use crate::index::SparseLexicalIndex;
use crate::rrf::RrfRanker;
use thiserror::Error;
use uuid::Uuid;

/// Number of grounded chunks cited in an answer.
const TOP_K: usize = 3;

/// Error variants occurring during engine construction or execution.
#[derive(Debug, Error)]
pub enum RagError {
    #[error("Indexing or query error: {0}")]
    IndexError(String),

    #[error("Verification failed: {0}")]
    VerificationFailure(String),

    #[error("Graph topology error: {0}")]
    TopologyError(String),
}

/// Pure-Rust verified RAG processor: lexical retrieval, RRF fusion and cited, grounded answers.
#[derive(Debug, Clone)]
pub struct VerifiedRagEngine {
    embedding_dim: usize,
}

impl Default for VerifiedRagEngine {
    fn default() -> Self {
        Self::new(384)
    }
}

impl VerifiedRagEngine {
    /// Creates a new `VerifiedRagEngine` with specified embedding dimensions (default: 384 for MiniLM).
    #[must_use]
    pub fn new(embedding_dim: usize) -> Self {
        Self { embedding_dim }
    }

    /// Returns the configured embedding dimensions.
    #[must_use]
    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }

    /// Indexes a raw text corpus and answers the query from the chunks that support it.
    ///
    /// The answer is one line per cited chunk, `[n] <chunk text>`, best match first.
    pub async fn query_verified(
        &self,
        corpus: &[&str],
        query_text: &str,
    ) -> Result<String, RagError> {
        let mut index = SparseLexicalIndex::new();
        let mut chunks: Vec<(Uuid, &str)> = Vec::with_capacity(corpus.len());
        for (position, &chunk) in corpus.iter().enumerate() {
            if chunk.trim().is_empty() {
                return Err(RagError::IndexError(
                    "Failed to index chunk: it is empty".to_string(),
                ));
            }
            let id = Uuid::from_u128(position as u128 + 1);
            index.insert(id, format!("chunk_{position}"), chunk);
            chunks.push((id, chunk));
        }

        if query_text.trim().is_empty() {
            return Err(RagError::IndexError(
                "query failed: the query is empty".to_string(),
            ));
        }

        let lexical: Vec<Uuid> = index
            .search(query_text, TOP_K * 2)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let ranked = RrfRanker::default().fuse(&[], &lexical);

        let cited: Vec<String> = ranked
            .iter()
            .filter_map(|candidate| chunks.iter().find(|(id, _)| *id == candidate.id))
            .take(TOP_K)
            .enumerate()
            .map(|(n, (_, text))| format!("[{}] {}", n + 1, text))
            .collect();

        if cited.is_empty() {
            return Err(RagError::VerificationFailure(
                "no indexed chunk supports the query".to_string(),
            ));
        }
        Ok(cited.join("\n"))
    }

    /// Indexes topological graph entities and expanded relationships from an [`ExpandedSubgraph`]
    /// and executes verified retrieval.
    pub async fn query_graph_verified<K: std::fmt::Display>(
        &self,
        subgraph: &ExpandedSubgraph<K>,
        query_text: &str,
    ) -> Result<String, RagError> {
        let mut corpus = Vec::new();

        // Convert expanded nodes to textual context
        for (key, node) in &subgraph.nodes {
            corpus.push(format!(
                "Node [Key: {}] ({:?}) - hop distance {}, decay weight: {:.4}",
                key, node.relationship, node.hop_distance, node.weight
            ));
        }

        // Convert expanded edges to textual relations
        for edge in &subgraph.edges {
            corpus.push(format!(
                "Edge: {} --[{}]--> {} (confidence: {:.2})",
                edge.parent_key, edge.relationship, edge.child_key, edge.confidence
            ));
        }

        let string_refs: Vec<&str> = corpus.iter().map(String::as_str).collect();
        self.query_verified(&string_refs, query_text).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion::{ExpandedEdge, ExpandedNode, SeedRelationship};
    use embedag_core::model::ProvenanceKind;
    use rstest::rstest;
    use std::collections::HashMap;

    #[tokio::test]
    async fn a_verified_query_cites_the_supporting_chunk_first() {
        let engine = VerifiedRagEngine::new(384);
        let corpus = &[
            "Rust 2024 edition features sovereign zero-FFI compilation.",
            "The verified engine provides cited retrieval and grounded answers.",
            "Embedag manages dynamic hierarchical knowledge DAG arenas with petgraph.",
        ];

        let result = engine
            .query_verified(corpus, "What does the verified engine provide?")
            .await
            .expect("verified query succeeds");

        assert!(
            result.starts_with("[1] The verified engine provides"),
            "{result}"
        );
    }

    #[tokio::test]
    async fn a_query_no_chunk_supports_fails_verification() {
        let err = VerifiedRagEngine::new(8)
            .query_verified(&["alpha beta gamma"], "zzzzqqqq")
            .await
            .unwrap_err();
        assert!(matches!(err, RagError::VerificationFailure(_)), "{err}");
    }

    #[test]
    fn the_engine_reports_its_dimensions_and_defaults_to_minilm() {
        assert_eq!(VerifiedRagEngine::default().embedding_dim(), 384);
        assert_eq!(VerifiedRagEngine::new(8).embedding_dim(), 8);
    }

    #[rstest]
    #[case::empty_chunk(&[""], "anything", "Failed to index")]
    #[case::empty_query(&["some text"], "", "query failed")]
    #[tokio::test]
    async fn empty_chunks_and_queries_are_rejected(
        #[case] corpus: &[&str],
        #[case] query: &str,
        #[case] message: &str,
    ) {
        let err = VerifiedRagEngine::new(8)
            .query_verified(corpus, query)
            .await
            .unwrap_err();
        assert!(matches!(err, RagError::IndexError(ref m) if m.contains(message)));
    }

    #[rstest]
    #[case::index(RagError::IndexError("a".into()), "Indexing or query error: a")]
    #[case::verification(RagError::VerificationFailure("b".into()), "Verification failed: b")]
    #[case::topology(RagError::TopologyError("c".into()), "Graph topology error: c")]
    fn errors_describe_themselves(#[case] error: RagError, #[case] expected: &str) {
        assert_eq!(error.to_string(), expected);
    }

    #[tokio::test]
    async fn a_subgraph_is_verbalized_and_queried() {
        let node = |key: &str, hops, relationship| {
            (
                key.to_string(),
                ExpandedNode {
                    key: key.to_string(),
                    hop_distance: hops,
                    weight: 1.0,
                    relationship,
                },
            )
        };
        let subgraph = ExpandedSubgraph {
            seeds: vec!["sku".to_string()],
            nodes: HashMap::from([
                node("sku", 0, SeedRelationship::Seed),
                node("brand", 1, SeedRelationship::Ancestor),
            ]),
            edges: vec![ExpandedEdge {
                parent_key: "brand".to_string(),
                child_key: "sku".to_string(),
                relationship: "PARENT_OF".to_string(),
                confidence: 0.9,
                provenance: ProvenanceKind::DeterministicRule,
            }],
        };
        let answer = VerifiedRagEngine::new(32)
            .query_graph_verified(&subgraph, "Which brand owns the sku?")
            .await
            .unwrap();
        assert!(!answer.is_empty());
    }
}
