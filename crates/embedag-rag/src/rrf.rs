use std::collections::HashMap;
use uuid::Uuid;

/// Candidate result produced by Reciprocal Rank Fusion.
#[derive(Debug, Clone, PartialEq)]
pub struct RrfCandidate {
    pub id: Uuid,
    pub score: f32,
    pub dense_rank: Option<usize>,
    pub sparse_rank: Option<usize>,
}

/// Reciprocal Rank Fusion (RRF) ranker for merging multi-modal retrieval streams.
#[derive(Debug, Clone)]
pub struct RrfRanker {
    pub smoothing_k: f32,
}

impl Default for RrfRanker {
    fn default() -> Self {
        Self { smoothing_k: 60.0 }
    }
}

impl RrfRanker {
    pub fn new(smoothing_k: f32) -> Self {
        Self { smoothing_k }
    }

    /// Merges dense and sparse ranked lists into a unified scored candidate list.
    pub fn fuse(&self, dense_results: &[Uuid], sparse_results: &[Uuid]) -> Vec<RrfCandidate> {
        let mut scores: HashMap<Uuid, f32> = HashMap::new();
        let mut dense_ranks: HashMap<Uuid, usize> = HashMap::new();
        let mut sparse_ranks: HashMap<Uuid, usize> = HashMap::new();

        // 1. Accumulate dense scores (1-indexed rank)
        for (idx, &id) in dense_results.iter().enumerate() {
            let rank = idx + 1;
            let score = 1.0 / (self.smoothing_k + rank as f32);
            *scores.entry(id).or_insert(0.0) += score;
            dense_ranks.insert(id, rank);
        }

        // 2. Accumulate sparse scores (1-indexed rank)
        for (idx, &id) in sparse_results.iter().enumerate() {
            let rank = idx + 1;
            let score = 1.0 / (self.smoothing_k + rank as f32);
            *scores.entry(id).or_insert(0.0) += score;
            sparse_ranks.insert(id, rank);
        }

        let mut candidates: Vec<RrfCandidate> = scores
            .into_iter()
            .map(|(id, score)| RrfCandidate {
                id,
                score,
                dense_rank: dense_ranks.get(&id).copied(),
                sparse_rank: sparse_ranks.get(&id).copied(),
            })
            .collect();

        // Sort by score descending; deterministic tie-breaking via UUID bytes
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });

        candidates
    }

    /// Merges an arbitrary number of ranked candidate lists.
    pub fn fuse_multi(&self, ranked_lists: &[&[Uuid]]) -> Vec<(Uuid, f32)> {
        let mut scores: HashMap<Uuid, f32> = HashMap::new();

        for list in ranked_lists {
            for (idx, &id) in list.iter().enumerate() {
                let rank = idx + 1;
                let score = 1.0 / (self.smoothing_k + rank as f32);
                *scores.entry(id).or_insert(0.0) += score;
            }
        }

        let mut results: Vec<(Uuid, f32)> = scores.into_iter().collect();
        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_scoring_order() {
        let ranker = RrfRanker::default(); // k = 60.0

        let node_a = Uuid::new_v4();
        let node_b = Uuid::new_v4();
        let node_c = Uuid::new_v4();

        // Dense list: Node A (rank 1), Node B (rank 2)
        let dense = vec![node_a, node_b];

        // Sparse list: Node B (rank 1), Node C (rank 2)
        let sparse = vec![node_b, node_c];

        let fused = ranker.fuse(&dense, &sparse);

        // Expected scores:
        // Node B: 1/(60+2) + 1/(60+1) = 1/62 + 1/61 ≈ 0.016129 + 0.016393 = 0.032522
        // Node A: 1/(60+1) = 1/61 ≈ 0.016393
        // Node C: 1/(60+2) = 1/62 ≈ 0.016129
        assert_eq!(fused.len(), 3);
        assert_eq!(
            fused[0].id, node_b,
            "Node B present in both lists must rank #1"
        );
        assert_eq!(fused[1].id, node_a, "Node A (rank 1 in dense) must rank #2");
        assert_eq!(
            fused[2].id, node_c,
            "Node C (rank 2 in sparse) must rank #3"
        );

        let expected_b_score = (1.0 / 62.0) + (1.0 / 61.0);
        assert!((fused[0].score - expected_b_score).abs() < 1e-6);
        assert_eq!(fused[0].dense_rank, Some(2));
        assert_eq!(fused[0].sparse_rank, Some(1));
    }
}
