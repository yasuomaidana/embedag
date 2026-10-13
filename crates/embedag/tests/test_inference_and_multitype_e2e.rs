use chrono::{DateTime, TimeZone, Utc};
use diesel::Connection;
use diesel::connection::SimpleConnection;
use embedag::llm::{
    ExtractionDirective, ExtractionEnvelope, LlmProvider, OxibonsaiProvider, OxillamaProvider,
    generate_extraction_schema,
};
use embedag::persistence::harness::PostgresHarness;
use embedag::persistence::migrations::{run_postgres_migration, run_sqlite_migration};

const SQLITE_E2E_MIGRATION_UP: &str = r#"
CREATE TABLE IF NOT EXISTS semantic_nodes (
    id TEXT PRIMARY KEY,
    business_key TEXT NOT NULL UNIQUE,
    entity_level TEXT NOT NULL,
    name TEXT NOT NULL,
    attributes TEXT NOT NULL DEFAULT '{}',
    embedding BLOB,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS semantic_edges (
    parent_id TEXT NOT NULL REFERENCES semantic_nodes(id) ON DELETE CASCADE,
    child_id TEXT NOT NULL REFERENCES semantic_nodes(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    provenance TEXT NOT NULL,
    valid_from TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    valid_to TEXT DEFAULT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (parent_id, child_id, relationship_type)
);
"#;

const SQLITE_E2E_MIGRATION_DOWN: &str = r#"
DROP TABLE IF EXISTS semantic_edges;
DROP TABLE IF EXISTS semantic_nodes;
"#;

const POSTGRES_E2E_MIGRATION_UP: &str = r#"
CREATE TABLE IF NOT EXISTS semantic_nodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    business_key VARCHAR(255) NOT NULL UNIQUE,
    entity_level VARCHAR(64) NOT NULL,
    name VARCHAR(255) NOT NULL,
    attributes JSONB NOT NULL DEFAULT '{}',
    embedding vector(1024),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE IF NOT EXISTS semantic_edges (
    parent_id UUID NOT NULL REFERENCES semantic_nodes(id) ON DELETE CASCADE,
    child_id UUID NOT NULL REFERENCES semantic_nodes(id) ON DELETE CASCADE,
    relationship_type VARCHAR(64) NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    provenance VARCHAR(64) NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    valid_to TIMESTAMPTZ DEFAULT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (parent_id, child_id, relationship_type)
);
"#;

const POSTGRES_E2E_MIGRATION_DOWN: &str = r#"
DROP TABLE IF EXISTS semantic_edges;
DROP TABLE IF EXISTS semantic_nodes;
"#;
use embedag::persistence::ops::{
    EdgeUpsertRecord, NodeUpsertRecord, batch_upsert_edges, batch_upsert_nodes,
};
use embedag::persistence::traversal::RecursiveTraversalBuilder;
use embedag::prelude::*;
use h3o::{CellIndex, LatLng, Resolution};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Multi-typed enterprise domain struct exercising various Rust primitive and composite types:
/// - `Uuid`: Globally unique persistent entity identifier
/// - `String`: Categorical tags and identifiers
/// - `f64`: Floating-point scalar metric
/// - `i64`: 64-bit signed integer counter
/// - `bool`: Boolean state flag
/// - `DateTime<Utc>`: Chrono temporal point-in-time timestamp
/// - `Option<String>`: Optional nullable text field
/// - `Option<f64>`: Optional nullable numeric metric
/// - `u64`: Geospatial H3 cell index representation
/// - `MultimodalBlob`: Raw binary media payload with MIME classification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, AsGraphContext)]
pub struct MultimodalIndustrialAsset {
    pub id: Uuid,
    #[graph(key)]
    pub asset_code: String,
    #[graph(text)]
    pub asset_name: String,
    pub entity_level: String,
    #[graph(scalar = "capacity_mw")]
    pub rated_capacity_mw: f64,
    pub operational_hours: i64,
    pub is_online: bool,
    #[graph(temporal)]
    pub commissioned_at: DateTime<Utc>,
    #[graph(tag = "region")]
    pub geographic_zone: String,
    pub secondary_specs: Option<String>,
    pub degradation_ratio: Option<f64>,
    pub geo_h3_cell: u64,
    #[graph(blob)]
    pub inspection_blob: MultimodalBlob,
}

impl NodePayload for MultimodalIndustrialAsset {
    type Key = String;
    fn key(&self) -> &Self::Key {
        &self.asset_code
    }
    fn name(&self) -> &str {
        &self.asset_name
    }
    fn entity_level(&self) -> &str {
        &self.entity_level
    }
}

#[tokio::test]
async fn test_inference_and_multitype_e2e_pipeline() {
    // =========================================================================
    // SECTION 1: Validate Multi-Type Struct & Domain Trait Implementations
    // =========================================================================
    let coord = LatLng::new(37.7749, -122.4194).expect("valid coordinates");
    let h3_index = coord.to_cell(Resolution::Eight);
    let h3_u64 = u64::from(h3_index);
    let cell_roundtrip = CellIndex::try_from(h3_u64).expect("valid H3 cell index");
    assert_eq!(h3_index, cell_roundtrip);

    let test_timestamp = Utc.with_ymd_and_hms(2026, 6, 15, 12, 30, 0).unwrap();
    let blob = MultimodalBlob::new(
        "image/webp",
        "s3://facility-telemetry/west/inverter_diag.webp",
    );

    let root_asset = MultimodalIndustrialAsset {
        id: Uuid::new_v4(),
        asset_code: "FACILITY-GLOBAL-ALPHA".into(),
        asset_name: "Clean Energy Microgrid Facility".into(),
        entity_level: "Facility".into(),
        rated_capacity_mw: 150.0,
        operational_hours: 12450,
        is_online: true,
        commissioned_at: test_timestamp,
        geographic_zone: "US-WEST".into(),
        secondary_specs: Some("Redundant dual-bus topology".into()),
        degradation_ratio: Some(0.012),
        geo_h3_cell: h3_u64,
        inspection_blob: blob.clone(),
    };

    // Verify AsGraphContext procedural derive methods across multiple types
    assert_eq!(root_asset.context_key(), "FACILITY-GLOBAL-ALPHA");
    assert_eq!(root_asset.text_repr(), "Clean Energy Microgrid Facility");
    let scalars = root_asset.scalar_features();
    assert_eq!(scalars.get("capacity_mw"), Some(&150.0));
    let tags = root_asset.categorical_tags();
    assert_eq!(tags.get("region"), Some(&"US-WEST".to_string()));
    assert_eq!(root_asset.timestamp_utc(), Some(test_timestamp));
    assert_eq!(root_asset.multimodal_blobs().len(), 1);
    assert_eq!(root_asset.multimodal_blobs()[0].mime_type, "image/webp");

    // Verify Serde JSON serialization & deserialization roundtrip
    let json_serialized = serde_json::to_string(&root_asset).expect("serialize struct to json");
    let deserialized_asset: MultimodalIndustrialAsset =
        serde_json::from_str(&json_serialized).expect("deserialize struct from json");
    assert_eq!(root_asset, deserialized_asset);

    // =========================================================================
    // SECTION 2: Constrained LLM Inference with Oxibonsai & Pure-Rust Oxillama
    // =========================================================================
    let extraction_schema = generate_extraction_schema();

    // 2A. Oxibonsai Provider (Sub-2-bit ternary quantization, PDA logit masking)
    let bonsai_directive = ExtractionDirective::InsertNode {
        parent_key: "FACILITY-GLOBAL-ALPHA".into(),
        entity_name: "Solar Generation Array Alpha".into(),
        entity_level: "GenerationSubsystem".into(),
        relationship: "CONTAINS_GENERATOR".into(),
        confidence: 0.97,
    };
    let oxibonsai_engine = OxibonsaiProvider::with_simulated_output(
        "oxibonsai-ternary-0.5b",
        bonsai_directive.clone(),
    );

    let bonsai_meta = oxibonsai_engine.model_info();
    assert_eq!(bonsai_meta.provider_name, "oxibonsai");
    assert!(bonsai_meta.is_local_zero_ffi);

    let bonsai_output = oxibonsai_engine
        .generate_structured(
            "Extract generation asset hierarchy from facility manifest",
            &extraction_schema,
        )
        .await
        .expect("oxibonsai structured generation");

    let bonsai_envelope: ExtractionEnvelope =
        serde_json::from_value(bonsai_output).expect("parse oxibonsai envelope");
    assert_eq!(bonsai_envelope.directive, bonsai_directive);
    assert!(bonsai_envelope.chain_of_thought.contains("oxibonsai"));

    // 2B. Oxillama Provider (Pure-Rust GGUF engine, zero C/C++ FFI)
    let llama_directive = ExtractionDirective::InsertNode {
        parent_key: "FACILITY-GLOBAL-ALPHA".into(),
        entity_name: "Battery Energy Storage System".into(),
        entity_level: "StorageSubsystem".into(),
        relationship: "CONTAINS_STORAGE".into(),
        confidence: 0.99,
    };
    let oxillama_engine =
        OxillamaProvider::with_simulated_output("oxillama-q4km-3b", llama_directive.clone());

    let llama_meta = oxillama_engine.model_info();
    assert_eq!(llama_meta.provider_name, "oxillama");
    assert!(llama_meta.is_local_zero_ffi);

    let llama_output = oxillama_engine
        .generate_structured(
            "Extract storage subsystem specifications from technical telemetry",
            &extraction_schema,
        )
        .await
        .expect("oxillama structured generation");

    let llama_envelope: ExtractionEnvelope =
        serde_json::from_value(llama_output).expect("parse oxillama envelope");
    assert_eq!(llama_envelope.directive, llama_directive);
    assert!(llama_envelope.chain_of_thought.contains("oxillama"));

    // =========================================================================
    // SECTION 3: Construct Memory DAG with Multi-Parent Diamond & Cycle Guards
    // =========================================================================
    let mut dag = MemoryDag::new();

    let subcat_solar = MultimodalIndustrialAsset {
        id: Uuid::new_v4(),
        asset_code: "SUBSYSTEM-SOLAR-01".into(),
        asset_name: "Solar Generation Array Alpha".into(),
        entity_level: "GenerationSubsystem".into(),
        rated_capacity_mw: 90.0,
        operational_hours: 8200,
        is_online: true,
        commissioned_at: test_timestamp,
        geographic_zone: "US-WEST".into(),
        secondary_specs: None,
        degradation_ratio: Some(0.005),
        geo_h3_cell: h3_u64,
        inspection_blob: blob.clone(),
    };

    let subcat_battery = MultimodalIndustrialAsset {
        id: Uuid::new_v4(),
        asset_code: "SUBSYSTEM-BATTERY-01".into(),
        asset_name: "Battery Energy Storage System".into(),
        entity_level: "StorageSubsystem".into(),
        rated_capacity_mw: 60.0,
        operational_hours: 4250,
        is_online: true,
        commissioned_at: test_timestamp,
        geographic_zone: "US-WEST".into(),
        secondary_specs: Some("Lithium Iron Phosphate (LFP)".into()),
        degradation_ratio: Some(0.008),
        geo_h3_cell: h3_u64,
        inspection_blob: blob.clone(),
    };

    let leaf_inverter = MultimodalIndustrialAsset {
        id: Uuid::new_v4(),
        asset_code: "DEVICE-INVERTER-SMART-01".into(),
        asset_name: "Hybrid Grid-Forming Smart Inverter 2500kVA".into(),
        entity_level: "PowerElectronicsDevice".into(),
        rated_capacity_mw: 2.5,
        operational_hours: 1200,
        is_online: true,
        commissioned_at: test_timestamp,
        geographic_zone: "US-WEST".into(),
        secondary_specs: Some("IEEE 1547-2018 Compliant".into()),
        degradation_ratio: None,
        geo_h3_cell: h3_u64,
        inspection_blob: blob.clone(),
    };

    dag.upsert_node(root_asset.clone());
    dag.upsert_node(subcat_solar.clone());
    dag.upsert_node(subcat_battery.clone());
    dag.upsert_node(leaf_inverter.clone());

    assert_eq!(dag.node_count(), 4);

    // Build Diamond Multi-Parent DAG:
    // Root -> Solar, Root -> Battery
    // Solar -> Inverter, Battery -> Inverter (multi-parent diamond convergence)
    let edge_root_to_solar =
        EdgePayload::new("CONTAINS_GENERATOR", 1.0, ProvenanceKind::DeterministicRule)
            .expect("valid edge");
    let edge_root_to_battery =
        EdgePayload::new("CONTAINS_STORAGE", 1.0, ProvenanceKind::DeterministicRule)
            .expect("valid edge");
    let edge_solar_to_inverter = EdgePayload::new(
        "POWERS_DEVICE",
        0.98,
        ProvenanceKind::LlmConstrainedInference,
    )
    .expect("valid edge");
    let edge_battery_to_inverter = EdgePayload::new(
        "BACKS_UP_DEVICE",
        0.95,
        ProvenanceKind::LlmConstrainedInference,
    )
    .expect("valid edge");

    dag.add_edge_by_keys_guarded(
        &root_asset.asset_code,
        &subcat_solar.asset_code,
        edge_root_to_solar,
    )
    .expect("add root->solar edge");

    dag.add_edge_by_keys_guarded(
        &root_asset.asset_code,
        &subcat_battery.asset_code,
        edge_root_to_battery,
    )
    .expect("add root->battery edge");

    dag.add_edge_by_keys_guarded(
        &subcat_solar.asset_code,
        &leaf_inverter.asset_code,
        edge_solar_to_inverter,
    )
    .expect("add solar->inverter edge");

    dag.add_edge_by_keys_guarded(
        &subcat_battery.asset_code,
        &leaf_inverter.asset_code,
        edge_battery_to_inverter,
    )
    .expect("add battery->inverter edge (multi-parent)");

    assert_eq!(dag.edge_count(), 4);
    assert!(dag.is_acyclic(), "DAG must be strictly acyclic");

    // Verify Leaf Inverter has 2 distinct parents (Diamond DAG)
    let parents = dag.parents_of(&leaf_inverter.asset_code);
    assert_eq!(
        parents.len(),
        2,
        "Leaf inverter must have 2 distinct parents"
    );
    let parent_keys: Vec<&String> = parents.iter().map(|p| p.key()).collect();
    assert!(parent_keys.contains(&&subcat_solar.asset_code));
    assert!(parent_keys.contains(&&subcat_battery.asset_code));

    // Verify Transitive Ancestors contain all upstream nodes
    let ancestors = dag.transitive_ancestors(&leaf_inverter.asset_code);
    assert_eq!(
        ancestors.len(),
        3,
        "Ancestors must include solar, battery, and facility root"
    );
    let ancestor_keys: Vec<&String> = ancestors.iter().map(|a| a.key()).collect();
    assert!(ancestor_keys.contains(&&root_asset.asset_code));
    assert!(ancestor_keys.contains(&&subcat_solar.asset_code));
    assert!(ancestor_keys.contains(&&subcat_battery.asset_code));

    // Guardrail Verification: Backward edge Leaf -> Root must be rejected to prevent cycle
    assert!(
        dag.would_create_cycle(&leaf_inverter.asset_code, &root_asset.asset_code),
        "Cycle detection must flag backward cycle"
    );
    let cycle_attempt = dag.add_edge_by_keys_guarded(
        &leaf_inverter.asset_code,
        &root_asset.asset_code,
        EdgePayload::new("CYC_EDGE", 0.5, ProvenanceKind::LlmConstrainedInference).unwrap(),
    );
    assert!(
        cycle_attempt.is_err(),
        "Guardrail must prevent cyclic edge insertion"
    );
    assert!(dag.is_acyclic());

    // =========================================================================
    // SECTION 4: Two-Stage Hybrid GraphRAG Retrieval & Grounded Citation Packaging
    // =========================================================================
    // Stage 1: Dense Vector Index + Sparse BM25 Lexical Index
    let mut dense_idx = DenseVectorIndex::with_dimensions(4);
    dense_idx
        .insert(leaf_inverter.id, vec![0.92, 0.85, 0.12, 0.05])
        .expect("dense insert inverter");
    dense_idx
        .insert(subcat_solar.id, vec![0.75, 0.60, 0.20, 0.10])
        .expect("dense insert solar");
    dense_idx
        .insert(subcat_battery.id, vec![0.70, 0.55, 0.30, 0.15])
        .expect("dense insert battery");
    dense_idx
        .insert(root_asset.id, vec![0.30, 0.20, 0.10, 0.80])
        .expect("dense insert root");

    let mut sparse_idx = SparseLexicalIndex::new();
    sparse_idx.insert(
        leaf_inverter.id,
        &leaf_inverter.asset_code,
        &leaf_inverter.asset_name,
    );
    sparse_idx.insert(
        subcat_solar.id,
        &subcat_solar.asset_code,
        &subcat_solar.asset_name,
    );
    sparse_idx.insert(
        subcat_battery.id,
        &subcat_battery.asset_code,
        &subcat_battery.asset_name,
    );
    sparse_idx.insert(
        root_asset.id,
        &root_asset.asset_code,
        &root_asset.asset_name,
    );

    // Query both modalities for hybrid matching
    let query_vector = [0.90, 0.82, 0.15, 0.08];
    let dense_hits = dense_idx.search(&query_vector, 3).expect("dense search");
    let dense_ids: Vec<Uuid> = dense_hits.into_iter().map(|(id, _)| id).collect();

    let sparse_hits = sparse_idx.search("smart hybrid inverter grid forming", 3);
    let sparse_ids: Vec<Uuid> = sparse_hits.into_iter().map(|(id, _)| id).collect();

    let ranker = RrfRanker::default();
    let fused_candidates = ranker.fuse(&dense_ids, &sparse_ids);
    assert!(
        !fused_candidates.is_empty(),
        "RRF fusion must produce candidates"
    );
    assert_eq!(
        fused_candidates[0].id, leaf_inverter.id,
        "Leaf inverter must be top candidate after RRF fusion"
    );

    // Stage 2: Topological Subgraph Expansion with Decay
    let expander = SubgraphExpander::new(0.80, 2, false);
    let seed_keys = vec![(leaf_inverter.asset_code.clone(), fused_candidates[0].score)];
    let expanded_subgraph = expander.expand(&dag, &seed_keys);

    assert_eq!(
        expanded_subgraph.nodes.len(),
        4,
        "Expanded subgraph must include seed and all 3 ancestors in diamond"
    );
    let inverter_weight = expanded_subgraph.nodes[&leaf_inverter.asset_code].weight;
    let solar_weight = expanded_subgraph.nodes[&subcat_solar.asset_code].weight;
    let battery_weight = expanded_subgraph.nodes[&subcat_battery.asset_code].weight;
    let root_weight = expanded_subgraph.nodes[&root_asset.asset_code].weight;

    assert!(inverter_weight > solar_weight);
    assert!(inverter_weight > battery_weight);
    assert!(solar_weight > root_weight);
    assert!(battery_weight > root_weight);

    // Grounded Evidence Context Packaging & Verification
    let packager = GroundedEvidencePackager::new();
    let context_block = packager.package(&dag, &expanded_subgraph);

    assert!(context_block.contains("### Grounded Evidence Context"));
    assert!(context_block.contains("[Node: DEVICE-INVERTER-SMART-01]"));
    assert!(context_block.contains("[Node: SUBSYSTEM-SOLAR-01]"));
    assert!(context_block.contains("[Node: SUBSYSTEM-BATTERY-01]"));
    assert!(context_block.contains("[Node: FACILITY-GLOBAL-ALPHA]"));

    let synthetic_llm_response = format!(
        "The smart inverter [Node: {}] is fed by [Node: {}] and backed up by [Node: {}] within facility [Node: {}].",
        leaf_inverter.asset_code,
        subcat_solar.asset_code,
        subcat_battery.asset_code,
        root_asset.asset_code
    );
    let extracted_citations = packager.extract_citations(&synthetic_llm_response);
    assert_eq!(extracted_citations.len(), 4);
    assert!(extracted_citations.contains(&leaf_inverter.asset_code));
    assert!(extracted_citations.contains(&subcat_solar.asset_code));
    assert!(extracted_citations.contains(&subcat_battery.asset_code));
    assert!(extracted_citations.contains(&root_asset.asset_code));

    // =========================================================================
    // SECTION 5: Relational Persistence Layer Validation (SQLite & PostgreSQL)
    // =========================================================================

    // -------------------------------------------------------------------------
    // 5A. SQLite Persistence Validation
    // -------------------------------------------------------------------------
    let mut sqlite_conn =
        diesel::SqliteConnection::establish(":memory:").expect("establish in-memory sqlite");

    // Execute SQLite schema migration UP
    run_sqlite_migration(&mut sqlite_conn, SQLITE_E2E_MIGRATION_UP).expect("sqlite migrations up");

    // Insert nodes into SQLite semantic_nodes
    let sqlite_node_insert = format!(
        "INSERT INTO semantic_nodes (id, business_key, entity_level, name, attributes) VALUES \
        ('{id_root}', '{key_root}', 'Facility', 'Clean Energy Microgrid Facility', '{attrs_root}'), \
        ('{id_solar}', '{key_solar}', 'GenerationSubsystem', 'Solar Generation Array Alpha', '{attrs_solar}'), \
        ('{id_battery}', '{key_battery}', 'StorageSubsystem', 'Battery Energy Storage System', '{attrs_battery}'), \
        ('{id_inverter}', '{key_inverter}', 'PowerElectronicsDevice', 'Hybrid Grid-Forming Smart Inverter', '{attrs_inverter}');",
        id_root = root_asset.id,
        key_root = root_asset.asset_code,
        attrs_root =
            serde_json::to_string(&serde_json::json!({ "capacity_mw": 150.0, "zone": "US-WEST" }))
                .unwrap(),
        id_solar = subcat_solar.id,
        key_solar = subcat_solar.asset_code,
        attrs_solar = serde_json::to_string(&serde_json::json!({ "capacity_mw": 90.0 })).unwrap(),
        id_battery = subcat_battery.id,
        key_battery = subcat_battery.asset_code,
        attrs_battery = serde_json::to_string(&serde_json::json!({ "capacity_mw": 60.0 })).unwrap(),
        id_inverter = leaf_inverter.id,
        key_inverter = leaf_inverter.asset_code,
        attrs_inverter = serde_json::to_string(&serde_json::json!({ "capacity_mw": 2.5 })).unwrap(),
    );
    sqlite_conn
        .batch_execute(&sqlite_node_insert)
        .expect("batch insert sqlite nodes");

    // Insert diamond edges into SQLite semantic_edges
    let sqlite_edge_insert = format!(
        "INSERT INTO semantic_edges (parent_id, child_id, relationship_type, confidence, provenance) VALUES \
        ('{id_root}', '{id_solar}', 'CONTAINS_GENERATOR', 1.0, 'rule'), \
        ('{id_root}', '{id_battery}', 'CONTAINS_STORAGE', 1.0, 'rule'), \
        ('{id_solar}', '{id_inverter}', 'POWERS_DEVICE', 0.98, 'llm_inferred'), \
        ('{id_battery}', '{id_inverter}', 'BACKS_UP_DEVICE', 0.95, 'llm_inferred');",
        id_root = root_asset.id,
        id_solar = subcat_solar.id,
        id_battery = subcat_battery.id,
        id_inverter = leaf_inverter.id,
    );
    sqlite_conn
        .batch_execute(&sqlite_edge_insert)
        .expect("batch insert sqlite edges");

    // Verify foreign key integrity constraint enforcement in SQLite
    let orphan_parent = Uuid::new_v4();
    let orphan_insert_result = sqlite_conn.batch_execute(&format!(
        "INSERT INTO semantic_edges (parent_id, child_id, relationship_type, confidence, provenance) VALUES ('{orphan_parent}', '{leaf_id}', 'INVALID', 0.5, 'test');",
        leaf_id = leaf_inverter.id
    ));
    assert!(
        orphan_insert_result.is_err(),
        "SQLite foreign keys must reject orphaned edge"
    );

    // Verify SQLite Migration DOWN (clean schema rollback)
    run_sqlite_migration(&mut sqlite_conn, SQLITE_E2E_MIGRATION_DOWN)
        .expect("sqlite migrations down");

    // -------------------------------------------------------------------------
    // 5B. PostgreSQL Persistence Validation with pgvector & Recursive CTEs
    // -------------------------------------------------------------------------
    let postgres_harness = PostgresHarness::start()
        .await
        .expect("start postgres harness container");
    let pg_client = postgres_harness
        .connect()
        .await
        .expect("connect to postgres");

    // Verify container migration up has already created tables and vector extension
    run_postgres_migration(&pg_client, POSTGRES_E2E_MIGRATION_UP)
        .await
        .expect("postgres migrations idempotent up");

    // Batch upsert multi-type nodes into PostgreSQL
    let pg_node_records = vec![
        NodeUpsertRecord {
            id: root_asset.id,
            business_key: root_asset.asset_code.clone(),
            entity_level: root_asset.entity_level.clone(),
            name: root_asset.asset_name.clone(),
            attributes: serde_json::json!({
                "capacity_mw": root_asset.rated_capacity_mw,
                "operational_hours": root_asset.operational_hours,
                "is_online": root_asset.is_online,
                "geo_h3": root_asset.geo_h3_cell,
            }),
            embedding: Some(vec![0.30, 0.20, 0.10, 0.80]),
        },
        NodeUpsertRecord {
            id: subcat_solar.id,
            business_key: subcat_solar.asset_code.clone(),
            entity_level: subcat_solar.entity_level.clone(),
            name: subcat_solar.asset_name.clone(),
            attributes: serde_json::json!({
                "capacity_mw": subcat_solar.rated_capacity_mw,
                "operational_hours": subcat_solar.operational_hours,
            }),
            embedding: Some(vec![0.75, 0.60, 0.20, 0.10]),
        },
        NodeUpsertRecord {
            id: subcat_battery.id,
            business_key: subcat_battery.asset_code.clone(),
            entity_level: subcat_battery.entity_level.clone(),
            name: subcat_battery.asset_name.clone(),
            attributes: serde_json::json!({
                "capacity_mw": subcat_battery.rated_capacity_mw,
                "operational_hours": subcat_battery.operational_hours,
            }),
            embedding: Some(vec![0.70, 0.55, 0.30, 0.15]),
        },
        NodeUpsertRecord {
            id: leaf_inverter.id,
            business_key: leaf_inverter.asset_code.clone(),
            entity_level: leaf_inverter.entity_level.clone(),
            name: leaf_inverter.asset_name.clone(),
            attributes: serde_json::json!({
                "capacity_mw": leaf_inverter.rated_capacity_mw,
                "operational_hours": leaf_inverter.operational_hours,
            }),
            embedding: Some(vec![0.92, 0.85, 0.12, 0.05]),
        },
    ];
    batch_upsert_nodes(&pg_client, &pg_node_records)
        .await
        .expect("batch upsert nodes in pg");

    // Batch upsert diamond edges into PostgreSQL
    let pg_edge_records = vec![
        EdgeUpsertRecord {
            parent_id: root_asset.id,
            child_id: subcat_solar.id,
            relationship_type: "CONTAINS_GENERATOR".into(),
            confidence: 1.0,
            provenance: "rule".into(),
        },
        EdgeUpsertRecord {
            parent_id: root_asset.id,
            child_id: subcat_battery.id,
            relationship_type: "CONTAINS_STORAGE".into(),
            confidence: 1.0,
            provenance: "rule".into(),
        },
        EdgeUpsertRecord {
            parent_id: subcat_solar.id,
            child_id: leaf_inverter.id,
            relationship_type: "POWERS_DEVICE".into(),
            confidence: 0.98,
            provenance: "llm_inferred".into(),
        },
        EdgeUpsertRecord {
            parent_id: subcat_battery.id,
            child_id: leaf_inverter.id,
            relationship_type: "BACKS_UP_DEVICE".into(),
            confidence: 0.95,
            provenance: "llm_inferred".into(),
        },
    ];
    batch_upsert_edges(&pg_client, &pg_edge_records)
        .await
        .expect("batch upsert edges in pg");

    // Verify Recursive CTE Traversal on PostgreSQL: multi-hop ancestor discovery
    let pg_ancestors = RecursiveTraversalBuilder::fetch_ancestors(&pg_client, leaf_inverter.id, 10)
        .await
        .expect("fetch ancestors via recursive CTE in pg");

    // In a diamond DAG (Root -> Solar, Root -> Battery; Solar -> Leaf, Battery -> Leaf),
    // the recursive CTE traversal from leaf encounters depth 1 (Solar, Battery) and depth 2 (Root).
    assert!(
        pg_ancestors.len() >= 3,
        "Leaf inverter must discover all upstream ancestors in diamond DAG"
    );
    let ancestor_keys: Vec<String> = pg_ancestors.into_iter().map(|a| a.business_key).collect();
    assert!(ancestor_keys.contains(&subcat_solar.asset_code));
    assert!(ancestor_keys.contains(&subcat_battery.asset_code));
    assert!(ancestor_keys.contains(&root_asset.asset_code));

    // Verify Recursive CTE Traversal on PostgreSQL: multi-hop descendant discovery from Root
    let pg_descendants =
        RecursiveTraversalBuilder::fetch_descendants(&pg_client, root_asset.id, 10)
            .await
            .expect("fetch descendants via recursive CTE in pg");
    assert!(
        pg_descendants.len() >= 3,
        "Root facility must discover Solar, Battery, and Inverter as descendants"
    );
    let descendant_keys: Vec<String> = pg_descendants.into_iter().map(|d| d.business_key).collect();
    assert!(descendant_keys.contains(&subcat_solar.asset_code));
    assert!(descendant_keys.contains(&subcat_battery.asset_code));
    assert!(descendant_keys.contains(&leaf_inverter.asset_code));

    // Verify PostgreSQL clean teardown
    run_postgres_migration(&pg_client, POSTGRES_E2E_MIGRATION_DOWN)
        .await
        .expect("postgres migrations down");
}
