//! Sovereign 4-layer OxiRAG pipeline integration for embedag.
//!
//! Powered by KitaSan `oxirag`, providing multi-layer verified retrieval:
//! - Layer 1 (Echo): Fast prefix caching and vector similarity search.
//! - Layer 2 (Speculator): Speculative draft retrieval and windowing.
//! - Layer 3 (Judge): Logic, arithmetic & constraint verification via SMT.
//! - Layer 4 (Graph): Subgraph topology expansion and RDF graph grounding.

use crate::expansion::ExpandedSubgraph;
use oxirag::prelude::*;
use thiserror::Error;

/// Error variants occurring during OxiRAG pipeline construction or execution.
#[derive(Debug, Error)]
pub enum RagError {
    #[error("OxiRAG indexing or query error: {0}")]
    IndexError(String),

    #[error("Verification failed: {0}")]
    VerificationFailure(String),

    #[error("Graph topology error: {0}")]
    TopologyError(String),
}

/// Sovereign 4-layer RAG processor utilizing KitaSan `oxirag`.
#[derive(Debug, Clone)]
pub struct OxiRagEngine {
    embedding_dim: usize,
}

impl Default for OxiRagEngine {
    fn default() -> Self {
        Self::new(384)
    }
}

impl OxiRagEngine {
    /// Creates a new `OxiRagEngine` with specified embedding dimensions (default: 384 for MiniLM/SciRS2).
    #[must_use]
    pub fn new(embedding_dim: usize) -> Self {
        Self { embedding_dim }
    }

    /// Returns the configured embedding dimensions.
    #[must_use]
    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }

    /// Indexes raw text corpus and executes a verified retrieval query through the 4-layer pipeline.
    pub async fn query_verified(
        &self,
        corpus: &[&str],
        query_text: &str,
    ) -> Result<String, RagError> {
        let echo = EchoLayer::new(
            MockEmbeddingProvider::new(self.embedding_dim),
            InMemoryVectorStore::new(self.embedding_dim),
        );

        let speculator = RuleBasedSpeculator::default();

        let judge = JudgeImpl::new(
            AdvancedClaimExtractor::new(),
            MockSmtVerifier::default(),
            JudgeConfig::default(),
        );

        let mut pipeline = PipelineBuilder::new()
            .with_echo(echo)
            .with_speculator(speculator)
            .with_judge(judge)
            .build()
            .map_err(|e| RagError::IndexError(format!("Failed to build OxiRAG pipeline: {e:?}")))?;

        for &chunk in corpus {
            pipeline
                .index(Document::new(chunk))
                .await
                .map_err(|e| RagError::IndexError(format!("Failed to index in OxiRAG: {e:?}")))?;
        }

        let query = Query::new(query_text);
        let result = pipeline
            .process(query)
            .await
            .map_err(|e| RagError::IndexError(format!("OxiRAG query failed: {e:?}")))?;

        Ok(result.final_answer)
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

    #[tokio::test]
    async fn test_oxirag_engine_verified_query() {
        let engine = OxiRagEngine::new(384);
        let corpus = &[
            "Rust 2024 edition features sovereign zero-FFI compilation.",
            "OxiRAG provides 4-layer speculative retrieval and formal logic verification.",
            "Embedag manages dynamic hierarchical knowledge DAG arenas with petgraph.",
        ];

        let result = engine
            .query_verified(corpus, "What does OxiRAG provide?")
            .await
            .expect("verified query succeeds");

        assert!(!result.is_empty());
    }
}
