//! Helpers shared by the integration tests; each test binary uses only some of them.
#![allow(dead_code)]

/// Shared test fixture schema for graph operations tests.
pub async fn setup_graph_test_schema(client: &tokio_postgres::Client) {
    client
        .batch_execute(
            r#"
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
            valid_from TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            valid_to TIMESTAMPTZ DEFAULT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (parent_id, child_id, relationship_type)
        );
    "#,
        )
        .await
        .expect("setup graph test schema");
}

use embedag_diesel::cte::{HierarchicalModel, HierarchyItem};
use embedag_diesel::ops::{MutationDirectiveRecord, NodeUpsertRecord};
use uuid::Uuid;

/// A model over the table `dept(id, parent_id, name)`.
pub struct Dept;

impl HierarchicalModel for Dept {
    fn table_name() -> &'static str {
        "dept"
    }
    fn id_column() -> &'static str {
        "id"
    }
    fn parent_column() -> &'static str {
        "parent_id"
    }
    fn name_column() -> &'static str {
        "name"
    }
}

/// The names of the items, in order.
pub fn names(items: &[HierarchyItem]) -> Vec<&str> {
    items.iter().map(|i| i.name.as_str()).collect()
}

/// Two inserted nodes and the directive that attaches the second under the first.
pub fn attach_directives(parent: Uuid, child: Uuid) -> [MutationDirectiveRecord; 3] {
    [
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord::new(parent, "p", "dept", "P")),
        MutationDirectiveRecord::InsertNode(NodeUpsertRecord::new(child, "c", "sku", "C")),
        MutationDirectiveRecord::AttachParent {
            parent_id: parent,
            child_id: child,
            relationship_type: "PARENT_OF".into(),
            confidence: 1.0,
            provenance: "test".into(),
        },
    ]
}
