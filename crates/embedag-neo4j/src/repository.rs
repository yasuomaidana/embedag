//! Implementation of `embedag-core` persistence repository capability traits for [`Neo4jEngine`].

use crate::engine::Neo4jEngine;

embedag_core::impl_async_engine_repository!(Neo4jEngine, crate::error::Neo4jStorageError);
