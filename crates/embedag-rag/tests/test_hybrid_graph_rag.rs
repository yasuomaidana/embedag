use embedag_core::dag::MemoryDag;
use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::traits::NodePayload;
use embedag_rag::evidence::GroundedEvidencePackager;
use embedag_rag::expansion::SubgraphExpander;
use embedag_rag::index::{DenseVectorIndex, SparseLexicalIndex};
use embedag_rag::rrf::RrfRanker;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CatalogItem {
    pub id: Uuid,
    pub business_key: String,
    pub name: String,
    pub level: String,
}

impl CatalogItem {
    pub fn new(
        id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        level: impl Into<String>,
    ) -> Self {
        Self {
            id,
            business_key: key.into(),
            name: name.into(),
            level: level.into(),
        }
    }
}

impl NodePayload for CatalogItem {
    type Key = String;
    fn key(&self) -> &Self::Key {
        &self.business_key
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn entity_level(&self) -> &str {
        &self.level
    }
}

fn create_edge(rel: &str, conf: f32, prov: ProvenanceKind) -> EdgePayload {
    EdgePayload::new(rel, conf, prov).expect("valid edge payload")
}

#[tokio::test]
async fn test_two_stage_hybrid_graph_rag_pipeline() {
    let mut dag = MemoryDag::new();
    let mut dense_index = DenseVectorIndex::with_dimensions(4);
    let mut sparse_index = SparseLexicalIndex::new();

    // 1. Setup Taxonomy Hierarchy
    // Category (Food) -> Brand (Carlos V) -> Product (Carlos V Dark 50g) -> Bundle (Pack of 10)
    let cat_id = Uuid::new_v4();
    let brand_id = Uuid::new_v4();
    let prod_id = Uuid::new_v4();
    let bundle_id = Uuid::new_v4();

    let cat = CatalogItem::new(cat_id, "CAT-CONFECTIONERY", "Confectionery", "Category");
    let brand = CatalogItem::new(brand_id, "BRAND-CARLOS-V", "Carlos V", "Brand");
    let prod = CatalogItem::new(
        prod_id,
        "SKU-CV-DARK-50",
        "Carlos V Dark Chocolate 50g",
        "Product",
    );
    let bundle = CatalogItem::new(
        bundle_id,
        "SKU-CV-DARK-10PK",
        "Carlos V Dark 50g 10-Pack",
        "Bundle",
    );

    dag.upsert_node(cat.clone());
    dag.upsert_node(brand.clone());
    dag.upsert_node(prod.clone());
    dag.upsert_node(bundle.clone());

    dag.add_edge_by_keys(
        &cat.business_key,
        &brand.business_key,
        create_edge("PARENT_OF", 1.0, ProvenanceKind::DeterministicRule),
    )
    .expect("add edge");

    dag.add_edge_by_keys(
        &brand.business_key,
        &prod.business_key,
        create_edge("MANUFACTURES", 1.0, ProvenanceKind::DeterministicRule),
    )
    .expect("add edge");

    dag.add_edge_by_keys(
        &prod.business_key,
        &bundle.business_key,
        create_edge("BUNDLED_IN", 0.95, ProvenanceKind::DenseSimilarityMatch),
    )
    .expect("add edge");

    // 2. Index entities in Dense and Sparse indices
    dense_index
        .insert(cat_id, vec![0.1, 0.1, 0.0, 0.0])
        .expect("insert dense");
    dense_index
        .insert(brand_id, vec![0.8, 0.5, 0.1, 0.0])
        .expect("insert dense");
    dense_index
        .insert(prod_id, vec![0.9, 0.8, 0.2, 0.1])
        .expect("insert dense");
    dense_index
        .insert(bundle_id, vec![0.85, 0.75, 0.2, 0.1])
        .expect("insert dense");

    sparse_index.insert(cat_id, &cat.business_key, &cat.name);
    sparse_index.insert(brand_id, &brand.business_key, &brand.name);
    sparse_index.insert(prod_id, &prod.business_key, &prod.name);
    sparse_index.insert(bundle_id, &bundle.business_key, &bundle.name);

    // 3. Stage 1: Parallel Dispatch & Retrieval
    // Dense query embedding for "carlos chocolate bar"
    let query_vector = vec![0.92, 0.78, 0.18, 0.08];
    let dense_results = dense_index.search(&query_vector, 3).expect("dense search");
    let dense_ids: Vec<Uuid> = dense_results.into_iter().map(|(id, _)| id).collect();

    // Sparse search for query with typo: "chcolate carlos dark"
    let sparse_results = sparse_index.search("chcolate carlos dark", 3);
    let sparse_ids: Vec<Uuid> = sparse_results.into_iter().map(|(id, _)| id).collect();

    // 4. Reciprocal Rank Fusion (RRF)
    let ranker = RrfRanker::default();
    let fused = ranker.fuse(&dense_ids, &sparse_ids);
    assert!(!fused.is_empty(), "RRF fusion must produce candidates");

    // Carlos V Dark product must rank at top of fused list
    assert_eq!(
        fused[0].id, prod_id,
        "Top fused candidate should be the dark chocolate product"
    );

    // Map Uuid back to business key
    let id_to_key = |id: Uuid| -> Option<String> {
        if id == cat_id {
            Some(cat.business_key.clone())
        } else if id == brand_id {
            Some(brand.business_key.clone())
        } else if id == prod_id {
            Some(prod.business_key.clone())
        } else if id == bundle_id {
            Some(bundle.business_key.clone())
        } else {
            None
        }
    };

    let seed_candidates: Vec<(String, f32)> = fused
        .iter()
        .take(1)
        .filter_map(|c| id_to_key(c.id).map(|k| (k, c.score)))
        .collect();

    // 5. Stage 2: Topological Subgraph Expansion with Distance Decay
    let expander = SubgraphExpander::new(0.70, 2, true);
    let subgraph = expander.expand(&dag, &seed_candidates);

    // Assert that topological context contains seed product, 1-hop brand, 2-hop category, and 1-hop bundle
    assert!(subgraph.nodes.contains_key("SKU-CV-DARK-50"));
    assert!(subgraph.nodes.contains_key("BRAND-CARLOS-V"));
    assert!(subgraph.nodes.contains_key("CAT-CONFECTIONERY"));
    assert!(subgraph.nodes.contains_key("SKU-CV-DARK-10PK"));

    // Check decay weights
    let seed_w = subgraph.nodes["SKU-CV-DARK-50"].weight;
    let brand_w = subgraph.nodes["BRAND-CARLOS-V"].weight;
    let cat_w = subgraph.nodes["CAT-CONFECTIONERY"].weight;
    assert!(brand_w < seed_w, "1-hop weight must be lower than seed");
    assert!(cat_w < brand_w, "2-hop weight must be lower than 1-hop");

    // 6. Grounded Evidence Context Packaging
    let packager = GroundedEvidencePackager::new();
    let prompt_block = packager.package(&dag, &subgraph);

    assert!(prompt_block.contains("### Grounded Evidence Context"));
    assert!(prompt_block.contains("[Node: SKU-CV-DARK-50]"));
    assert!(prompt_block.contains("[Node: BRAND-CARLOS-V]"));
    assert!(prompt_block.contains("[Node: CAT-CONFECTIONERY]"));
    assert!(prompt_block.contains("Citation Requirement"));

    // 7. Verify Citation Extraction on Simulated Generation
    let answer = "According to [Node: SKU-CV-DARK-50], the product is manufactured by [Node: BRAND-CARLOS-V] within category [Node: CAT-CONFECTIONERY].";
    let citations = packager.extract_citations(answer);
    assert_eq!(citations.len(), 3);
    assert!(citations.contains("SKU-CV-DARK-50"));
    assert!(citations.contains("BRAND-CARLOS-V"));
    assert!(citations.contains("CAT-CONFECTIONERY"));
}
