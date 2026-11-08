//! Structural AST similarity matching via weighted Jaccard over subtree hash multisets.

use std::collections::BTreeMap;

use crate::tree::Tree;

/// Trait for computing structural similarity in `[0.0, 1.0]` between two syntax [`Tree`]s.
pub trait Matcher {
    /// Returns a similarity score in `[0.0, 1.0]` between `a` and `b`.
    fn score(&self, a: &Tree, b: &Tree) -> f32;
}

/// Structural AST matcher using weighted Jaccard similarity over subtree-hash multisets.
///
/// Each subtree with `size >= min_subtree` contributes its structural hash weighted by its
/// subtree size. Renamed identifiers (labels) still yield a `1.0` match while structurally
/// distinct trees score near `0.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AstMatcher {
    /// Minimum subtree size (in nodes) to include in structural comparison (defaults to `3`).
    pub min_subtree: usize,
}

impl Default for AstMatcher {
    fn default() -> Self {
        Self { min_subtree: 3 }
    }
}

impl AstMatcher {
    /// Creates a new [`AstMatcher`] with the specified `min_subtree` threshold.
    #[must_use]
    pub fn new(min_subtree: usize) -> Self {
        Self { min_subtree }
    }
}

/// `(subtree hash, subtree size)` pairs of one tree.
type Hashes = Vec<(u64, usize)>;

impl AstMatcher {
    /// Subtree hashes of both trees; when neither has one at `min_subtree`, retried with size 1.
    fn hashes_of_both(&self, a: &Tree, b: &Tree) -> (Hashes, Hashes) {
        let min_sz = self.min_subtree.max(1);
        let (hashes_a, hashes_b) = (a.subtree_hashes(min_sz), b.subtree_hashes(min_sz));
        if hashes_a.is_empty() && hashes_b.is_empty() && min_sz > 1 {
            return (a.subtree_hashes(1), b.subtree_hashes(1));
        }
        (hashes_a, hashes_b)
    }
}

impl Matcher for AstMatcher {
    fn score(&self, a: &Tree, b: &Tree) -> f32 {
        let (hashes_a, hashes_b) = self.hashes_of_both(a, b);
        // Retried with size 1 when both are empty, so at most one side can still be empty.
        if hashes_a.is_empty() || hashes_b.is_empty() {
            return 0.0;
        }
        weighted_jaccard(&hash_weights(hashes_a, hashes_b))
    }
}

/// Total subtree size per hash, as `(in a, in b)`.
fn hash_weights(hashes_a: Hashes, hashes_b: Hashes) -> BTreeMap<u64, (usize, usize)> {
    let mut weights: BTreeMap<u64, (usize, usize)> = BTreeMap::new();
    for (h, sz) in hashes_a {
        weights.entry(h).or_insert((0, 0)).0 += sz;
    }
    for (h, sz) in hashes_b {
        weights.entry(h).or_insert((0, 0)).1 += sz;
    }
    weights
}

fn weighted_jaccard(weights: &BTreeMap<u64, (usize, usize)>) -> f32 {
    let mut intersection = 0usize;
    let mut union = 0usize;
    for (wa, wb) in weights.values() {
        intersection += (*wa).min(*wb);
        union += (*wa).max(*wb);
    }
    // Every hash comes from a subtree of at least one node, so the union is never empty.
    ((intersection as f64) / (union as f64)).clamp(0.0, 1.0) as f32
}
