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
    ($name:ident, $ty:ident, $table:literal) => {
        #[test]
        fn $name() {
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
        assert!($ty::insert_batch_sqlite(std::slice::from_ref(&root), &mut conn).is_err());
        assert_eq!($ty::insert_batch_sqlite(&[child.clone()], &mut conn).unwrap(), 1);
        child.category_name = "B2".into();
        assert_eq!(child.update_sqlite(&mut conn).unwrap(), 1);
        assert_eq!($ty::delete_by_id_sqlite(&mut conn, "b").unwrap(), 1);
        assert!(!$ty::table_context().is_empty());
        }
    };
}

sqlite_cycle!(
    receipt_node_has_sqlite_crud,
    ReceiptCategoryNode,
    "receipt_categories"
);
sqlite_cycle!(
    merchant_node_has_sqlite_crud,
    MerchantCategoryNode,
    "merchant_categories"
);
sqlite_cycle!(
    item_node_has_sqlite_crud,
    ItemCategoryNode,
    "item_categories"
);
sqlite_cycle!(
    purchase_node_has_sqlite_crud,
    PurchaseCategoryNode,
    "purchase_categories"
);
sqlite_cycle!(
    invoice_node_has_sqlite_crud,
    InvoiceCategoryNode,
    "invoice_categories"
);

/// Runs every generated PostgreSQL method of `$ty`, blocking and async, against `$table`.
macro_rules! postgres_cycle {
    ($name:ident, $ty:ident) => {
        async fn $name(sync: &mut diesel::PgConnection, asynchronous: &mut AsyncPgConnection) {
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
            assert_eq!(root.insert_pg(sync).unwrap(), 1);
            assert!($ty::insert_batch_pg(std::slice::from_ref(&root), sync).is_err());
            assert_eq!($ty::insert_batch_pg(&[child.clone()], sync).unwrap(), 1);
            child.category_name = "B2".into();
            assert_eq!(child.update_pg(sync).unwrap(), 1);
            assert_eq!($ty::delete_by_id_pg(sync, "b").unwrap(), 1);
            assert_eq!($ty::delete_by_id_pg(sync, "a").unwrap(), 1);
            // Async connection.
            assert_eq!(root.insert_pg_async(asynchronous).await.unwrap(), 1);
            assert!(
                $ty::insert_batch_pg_async(std::slice::from_ref(&root), asynchronous)
                    .await
                    .is_err()
            );
            assert_eq!(
                $ty::insert_batch_pg_async(&[child.clone()], asynchronous)
                    .await
                    .unwrap(),
                1
            );
            assert_eq!(child.update_pg_async(asynchronous).await.unwrap(), 1);
            assert_eq!(
                $ty::delete_by_id_pg_async(asynchronous, "b").await.unwrap(),
                1
            );
        }
    };
}

postgres_cycle!(receipt_postgres, ReceiptCategoryNode);
postgres_cycle!(merchant_postgres, MerchantCategoryNode);
postgres_cycle!(item_postgres, ItemCategoryNode);
postgres_cycle!(purchase_postgres, PurchaseCategoryNode);
postgres_cycle!(invoice_postgres, InvoiceCategoryNode);

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
    receipt_postgres(&mut sync, &mut asynchronous).await;
    merchant_postgres(&mut sync, &mut asynchronous).await;
    item_postgres(&mut sync, &mut asynchronous).await;
    purchase_postgres(&mut sync, &mut asynchronous).await;
    invoice_postgres(&mut sync, &mut asynchronous).await;
}
