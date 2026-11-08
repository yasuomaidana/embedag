use embedag_surrealdb::{SurrealDbEngine, SurrealDbStorageError};

embedag_core::persistence_suite!(SurrealDbEngine::in_memory());

embedag_core::persistence_error_suite!(SurrealDbEngine::in_memory(), SurrealDbStorageError);
