//! The engine's lower-level API, its configuration and the shapes of data it must tolerate.

use embedag_core::model::{EdgePayload, ProvenanceKind};
use embedag_core::testing::TestNode;
use embedag_core::traits::{ReadOnlyGraphRepository, VectorSearchRepository};
use embedag_oxirs::{
    OxiRsTdbConfig, OxiRsTdbEngine, StorageMode, SyncPolicy, make_graph_name, make_node_iri,
};
use oxirs_core::model::{GraphName, Literal, NamedNode, Object, Predicate, Quad, Subject};
use rstest::rstest;
use tempfile::tempdir;

fn node(id: &str) -> TestNode {
    TestNode::new(id, &format!("{id} name"), "Category")
}

fn edge(relationship: &str, provenance: ProvenanceKind) -> EdgePayload {
    EdgePayload::new(relationship, 0.9, provenance).expect("valid edge")
}

fn engine_with_edge(relationship: &str) -> OxiRsTdbEngine {
    let engine = OxiRsTdbEngine::in_memory().unwrap();
    for id in ["a", "b"] {
        engine.insert_node(&node(id)).unwrap();
    }
    let edge = edge(relationship, ProvenanceKind::DeterministicRule);
    engine
        .insert_edge(&"a".to_string(), &"b".to_string(), &edge)
        .unwrap();
    engine
}

#[test]
fn exposes_its_parts_and_counts_quads() {
    let engine = OxiRsTdbEngine::in_memory().unwrap();
    assert!(engine.is_empty().unwrap());
    engine.insert_node(&node("a")).unwrap();
    assert!(!engine.is_empty().unwrap());
    assert_eq!(engine.len().unwrap(), engine.inner().len().unwrap());
    assert_eq!(engine.config().base_iri, "urn:embedag:node:");
    assert!(engine.vocab().schema_ns.ends_with('#'));
    engine.flush().unwrap();
}

#[test]
fn configuration_builders_and_accessors() {
    let dir = tempdir().unwrap();
    let config = OxiRsTdbConfig::persistent(dir.path())
        .with_schema_ns("https://example.org/schema")
        .with_named_graph("https://example.org/graph")
        .with_sync_policy(SyncPolicy::EveryN(2));
    assert!(config.is_persistent());
    assert_eq!(config.dataset_path(), Some(dir.path()));
    assert_eq!(config.schema_ns, "https://example.org/schema");
    assert_eq!(config.sync_policy, SyncPolicy::EveryN(2));
    assert!(matches!(config.mode, StorageMode::Persistent(_)));

    let in_memory = OxiRsTdbConfig::in_memory();
    assert!(!in_memory.is_persistent());
    assert_eq!(in_memory.dataset_path(), None);
}

#[test]
fn a_named_graph_and_a_schema_namespace_without_separator_work_end_to_end() {
    let config = OxiRsTdbConfig::in_memory()
        .with_schema_ns("https://example.org/schema")
        .with_named_graph("https://example.org/graph");
    assert!(matches!(
        make_graph_name(&config).unwrap(),
        GraphName::NamedNode(_)
    ));
    let engine = OxiRsTdbEngine::open(config).unwrap();
    engine.insert_node(&node("a")).unwrap();
    assert!(
        engine
            .get_node::<TestNode>(&"a".to_string())
            .unwrap()
            .is_some()
    );
}

#[rstest]
#[case::slash("https://example.org/n/", "https://example.org/n/k")]
#[case::hash("https://example.org/n#", "https://example.org/n#k")]
#[case::colon("urn:n:", "urn:n:k")]
#[case::bare("https://example.org/n", "https://example.org/n/k")]
fn node_iris_join_the_base_and_the_key(#[case] base: &str, #[case] expected: &str) {
    assert_eq!(make_node_iri(base, "k").unwrap().as_str(), expected);
}

#[test]
fn awkward_keys_are_sanitized_and_invalid_bases_are_errors() {
    let iri = make_node_iri("urn:n:", "a b/c").unwrap();
    assert_eq!(iri.as_str(), "urn:n:a_b_c");
    assert!(make_node_iri("not an iri", "k").is_err());
    let config = OxiRsTdbConfig::in_memory().with_named_graph("not an iri");
    assert!(make_graph_name(&config).is_err());
}

#[rstest]
#[case(ProvenanceKind::DeterministicRule)]
#[case(ProvenanceKind::DenseSimilarityMatch)]
#[case(ProvenanceKind::LlmConstrainedInference)]
#[case(ProvenanceKind::HumanVerified)]
fn edges_of_every_provenance_can_be_stored(#[case] provenance: ProvenanceKind) {
    let engine = OxiRsTdbEngine::in_memory().unwrap();
    for id in ["a", "b"] {
        engine.insert_node(&node(id)).unwrap();
    }
    engine
        .insert_edge(
            &"a".to_string(),
            &"b".to_string(),
            &edge("LINKS", provenance),
        )
        .unwrap();
    let children: Vec<TestNode> = engine.get_children(&"a".to_string()).unwrap();
    assert_eq!(children.len(), 1);
}

#[test]
fn missing_nodes_are_none_and_removing_them_reports_nothing_removed() {
    let engine = OxiRsTdbEngine::in_memory().unwrap();
    assert!(
        engine
            .get_node::<TestNode>(&"zzz".to_string())
            .unwrap()
            .is_none()
    );
    assert!(!engine.remove_node_by_key("zzz").unwrap());
    engine.insert_node(&node("a")).unwrap();
    assert!(engine.remove_node_by_key("a").unwrap());
}

#[test]
fn parents_are_found_and_edge_entities_are_not_mistaken_for_parents() {
    let engine = engine_with_edge("CONTAINS");
    let parents: Vec<TestNode> = engine.get_parents(&"b".to_string()).unwrap();
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].id, "a");
}

#[rstest]
#[case::any_relationship(None, true)]
#[case::matching_relationship(Some("CONTAINS"), true)]
#[case::other_relationship(Some("OTHER"), false)]
fn removing_edges(#[case] relationship: Option<&str>, #[case] gone: bool) {
    let engine = engine_with_edge("CONTAINS");
    let (a, b) = ("a".to_string(), "b".to_string());
    engine.remove_edge(&a, &b, relationship).unwrap();
    let children: Vec<TestNode> = engine.get_children(&a).unwrap();
    // The direct triple is removed by the filter; the reified entity only on a relationship match.
    assert_eq!(children.is_empty(), gone);
}

#[test]
fn raw_quads_can_be_inserted_found_and_removed() {
    let engine = OxiRsTdbEngine::in_memory().unwrap();
    let graph = make_graph_name(engine.config()).unwrap();
    let subject = Subject::NamedNode(NamedNode::new("urn:s").unwrap());
    let predicate = Predicate::NamedNode(NamedNode::new("urn:p").unwrap());
    let quad = |value: &str| {
        Quad::new(
            subject.clone(),
            predicate.clone(),
            Object::Literal(Literal::new(value)),
            graph.clone(),
        )
    };

    assert!(engine.insert_quad(quad("one")).unwrap());
    assert_eq!(
        engine
            .bulk_insert_quads(vec![quad("two"), quad("three")])
            .unwrap(),
        2
    );
    assert_eq!(
        engine.find_quads(Some(&subject), None, None).unwrap().len(),
        3
    );
    assert!(engine.remove_quad(&quad("one")).unwrap());
    assert_eq!(engine.len().unwrap(), 2);
}

#[test]
fn sparql_and_snapshots() {
    let dir = tempdir().unwrap();
    let engine = OxiRsTdbEngine::open(OxiRsTdbConfig::persistent(dir.path())).unwrap();
    engine.insert_node(&node("a")).unwrap();
    engine.flush().unwrap();
    assert!(engine.query_sparql("SELECT ?s WHERE { ?s ?p ?o }").is_ok());

    let snapshot = dir.path().join("snapshot.oxsnap");
    engine.save_snapshot(&snapshot).unwrap();
    assert!(snapshot.exists());
    assert!(OxiRsTdbEngine::build_snapshot(dir.path()).is_ok());
    assert!(OxiRsTdbEngine::open_persistent(dir.path()).is_ok());
}

#[tokio::test]
async fn the_repository_traits_delegate_to_the_engine() {
    let engine = engine_with_edge("CONTAINS");
    engine.index_embedding("a", vec![1.0, 0.0]);
    let key = "b".to_string();

    let parents = ReadOnlyGraphRepository::<TestNode>::get_parents(&engine, &key).await;
    assert_eq!(parents.unwrap().len(), 1);
    let node = ReadOnlyGraphRepository::<TestNode>::get_node(&engine, &key).await;
    assert!(node.unwrap().is_some());
    let near =
        VectorSearchRepository::<TestNode>::find_nearest_neighbors(&engine, &[1.0, 0.0], 1).await;
    assert_eq!(near.unwrap().len(), 1);
}

/// An edge entity the engine did not write: the loader must skip it, not fail.
#[rstest]
#[case::no_endpoints(false, false)]
#[case::no_relationship(true, false)]
#[case::complete_but_unknown_nodes(true, true)]
fn incomplete_edge_entities_are_skipped_when_loading(
    #[case] with_endpoints: bool,
    #[case] with_relationship: bool,
) {
    let engine = OxiRsTdbEngine::in_memory().unwrap();
    for id in ["a", "b"] {
        engine.insert_node(&node(id)).unwrap();
    }
    let graph = make_graph_name(engine.config()).unwrap();
    let vocab = engine.vocab();
    let entity = Subject::NamedNode(NamedNode::new("urn:embedag:edge/dangling").unwrap());
    let add = |predicate: &NamedNode, object: Object| {
        let quad = Quad::new(
            entity.clone(),
            Predicate::NamedNode(predicate.clone()),
            object,
            graph.clone(),
        );
        engine.insert_quad(quad).unwrap();
    };
    add(&vocab.rdf_type, Object::NamedNode(vocab.edge_type.clone()));
    if with_endpoints {
        // `with_relationship` also means the edge is complete, but names nodes that do not exist.
        let (parent, child) = if with_relationship {
            ("ghost-a", "ghost-b")
        } else {
            ("a", "b")
        };
        let a = make_node_iri(&engine.config().base_iri, parent).unwrap();
        let b = make_node_iri(&engine.config().base_iri, child).unwrap();
        add(&vocab.parent_rel, Object::NamedNode(a));
        add(&vocab.child_rel, Object::NamedNode(b));
    }
    if with_relationship {
        add(&vocab.relationship, Object::Literal(Literal::new("X")));
        add(&vocab.confidence, Object::Literal(Literal::new("0.5")));
    }

    let dag = engine.load_dag::<TestNode>().unwrap();
    assert_eq!(dag.edge_count(), 0);
}

#[test]
fn a_stored_edge_is_loaded_back_into_the_dag() {
    let engine = engine_with_edge("CONTAINS");
    let dag = engine.load_dag::<TestNode>().unwrap();
    assert_eq!(dag.node_count(), 2);
    assert_eq!(dag.edge_count(), 1);
}

#[test]
fn edge_entities_are_skipped_as_parents_under_a_slash_terminated_base() {
    let engine = OxiRsTdbEngine::open(
        OxiRsTdbConfig::in_memory().with_base_iri("https://example.org/node/"),
    )
    .unwrap();
    for id in ["a", "b"] {
        engine.insert_node(&node(id)).unwrap();
    }
    let link = edge("CONTAINS", ProvenanceKind::DeterministicRule);
    engine
        .insert_edge(&"a".to_string(), &"b".to_string(), &link)
        .unwrap();
    let parents: Vec<TestNode> = engine.get_parents(&"b".to_string()).unwrap();
    assert_eq!(parents.len(), 1);
}

#[test]
fn payload_quads_with_a_non_literal_object_are_ignored_when_loading() {
    let engine = OxiRsTdbEngine::in_memory().unwrap();
    let graph = make_graph_name(engine.config()).unwrap();
    let quad = Quad::new(
        Subject::NamedNode(NamedNode::new("urn:odd").unwrap()),
        Predicate::NamedNode(engine.vocab().payload.clone()),
        Object::NamedNode(NamedNode::new("urn:not-a-literal").unwrap()),
        graph,
    );
    engine.insert_quad(quad).unwrap();
    assert_eq!(engine.load_dag::<TestNode>().unwrap().node_count(), 0);
}

#[test]
fn every_lookup_fails_cleanly_when_the_named_graph_is_invalid() {
    let engine =
        OxiRsTdbEngine::open(OxiRsTdbConfig::in_memory().with_named_graph("not an iri")).unwrap();
    let key = "a".to_string();
    engine.index_embedding("a", vec![1.0]);
    assert!(engine.get_node::<TestNode>(&key).is_err());
    assert!(engine.get_children::<TestNode>(&key).is_err());
    assert!(engine.get_parents::<TestNode>(&key).is_err());
    assert!(engine.remove_node_by_key("a").is_err());
    assert!(engine.remove_edge(&key, &key, None).is_err());
    assert!(engine.load_dag::<TestNode>().is_err());
    assert!(
        engine
            .find_nearest_neighbors::<TestNode>(&[1.0], 1)
            .is_err()
    );
    assert!(engine.insert_node(&node("a")).is_err());
}

#[test]
fn blank_node_subjects_are_not_parents() {
    use oxirs_core::model::BlankNode;
    let engine = OxiRsTdbEngine::in_memory().unwrap();
    engine.insert_node(&node("b")).unwrap();
    let graph = make_graph_name(engine.config()).unwrap();
    let child = make_node_iri(&engine.config().base_iri, "b").unwrap();
    let quad = Quad::new(
        Subject::BlankNode(BlankNode::new("anon").unwrap()),
        Predicate::NamedNode(NamedNode::new("urn:relates").unwrap()),
        Object::NamedNode(child),
        graph,
    );
    engine.insert_quad(quad).unwrap();
    let parents: Vec<TestNode> = engine.get_parents(&"b".to_string()).unwrap();
    assert!(parents.is_empty());
}
