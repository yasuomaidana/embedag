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
