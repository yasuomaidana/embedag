//! Temporal context extraction and string sanitization utilities.

use chrono::{DateTime, Datelike, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Temporal context extracted from date timestamp providing insightful context to LLM inference.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TemporalContext {
    pub is_holiday_season: bool,
    pub is_weekend: bool,
    pub month: u32,
    pub day: u32,
    pub holiday_description: Option<String>,
}

/// Evaluates insightful temporal context from an invoice timestamp.
pub fn evaluate_temporal_context(dt: &DateTime<Utc>) -> TemporalContext {
    let m = dt.month();
    let d = dt.day();
    let is_dec_holiday = m == 12 && (d >= 20);
    let is_thanksgiving = m == 11 && (24..=30).contains(&d);
    let is_holiday_season = is_dec_holiday || is_thanksgiving;

    let holiday_description = if is_dec_holiday {
        Some("Christmas / New Year Holiday Gifting Season".to_string())
    } else if is_thanksgiving {
        Some("Thanksgiving Holiday Season".to_string())
    } else {
        None
    };

    let weekday = dt.weekday().number_from_monday();
    let is_weekend = weekday >= 6;

    TemporalContext {
        is_holiday_season,
        is_weekend,
        month: m,
        day: d,
        holiday_description,
    }
}

/// Helper evaluating whether an invoice timestamp falls within a holiday season.
pub fn is_near_holiday(dt: &DateTime<Utc>) -> bool {
    let m = dt.month();
    let d = dt.day();
    (m == 12 && d >= 20) || (m == 11 && (24..=30).contains(&d))
}

pub fn sanitize_slug(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
