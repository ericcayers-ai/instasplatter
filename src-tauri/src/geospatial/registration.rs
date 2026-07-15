//! Drone / survey georegistration (stub).
//!
//! Later phases: EXIF/DJI/RTK/GCP ingestion, ENU/ECEF/CRS transforms, metric
//! scale, and adaptive extent planning.

use crate::project::GeoReference;
use std::path::Path;

/// Result of a registration attempt (placeholder).
#[derive(Debug, Clone, Default)]
pub struct RegistrationResult {
    pub geo_reference: GeoReference,
    pub camera_count: usize,
    pub warnings: Vec<String>,
}

/// Scan imagery / telemetry under `sources_dir` and produce a GeoReference.
/// Phase 1 returns an empty reference and a "not implemented" warning.
pub fn register_from_sources(sources_dir: &Path) -> Result<RegistrationResult, String> {
    if !sources_dir.exists() {
        return Err(format!(
            "Sources directory does not exist: {}",
            sources_dir.display()
        ));
    }
    Ok(RegistrationResult {
        geo_reference: GeoReference {
            provenance: Some("registration stub — await drone-georeg phase".into()),
            ..Default::default()
        },
        camera_count: 0,
        warnings: vec![
            "Automatic drone georegistration is not available yet.".into(),
        ],
    })
}

/// Apply GCPs to refine a GeoReference (stub).
pub fn refine_with_gcps(
    base: &GeoReference,
    _gcp_csv: &Path,
) -> Result<RegistrationResult, String> {
    Ok(RegistrationResult {
        geo_reference: base.clone(),
        camera_count: 0,
        warnings: vec!["GCP refinement is not available yet.".into()],
    })
}
