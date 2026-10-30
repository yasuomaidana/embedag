//! Error types for SurrealDB storage adapter.

embedag_core::storage_error! {
    /// Error variants encountered during SurrealDB multi-model storage operations.
    SurrealDbStorageError {
        /// SurrealDB driver or engine error.
        Surreal(surrealdb::Error) = "SurrealDB driver error: {0}",
        connection = "Database connection failure: {0}",
        other = "SurrealDB adapter error: {0}",
    }
}
