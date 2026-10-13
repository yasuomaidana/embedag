use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use embedag_diesel::cte::{HierarchicalModel, HierarchicalNode, HierarchyItem};

// Model 1: Using default field name detection (id, parent_id, name)
#[derive(Debug, Clone, HierarchicalModel, PartialEq, Eq)]
#[diesel(table_name = standard_categories)]
pub struct StandardCategory {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
}

// Model 2: Using custom attribute overrides (custom column and table names)
#[derive(Debug, Clone, HierarchicalModel, PartialEq, Eq)]
#[hierarchical(
    table = "custom_taxonomy",
    id = "tax_id",
    parent = "parent_tax_id",
    name = "tax_label"
)]
pub struct CustomTaxonomy {
    pub tax_id: String,
    pub parent_tax_id: Option<String>,
    pub tax_label: String,
}

#[test]
fn test_macro_generated_metadata_and_sql_fragments() {
    // 1. Verify StandardCategory metadata
    assert_eq!(StandardCategory::table_name(), "standard_categories");
    assert_eq!(StandardCategory::id_column(), "id");
    assert_eq!(StandardCategory::parent_column(), "parent_id");
    assert_eq!(StandardCategory::name_column(), "name");

    let step_sql = StandardCategory::ancestor_step_sql(5);
    assert_eq!(
        step_sql,
        "SELECT t.id, t.parent_id, a.depth + 1, t.name FROM standard_categories t INNER JOIN ancestors_cte a ON t.id = a.parent_ref WHERE a.depth < 5"
    );

    // 2. Verify CustomTaxonomy metadata
    assert_eq!(CustomTaxonomy::table_name(), "custom_taxonomy");
    assert_eq!(CustomTaxonomy::id_column(), "tax_id");
    assert_eq!(CustomTaxonomy::parent_column(), "parent_tax_id");
    assert_eq!(CustomTaxonomy::name_column(), "tax_label");

    let custom_step_sql = CustomTaxonomy::ancestor_step_sql(10);
    assert_eq!(
        custom_step_sql,
        "SELECT t.tax_id, t.parent_tax_id, a.depth + 1, t.tax_label FROM custom_taxonomy t INNER JOIN ancestors_cte a ON t.tax_id = a.parent_ref WHERE a.depth < 10"
    );
}

#[test]
fn test_macro_generated_hierarchical_node_trait() {
    let node = StandardCategory {
        id: "cat_1".into(),
        parent_id: Some("root".into()),
        name: "Beverages".into(),
    };

    assert_eq!(node.node_id(), "cat_1");
    assert_eq!(node.parent_id(), Some("root".into()));
    assert_eq!(node.node_name(), "Beverages");
}

#[test]
fn test_macro_generated_crud_operations() {
    let mut conn = diesel::SqliteConnection::establish(":memory:").expect("in-memory sqlite");

    conn.batch_execute(
        r#"
        CREATE TABLE standard_categories (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES standard_categories(id),
            name TEXT NOT NULL
        );
    "#,
    )
    .expect("create table");

    // 1. Insert single
    let root = StandardCategory {
        id: "root".into(),
        parent_id: None,
        name: "Department Store".into(),
    };
    let inserted = root.insert_sqlite(&mut conn).expect("insert root");
    assert_eq!(inserted, 1);

    // 2. Batch insert
    let batch = vec![
        StandardCategory {
            id: "dept_apparel".into(),
            parent_id: Some("root".into()),
            name: "Apparel".into(),
        },
        StandardCategory {
            id: "dept_electronics".into(),
            parent_id: Some("root".into()),
            name: "Electronics".into(),
        },
    ];
    let batch_count =
        StandardCategory::insert_batch_sqlite(&batch, &mut conn).expect("batch insert");
    assert_eq!(batch_count, 2);

    // 3. Update
    let updated_root = StandardCategory {
        id: "root".into(),
        parent_id: None,
        name: "Mega Store".into(),
    };
    let updated = updated_root.update_sqlite(&mut conn).expect("update");
    assert_eq!(updated, 1);

    // 4. Delete
    let deleted =
        StandardCategory::delete_by_id_sqlite(&mut conn, "dept_electronics").expect("delete");
    assert_eq!(deleted, 1);
}

#[test]
fn test_macro_generated_sqlite_traversal_execution() {
    let mut conn = diesel::SqliteConnection::establish(":memory:").expect("in-memory sqlite");

    // Developer creates their own table in their own migration
    conn.batch_execute(
        r#"
        CREATE TABLE standard_categories (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES standard_categories(id),
            name TEXT NOT NULL
        );
        INSERT INTO standard_categories (id, parent_id, name) VALUES
            ('root', NULL, 'Retail Store'),
            ('dept_food', 'root', 'Food & Pantry'),
            ('cat_coffee', 'dept_food', 'Specialty Coffee'),
            ('sku_roast', 'cat_coffee', 'Dark Roast Whole Bean');
    "#,
    )
    .expect("user migration and seed");

    // Execute ancestor traversal directly via the macro-generated inherent method on StandardCategory
    let ancestors: Vec<HierarchyItem> =
        StandardCategory::fetch_ancestors_sqlite(&mut conn, "sku_roast", 10)
            .expect("fetch ancestors via macro method");

    assert_eq!(ancestors.len(), 3);
    assert_eq!(ancestors[0].depth, 1);
    assert_eq!(ancestors[0].name, "Specialty Coffee");
    assert_eq!(ancestors[1].depth, 2);
    assert_eq!(ancestors[1].name, "Food & Pantry");
    assert_eq!(ancestors[2].depth, 3);
    assert_eq!(ancestors[2].name, "Retail Store");

    // Execute descendant traversal directly via the macro-generated inherent method
    let descendants: Vec<HierarchyItem> =
        StandardCategory::fetch_descendants_sqlite(&mut conn, "root", 10)
            .expect("fetch descendants via macro method");

    assert_eq!(descendants.len(), 3);
    assert_eq!(descendants[0].name, "Food & Pantry");
    assert_eq!(descendants[1].name, "Specialty Coffee");
    assert_eq!(descendants[2].name, "Dark Roast Whole Bean");
}
