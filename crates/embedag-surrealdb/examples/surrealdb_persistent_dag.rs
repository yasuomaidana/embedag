//! Example demonstrating embedag multi-model persistence with SurrealDB.

use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use embedag_surrealdb::SurrealDbEngine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct OrgUnit {
    code: String,
    name: String,
    unit_type: String,
}

impl NodePayload for OrgUnit {
    type Key = String;

    fn key(&self) -> &Self::Key {
        &self.code
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn entity_level(&self) -> &str {
        &self.unit_type
    }

    fn attributes(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "unit_type": self.unit_type,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== embedag SurrealDB Multi-Model Storage Demo ===");

    let engine = SurrealDbEngine::in_memory().await?;
    println!("✓ In-memory SurrealDB instance initialized.");

    // Build an in-memory organization hierarchy DAG
    let mut dag = MemoryDag::<OrgUnit>::new();

    let company = OrgUnit {
        code: "CORP".into(),
        name: "Enterprise Corp".into(),
        unit_type: "Headquarters".into(),
    };
    let engineering = OrgUnit {
        code: "ENG".into(),
        name: "Engineering & R&D".into(),
        unit_type: "Division".into(),
    };
    let ai_team = OrgUnit {
        code: "AI_LAB".into(),
        name: "Applied AI Core".into(),
        unit_type: "Department".into(),
    };

    dag.upsert_node(company.clone());
    dag.upsert_node(engineering.clone());
    dag.upsert_node(ai_team.clone());

    dag.add_edge_by_keys(
        &company.code,
        &engineering.code,
        EdgePayload::new("MANAGES", 1.0, ProvenanceKind::DeterministicRule)?,
    )?;
    dag.add_edge_by_keys(
        &engineering.code,
        &ai_team.code,
        EdgePayload::new("INCLUDES", 0.98, ProvenanceKind::DeterministicRule)?,
    )?;

    println!(
        "DAG created: {} nodes, {} edges.",
        dag.node_count(),
        dag.edge_count()
    );

    // Export to SurrealDB
    engine.export_dag(&dag).await?;
    println!("✓ Exported DAG to SurrealDB.");

    // Query immediate children of CORP
    let corp_children: Vec<OrgUnit> = engine.get_children(&company.code).await?;
    println!(
        "CORP direct children in SurrealDB: {:?}",
        corp_children.iter().map(|u| &u.name).collect::<Vec<_>>()
    );

    // Query parents of AI_LAB
    let lab_parents: Vec<OrgUnit> = engine.get_parents(&ai_team.code).await?;
    println!(
        "AI_LAB direct parents in SurrealDB: {:?}",
        lab_parents.iter().map(|u| &u.name).collect::<Vec<_>>()
    );

    // Reconstruct into fresh DAG
    let reconstructed = engine.load_dag::<OrgUnit>().await?;
    println!(
        "✓ Reconstructed DAG from SurrealDB: {} nodes, {} edges.",
        reconstructed.node_count(),
        reconstructed.edge_count()
    );

    println!("=== SurrealDB Demo Complete ===");
    Ok(())
}
