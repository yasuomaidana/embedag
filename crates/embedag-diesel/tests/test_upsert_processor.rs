use embedag_diesel::ops::{ConflictStrategy, UpsertDescriptor, UpsertProcessor};

#[test]
fn test_upsert_descriptor_sqlite_generation() {
    let descriptor = UpsertDescriptor::new(
        "custom_catalog",
        vec!["id", "sku", "title", "price"],
        vec!["sku"],
        ConflictStrategy::DoUpdate {
            columns: vec!["title".into(), "price".into()],
        },
    );

    let sql = descriptor.build_sqlite_sql();
    assert_eq!(
        sql,
        "INSERT INTO custom_catalog (id, sku, title, price) VALUES (?, ?, ?, ?) ON CONFLICT (sku) DO UPDATE SET title = excluded.title, price = excluded.price;"
    );
}

#[test]
fn test_upsert_descriptor_postgres_generation() {
    let descriptor = UpsertDescriptor::new(
        "custom_catalog",
        vec!["id", "sku", "title", "price"],
        vec!["sku"],
        ConflictStrategy::DoUpdate {
            columns: vec!["title".into(), "price".into()],
        },
    );

    let sql = descriptor.build_postgres_sql();
    assert_eq!(
        sql,
        "INSERT INTO custom_catalog (id, sku, title, price) VALUES ($1, $2, $3, $4) ON CONFLICT (sku) DO UPDATE SET title = EXCLUDED.title, price = EXCLUDED.price;"
    );
}

#[test]
fn test_upsert_descriptor_do_nothing_strategy() {
    let descriptor = UpsertDescriptor::new(
        "tag_links",
        vec!["item_id", "tag_id"],
        vec!["item_id", "tag_id"],
        ConflictStrategy::DoNothing,
    );

    let sql = descriptor.build_sqlite_sql();
    assert_eq!(
        sql,
        "INSERT INTO tag_links (item_id, tag_id) VALUES (?, ?) ON CONFLICT (item_id, tag_id) DO NOTHING;"
    );
}

#[test]
fn test_upsert_processor_custom_function_hook() {
    struct MockConnection {
        pub logged_calls: Vec<String>,
    }

    struct CustomItem {
        pub code: String,
        pub value: i32,
    }

    let mut conn = MockConnection {
        logged_calls: Vec::new(),
    };
    let item = CustomItem {
        code: "ITEM_1".into(),
        value: 100,
    };

    let result = UpsertProcessor::execute_with_custom(&mut conn, &item, |c, it| {
        c.logged_calls.push(format!("UPSERT {}:{}", it.code, it.value));
        1u64
    });

    assert_eq!(result, 1);
    assert_eq!(conn.logged_calls.len(), 1);
    assert_eq!(conn.logged_calls[0], "UPSERT ITEM_1:100");
}
