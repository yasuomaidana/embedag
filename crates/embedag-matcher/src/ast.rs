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

impl Matcher for AstMatcher {
    fn score(&self, a: &Tree, b: &Tree) -> f32 {
        let min_sz = self.min_subtree.max(1);
        let mut hashes_a = a.subtree_hashes(min_sz);
        let mut hashes_b = b.subtree_hashes(min_sz);

        if hashes_a.is_empty() && hashes_b.is_empty() && min_sz > 1 {
            hashes_a = a.subtree_hashes(1);
            hashes_b = b.subtree_hashes(1);
        }

        if hashes_a.is_empty() && hashes_b.is_empty() {
            return 1.0;
        }
        if hashes_a.is_empty() || hashes_b.is_empty() {
            return 0.0;
        }

        let mut weights: BTreeMap<u64, (usize, usize)> = BTreeMap::new();
        for (h, sz) in hashes_a {
            weights.entry(h).or_insert((0, 0)).0 += sz;
        }
        for (h, sz) in hashes_b {
            weights.entry(h).or_insert((0, 0)).1 += sz;
        }

        let mut intersection = 0usize;
        let mut union = 0usize;
        for (wa, wb) in weights.values() {
            intersection += (*wa).min(*wb);
            union += (*wa).max(*wb);
        }

        if union == 0 {
            1.0
        } else {
            ((intersection as f64) / (union as f64)).clamp(0.0, 1.0) as f32
        }
    }
}
