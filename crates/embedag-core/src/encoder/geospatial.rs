//! Discrete hexagonal geospatial indexing and topological distance via `h3o`.
//!
//! Traceability: `doc/07_Deterministic_Feature_Encoding.md` §2.3.

#[cfg(feature = "h3o")]
use h3o::{CellIndex, LatLng, Resolution};

use crate::error::EncodingError;

/// Discrete hexagonal geospatial encoder mapping WGS84 coordinates into H3 cells.
#[derive(Debug, Clone, Copy, Default)]
pub struct GeospatialEncoder;

#[cfg(feature = "h3o")]
impl GeospatialEncoder {
    /// Encodes longitude and latitude into a discrete 64-bit H3 cell index at the specified resolution (0..=15).
    ///
    /// # Errors
    /// - [`EncodingError::InvalidResolution`]: If `resolution > 15`.
    /// - [`EncodingError::InvalidCoordinates`]: If `lon` or `lat` are non-finite or outside valid WGS84 bounds.
    pub fn encode(lon: f64, lat: f64, resolution: u8) -> Result<u64, EncodingError> {
        h3_encode(lon, lat, resolution)
    }

    /// Computes the discrete topological grid ring distance between two H3 cell indices.
    ///
    /// # Errors
    /// - [`EncodingError::InvalidCellIndex`]: If either `cell_a` or `cell_b` is not a valid H3 cell.
    /// - [`EncodingError::GridDistanceUndefined`]: If the cells cannot be compared (e.g. pentagon distortion or different resolutions).
    pub fn grid_distance(cell_a: u64, cell_b: u64) -> Result<u32, EncodingError> {
        h3_distance(cell_a, cell_b)
    }

    /// Computes normalized topological distance in `[0.0, 1.0]`, bounded by `max_rings`.
    ///
    /// # Errors
    /// Propagates errors from [`h3_distance`].
    pub fn normalized_distance(
        cell_a: u64,
        cell_b: u64,
        max_rings: u32,
    ) -> Result<f64, EncodingError> {
        h3_normalized_distance(cell_a, cell_b, max_rings)
    }
}

/// Encodes `(lon, lat)` into a 64-bit integer H3 cell index according to WGS84 bounds.
///
/// Valid coordinates: latitude $\in [-90.0, 90.0]$, longitude $\in [-180.0, 180.0]$.
///
/// # Errors
/// - [`EncodingError::InvalidResolution`] on invalid resolution (> 15).
/// - [`EncodingError::InvalidCoordinates`] on out-of-bounds or non-finite coordinates.
#[cfg(feature = "h3o")]
pub fn h3_encode(lon: f64, lat: f64, resolution: u8) -> Result<u64, EncodingError> {
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return Err(EncodingError::InvalidCoordinates { lon, lat });
    }
    let res = Resolution::try_from(resolution)
        .map_err(|_| EncodingError::InvalidResolution(resolution))?;
    let coord =
        LatLng::new(lat, lon).map_err(|_| EncodingError::InvalidCoordinates { lon, lat })?;
    let cell = coord.to_cell(res);
    Ok(u64::from(cell))
}

/// Computes the integer hexagonal grid distance (ring distance) between two H3 cells.
///
/// # Errors
/// - [`EncodingError::InvalidCellIndex`] if either integer is not a valid cell.
/// - [`EncodingError::GridDistanceUndefined`] if distance is undefined between the cells.
#[cfg(feature = "h3o")]
pub fn h3_distance(cell_a: u64, cell_b: u64) -> Result<u32, EncodingError> {
    let c_a = CellIndex::try_from(cell_a).map_err(|_| EncodingError::InvalidCellIndex(cell_a))?;
    let c_b = CellIndex::try_from(cell_b).map_err(|_| EncodingError::InvalidCellIndex(cell_b))?;
    let dist = c_a
        .grid_distance(c_b)
        .map_err(|err| EncodingError::GridDistanceUndefined {
            a: cell_a,
            b: cell_b,
            reason: err.to_string(),
        })?;
    let unsigned_dist = u32::try_from(dist.max(0)).unwrap_or(u32::MAX);
    Ok(unsigned_dist)
}

/// Computes normalized distance in `[0.0, 1.0]` based on grid distance clamped to `max_rings`.
///
/// If `max_rings == 0`, returns `0.0` when `cell_a == cell_b` and `1.0` otherwise.
///
/// # Errors
/// Propagates cell parsing or distance errors from [`h3_distance`].
#[cfg(feature = "h3o")]
pub fn h3_normalized_distance(
    cell_a: u64,
    cell_b: u64,
    max_rings: u32,
) -> Result<f64, EncodingError> {
    let dist = h3_distance(cell_a, cell_b)?;
    if max_rings == 0 {
        return Ok(if dist == 0 { 0.0 } else { 1.0 });
    }
    let norm = (f64::from(dist) / f64::from(max_rings)).clamp(0.0, 1.0);
    Ok(norm)
}

#[cfg(all(test, feature = "h3o"))]
mod tests {
    use super::*;

    #[test]
    fn test_h3_roundtrip_and_ring_distance() {
        // Paris coordinates (approx 2.3522 E, 48.8566 N)
        let cell_paris = h3_encode(2.3522, 48.8566, 8).expect("valid Paris coordinate");
        assert_ne!(cell_paris, 0);

        // Same location distance is 0
        let self_dist = h3_distance(cell_paris, cell_paris).expect("self distance");
        assert_eq!(self_dist, 0);

        let self_norm =
            h3_normalized_distance(cell_paris, cell_paris, 10).expect("self norm distance");
        assert_eq!(self_norm, 0.0);

        // Slightly shifted coordinate (adjacent or near cell)
        let cell_nearby = h3_encode(2.3550, 48.8566, 8).expect("nearby coordinate");
        let dist = h3_distance(cell_paris, cell_nearby).expect("grid distance nearby");
        assert!(dist <= 2);
    }

    #[test]
    fn test_h3_invalid_coordinates() {
        // Latitude > 90.0 is invalid
        let err_lat = h3_encode(0.0, 95.0, 8).unwrap_err();
        assert!(matches!(err_lat, EncodingError::InvalidCoordinates { .. }));

        // Longitude > 180.0 is invalid
        let err_lon = h3_encode(185.0, 0.0, 8).unwrap_err();
        assert!(matches!(err_lon, EncodingError::InvalidCoordinates { .. }));

        // Latitude < -90.0 is invalid
        let err_neg_lat = h3_encode(0.0, -95.0, 8).unwrap_err();
        assert!(matches!(
            err_neg_lat,
            EncodingError::InvalidCoordinates { .. }
        ));

        // Longitude < -180.0 is invalid
        let err_neg_lon = h3_encode(-185.0, 0.0, 8).unwrap_err();
        assert!(matches!(
            err_neg_lon,
            EncodingError::InvalidCoordinates { .. }
        ));

        // Non-finite
        let err_nan = h3_encode(f64::NAN, 0.0, 8).unwrap_err();
        assert!(matches!(err_nan, EncodingError::InvalidCoordinates { .. }));

        // Invalid resolution > 15
        let err_res = h3_encode(0.0, 0.0, 16).unwrap_err();
        assert!(matches!(err_res, EncodingError::InvalidResolution(16)));
    }

    #[test]
    fn test_h3_invalid_cell_index() {
        let err = h3_distance(0, 1).unwrap_err();
        assert!(matches!(err, EncodingError::InvalidCellIndex(_)));
    }
}
