//! Deterministic mathematical feature encoders for numbers, time, geography, and hybrid distances.
//!
//! Traceability: `doc/07_Deterministic_Feature_Encoding.md`.

pub mod distance;
pub mod geospatial;
pub mod scalar;
pub mod temporal;

// Re-exports
pub use distance::{
    ModalityWeights, categorical_tag_distance, composite_distance, cosine_distance,
    lexical_token_distance, scalar_gower_distance, temporal_distance_opt,
};
pub use geospatial::GeospatialEncoder;
#[cfg(feature = "h3o")]
pub use geospatial::{h3_distance, h3_encode, h3_normalized_distance};
pub use scalar::{ScalarScaling, encode_linear, encode_log, encode_standard_score};
pub use temporal::{CyclicalTimeEncoder, encode_cyclical_time, temporal_distance};
