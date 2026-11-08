//! Topological degree pagination contracts and types for high-degree graph hubs.
//!
//! Traceability: `doc/21_Heavy_Graph_Handling_and_Topological_Pagination.md`.

use petgraph::Direction;

use crate::model::EdgePayload;
use crate::traits::NodePayload;

/// Request parameters for paginating immediate neighbors of a high-degree node.
#[derive(Debug, Clone)]
pub struct NeighborPageRequest<'a, K> {
    /// Target node business key.
    pub key: &'a K,
    /// Traversal direction: [`Direction::Incoming`] for parents, [`Direction::Outgoing`] for children.
    pub direction: Direction,
    /// Zero-based offset of adjacent edges to skip.
    pub offset: usize,
    /// Maximum number of neighbors to return per page.
    pub limit: usize,
    /// Optional relationship filter (e.g. "is_a", "belongs_to").
    pub relationship: Option<&'a str>,
}

impl<'a, K> NeighborPageRequest<'a, K> {
    /// Creates a new page request for outgoing children.
    #[must_use]
    pub fn children(key: &'a K, offset: usize, limit: usize) -> Self {
        Self {
            key,
            direction: Direction::Outgoing,
            offset,
            limit,
            relationship: None,
        }
    }

    /// Creates a new page request for incoming parents.
    #[must_use]
    pub fn parents(key: &'a K, offset: usize, limit: usize) -> Self {
        Self {
            key,
            direction: Direction::Incoming,
            offset,
            limit,
            relationship: None,
        }
    }

    /// Attaches an optional relationship filter to the request.
    #[must_use]
    pub fn with_relationship(mut self, rel: &'a str) -> Self {
        self.relationship = Some(rel);
        self
    }
}

/// Paginated page of neighbor node payloads and connecting edge metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct NeighborPage<'a, T: NodePayload> {
    /// Items on the current page: neighbor payload reference and connecting edge payload.
    pub items: Vec<(&'a T, &'a EdgePayload)>,
    /// Total count of matching adjacent neighbors.
    pub total_count: usize,
    /// Offset for the next page if `has_more` is true.
    pub next_offset: Option<usize>,
    /// Indicates whether additional items exist beyond this page.
    pub has_more: bool,
}
