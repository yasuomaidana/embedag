# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- `ReceiptDocument`, `MerchantEntity`, `ProductSku` (`#[derive(AsGraphContext)]`), `ReceiptTaxonomyTarget` (`#[derive(GraphTarget)]`), and `CategoryNode` (`#[derive(HierarchicalModel)]`) reference domain models (Ticket 1.4.1, Ticket 1.4.2, Ticket 5.2.2).
- Custom SQLite schema initialization (`initialize_custom_receipt_schema`), constrained LLM receipt categorization (`categorize_and_persist_receipt`), recursive CTE lineage helpers (`fetch_category_ancestors`, `fetch_category_descendants`), `build_receipt_Synthesizer`, and `build_receipt_extractor` (Ticket 4.3.3, Ticket 5.2.2).
- Multi-DAG `FinanceCategorizationEngine` (`finance` module) managing separate `merchant_dag`, `item_dag`, `purchase_dag`, and `invoice_dag` hierarchies with temporal holiday detection (`evaluate_temporal_context`), custom density evaluation (`InvoiceCategorizationMode::CustomDensity`), and dynamic user reclassification (`user_update_merchant_category`, `user_add_item_category_and_reclassify`).
- End-to-end SQLite (`tests/finance_categorization_e2e.rs`) and Testcontainers PostgreSQL (`tests/postgres_Categorization_e2e.rs`) integration test suites.
