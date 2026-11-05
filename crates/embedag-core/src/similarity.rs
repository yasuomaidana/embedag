//! Cosine ranking over small in-memory embedding indexes, shared by the storage adapters.

use std::collections::HashMap;

/// Maps NaN to the lowest score, so an embedding with NaN or infinite parts never outranks a real match.
fn nan_last(score: f32) -> f32 {
    if score.is_nan() {
        f32::NEG_INFINITY
    } else {
        score
    }
}

/// The `limit` keys of `index` most similar to `query` by cosine similarity, best first.
///
/// Embeddings whose length differs from the query's are skipped, an all-zero embedding scores
/// `0.0`, and an all-zero query matches nothing. Scores that are NaN rank last, and equal scores are
/// ordered by key, so the result is the same on every run.
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

    // `total_cmp` is a total order even for NaN (a NaN score sorts after every real score); ties fall
    // back to the key so the result does not depend on `HashMap` iteration order.
    scored.sort_by(|a, b| {
        nan_last(b.1)
            .total_cmp(&nan_last(a.1))
            .then_with(|| a.0.cmp(&b.0))
    });
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

    #[test]
    fn nan_scores_rank_last_without_panicking() {
        let index = HashMap::from([
            ("bad".to_string(), vec![f32::NAN, 1.0]),
            ("inf".to_string(), vec![f32::INFINITY, 0.0]),
            ("x".to_string(), vec![1.0, 0.0]),
        ]);
        let ranked = rank_by_cosine(&index, &[1.0, 0.0], 3);
        assert_eq!(ranked[0].0, "x");
        assert!(ranked.iter().skip(1).all(|(k, _)| k == "bad" || k == "inf"));
    }

    #[test]
    fn equal_scores_are_ordered_by_key() {
        let index: HashMap<String, Vec<f32>> = ["d", "b", "a", "c"]
            .into_iter()
            .map(|k| (k.to_string(), vec![1.0, 0.0]))
            .collect();
        for _ in 0..20 {
            let keys: Vec<String> = rank_by_cosine(&index, &[1.0, 0.0], 4)
                .into_iter()
                .map(|(k, _)| k)
                .collect();
            assert_eq!(keys, ["a", "b", "c", "d"]);
        }
    }
}
