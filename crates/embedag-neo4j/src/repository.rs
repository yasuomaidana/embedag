//! Implementation of `embedag-core` persistence repository capability traits for [`Neo4jEngine`].

use async_trait::async_trait;
use embedag_core::error::DagError;
use embedag_core::mutation::MutationDirective;
use embedag_core::traits::{
    NodePayload, ReadOnlyGraphRepository, TransactionalGraphRepository, VectorSearchRepository,
};
use serde::{Serialize, de::DeserializeOwned};

use crate::engine::Neo4jEngine;

#[async_trait]
impl<T> ReadOnlyGraphRepository<T> for Neo4jEngine
where
    T: NodePayload + Serialize + DeserializeOwned,
{
    async fn get_node(&self, key: &T::Key) -> Result<Option<T>, DagError> {
        self.get_node::<T>(key).await.map_err(Into::into)
    }

    async fn get_parents(&self, key: &T::Key) -> Result<Vec<T>, DagError> {
        self.get_parents::<T>(key).await.map_err(Into::into)
    }

    async fn get_children(&self, key: &T::Key) -> Result<Vec<T>, DagError> {
        self.get_children::<T>(key).await.map_err(Into::into)
    }
}

#[async_trait]
impl<T> VectorSearchRepository<T> for Neo4jEngine
where
    T: NodePayload + Serialize + DeserializeOwned,
{
    async fn find_nearest_neighbors(
        &self,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(T, f32)>, DagError> {
        self.find_nearest_neighbors::<T>(embedding, limit)
            .await
            .map_err(Into::into)
    }
}

#[async_trait]
impl<T> TransactionalGraphRepository<T> for Neo4jEngine
where
    T: NodePayload + Serialize + DeserializeOwned,
{
    async fn apply_mutation_batch(
        &self,
        directives: &[MutationDirective<T>],
    ) -> Result<(), DagError> {
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
