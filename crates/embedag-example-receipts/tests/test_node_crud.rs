//! The CRUD methods `#[derive(HierarchicalModel)]` generates, on every node type of the crate:
//! SQLite always, PostgreSQL (blocking and async) against a container.

use diesel::Connection;
use diesel::SqliteConnection;
use diesel::connection::SimpleConnection;
use diesel_async::{AsyncConnection, AsyncPgConnection, SimpleAsyncConnection};
use embedag_example_receipts::ReceiptCategoryNode;
use embedag_example_receipts::finance::{
    InvoiceCategoryNode, ItemCategoryNode, MerchantCategoryNode, PurchaseCategoryNode,
};

const COLUMNS: &str = "(category_id TEXT PRIMARY KEY, parent_category_id TEXT, \
                       category_name TEXT NOT NULL, entity_level TEXT NOT NULL);";

/// Runs every generated SQLite method of `$ty` against its table `$table`.
macro_rules! sqlite_cycle {
    ($ty:ident, $table:literal) => {{
        let mut conn = SqliteConnection::establish(":memory:").expect("sqlite");
        conn.batch_execute(concat!("CREATE TABLE ", $table, " ", "(category_id TEXT PRIMARY KEY, parent_category_id TEXT, category_name TEXT NOT NULL, entity_level TEXT NOT NULL);"))
            .expect("schema");
        let root = $ty {
            category_id: "a".into(),
            parent_category_id: None,
            category_name: "A".into(),
            entity_level: "root".into(),
        };
        let mut child = $ty {
            category_id: "b".into(),
            parent_category_id: Some("a".into()),
            category_name: "B".into(),
            entity_level: "leaf".into(),
        };
        assert_eq!(root.insert_sqlite(&mut conn).unwrap(), 1);
        // A batch containing an existing key fails as a whole.
        assert!($ty::insert_batch_sqlite(&[root.clone()], &mut conn).is_err());
        assert_eq!($ty::insert_batch_sqlite(&[child.clone()], &mut conn).unwrap(), 1);
        child.category_name = "B2".into();
        assert_eq!(child.update_sqlite(&mut conn).unwrap(), 1);
        assert_eq!($ty::delete_by_id_sqlite(&mut conn, "b").unwrap(), 1);
        assert!(!$ty::table_context().is_empty());
    }};
}

#[test]
fn every_node_type_has_working_sqlite_crud() {
    sqlite_cycle!(ReceiptCategoryNode, "receipt_categories");
    sqlite_cycle!(MerchantCategoryNode, "merchant_categories");
    sqlite_cycle!(ItemCategoryNode, "item_categories");
    sqlite_cycle!(PurchaseCategoryNode, "purchase_categories");
    sqlite_cycle!(InvoiceCategoryNode, "invoice_categories");
}

/// Runs every generated PostgreSQL method of `$ty`, blocking and async, against `$table`.
macro_rules! postgres_cycle {
    ($ty:ident, $table:literal, $sync:expr, $async_conn:expr) => {{
        let root = $ty {
            category_id: "a".into(),
            parent_category_id: None,
            category_name: "A".into(),
            entity_level: "root".into(),
        };
        let mut child = $ty {
            category_id: "b".into(),
            parent_category_id: Some("a".into()),
            category_name: "B".into(),
            entity_level: "leaf".into(),
        };
        // Blocking connection.
        assert_eq!(root.insert_pg($sync).unwrap(), 1);
        assert!($ty::insert_batch_pg(&[root.clone()], $sync).is_err());
        assert_eq!($ty::insert_batch_pg(&[child.clone()], $sync).unwrap(), 1);
        child.category_name = "B2".into();
        assert_eq!(child.update_pg($sync).unwrap(), 1);
        assert_eq!($ty::delete_by_id_pg($sync, "b").unwrap(), 1);
        assert_eq!($ty::delete_by_id_pg($sync, "a").unwrap(), 1);
        // Async connection.
        assert_eq!(root.insert_pg_async($async_conn).await.unwrap(), 1);
        assert!(
            $ty::insert_batch_pg_async(&[root.clone()], $async_conn)
                .await
                .is_err()
        );
        assert_eq!(
            $ty::insert_batch_pg_async(&[child.clone()], $async_conn)
                .await
                .unwrap(),
            1
        );
        assert_eq!(child.update_pg_async($async_conn).await.unwrap(), 1);
        assert_eq!(
            $ty::delete_by_id_pg_async($async_conn, "b").await.unwrap(),
            1
        );
    }};
}

#[tokio::test]
#[ignore = "starts a Postgres container: runs in the persistence-testcontainers and coverage jobs"]
async fn every_node_type_has_working_postgres_crud() {
    use embedag_diesel::harness::PostgresHarness;

    let harness = PostgresHarness::start().await.expect("postgres");
    let url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        harness.host, harness.port
    );
    let mut sync = diesel::PgConnection::establish(&url).expect("blocking connection");
    let mut asynchronous = AsyncPgConnection::establish(&url)
        .await
        .expect("async connection");
    for table in [
        "receipt_categories",
        "merchant_categories",
        "item_categories",
        "purchase_categories",
        "invoice_categories",
    ] {
        asynchronous
            .batch_execute(&format!("CREATE TABLE {table} {COLUMNS}"))
            .await
            .expect("schema");
    }
    let (s, a) = (&mut sync, &mut asynchronous);
    postgres_cycle!(ReceiptCategoryNode, "receipt_categories", s, a);
    postgres_cycle!(MerchantCategoryNode, "merchant_categories", s, a);
    postgres_cycle!(ItemCategoryNode, "item_categories", s, a);
    postgres_cycle!(PurchaseCategoryNode, "purchase_categories", s, a);
    postgres_cycle!(InvoiceCategoryNode, "invoice_categories", s, a);
}
