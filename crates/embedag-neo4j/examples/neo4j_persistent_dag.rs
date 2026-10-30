//! Example demonstrating embedag native property graph persistence with Neo4j Bolt adapter.

use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_neo4j::Neo4jEngine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SkillNode {
    skill_id: String,
    title: String,
    domain: String,
}

embedag_core::impl_node_payload!(
    SkillNode,
    key = |n| &n.skill_id,
    name = |n| &n.title,
    level = |n| &n.domain,
    attributes = |n| {
        serde_json::json!({
            "title": n.title,
            "domain": n.domain,
        })
    }
);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== embedag Neo4j OpenCypher Bolt Adapter Demo ===");

    // In-memory mock engine (or use Neo4jConfig::bolt(...) for live server)
    let engine = Neo4jEngine::in_memory().await?;
    println!("✓ Neo4j storage engine initialized.");

    // Build DAG of prerequisite skills
    let mut dag = MemoryDag::<SkillNode>::new();

    let rust_basics = SkillNode {
        skill_id: "RUST_101".into(),
        title: "Rust Core & Ownership".into(),
        domain: "Programming".into(),
    };
    let async_rust = SkillNode {
        skill_id: "RUST_201".into(),
        title: "Asynchronous Tokio & Futures".into(),
        domain: "Systems".into(),
    };
    let graph_rag = SkillNode {
        skill_id: "RAG_301".into(),
        title: "Deterministic Hybrid Graph-RAG".into(),
        domain: "AI Engineering".into(),
    };

    dag.upsert_node(rust_basics.clone());
    dag.upsert_node(async_rust.clone());
    dag.upsert_node(graph_rag.clone());

    dag.add_edge_by_keys(
        &rust_basics.skill_id,
        &async_rust.skill_id,
        EdgePayload::new("PREREQUISITE_FOR", 1.0, ProvenanceKind::DeterministicRule)?,
    )?;
    dag.add_edge_by_keys(
        &async_rust.skill_id,
        &graph_rag.skill_id,
        EdgePayload::new("PREREQUISITE_FOR", 0.95, ProvenanceKind::DeterministicRule)?,
    )?;

    println!(
        "DAG created: {} nodes, {} edges.",
        dag.node_count(),
        dag.edge_count()
    );

    // Export to Neo4j
    engine.export_dag(&dag).await?;
    println!("✓ Exported DAG to Neo4j graph model.");

    // Traverse children
    let prerequisites: Vec<SkillNode> = engine.get_children(&rust_basics.skill_id).await?;
    println!(
        "RUST_101 unlocks: {:?}",
        prerequisites.iter().map(|s| &s.title).collect::<Vec<_>>()
    );

    // Reconstruct DAG from Neo4j
    let reconstructed = engine.load_dag::<SkillNode>().await?;
    println!(
        "✓ Reconstructed DAG from Neo4j: {} nodes, {} edges.",
        reconstructed.node_count(),
        reconstructed.edge_count()
    );

    println!("=== Neo4j Demo Complete ===");
    Ok(())
}
