use embedag::persistence::harness::PostgresHarness;
use embedag::persistence::ops::{
    EdgeUpsertRecord, NodeUpsertRecord, batch_upsert_edges, batch_upsert_nodes,
};
use embedag::persistence::traversal::RecursiveTraversalBuilder;
use embedag::prelude::*;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, AsGraphContext)]
struct GroceryItem {
    pub id: Uuid,
    #[graph(key)]
    pub sku: String,
    #[graph(text)]
    pub title: String,
    pub level: String,
}

impl NodePayload for GroceryItem {
    type Key = String;
    fn key(&self) -> &Self::Key {
        &self.sku
    }
    fn name(&self) -> &str {
        &self.title
    }
    fn entity_level(&self) -> &str {
        &self.level
    }
}

#[tokio::test]
async fn test_end_to_end_pipeline() {
    // 1. Initialize Testcontainers ephemeral PostgreSQL + pgvector container
    let harness = PostgresHarness::start()
        .await
        .expect("start testcontainers postgres");
    let client = harness.connect().await.expect("connect to container");
    client.batch_execute(r#"
        CREATE TABLE IF NOT EXISTS semantic_nodes (
            id UUID PRIMARY KEY,
            business_key VARCHAR(255) NOT NULL UNIQUE,
            entity_level VARCHAR(64) NOT NULL,
            name VARCHAR(255) NOT NULL,
            attributes JSONB NOT NULL DEFAULT '{}',
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE IF NOT EXISTS semantic_edges (
            parent_id UUID NOT NULL REFERENCES semantic_nodes(id) ON DELETE CASCADE,
            child_id UUID NOT NULL REFERENCES semantic_nodes(id) ON DELETE CASCADE,
            relationship_type VARCHAR(64) NOT NULL,
            confidence REAL NOT NULL,
            provenance VARCHAR(64) NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (parent_id, child_id, relationship_type)
        );
    "#).await.expect("create test tables");

    // 2. Build In-Memory DAG arena with cycle guardrails
    let mut dag = MemoryDag::new();

    let root_cat_id = Uuid::new_v4();
    let subcat_id = Uuid::new_v4();
    let product_id = Uuid::new_v4();

    let root_cat = GroceryItem {
        id: root_cat_id,
        sku: "CAT-BEVERAGES".into(),
        title: "Beverages & Drinks".into(),
        level: "Category".into(),
    };
    let subcat = GroceryItem {
        id: subcat_id,
        sku: "SUBCAT-COFFEE".into(),
        title: "Specialty Coffee".into(),
        level: "Subcategory".into(),
    };
    let product = GroceryItem {
        id: product_id,
        sku: "SKU-ESPRESSO-250G".into(),
        title: "Artisanal Espresso Beans 250g".into(),
        level: "Product".into(),
    };

    // Verify AsGraphContext implementation works
    assert_eq!(product.context_key(), "SKU-ESPRESSO-250G");
    assert_eq!(product.text_repr(), "Artisanal Espresso Beans 250g");

    dag.upsert_node(root_cat.clone());
    dag.upsert_node(subcat.clone());
    dag.upsert_node(product.clone());

    let edge_1 =
        EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).expect("valid edge");
    let edge_2 =
        EdgePayload::new("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule).expect("valid edge");

    dag.add_edge_by_keys_guarded(&root_cat.sku, &subcat.sku, edge_1)
        .expect("add edge 1");
    dag.add_edge_by_keys_guarded(&subcat.sku, &product.sku, edge_2)
        .expect("add edge 2");

    // Cycle detection guardrail verification: adding product -> root_cat must be rejected
    assert!(dag.would_create_cycle(&product.sku, &root_cat.sku));
    assert!(dag.is_acyclic(), "DAG must be strictly acyclic");

    // 3. Persist DAG nodes and edges into containerized PostgreSQL
    let node_records = vec![
        NodeUpsertRecord {
            id: root_cat_id,
            business_key: root_cat.sku.clone(),
            entity_level: root_cat.level.clone(),
            name: root_cat.title.clone(),
            attributes: serde_json::json!({ "department": "Beverages" }),
            embedding: None,
        },
        NodeUpsertRecord {
            id: subcat_id,
            business_key: subcat.sku.clone(),
            entity_level: subcat.level.clone(),
            name: subcat.title.clone(),
            attributes: serde_json::json!({ "roast": "Various" }),
            embedding: None,
        },
        NodeUpsertRecord {
            id: product_id,
            business_key: product.sku.clone(),
            entity_level: product.level.clone(),
            name: product.title.clone(),
            attributes: serde_json::json!({ "weight_grams": 250, "origin": "Colombia" }),
            embedding: None,
        },
    ];
    batch_upsert_nodes(&client, &node_records)
        .await
        .expect("batch upsert nodes");

    let edge_records = vec![
        EdgeUpsertRecord {
            parent_id: root_cat_id,
            child_id: subcat_id,
            relationship_type: "PARENT_OF".into(),
            confidence: 1.0,
            provenance: "taxonomy".into(),
        },
        EdgeUpsertRecord {
            parent_id: subcat_id,
            child_id: product_id,
            relationship_type: "PARENT_OF".into(),
            confidence: 1.0,
            provenance: "taxonomy".into(),
        },
    ];
    batch_upsert_edges(&client, &edge_records)
        .await
        .expect("batch upsert edges");

    // 4. Verify Recursive CTE traversal against PostgreSQL database
    let ancestors = RecursiveTraversalBuilder::fetch_ancestors(&client, product_id, 10)
        .await
        .expect("recursive CTE ancestors");
    assert_eq!(
        ancestors.len(),
        2,
        "Product must have 2 ancestors in database"
    );
    assert_eq!(ancestors[0].business_key, "SUBCAT-COFFEE");
    assert_eq!(ancestors[1].business_key, "CAT-BEVERAGES");

    // 5. Two-Stage Hybrid Graph-RAG Retrieval:
    // Stage 1: Dense + Sparse search
    let mut dense_idx = DenseVectorIndex::with_dimensions(3);
    dense_idx
        .insert(product_id, vec![0.9, 0.8, 0.1])
        .expect("insert dense");
    dense_idx
        .insert(subcat_id, vec![0.7, 0.6, 0.1])
        .expect("insert dense");
    dense_idx
        .insert(root_cat_id, vec![0.2, 0.1, 0.1])
        .expect("insert dense");

    let mut sparse_idx = SparseLexicalIndex::new();
    sparse_idx.insert(product_id, &product.sku, &product.title);
    sparse_idx.insert(subcat_id, &subcat.sku, &subcat.title);
    sparse_idx.insert(root_cat_id, &root_cat.sku, &root_cat.title);

    let dense_candidates = dense_idx
        .search(&[0.92, 0.82, 0.09], 2)
        .expect("dense search");
    let dense_ids: Vec<Uuid> = dense_candidates.into_iter().map(|(id, _)| id).collect();

    let sparse_candidates = sparse_idx.search("espreso artisnal beans", 2);
    let sparse_ids: Vec<Uuid> = sparse_candidates.into_iter().map(|(id, _)| id).collect();

    let ranker = RrfRanker::default();
    let fused = ranker.fuse(&dense_ids, &sparse_ids);
    assert_eq!(
        fused[0].id, product_id,
        "Product must be top fused candidate"
    );

    // Stage 2: Topological Subgraph Expansion with Distance Decay
    let expander = SubgraphExpander::new(0.70, 2, false);
    let seed_keys = vec![(product.sku.clone(), fused[0].score)];
    let subgraph = expander.expand(&dag, &seed_keys);

    assert_eq!(
        subgraph.nodes.len(),
        3,
        "Subgraph must contain seed and both ancestors"
    );
    assert!(subgraph.nodes["SKU-ESPRESSO-250G"].weight > subgraph.nodes["SUBCAT-COFFEE"].weight);
    assert!(subgraph.nodes["SUBCAT-COFFEE"].weight > subgraph.nodes["CAT-BEVERAGES"].weight);

    // 6. Grounded Evidence Context Packaging with Citations
    let packager = GroundedEvidencePackager::new();
    let context_block = packager.package(&dag, &subgraph);

    assert!(context_block.contains("### Grounded Evidence Context"));
    assert!(context_block.contains("[Node: SKU-ESPRESSO-250G]"));
    assert!(context_block.contains("[Node: SUBCAT-COFFEE]"));
    assert!(context_block.contains("[Node: CAT-BEVERAGES]"));
    assert!(context_block.contains("Citation Requirement"));

    // Extract and audit simulated citations
    let simulated_answer = "The product [Node: SKU-ESPRESSO-250G] is in [Node: SUBCAT-COFFEE] under [Node: CAT-BEVERAGES].";
    let cited = packager.extract_citations(simulated_answer);
    assert_eq!(cited.len(), 3);
    assert!(cited.contains("SKU-ESPRESSO-250G"));
    assert!(cited.contains("SUBCAT-COFFEE"));
    assert!(cited.contains("CAT-BEVERAGES"));
}
