//! Macros for implementing the crate's traits without repeating their boilerplate.

/// Implements [`NodePayload`](crate::traits::NodePayload) for `$ty` with a `String` key.
///
/// Each accessor is a closure over the node: `key` and `name` return a reference into it, `level`
/// returns the entity level, and the optional `attributes` returns its JSON metadata.
///
/// ```
/// #[derive(Clone)]
/// struct Item { sku: String, title: String }
/// embedag_core::impl_node_payload!(
///     Item,
///     key = |n| &n.sku,
///     name = |n| &n.title,
///     level = |_| "Item"
/// );
/// ```
#[macro_export]
macro_rules! impl_node_payload {
    (
        $ty:ty,
        key = $key:expr,
        name = $name:expr,
        level = $level:expr
        $(, attributes = $attributes:expr)? $(,)?
    ) => {
        impl $crate::traits::NodePayload for $ty {
            type Key = String;

            fn key(&self) -> &Self::Key {
                let f: fn(&Self) -> &String = $key;
                f(self)
            }

            fn name(&self) -> &str {
                let f: fn(&Self) -> &str = $name;
                f(self)
            }

            fn entity_level(&self) -> &str {
                let f: fn(&Self) -> &str = $level;
                f(self)
            }

            $(
                fn attributes(&self) -> serde_json::Value {
                    let f: fn(&Self) -> serde_json::Value = $attributes;
                    f(self)
                }
            )?
        }
    };
}

/// Implements the repository capability traits ([`ReadOnlyGraphRepository`],
/// [`VectorSearchRepository`] and [`TransactionalGraphRepository`]) for an engine whose inherent
/// `async` methods carry the same names (`get_node`, `get_parents`, `get_children`,
/// `find_nearest_neighbors`, `insert_node`, `insert_edge`, `remove_edge`) and whose error `$err`
/// converts into [`DagError`](crate::error::DagError). It also adds the crate-private
/// `apply_directives`. The calling crate needs `serde`.
///
/// [`ReadOnlyGraphRepository`]: crate::traits::ReadOnlyGraphRepository
/// [`VectorSearchRepository`]: crate::traits::VectorSearchRepository
/// [`TransactionalGraphRepository`]: crate::traits::TransactionalGraphRepository
#[macro_export]
macro_rules! impl_async_engine_repository {
    ($engine:ty, $err:ty) => {
        #[$crate::async_trait::async_trait]
        impl<T> $crate::traits::ReadOnlyGraphRepository<T> for $engine
        where
            T: $crate::traits::NodePayload + ::serde::Serialize + ::serde::de::DeserializeOwned,
        {
            async fn get_node(&self, key: &T::Key) -> Result<Option<T>, $crate::error::DagError> {
                self.get_node::<T>(key).await.map_err(Into::into)
            }

            async fn get_parents(&self, key: &T::Key) -> Result<Vec<T>, $crate::error::DagError> {
                self.get_parents::<T>(key).await.map_err(Into::into)
            }

            async fn get_children(&self, key: &T::Key) -> Result<Vec<T>, $crate::error::DagError> {
                self.get_children::<T>(key).await.map_err(Into::into)
            }
        }

        #[$crate::async_trait::async_trait]
        impl<T> $crate::traits::VectorSearchRepository<T> for $engine
        where
            T: $crate::traits::NodePayload + ::serde::Serialize + ::serde::de::DeserializeOwned,
        {
            async fn find_nearest_neighbors(
                &self,
                embedding: &[f32],
                limit: usize,
            ) -> Result<Vec<(T, f32)>, $crate::error::DagError> {
                self.find_nearest_neighbors::<T>(embedding, limit)
                    .await
                    .map_err(Into::into)
            }
        }

        #[$crate::async_trait::async_trait]
        impl<T> $crate::traits::TransactionalGraphRepository<T> for $engine
        where
            T: $crate::traits::NodePayload + ::serde::Serialize + ::serde::de::DeserializeOwned,
        {
            async fn apply_mutation_batch(
                &self,
                directives: &[$crate::mutation::MutationDirective<T>],
            ) -> Result<(), $crate::error::DagError> {
                self.apply_directives(directives).await.map_err(Into::into)
            }
        }

        impl $engine {
            /// Applies the directives in order, stopping at the first failure.
            pub(crate) async fn apply_directives<T>(
                &self,
                directives: &[$crate::mutation::MutationDirective<T>],
            ) -> Result<(), $err>
            where
                T: $crate::traits::NodePayload + ::serde::Serialize,
            {
                use $crate::mutation::MutationDirective;
                for directive in directives {
                    match directive {
                        MutationDirective::UpsertNode(node) => {
                            self.insert_node(node).await?;
                        }
                        MutationDirective::AddEdge {
                            parent,
                            child,
                            edge,
                        } => {
                            self.insert_edge(parent, child, edge).await?;
                        }
                        MutationDirective::RemoveEdge {
                            parent,
                            child,
                            relationship,
                        } => {
                            self.remove_edge(parent, child, relationship.as_deref())
                                .await?;
                        }
                    }
                }
                Ok(())
            }
        }
    };
}

/// Implements `From<$err> for DagError` for a storage error enum with `Dag(DagError)` and
/// `NodeNotFound(String)` variants: those two map to their core equivalents, anything else to
/// [`DagError::StorageError`](crate::error::DagError::StorageError) carrying the message.
#[macro_export]
macro_rules! impl_storage_error_into_dag {
    ($err:ident) => {
        impl From<$err> for $crate::error::DagError {
            fn from(err: $err) -> Self {
                match err {
                    $err::Dag(e) => e,
                    $err::NodeNotFound(key) => $crate::error::DagError::NodeNotFound { key },
                    other => $crate::error::DagError::StorageError(other.to_string()),
                }
            }
        }
    };
}

/// Defines the error enum of a client/server storage adapter: its driver error under the
/// variant `$driver`, plus the topology, serialization, key, not-found, connection and generic
/// variants every adapter shares, and the conversion into [`DagError`](crate::error::DagError).
/// The calling crate needs `thiserror` and `serde_json`.
#[macro_export]
macro_rules! storage_error {
    (
        $(#[$meta:meta])*
        $name:ident {
            $(#[$driver_meta:meta])*
            $driver:ident($driver_ty:ty) = $driver_msg:literal,
            connection = $connection_msg:literal,
            other = $other_msg:literal $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, thiserror::Error)]
        pub enum $name {
            $(#[$driver_meta])*
            #[error($driver_msg)]
            $driver(#[from] $driver_ty),

            /// Graph topology or acyclicity violation.
            #[error("Graph topology violation: {0}")]
            Dag(#[from] $crate::error::DagError),

            /// JSON serialization or deserialization failure.
            #[error("JSON serialization error: {0}")]
            Serialization(#[from] serde_json::Error),

            /// Business key is invalid.
            #[error("Invalid business key: {0}")]
            InvalidKey(String),

            /// Requested node was not found in the database.
            #[error("Node not found: {0}")]
            NodeNotFound(String),

            /// Connection failure or invalid endpoint.
            #[error($connection_msg)]
            Connection(String),

            /// Generic adapter error.
            #[error($other_msg)]
            Other(String),
        }

        $crate::impl_storage_error_into_dag!($name);
    };
}

/// Defines the stored form of a graph edge: the relationship label, confidence, provenance label
/// and RFC 3339 creation time, with `from_edge` converting an
/// [`EdgePayload`](crate::model::EdgePayload). The attributes (derives, docs) go on the struct.
#[macro_export]
macro_rules! edge_record {
    ($(#[$meta:meta])* $vis:vis struct $name:ident;) => {
        $(#[$meta])*
        $vis struct $name {
            /// Relationship type label.
            pub relationship: String,
            /// Traversal confidence score in `[0.0, 1.0]`.
            pub confidence: f32,
            /// Provenance origin label.
            pub provenance: String,
            /// Creation timestamp in RFC 3339.
            pub created_at: String,
        }

        impl $name {
            /// Converts an [`EdgePayload`]($crate::model::EdgePayload) into its stored form.
            pub fn from_edge(edge: &$crate::model::EdgePayload) -> Self {
                Self {
                    relationship: edge.relationship.clone(),
                    confidence: edge.confidence,
                    provenance: edge.provenance.as_str().to_string(),
                    created_at: edge.created_at.to_rfc3339(),
                }
            }
        }
    };
}
