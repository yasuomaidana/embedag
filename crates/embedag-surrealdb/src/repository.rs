//! Implementation of `embedag-core` persistence repository capability traits for [`SurrealDbEngine`].

use crate::engine::SurrealDbEngine;

embedag_core::impl_async_engine_repository!(SurrealDbEngine, crate::error::SurrealDbStorageError);
