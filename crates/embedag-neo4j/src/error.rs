//! Error types for Neo4j OpenCypher Bolt storage adapter.

embedag_core::storage_error! {
    /// Error variants encountered during Neo4j / Bolt graph storage operations.
    Neo4jStorageError {
        /// Neo4j Bolt driver or transport failure.
        Driver(neo4rs::Error) = "Neo4j Bolt driver error: {0}",
        connection = "Bolt connection failure: {0}",
        other = "Neo4j adapter error: {0}",
    }
}
