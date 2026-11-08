use embedag_core::testing::StoredNode;
use embedag_surrealdb::mapping::SurrealNodeRecord;

#[test]
fn a_record_can_be_built_with_or_without_an_embedding() {
    let node = StoredNode {
        sku: "SKU-1".into(),
        title: "Tea".into(),
        category: "Drink".into(),
    };
    let plain = SurrealNodeRecord::from_node(&node).unwrap();
    assert!(plain.embedding.is_none() && plain.id.is_none());
    assert_eq!(plain.business_key, "SKU-1");

    let embedded = SurrealNodeRecord::from_node_with_embedding(&node, Some(vec![1.0])).unwrap();
    assert_eq!(embedded.embedding, Some(vec![1.0]));
    assert_eq!(embedded.to_node::<StoredNode>().unwrap(), node);
}
