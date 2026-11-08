# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0]

### Added

- `#[derive(AsGraphContext)]` procedural macro with `#[graph(key)]`, `#[graph(text)]`, `#[graph(scalar)]`, `#[graph(time)]`/`#[graph(temporal)]`, `#[graph(geo)]`/`#[graph(geospatial)]`, `#[graph(tag)]`, `#[graph(blob)]`/`#[graph(blobs)]`, and `#[graph(tokens)]`/`#[graph(sparse)]` attributes (Ticket 1.4.1).
- `#[derive(GraphTarget)]` procedural macro for enums and structs with `#[target(level = "...")]`, `#[target(relationship = "...")]`, and container-level `#[target(levels(...), relationships(...))]` attributes (Ticket 1.4.2).
- `#[derive(HierarchicalModel)]` procedural macro generating `embedag_diesel::cte::HierarchicalModel`, `HierarchicalNode`, `embedag_core::traits::NodePayload`, dialect-specific `CREATE TABLE` DDL helpers, and SQLite/PostgreSQL CRUD methods (`insert_sqlite`, `insert_batch_sqlite`, `delete_by_id_sqlite`, `update_sqlite`, `insert_pg`, `insert_pg_async`, etc.) (Ticket 5.2.2).
