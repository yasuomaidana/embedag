//! Cyclical trigonometric temporal encoding.
//!
//! Traceability: `doc/07_Deterministic_Feature_Encoding.md` §2.2.

use std::f64::consts::PI;

use chrono::{DateTime, Datelike, Timelike, Utc};

/// Trigonometric cyclical temporal encoder projecting timestamps onto unit circles.
#[derive(Debug, Clone, Copy, Default)]
pub struct CyclicalTimeEncoder;

impl CyclicalTimeEncoder {
    /// Encodes a UTC timestamp into a 4-dimensional continuous vector:
    /// `[sin_hour, cos_hour, sin_day, cos_day]`.
    ///
    /// - Dimensions `0..2` capture diurnal (24-hour) periodicity.
    /// - Dimensions `2..4` capture annual (365.25-day) periodicity.
    ///
    /// Continuity across midnight (23:59 -> 00:01) is preserved.
    #[must_use]
    pub fn encode(dt: &DateTime<Utc>) -> [f64; 4] {
        encode_cyclical_time(dt)
    }

    /// Computes the normalized temporal distance in `[0.0, 1.0]` between two encoded temporal vectors.
    ///
    /// Uses Euclidean distance normalized by the theoretical maximum distance $\sqrt{8} \approx 2.8284$.
    #[must_use]
    pub fn distance(a: &[f64; 4], b: &[f64; 4]) -> f64 {
        temporal_distance(a, b)
    }
}

/// Standalone projection function mapping a [`DateTime<Utc>`] to `[f64; 4]`.
#[must_use]
pub fn encode_cyclical_time(dt: &DateTime<Utc>) -> [f64; 4] {
    let second_fraction = dt.second() as f64 + (f64::from(dt.nanosecond()) / 1_000_000_000.0);
    let hour = dt.hour() as f64 + (dt.minute() as f64 / 60.0) + (second_fraction / 3600.0);
    let tau = 2.0 * PI;

    let hour_angle = (hour / 24.0) * tau;
    let day_angle = (f64::from(dt.ordinal0()) / 365.25) * tau;

    [
        hour_angle.sin(),
        hour_angle.cos(),
        day_angle.sin(),
        day_angle.cos(),
    ]
}

/// Computes the normalized Euclidean distance between two cyclical time vectors in `[0.0, 1.0]`.
#[must_use]
pub fn temporal_distance(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let mut sum_sq = 0.0;
    for i in 0..4 {
        let diff = a[i] - b[i];
        sum_sq += diff * diff;
    }
    // Max Euclidean distance across two 2D unit circles is sqrt(2^2 + 2^2) = sqrt(8)
    let max_dist = 8.0_f64.sqrt();
    (sum_sq.sqrt() / max_dist).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_midnight_boundary_continuity() {
        // 23:59 on day 1 vs 00:01 on day 2
        let t1 = Utc
            .with_ymd_and_hms(2026, 6, 15, 23, 59, 0)
            .single()
            .unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 6, 16, 0, 1, 0).single().unwrap();

        let enc1 = encode_cyclical_time(&t1);
        let enc2 = encode_cyclical_time(&t2);

        // Hour distance alone (2 minutes apart = 2/1440 of circle)
        let hour_diff_sq = (enc1[0] - enc2[0]).powi(2) + (enc1[1] - enc2[1]).powi(2);
        let hour_euclidean = hour_diff_sq.sqrt();
        assert!(
            hour_euclidean < 0.05,
            "Euclidean distance between 23:59 and 00:01 must be < 0.05, was {}",
            hour_euclidean
        );

        // Overall normalized temporal distance
        let dist = temporal_distance(&enc1, &enc2);
        assert!(
            dist < 0.05,
            "normalized distance must be < 0.05, was {}",
            dist
        );
    }

    #[test]
    fn test_temporal_determinism() {
        let t = Utc
            .with_ymd_and_hms(2026, 10, 12, 14, 30, 0)
            .single()
            .unwrap();
        let enc_a = encode_cyclical_time(&t);
        let enc_b = CyclicalTimeEncoder::encode(&t);

        assert_eq!(enc_a, enc_b);
        assert_eq!(temporal_distance(&enc_a, &enc_b), 0.0);
    }

    #[test]
    fn test_unit_circle_invariant() {
        let t = Utc
            .with_ymd_and_hms(2026, 3, 21, 6, 45, 12)
            .single()
            .unwrap();
        let enc = encode_cyclical_time(&t);

        let hour_unit = enc[0].powi(2) + enc[1].powi(2);
        let day_unit = enc[2].powi(2) + enc[3].powi(2);

        assert!((hour_unit - 1.0).abs() < 1e-9);
        assert!((day_unit - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_opposite_times_distance() {
        // Noon vs Midnight: diametrically opposite on hour circle
        let noon = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).single().unwrap();
        let midnight = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();

        let enc_noon = encode_cyclical_time(&noon);
        let enc_midnight = encode_cyclical_time(&midnight);

        let hour_dist = ((enc_noon[0] - enc_midnight[0]).powi(2)
            + (enc_noon[1] - enc_midnight[1]).powi(2))
        .sqrt();
        assert!((hour_dist - 2.0).abs() < 1e-6);
    }
}
