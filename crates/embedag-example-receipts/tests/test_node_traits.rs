//! The traits derived on every node type: equality, cloning, printing, serde and JSON Schema.

use embedag_example_receipts::ReceiptCategoryNode;
use embedag_example_receipts::finance::{
    InvoiceCategoryNode, ItemCategoryNode, MerchantCategoryNode, PurchaseCategoryNode,
};

macro_rules! derived_traits {
    ($($name:ident: $ty:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let node = $ty {
                category_id: "a".into(),
                parent_category_id: Some("root".into()),
                category_name: "A".into(),
                entity_level: "leaf".into(),
            };
            let copy = node.clone();
            assert_eq!(node, copy);
            assert!(format!("{node:?}").contains("category_id"));

            let json = serde_json::to_string(&node).unwrap();
            assert_eq!(serde_json::from_str::<$ty>(&json).unwrap(), node);
            let schema = schemars::schema_for!($ty);
            assert!(serde_json::to_string(&schema).unwrap().contains("category_name"));
        }
    )*};
}

derived_traits! {
    receipt_nodes_derive_their_traits: ReceiptCategoryNode,
    merchant_nodes_derive_their_traits: MerchantCategoryNode,
    item_nodes_derive_their_traits: ItemCategoryNode,
    purchase_nodes_derive_their_traits: PurchaseCategoryNode,
    invoice_nodes_derive_their_traits: InvoiceCategoryNode,
}
