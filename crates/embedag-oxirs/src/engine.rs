//! High-level OxiRS TDB2 engine adapter for embedag.

use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use oxirs_core::model::{Object, Predicate, Quad, Subject};
use oxirs_core::rdf_store::{OxirsQueryResults, RdfStore, Store};
use parking_lot::RwLock;
use petgraph::visit::EdgeRef;
use serde::{de::DeserializeOwned, Serialize};

use crate::config::{OxiRsTdbConfig, StorageMode};
use crate::error::OxiRsStorageError;
use crate::mapping::{
    edge_to_quads, make_graph_name, make_node_iri, node_to_quads, reconstruct_node,
    SchemaVocab,
};

/// High-performance native OxiRS TDB2 engine.
///
/// Features:
/// - Pure Rust RDF store with zero external daemons or Docker requirements.
/// - Line-atomic append persistence (`data.nq`), crash-safe fsync sync policies.
/// - Sub-second cold starts via memory-mapped snapshots (`snapshot.oxsnap`).
/// - Built-in SPARQL 1.1/1.2 query and pattern-matching capabilities.
/// - Seamless bidirectional import/export with [`MemoryDag`].
#[derive(Clone)]
pub struct OxiRsTdbEngine {
    store: Arc<RdfStore>,
    config: OxiRsTdbConfig,
    vocab: SchemaVocab,
    vector_index: Arc<RwLock<HashMap<String, Vec<f32>>>>,
}

impl OxiRsTdbEngine {
    /// Opens or creates an OxiRS TDB2 storage engine according to the given configuration.
    pub fn open(config: OxiRsTdbConfig) -> Result<Self, OxiRsStorageError> {
        let vocab = SchemaVocab::new(&config.schema_ns)?;

        let store = match &config.mode {
            StorageMode::InMemory => RdfStore::new()?,
            StorageMode::Persistent(path) => {
                RdfStore::open_with_sync_policy(path, config.sync_policy)?
            }
        };

        Ok(Self {
            store: Arc::new(store),
            config,
            vocab,
            vector_index: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Opens a persistent OxiRS TDB2 store at `path` with default settings.
    pub fn open_persistent(path: impl AsRef<Path>) -> Result<Self, OxiRsStorageError> {
        Self::open(OxiRsTdbConfig::persistent(path))
    }

    /// Creates an in-memory transient OxiRS store for testing.
    pub fn in_memory() -> Result<Self, OxiRsStorageError> {
        Self::open(OxiRsTdbConfig::in_memory())
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &OxiRsTdbConfig {
        &self.config
    }

    /// Returns a reference to the schema vocabulary.
    pub fn vocab(&self) -> &SchemaVocab {
        &self.vocab
    }

    /// Returns a reference to the underlying `RdfStore`.
    pub fn inner(&self) -> &RdfStore {
        &self.store
    }

    /// Flushes pending appends and deletions out to durable disk.
    pub fn flush(&self) -> Result<(), OxiRsStorageError> {
        self.store.flush()?;
        Ok(())
    }

    /// Returns total number of quads stored.
    pub fn len(&self) -> Result<usize, OxiRsStorageError> {
        self.store.len().map_err(Into::into)
    }

    /// Returns `true` if the store contains zero quads.
    pub fn is_empty(&self) -> Result<bool, OxiRsStorageError> {
        self.store.is_empty().map_err(Into::into)
    }

    /// Inserts a raw RDF quad into the store.
    pub fn insert_quad(&self, quad: Quad) -> Result<bool, OxiRsStorageError> {
        self.store.insert_quad(quad).map_err(Into::into)
    }

    /// Bulk inserts an array of RDF quads into the store in a single append transaction.
    pub fn bulk_insert_quads(&self, quads: Vec<Quad>) -> Result<usize, OxiRsStorageError> {
        self.store.bulk_insert_quads(quads).map_err(Into::into)
    }

    /// Removes a quad from the store.
    pub fn remove_quad(&self, quad: &Quad) -> Result<bool, OxiRsStorageError> {
        self.store.remove_quad(quad).map_err(Into::into)
    }

    /// Finds quads matching an optional pattern.
    pub fn find_quads(
        &self,
        subject: Option<&Subject>,
        predicate: Option<&Predicate>,
        object: Option<&Object>,
    ) -> Result<Vec<Quad>, OxiRsStorageError> {
        let graph = make_graph_name(&self.config)?;
        self.store
            .find_quads(subject, predicate, object, Some(&graph))
            .map_err(Into::into)
    }

    /// Executes a SPARQL query against the persistent store.
    pub fn query_sparql(&self, sparql: &str) -> Result<OxirsQueryResults, OxiRsStorageError> {
        self.store.query(sparql).map_err(Into::into)
    }

    /// Serializes and writes a frozen memory-mapped snapshot (`snapshot.oxsnap`) to disk.
    pub fn save_snapshot(&self, snapshot_path: impl AsRef<Path>) -> Result<(), OxiRsStorageError> {
        self.store.write_snapshot_to(snapshot_path.as_ref())?;
        Ok(())
    }

    /// Offline builder: reads `data.nq` and generates a compiled `snapshot.oxsnap`.
    pub fn build_snapshot(dataset_dir: impl AsRef<Path>) -> Result<PathBuf, OxiRsStorageError> {
        RdfStore::build_snapshot(dataset_dir).map_err(Into::into)
    }

    /// Inserts or updates a domain node `T: NodePayload + Serialize`.
    pub fn insert_node<T: NodePayload + Serialize>(&self, node: &T) -> Result<(), OxiRsStorageError> {
        // Remove existing statements for this node first if present
        self.remove_node_by_key(&node.key().to_string())?;

        let quads = node_to_quads(&self.config, &self.vocab, node)?;
        self.bulk_insert_quads(quads)?;
        Ok(())
    }

    /// Adds a directed edge from parent to child with metadata.
    pub fn insert_edge<K: Display>(
        &self,
        parent: &K,
        child: &K,
        edge: &EdgePayload,
    ) -> Result<(), OxiRsStorageError> {
        let quads = edge_to_quads(&self.config, &self.vocab, parent, child, edge)?;
        self.bulk_insert_quads(quads)?;
        Ok(())
    }

    /// Removes all RDF quads associated with a node's business key.
    pub fn remove_node_by_key(&self, key: &str) -> Result<bool, OxiRsStorageError> {
        let node_iri = make_node_iri(&self.config.base_iri, key)?;
        let subj = Subject::NamedNode(node_iri);
        let quads = self.find_quads(Some(&subj), None, None)?;

        let mut removed_any = false;
        for quad in &quads {
            if self.remove_quad(quad)? {
                removed_any = true;
            }
        }

        // Also remove from vector index if present
        self.vector_index.write().remove(key);

        Ok(removed_any)
    }

    /// Removes a directed edge between parent and child, optionally filtered by relationship.
    pub fn remove_edge<K: Display>(
        &self,
        parent: &K,
        child: &K,
        relationship: Option<&str>,
    ) -> Result<bool, OxiRsStorageError> {
        let parent_iri = make_node_iri(&self.config.base_iri, &parent.to_string())?;
        let child_iri = make_node_iri(&self.config.base_iri, &child.to_string())?;

        let pred = if let Some(rel) = relationship {
            Some(Predicate::NamedNode(self.vocab.custom_relation(rel)?))
        } else {
            None
        };

        let quads = self.find_quads(
            Some(&Subject::NamedNode(parent_iri.clone())),
            pred.as_ref(),
            Some(&Object::NamedNode(child_iri.clone())),
        )?;

        let mut removed = false;
        for q in &quads {
            if self.remove_quad(q)? {
                removed = true;
            }
        }

        // Also clean up reified edge entities
        let edge_quads = self.find_quads(
            None,
            Some(&Predicate::NamedNode(self.vocab.parent_rel.clone())),
            Some(&Object::NamedNode(parent_iri)),
        )?;

        for eq in edge_quads {
            let edge_subj = eq.subject();
            // Check if child matches
            let child_matches = self.find_quads(
                Some(edge_subj),
                Some(&Predicate::NamedNode(self.vocab.child_rel.clone())),
                Some(&Object::NamedNode(child_iri.clone())),
            )?;

            if !child_matches.is_empty() {
                // If relationship filter is specified, check relationship
                if let Some(rel) = relationship {
                    let rel_matches = self.find_quads(
                        Some(edge_subj),
                        Some(&Predicate::NamedNode(self.vocab.relationship.clone())),
                        Some(&Object::Literal(oxirs_core::model::Literal::new(rel))),
                    )?;
                    if rel_matches.is_empty() {
                        continue;
                    }
                }

                // Delete all triples belonging to this edge subject
                let all_edge_triples = self.find_quads(Some(edge_subj), None, None)?;
                for t in &all_edge_triples {
                    self.remove_quad(t)?;
                }
            }
        }

        Ok(removed)
    }

    /// Fetches a node by its business key.
    pub fn get_node<T: NodePayload + DeserializeOwned>(
        &self,
        key: &T::Key,
    ) -> Result<Option<T>, OxiRsStorageError> {
        let node_iri = make_node_iri(&self.config.base_iri, &key.to_string())?;
        let quads = self.find_quads(Some(&Subject::NamedNode(node_iri)), None, None)?;

        if quads.is_empty() {
            return Ok(None);
        }

        reconstruct_node(&quads, &self.vocab)
    }

    /// Fetches immediate child nodes of a given parent key.
    pub fn get_children<T: NodePayload + DeserializeOwned>(
        &self,
        parent_key: &T::Key,
    ) -> Result<Vec<T>, OxiRsStorageError> {
        let parent_iri = make_node_iri(&self.config.base_iri, &parent_key.to_string())?;
        let quads = self.find_quads(Some(&Subject::NamedNode(parent_iri)), None, None)?;

        let mut children = Vec::new();
        for quad in quads {
            if let Object::NamedNode(child_node) = quad.object() {
                // Query node properties for child
                let child_quads =
                    self.find_quads(Some(&Subject::NamedNode(child_node.clone())), None, None)?;
                if let Some(child) = reconstruct_node::<T>(&child_quads, &self.vocab)? {
                    children.push(child);
                }
            }
        }

        Ok(children)
    }

    /// Fetches immediate parent nodes of a given child key.
    pub fn get_parents<T: NodePayload + DeserializeOwned>(
        &self,
        child_key: &T::Key,
    ) -> Result<Vec<T>, OxiRsStorageError> {
        let child_iri = make_node_iri(&self.config.base_iri, &child_key.to_string())?;
        let quads = self.find_quads(None, None, Some(&Object::NamedNode(child_iri)))?;

        let mut parents = Vec::new();
        for quad in quads {
            if let Subject::NamedNode(parent_node) = quad.subject() {
                // If subject is an edge reification, skip it
                if parent_node.as_str().contains("/edge/") {
                    continue;
                }
                let parent_quads =
                    self.find_quads(Some(&Subject::NamedNode(parent_node.clone())), None, None)?;
                if let Some(parent) = reconstruct_node::<T>(&parent_quads, &self.vocab)? {
                    parents.push(parent);
                }
            }
        }

        Ok(parents)
    }

    /// Indexes a dense vector embedding for a node key.
    pub fn index_embedding(&self, key: impl Into<String>, embedding: Vec<f32>) {
        self.vector_index.write().insert(key.into(), embedding);
    }

    /// Finds approximate nearest neighbors by cosine similarity across indexed embeddings.
    pub fn find_nearest_neighbors<T: NodePayload + DeserializeOwned>(
        &self,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<(T, f32)>, OxiRsStorageError> {
        let index = self.vector_index.read();
        let mut scored: Vec<(String, f32)> = Vec::with_capacity(index.len());

        let q_norm = query.iter().map(|v| v * v).sum::<f32>().sqrt();
        if q_norm == 0.0 {
            return Ok(Vec::new());
        }

        for (key, vec) in index.iter() {
            if vec.len() != query.len() {
                continue;
            }
            let dot = query.iter().zip(vec.iter()).map(|(a, b)| a * b).sum::<f32>();
            let v_norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
            let score = if v_norm > 0.0 {
                dot / (q_norm * v_norm)
            } else {
                0.0
            };
            scored.push((key.clone(), score));
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        let mut results = Vec::with_capacity(scored.len());
        for (key_str, score) in scored {
            // Lookup node in OxiRS
            let node_iri = make_node_iri(&self.config.base_iri, &key_str)?;
            let quads = self.find_quads(Some(&Subject::NamedNode(node_iri)), None, None)?;
            if let Some(node) = reconstruct_node::<T>(&quads, &self.vocab)? {
                results.push((node, score));
            }
        }

        Ok(results)
    }

    /// Exports an entire in-memory [`MemoryDag<T>`] into the persistent OxiRS store.
    pub fn export_dag<T: NodePayload + Serialize>(
        &self,
        dag: &MemoryDag<T>,
    ) -> Result<usize, OxiRsStorageError> {
        let inner = dag.inner();
        let mut all_quads = Vec::new();

        // 1. Export all nodes
        for idx in inner.node_indices() {
            if let Some(payload) = inner.node_weight(idx) {
                let quads = node_to_quads(&self.config, &self.vocab, payload)?;
                all_quads.extend(quads);
            }
        }

        // 2. Export all directed edges
        for edge_ref in inner.edge_references() {
            let src = &inner[edge_ref.source()];
            let tgt = &inner[edge_ref.target()];
            let edge = edge_ref.weight();
            let quads = edge_to_quads(&self.config, &self.vocab, src.key(), tgt.key(), edge)?;
            all_quads.extend(quads);
        }

        let count = self.bulk_insert_quads(all_quads)?;
        self.flush()?;
        Ok(count)
    }

    /// Reconstructs and validates an entire [`MemoryDag<T>`] from the persistent OxiRS store.
    pub fn load_dag<T: NodePayload + DeserializeOwned>(&self) -> Result<MemoryDag<T>, OxiRsStorageError> {
        let mut dag = MemoryDag::new();

        // 1. Query all nodes having embedag:payload
        let node_quads = self.find_quads(
            None,
            Some(&Predicate::NamedNode(self.vocab.payload.clone())),
            None,
        )?;

        let mut node_keys = HashMap::new();
        for q in node_quads {
            if let Subject::NamedNode(_subj) = q.subject() {
                if let Object::Literal(lit) = q.object() {
                    let node: T = serde_json::from_str(lit.value())?;
                    let key_str = node.key().to_string();
                    let idx = dag.upsert_node(node);
                    node_keys.insert(key_str, idx);
                }
            }
        }

        // 2. Query all reified edges (type embedag:Edge)
        let edge_quads = self.find_quads(
            None,
            Some(&Predicate::NamedNode(self.vocab.rdf_type.clone())),
            Some(&Object::NamedNode(self.vocab.edge_type.clone())),
        )?;

        for eq in edge_quads {
            let edge_subj = eq.subject();

            // Fetch parent IRI, child IRI, relationship, confidence, provenance
            let parent_quads = self.find_quads(
                Some(edge_subj),
                Some(&Predicate::NamedNode(self.vocab.parent_rel.clone())),
                None,
            )?;
            let child_quads = self.find_quads(
                Some(edge_subj),
                Some(&Predicate::NamedNode(self.vocab.child_rel.clone())),
                None,
            )?;
            let rel_quads = self.find_quads(
                Some(edge_subj),
                Some(&Predicate::NamedNode(self.vocab.relationship.clone())),
                None,
            )?;
            let conf_quads = self.find_quads(
                Some(edge_subj),
                Some(&Predicate::NamedNode(self.vocab.confidence.clone())),
                None,
            )?;

            if let (Some(pq), Some(cq), Some(rq), Some(cfq)) = (
                parent_quads.first(),
                child_quads.first(),
                rel_quads.first(),
                conf_quads.first(),
            ) {
                if let (Object::NamedNode(p_node), Object::NamedNode(c_node)) =
                    (pq.object(), cq.object())
                {
                    let p_key = extract_key_from_iri(p_node.as_str());
                    let c_key = extract_key_from_iri(c_node.as_str());
                    let rel = rq.object().to_string().replace('"', "");
                    let conf: f32 = cfq
                        .object()
                        .to_string()
                        .replace('"', "")
                        .parse()
                        .unwrap_or(1.0);

                    if let (Some(&p_idx), Some(&c_idx)) =
                        (node_keys.get(&p_key), node_keys.get(&c_key))
                    {
                        let payload =
                            EdgePayload::new(rel, conf, ProvenanceKind::DeterministicRule)?;
                        dag.add_edge(p_idx, c_idx, payload);
                    }
                }
            }
        }

        // Validate acyclic invariants
        dag.validate_acyclic()?;
        Ok(dag)
    }
}

fn extract_key_from_iri(iri: &str) -> String {
    if let Some(idx) = iri.rfind('/') {
        iri[idx + 1..].to_string()
    } else if let Some(idx) = iri.rfind(':') {
        iri[idx + 1..].to_string()
    } else {
        iri.to_string()
    }
}
