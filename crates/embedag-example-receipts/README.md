# embedag-example-receipts

`embedag-example-receipts` is the reference domain implementation for the `embedag` workspace. It demonstrates how to combine `embedag-core`, `embedag-derive`, `embedag-diesel`, and `embedag-llm` to categorize retail receipts, line-item SKUs, merchants, and commercial invoices across multiple hierarchical taxonomies, persist those hierarchies in SQLite or PostgreSQL using `#[derive(HierarchicalModel)]` and `diesel-cte-ext`, and support user-driven taxonomy reclassification.

## When to use it

Use `embedag-example-receipts` as a reference architecture and integration testbed when learning how to wire together:

- `#[derive(AsGraphContext)]`, `#[derive(GraphTarget)]`, and `#[derive(HierarchicalModel)]` on domain structs.
- Custom application SQL migrations (`initialize_custom_receipt_schema`) paired with `CteHierarchyBuilder` ancestor/descendant lineage queries.
- Multi-DAG orchestration (`FinanceCategorizationEngine`) separating merchant categories (`merchant_dag`), product-intrinsic item categories (`item_dag`), context-dependent purchase categories (`purchase_dag`), and invoice-level classifications (`invoice_dag`).
- User-triggered hierarchy overrides (`user_update_merchant_category`, `user_add_item_category_and_reclassify`).

Do not depend on `embedag-example-receipts` in production libraries or unrelated domain services; its types (`ReceiptDocument`, `FinanceInvoice`, `CategoryNode`, `FinanceCategorizationEngine`) are domain-specific examples rather than generic framework primitives.

### Planned extensions

- **Merchant normalization, kNN categorization, and recurring expense cadence**: [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) (§5, Tickets 10.2.1–10.2.3) specifies deterministic payment-processor prefix stripping, weighted kNN category voting, and recurring subscription cadence detection to extend this reference domain in Milestone 10.

## Main types and functions

- **Receipt and category hierarchy (`lib.rs`)**:
  - `ReceiptCategoryNode` (`ReceiptCategoryRecord`): Annotated with `#[derive(HierarchicalModel)]` (`table = "receipt_categories"`, `id = "category_id"`, `parent = "parent_category_id"`, `name = "category_name"`).
  - `ReceiptGraphPipeline<P>` and `StageExecutionReport`: Dynamic on-the-fly receipt item categorization and DAG deduplication pipeline.
  - `run_receipt_categories_sqlite_migration(&mut SqliteConnection)` and `run_receipt_categories_postgres_migration(&mut diesel_async::AsyncPgConnection)`: Initialize the `receipt_categories` schema.
  - `categorize_ticket_invoice(&FinanceInvoice) -> Vec<ReceiptCategoryRecord>`: Deterministic heuristic categorization for relational persistence tests.
- **Multi-DAG financial invoice engine (`finance`)**:
  - DTOs (`finance::dto`): `FinanceInvoice` (`InvoiceTicket`), `InvoiceItem`, `ProductVariant`, `FinanceMerchant`, `CategorizedInvoice`, `InvoiceCategorizationMode` (`LlmDecided`, `CustomDensity`), `parse_date`.
  - Taxonomy nodes (`finance::nodes`): `MerchantCategoryNode`, `ItemCategoryNode`, `PurchaseCategoryNode`, `InvoiceCategoryNode` (each deriving `HierarchicalModel`).
  - Temporal context (`finance::temporal`): `TemporalContext`, `evaluate_temporal_context`, `is_near_holiday`.
  - Engine (`finance::engine`, `item_inference`, `user_actions`): `FinanceCategorizationEngine<P>` with `new`, `register_merchant`, `process_invoice`, `user_update_merchant_category`, and `user_add_item_category_and_reclassify`.

### Example

```rust
use embedag_example_receipts::finance::{
    FinanceCategorizationEngine, FinanceInvoice, FinanceMerchant, InvoiceCategorizationMode,
    InvoiceItem, parse_date,
};
use embedag_llm::provider::CandleProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = CandleProvider::new("phi-3-mini");
    let mut engine = FinanceCategorizationEngine::new(provider);

    engine.register_merchant(FinanceMerchant {
        merchant_id: "m_corner".into(),
        name: "Corner Confectionery".into(),
        tags: vec!["retail".into()],
        category_id: "mcat_retail".into(),
        category_name: "Retail Store".into(),
    });

    let invoice = FinanceInvoice::new(
        "inv_001",
        "m_corner",
        parse_date("2025-12-24").unwrap(),
        vec![InvoiceItem::new("item_1", "Ferrero Rocher Box", 18.0, 2, None)],
        36.0,
    )
    .with_purchase_content("today is John's birthday");

    let categorized = engine
        .process_invoice(invoice, &InvoiceCategorizationMode::LlmDecided)
        .await?;

    assert_eq!(
        categorized.item_categories.get("item_1"),
        Some(&("cat_hazelnut_chocolate".into(), "Hazelnut Chocolate".into()))
    );
    assert!(categorized.invoice_categories.contains(&"Holiday".to_string()));
    Ok(())
}
```

## Feature flags

`embedag-example-receipts` defines no Cargo feature flags of its own; it uses the default features of `embedag-core`, `embedag-diesel` (`sqlite`, `postgres-async`, `test-harness`), `embedag-derive`, and `embedag-llm`.

## Workspace relationship

Within the workspace, `embedag-example-receipts` is a leaf example crate that exercises [`embedag-core`](https://docs.rs/embedag-core), [`embedag-derive`](https://docs.rs/embedag-derive), [`embedag-diesel`](https://docs.rs/embedag-diesel), and [`embedag-llm`](https://docs.rs/embedag-llm) together in end-to-end SQLite and Testcontainers PostgreSQL tests (`tests/finance_categorization_e2e.rs`, `tests/postgres_Categorization_e2e.rs`).


## Build and test

```bash
# Run unit and in-memory SQLite end-to-end tests
cargo test -p embedag-example-receipts

# Run containerized PostgreSQL end-to-end tests (requires Docker)
cargo test -p embedag-example-receipts --test postgres_Categorization_e2e -- --include-ignored
```

## Specifications

- [`doc/06_Procedural_Derive_Macros.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/06_Procedural_Derive_Macros.md) — `AsGraphContext`, `GraphTarget`, and `HierarchicalModel` usage.
- [`doc/14_Dynamic_Graph_Mutations_and_Diffing.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/14_Dynamic_Graph_Mutations_and_Diffing.md) — Packaging hierarchy and semantic delta synthesis examples.
- [`doc/16_Hierarchical_Recursive_CTE_Traversals.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/16_Hierarchical_Recursive_CTE_Traversals.md) — Custom table schemas and recursive CTE lineage queries.
- [`doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md`](https://github.com/yasuomaidana/embedag/blob/main/doc/23_Text_Embeddings_Vector_Search_and_Classification_Primitives.md) — Planned merchant normalization, kNN categorization, and recurring expense extensions.
