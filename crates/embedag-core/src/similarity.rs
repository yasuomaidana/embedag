//! Cosine ranking over small in-memory embedding indexes, shared by the storage adapters.

use std::collections::HashMap;

/// The `limit` keys of `index` most similar to `query` by cosine similarity, best first.
///
/// Embeddings whose length differs from the query's are skipped, an all-zero embedding scores
/// `0.0`, and an all-zero query matches nothing.
#[must_use]
pub fn rank_by_cosine(
    index: &HashMap<String, Vec<f32>>,
    query: &[f32],
    limit: usize,
) -> Vec<(String, f32)> {
    let q_norm = query.iter().map(|v| v * v).sum::<f32>().sqrt();
    if q_norm == 0.0 {
        return Vec::new();
    }

    let mut scored: Vec<(String, f32)> = Vec::with_capacity(index.len());
    for (key, vec) in index {
        if vec.len() != query.len() {
            continue;
        }
        let dot = query.iter().zip(vec).map(|(a, b)| a * b).sum::<f32>();
        let v_norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        let score = if v_norm > 0.0 {
            dot / (q_norm * v_norm)
        } else {
            0.0
        };
        scored.push((key.clone(), score));
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn index() -> HashMap<String, Vec<f32>> {
        HashMap::from([
            ("x".to_string(), vec![1.0, 0.0]),
            ("y".to_string(), vec![0.0, 1.0]),
            ("zero".to_string(), vec![0.0, 0.0]),
            ("short".to_string(), vec![1.0]),
        ])
    }

    #[rstest]
    #[case::closest_first(&[0.9, 0.1], 2, &["x", "y"])]
    #[case::limited(&[0.1, 0.9], 1, &["y"])]
    #[case::zero_query_matches_nothing(&[0.0, 0.0], 3, &[])]
    fn ranks_by_cosine_similarity(
        #[case] query: &[f32],
        #[case] limit: usize,
        #[case] expected: &[&str],
    ) {
        let ranked = rank_by_cosine(&index(), query, limit);
        let keys: Vec<&str> = ranked.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, expected);
    }

    #[test]
    fn scores_are_cosines() {
        let ranked = rank_by_cosine(&index(), &[1.0, 0.0], 4);
        assert!((ranked[0].1 - 1.0).abs() < 1e-6);
        assert_eq!(
            ranked.len(),
            3,
            "the embedding of another length is skipped"
        );
    }
}
