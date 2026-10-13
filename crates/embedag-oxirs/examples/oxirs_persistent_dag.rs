//! Reference example demonstrating Native OxiRS TDB2 Persistent Storage Engine.
//!
//! Run with:
//! ```bash
//! cargo run --example oxirs_persistent_dag -p embedag-oxirs
//! ```

use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use embedag_oxirs::{OxiRsTdbConfig, OxiRsTdbEngine, SyncPolicy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tempfile::tempdir;

/// Developer-defined domain entity: Medical Concept hierarchy (SNOMED-like).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MedicalConcept {
    code: String,
    title: String,
    domain: String,
    metadata: HashMap<String, String>,
}

impl MedicalConcept {
    fn new(code: &str, title: &str, domain: &str) -> Self {
        Self {
            code: code.to_string(),
            title: title.to_string(),
            domain: domain.to_string(),
            metadata: HashMap::new(),
        }
    }
}

impl NodePayload for MedicalConcept {
    type Key = String;

    fn key(&self) -> &Self::Key {
        &self.code
    }

    fn name(&self) -> &str {
        &self.title
    }

    fn entity_level(&self) -> &str {
        &self.domain
    }

    fn attributes(&self) -> serde_json::Value {
        serde_json::to_value(&self.metadata).unwrap_or(serde_json::Value::Null)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== OxiRS TDB2 Persistent Storage Engine Demonstration ===");

    let tmp = tempdir()?;
    let db_path = tmp.path().join("medical_knowledge_tdb");

    // 1. Developer configures custom base URI, durability, and on-disk directory
    let config = OxiRsTdbConfig::persistent(&db_path)
        .with_base_iri("https://health.example.org/ontology/concept/")
        .with_schema_ns("https://health.example.org/schema#")
        .with_sync_policy(SyncPolicy::EveryN(1));

    println!("Dataset directory: {:?}", db_path);
    println!("Base IRI: {}", config.base_iri);

    // 2. Open persistent storage engine
    let engine = OxiRsTdbEngine::open(config.clone())?;

    // 3. Construct an in-memory DAG
    let mut dag = MemoryDag::new();

    let root = MedicalConcept::new("DIS-001", "Cardiovascular Disease", "Disorder");
    let sub1 = MedicalConcept::new("DIS-101", "Ischemic Heart Disease", "Disorder");
    let sub2 = MedicalConcept::new("DIS-201", "Myocardial Infarction", "Disorder");

    dag.upsert_node(root);
    dag.upsert_node(sub1);
    dag.upsert_node(sub2);

    dag.add_edge_by_keys(
        &"DIS-001".to_string(),
        &"DIS-101".to_string(),
        EdgePayload::new("IS_A", 1.0, ProvenanceKind::DeterministicRule)?,
    )?;

    dag.add_edge_by_keys(
        &"DIS-101".to_string(),
        &"DIS-201".to_string(),
        EdgePayload::new("IS_A", 0.98, ProvenanceKind::DeterministicRule)?,
    )?;

    println!(
        "In-memory DAG constructed with {} nodes and {} edges.",
        dag.node_count(),
        dag.edge_count()
    );

    // 4. Export DAG directly into OxiRS TDB2
    let quad_count = engine.export_dag(&dag)?;
    println!("Persisted {} RDF quads to on-disk append log.", quad_count);

    // 5. Query via standard SPARQL 1.2
    let sparql_query = r#"
        PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
        SELECT ?label WHERE {
            ?concept rdfs:label ?label .
        }
    "#;

    let results = engine.query_sparql(sparql_query)?;
    println!(
        "SPARQL query executed successfully! Found {} matching concepts.",
        results.len()
    );

    // 6. Bake frozen memory-mapped snapshot for sub-second cold starts
    let snapshot_file = db_path.join("snapshot.oxsnap");
    engine.save_snapshot(&snapshot_file)?;
    println!("Memory-mapped snapshot written: {:?}", snapshot_file);

    // 7. Reopen store from disk and reconstruct DAG
    drop(engine);
    let reopened = OxiRsTdbEngine::open(config)?;
    let reloaded_dag: MemoryDag<MedicalConcept> = reopened.load_dag()?;

    println!(
        "Reconstituted DAG from disk: {} nodes, {} edges. Invariants valid: {}",
        reloaded_dag.node_count(),
        reloaded_dag.edge_count(),
        reloaded_dag.validate_acyclic().is_ok()
    );

    let children = reloaded_dag.children_of(&"DIS-001".to_string());
    println!(
        "Immediate children of DIS-001: {:?}",
        children.iter().map(|c| &c.title).collect::<Vec<_>>()
    );

    println!("=== Demonstration Complete ===");
    Ok(())
}
