//! Composite hybrid distance metric combining semantic, lexical, scalar, temporal, and spatial modalities.
//!
//! Traceability: `doc/07_Deterministic_Feature_Encoding.md` and
//! `doc/09_Mixed_Modality_Representation_and_Similarity.md` §3.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "h3o")]
use crate::encoder::geospatial::h3_normalized_distance;
use crate::encoder::temporal::{encode_cyclical_time, temporal_distance};
use crate::error::EncodingError;
use crate::model::{FeatureMap, TagMap};
use crate::traits::AsGraphContext;

/// Relative importance weights assigned to each modality in composite hybrid distance calculations.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModalityWeights {
    /// Semantic linguistic dense embedding weight (cosine distance).
    pub text: f64,
    /// Lexical sparse BM25 token overlap weight.
    pub lexical: f64,
    /// Continuous scalar feature weight (Gower relative metric).
    pub scalar: f64,
    /// Cyclical temporal distance weight.
    pub temporal: f64,
    /// Discrete geospatial H3 ring distance weight.
    pub geospatial: f64,
    /// Categorical taxonomy and tag match weight.
    pub categorical: f64,
}

impl Default for ModalityWeights {
    fn default() -> Self {
        Self {
            text: 0.35,
            lexical: 0.15,
            scalar: 0.20,
            temporal: 0.10,
            geospatial: 0.10,
            categorical: 0.10,
        }
    }
}

impl ModalityWeights {
    /// Validates that weights are non-negative, finite, and have a non-zero sum.
    ///
    /// # Errors
    /// Returns [`EncodingError::InvalidWeights`] if any weight is negative, non-finite, or all are zero.
    pub fn validate(&self) -> Result<(), EncodingError> {
        let weights = [
            self.text,
            self.lexical,
            self.scalar,
            self.temporal,
            self.geospatial,
            self.categorical,
        ];
        for &w in &weights {
            if !w.is_finite() || w < 0.0 {
                return Err(EncodingError::InvalidWeights(format!(
                    "weight {w} is negative or non-finite"
                )));
            }
        }
        let sum: f64 = weights.iter().sum();
        if sum < f64::EPSILON {
            return Err(EncodingError::InvalidWeights(
                "sum of modality weights must be strictly positive".to_string(),
            ));
        }
        Ok(())
    }
}

/// Computes angular cosine distance between two dense vectors: `(1 - cos(a, b)) / 2`.
///
/// Output is bounded in `[0.0, 1.0]`.
///
/// # Errors
/// Returns [`EncodingError::DimensionMismatch`] if vectors differ in dimensionality.
pub fn cosine_distance(a: &[f32], b: &[f32]) -> Result<f64, EncodingError> {
    if a.len() != b.len() {
        return Err(EncodingError::DimensionMismatch {
            left: a.len(),
            right: b.len(),
        });
    }
    if a.is_empty() {
        return Ok(0.0);
    }

    let mut dot = 0.0_f64;
    let mut norm_a = 0.0_f64;
    let mut norm_b = 0.0_f64;

    for (&x, &y) in a.iter().zip(b.iter()) {
        let x_f = f64::from(x);
        let y_f = f64::from(y);
        dot += x_f * y_f;
        norm_a += x_f * x_f;
        norm_b += y_f * y_f;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < f64::EPSILON {
        return Ok(0.0);
    }

    let cos_sim = (dot / denom).clamp(-1.0, 1.0);
    let distance = (1.0 - cos_sim) / 2.0;
    Ok(distance.clamp(0.0, 1.0))
}

/// Computes Jaccard lexical distance over weighted sparse tokens: `1 - (intersection / union)`.
#[must_use]
pub fn lexical_token_distance(a: &[(u32, f32)], b: &[(u32, f32)]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    if a.is_empty() || b.is_empty() {
        return 1.0;
    }

    let set_a: HashSet<u32> = a.iter().map(|(id, _)| *id).collect();
    let set_b: HashSet<u32> = b.iter().map(|(id, _)| *id).collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    let jaccard = intersection as f64 / union as f64;
    (1.0 - jaccard).clamp(0.0, 1.0)
}

/// Computes Gower's relative distance across continuous scalar dimensions.
///
/// For each dimension $k$: `|a_k - b_k| / max(|a_k|, |b_k|, epsilon)`.
/// Dimensions missing from one side receive maximum penalty `1.0`.
///
/// Returns `None` if both maps are empty.
#[must_use]
pub fn scalar_gower_distance(a: &FeatureMap, b: &FeatureMap) -> Option<f64> {
    if a.is_empty() && b.is_empty() {
        return None;
    }

    let all_keys: HashSet<&String> = a.keys().chain(b.keys()).collect();

    let mut total_distance = 0.0;
    for &k in &all_keys {
        match (a.get(k), b.get(k)) {
            (Some(&val_a), Some(&val_b)) => {
                let max_abs = val_a.abs().max(val_b.abs()).max(f64::EPSILON);
                let diff = (val_a - val_b).abs();
                total_distance += (diff / max_abs).clamp(0.0, 1.0);
            }
            _ => {
                total_distance += 1.0;
            }
        }
    }

    Some((total_distance / all_keys.len() as f64).clamp(0.0, 1.0))
}

/// Computes cyclical temporal distance between two optional timestamps.
///
/// Returns `None` if either timestamp is absent.
#[must_use]
pub fn temporal_distance_opt(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<f64> {
    let (Some(dt_a), Some(dt_b)) = (a, b) else {
        return None;
    };
    let enc_a = encode_cyclical_time(&dt_a);
    let enc_b = encode_cyclical_time(&dt_b);
    Some(temporal_distance(&enc_a, &enc_b))
}

/// Computes categorical tag distance between two tag maps.
///
/// Returns `None` if both maps are empty.
#[must_use]
pub fn categorical_tag_distance(a: &TagMap, b: &TagMap) -> Option<f64> {
    if a.is_empty() && b.is_empty() {
        return None;
    }
    let all_keys: HashSet<&String> = a.keys().chain(b.keys()).collect();

    let mut mismatches = 0.0;
    for &k in &all_keys {
        match (a.get(k), b.get(k)) {
            (Some(v_a), Some(v_b)) if v_a == v_b => {}
            _ => mismatches += 1.0,
        }
    }

    Some((mismatches / all_keys.len() as f64).clamp(0.0, 1.0))
}

#[cfg(feature = "h3o")]
fn compute_geospatial_distance(
    coord_a: Option<(f64, f64)>,
    coord_b: Option<(f64, f64)>,
    resolution: u8,
    max_rings: u32,
) -> Option<f64> {
    let ((lon_a, lat_a), (lon_b, lat_b)) = (coord_a?, coord_b?);
    let cell_a = crate::encoder::geospatial::h3_encode(lon_a, lat_a, resolution).ok()?;
    let cell_b = crate::encoder::geospatial::h3_encode(lon_b, lat_b, resolution).ok()?;
    h3_normalized_distance(cell_a, cell_b, max_rings).ok()
}

/// Computes the composite hybrid distance between two items implementing [`AsGraphContext`].
///
/// Combines dense vectors, BM25 tokens, scalar features, temporal timestamps,
/// geospatial coordinates, and categorical tags.
///
/// **Graceful Fallback**: If modalities are missing (e.g. absent coordinates or timestamps),
/// only the available modalities participate, and their weights are dynamically re-normalized
/// so that the composite distance remains strictly bounded in `[0.0, 1.0]`.
///
/// # Errors
/// Returns [`EncodingError`] if weights are invalid or dense vector dimensions mismatch.
pub fn composite_distance<A: AsGraphContext + ?Sized, B: AsGraphContext + ?Sized>(
    ctx_a: &A,
    ctx_b: &B,
    dense_vec_a: &[f32],
    dense_vec_b: &[f32],
    weights: &ModalityWeights,
    h3_resolution: u8,
    max_h3_rings: u32,
) -> Result<f64, EncodingError> {
    weights.validate()?;

    let mut components: Vec<Component> = Vec::with_capacity(6);
    components.extend(dense_component(dense_vec_a, dense_vec_b, weights)?);
    let pair = Pair {
        a: ctx_a,
        b: ctx_b,
        weights,
    };
    components.extend(pair.lexical());
    components.extend(pair.scalar());
    components.extend(pair.temporal());
    components.extend(pair.geospatial(h3_resolution, max_h3_rings));
    components.extend(pair.categorical());
    Ok(weighted_mean(&components))
}

/// A modality's `(weight, distance)`.
type Component = (f64, f64);

/// Semantic dense embedding.
fn dense_component(
    dense_vec_a: &[f32],
    dense_vec_b: &[f32],
    weights: &ModalityWeights,
) -> Result<Option<Component>, EncodingError> {
    if dense_vec_a.is_empty() || dense_vec_b.is_empty() {
        return Ok(None);
    }
    Ok(Some((
        weights.text,
        cosine_distance(dense_vec_a, dense_vec_b)?,
    )))
}

/// The two items being compared and the modality weights.
struct Pair<'a, A: ?Sized, B: ?Sized> {
    a: &'a A,
    b: &'a B,
    weights: &'a ModalityWeights,
}

impl<A: AsGraphContext + ?Sized, B: AsGraphContext + ?Sized> Pair<'_, A, B> {
    /// Lexical sparse tokens.
    fn lexical(&self) -> Option<Component> {
        let tokens_a = self.a.sparse_lexical_tokens();
        let tokens_b = self.b.sparse_lexical_tokens();
        if tokens_a.is_empty() && tokens_b.is_empty() {
            return None;
        }
        Some((
            self.weights.lexical,
            lexical_token_distance(&tokens_a, &tokens_b),
        ))
    }

    /// Continuous scalar features.
    fn scalar(&self) -> Option<Component> {
        scalar_gower_distance(&self.a.scalar_features(), &self.b.scalar_features())
            .map(|d| (self.weights.scalar, d))
    }

    /// Temporal cyclical distance.
    fn temporal(&self) -> Option<Component> {
        temporal_distance_opt(self.a.timestamp_utc(), self.b.timestamp_utc())
            .map(|d| (self.weights.temporal, d))
    }

    /// Geospatial H3 distance.
    #[cfg(feature = "h3o")]
    fn geospatial(&self, h3_resolution: u8, max_h3_rings: u32) -> Option<Component> {
        compute_geospatial_distance(
            self.a.geographic_coordinates(),
            self.b.geographic_coordinates(),
            h3_resolution,
            max_h3_rings,
        )
        .map(|d| (self.weights.geospatial, d))
    }

    /// Without the `h3o` feature the geospatial modality never participates.
    #[cfg(not(feature = "h3o"))]
    fn geospatial(&self, _h3_resolution: u8, _max_h3_rings: u32) -> Option<Component> {
        None
    }

    /// Categorical tags.
    fn categorical(&self) -> Option<Component> {
        categorical_tag_distance(&self.a.categorical_tags(), &self.b.categorical_tags())
            .map(|d| (self.weights.categorical, d))
    }
}

/// The weight-normalized mean of the available modalities, in `[0.0, 1.0]`.
fn weighted_mean(components: &[Component]) -> f64 {
    let total_weight: f64 = components.iter().map(|(w, _)| *w).sum();
    if components.is_empty() || total_weight < f64::EPSILON {
        return 0.0;
    }
    let mut weighted_distance = 0.0;
    for (w, d) in components {
        weighted_distance += (w / total_weight) * d;
    }
    weighted_distance.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record with a key, a text and tokens and nothing else.
    fn record(key: &str, text: &str, tokens: Vec<(u32, f32)>) -> FullContextRecord {
        FullContextRecord {
            key: key.to_string(),
            text: text.to_string(),
            tokens,
            scalars: HashMap::new(),
            time: None,
            coords: None,
            tags: HashMap::new(),
        }
    }
    use chrono::TimeZone;
    use std::collections::HashMap;

    struct FullContextRecord {
        key: String,
        text: String,
        tokens: Vec<(u32, f32)>,
        scalars: FeatureMap,
        time: Option<DateTime<Utc>>,
        coords: Option<(f64, f64)>,
        tags: TagMap,
    }

    impl AsGraphContext for FullContextRecord {
        fn context_key(&self) -> String {
            self.key.clone()
        }
        fn text_repr(&self) -> String {
            self.text.clone()
        }
        fn sparse_lexical_tokens(&self) -> Vec<(u32, f32)> {
            self.tokens.clone()
        }
        fn scalar_features(&self) -> FeatureMap {
            self.scalars.clone()
        }
        fn timestamp_utc(&self) -> Option<DateTime<Utc>> {
            self.time
        }
        fn geographic_coordinates(&self) -> Option<(f64, f64)> {
            self.coords
        }
        fn categorical_tags(&self) -> TagMap {
            self.tags.clone()
        }
    }

    #[test]
    fn test_cosine_distance() {
        let v1 = vec![1.0_f32, 0.0, 0.0];
        let v2 = vec![1.0_f32, 0.0, 0.0];
        let v3 = vec![-1.0_f32, 0.0, 0.0];
        let v4 = vec![0.0_f32, 1.0, 0.0];

        // Identical: distance 0.0
        assert!((cosine_distance(&v1, &v2).unwrap() - 0.0).abs() < 1e-6);
        // Opposite: distance 1.0
        assert!((cosine_distance(&v1, &v3).unwrap() - 1.0).abs() < 1e-6);
        // Orthogonal: distance 0.5
        assert!((cosine_distance(&v1, &v4).unwrap() - 0.5).abs() < 1e-6);

        // Dimension mismatch
        let mismatch = cosine_distance(&v1, &[1.0, 2.0]);
        assert!(matches!(
            mismatch,
            Err(EncodingError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn test_composite_distance_all_modalities() {
        let t = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).single().unwrap();
        let full = |key: &str, text: &str| FullContextRecord {
            scalars: HashMap::from([("weight_grams".to_string(), 50.0)]),
            time: Some(t),
            coords: Some((2.3522, 48.8566)),
            tags: HashMap::from([("brand".to_string(), "Nestle".to_string())]),
            ..record(key, text, vec![(101, 1.0), (102, 1.0)])
        };
        let rec_a = full("sku-1", "Dark Chocolate");
        let rec_b = full("sku-2", "Dark Chocolate Bar");

        let vec_a = vec![1.0, 0.0, 0.0];
        let vec_b = vec![1.0, 0.0, 0.0];

        let weights = ModalityWeights::default();
        let dist = composite_distance(&rec_a, &rec_b, &vec_a, &vec_b, &weights, 8, 10).unwrap();

        // Identical attributes in all modalities: distance should be 0.0
        assert!(dist < 1e-5, "expected ~0.0 distance, got {dist}");
    }

    #[test]
    fn test_composite_distance_partial_fallback() {
        // Record A has temporal and coordinates; Record B does NOT
        let rec_a = FullContextRecord {
            time: Some(Utc::now()),
            coords: Some((10.0, 20.0)),
            ..record("sku-1", "Phone", vec![(1, 1.0)])
        };
        let rec_b = record("sku-2", "Phone", vec![(1, 1.0)]);

        let vec_a = vec![1.0, 0.0];
        let vec_b = vec![1.0, 0.0];

        let weights = ModalityWeights::default();
        let dist = composite_distance(&rec_a, &rec_b, &vec_a, &vec_b, &weights, 8, 10).unwrap();

        // Absent modalities fall back and re-normalize; active modalities are identical -> distance 0.0
        assert!((0.0..=1.0).contains(&dist));
        assert!(dist < 1e-5, "expected ~0.0, got {dist}");
    }

    use rstest::rstest;

    fn weights(values: [f64; 6]) -> ModalityWeights {
        let [text, lexical, scalar, temporal, geospatial, categorical] = values;
        ModalityWeights {
            text,
            lexical,
            scalar,
            temporal,
            geospatial,
            categorical,
        }
    }

    #[rstest]
    #[case::valid([1.0; 6], true)]
    #[case::negative([1.0, 1.0, 1.0, 1.0, 1.0, -1.0], false)]
    #[case::not_finite([f64::NAN, 1.0, 1.0, 1.0, 1.0, 1.0], false)]
    #[case::all_zero([0.0; 6], false)]
    fn weights_must_be_finite_non_negative_and_not_all_zero(
        #[case] values: [f64; 6],
        #[case] valid: bool,
    ) {
        assert_eq!(weights(values).validate().is_ok(), valid);
    }

    #[test]
    fn cosine_distance_of_empty_or_zero_vectors_is_zero() {
        assert_eq!(cosine_distance(&[], &[]).unwrap(), 0.0);
        assert_eq!(cosine_distance(&[0.0, 0.0], &[1.0, 1.0]).unwrap(), 0.0);
    }

    #[rstest]
    #[case::both_empty(&[], &[], 0.0)]
    #[case::one_empty(&[(1, 1.0)], &[], 1.0)]
    #[case::other_empty(&[], &[(1, 1.0)], 1.0)]
    #[case::identical(&[(1, 1.0), (2, 1.0)], &[(1, 1.0), (2, 1.0)], 0.0)]
    fn lexical_distance_edges(
        #[case] a: &[(u32, f32)],
        #[case] b: &[(u32, f32)],
        #[case] expected: f64,
    ) {
        assert!((lexical_token_distance(a, b) - expected).abs() < 1e-9);
    }

    #[test]
    fn scalar_distance_penalizes_missing_dimensions_and_has_none_when_empty() {
        let empty = FeatureMap::new();
        assert_eq!(scalar_gower_distance(&empty, &empty), None);

        let a = FeatureMap::from([("x".to_string(), 10.0)]);
        let b = FeatureMap::from([("y".to_string(), 10.0)]);
        // Each side lacks the other's dimension: both dimensions get the maximum penalty.
        assert_eq!(scalar_gower_distance(&a, &b), Some(1.0));
    }

    #[test]
    fn categorical_distance_counts_mismatching_tags() {
        let empty = TagMap::new();
        assert_eq!(categorical_tag_distance(&empty, &empty), None);

        let a = TagMap::from([("brand".to_string(), "A".to_string())]);
        let b = TagMap::from([("brand".to_string(), "B".to_string())]);
        assert_eq!(categorical_tag_distance(&a, &b), Some(1.0));
        assert_eq!(categorical_tag_distance(&a, &a), Some(0.0));
    }

    #[test]
    fn composite_distance_degrades_gracefully() {
        let a = record("a", "text", vec![]);
        let b = record("b", "text", vec![]);
        // No modality has data on either side: nothing to compare.
        assert_eq!(
            composite_distance(&a, &b, &[], &[], &ModalityWeights::default(), 8, 5).unwrap(),
            0.0
        );
        // Weights that fail validation are an error.
        let bad = weights([0.0; 6]);
        assert!(composite_distance(&a, &b, &[], &[], &bad, 8, 5).is_err());
        // A single modality with a zero weight leaves no weight to normalize by.
        let only_text = weights([0.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
        assert_eq!(
            composite_distance(&a, &b, &[1.0], &[1.0], &only_text, 8, 5).unwrap(),
            0.0
        );
    }
}
