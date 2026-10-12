//! Continuous scalar normalization and logarithmic dynamic-range compression.
//!
//! Traceability: `doc/07_Deterministic_Feature_Encoding.md` §2.1.

use serde::{Deserialize, Serialize};

/// Continuous scalar normalization strategies mapping real values to `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalarScaling {
    /// Bounded interval normalization: `(x - min) / (max - min)` clamped to `[0.0, 1.0]`.
    MinMax {
        /// Lower bound representing 0.0.
        min: f64,
        /// Upper bound representing 1.0.
        max: f64,
    },
    /// Standard score normalization with sigmoid squashing: `1 / (1 + exp(-(x - mean)/std))`.
    StandardScoreSigmoid {
        /// Population or distribution mean.
        mean: f64,
        /// Standard deviation.
        std_dev: f64,
    },
    /// Logarithmic compression for wide dynamic ranges: `ln(1 + x) / ln(1 + max)`.
    Logarithmic {
        /// Upper scale value representing 1.0.
        max: f64,
    },
}

impl ScalarScaling {
    /// Transforms the input scalar value into a bounded float in `[0.0, 1.0]`.
    ///
    /// Never panics or returns `NaN`. Out-of-bounds, negative, or degenerate values
    /// are gracefully clamped.
    #[must_use]
    pub fn transform(&self, value: f64) -> f64 {
        match *self {
            Self::MinMax { min, max } => encode_linear(value, min, max),
            Self::StandardScoreSigmoid { mean, std_dev } => {
                encode_standard_score(value, mean, std_dev)
            }
            Self::Logarithmic { max } => encode_log(value, 0.0, max),
        }
    }
}

/// Encodes a continuous scalar linearly into `[0.0, 1.0]`, clamping boundary overflows.
///
/// If `max <= min` or if `val` is `NaN`, returns `0.0`.
#[must_use]
pub fn encode_linear(val: f64, min: f64, max: f64) -> f64 {
    if val.is_nan() || min.is_nan() || max.is_nan() {
        return 0.0;
    }
    let range = max - min;
    if range.abs() < f64::EPSILON || range < 0.0 {
        return 0.0;
    }
    ((val - min) / range).clamp(0.0, 1.0)
}

/// Encodes a continuous scalar using logarithmic compression into `[0.0, 1.0]`.
///
/// Uses `ln(1 + max(0, val - min)) / ln(1 + max(0, max - min))`, providing smooth
/// non-linear squashing for quantities that span multiple orders of magnitude (e.g. price, weights).
///
/// If `max <= min` or inputs are `NaN`, returns `0.0`.
#[must_use]
pub fn encode_log(val: f64, min: f64, max: f64) -> f64 {
    if val.is_nan() || min.is_nan() || max.is_nan() {
        return 0.0;
    }
    let range = max - min;
    if range.abs() < f64::EPSILON || range <= 0.0 {
        return 0.0;
    }
    let delta = (val - min).max(0.0);
    let numerator = (1.0 + delta).ln();
    let denominator = (1.0 + range).ln();
    if denominator.abs() < f64::EPSILON {
        return 0.0;
    }
    (numerator / denominator).clamp(0.0, 1.0)
}

/// Encodes a scalar value via z-score and sigmoid squashing into `(0.0, 1.0)`.
///
/// `z = (val - mean) / std_dev`
/// `sigmoid(z) = 1 / (1 + exp(-z))`
#[must_use]
pub fn encode_standard_score(val: f64, mean: f64, std_dev: f64) -> f64 {
    if val.is_nan() || mean.is_nan() || std_dev.is_nan() {
        return 0.5;
    }
    let effective_std = std_dev.abs().max(f64::EPSILON);
    let z = (val - mean) / effective_std;
    let squashed = 1.0 / (1.0 + (-z).exp());
    squashed.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_scalar_encoding() {
        // Normal bounds [10.0, 50.0]
        assert!((encode_linear(10.0, 10.0, 50.0) - 0.0).abs() < 1e-6);
        assert!((encode_linear(50.0, 10.0, 50.0) - 1.0).abs() < 1e-6);
        assert!((encode_linear(30.0, 10.0, 50.0) - 0.5).abs() < 1e-6);

        // Clamping behavior
        assert!((encode_linear(5.0, 10.0, 50.0) - 0.0).abs() < 1e-6);
        assert!((encode_linear(100.0, 10.0, 50.0) - 1.0).abs() < 1e-6);

        // Degenerate bounds
        assert_eq!(encode_linear(30.0, 50.0, 50.0), 0.0);
        assert_eq!(encode_linear(30.0, 50.0, 10.0), 0.0);
        assert_eq!(encode_linear(f64::NAN, 10.0, 50.0), 0.0);
    }

    #[test]
    fn test_log_scalar_encoding() {
        // Multi-order range [0.0, 10000.0]
        assert!((encode_log(0.0, 0.0, 10000.0) - 0.0).abs() < 1e-6);
        assert!((encode_log(10000.0, 0.0, 10000.0) - 1.0).abs() < 1e-6);

        // Concavity: intermediate value compressed higher than linear
        let linear_mid = encode_linear(100.0, 0.0, 10000.0);
        let log_mid = encode_log(100.0, 0.0, 10000.0);
        assert_eq!(linear_mid, 0.01);
        assert!(log_mid > 0.45 && log_mid < 0.55); // ln(101) / ln(10001) ≈ 4.615 / 9.210 ≈ 0.501

        // Clamped bounds
        assert_eq!(encode_log(-50.0, 0.0, 10000.0), 0.0);
        assert_eq!(encode_log(20000.0, 0.0, 10000.0), 1.0);
        assert_eq!(encode_log(10.0, 10.0, 10.0), 0.0);
    }

    #[test]
    fn test_standard_score_sigmoid() {
        // Mean maps to 0.5
        assert!((encode_standard_score(100.0, 100.0, 15.0) - 0.5).abs() < 1e-6);

        // +3 standard deviations maps close to 0.95
        let high = encode_standard_score(145.0, 100.0, 15.0);
        assert!(high > 0.90 && high < 1.0);

        // -3 standard deviations maps close to 0.05
        let low = encode_standard_score(55.0, 100.0, 15.0);
        assert!(low > 0.0 && low < 0.10);

        // Degenerate std dev does not panic
        assert!(!encode_standard_score(100.0, 100.0, 0.0).is_nan());
    }

    #[test]
    fn test_scalar_scaling_enum() {
        let min_max = ScalarScaling::MinMax {
            min: 0.0,
            max: 100.0,
        };
        assert_eq!(min_max.transform(50.0), 0.5);

        let log = ScalarScaling::Logarithmic { max: 1000.0 };
        assert_eq!(log.transform(1000.0), 1.0);
        assert_eq!(log.transform(0.0), 0.0);

        let sigmoid = ScalarScaling::StandardScoreSigmoid {
            mean: 0.0,
            std_dev: 1.0,
        };
        assert_eq!(sigmoid.transform(0.0), 0.5);
    }
}
