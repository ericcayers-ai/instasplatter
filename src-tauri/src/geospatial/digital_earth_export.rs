//! Export a Gaussian-splat reconstruction as a Digital-Earth-ready manifest.
//!
//! Digital Earth (github.com/ericcayers-ai, sibling project) is a real-scale
//! (metres, Earth-centered) path tracer. Its world space is ECEF, which is
//! exactly what `EnuFrame::enu_to_ecef_matrix` already produces for the
//! Geospatial suite's AOI frame — so placing a splat in Digital Earth is
//! just handing it that matrix plus the PLY it already knows how to read.
//! This module writes the sidecar; it does not touch the PLY itself.

use super::transforms::{EnuFrame, Geodetic};
use crate::splat::SplatCloud;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DIGITAL_EARTH_MANIFEST_FORMAT: &str = "digital-earth-splat-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DigitalEarthSplatManifest {
    pub format: String,
    /// Path to the 3DGS PLY this manifest places, relative to the manifest
    /// file's own directory when possible.
    pub ply_path: String,
    pub splat_count: usize,
    pub origin: Geodetic,
    pub origin_ecef: [f64; 3],
    /// Row-major 4x4: local ENU metres -> ECEF metres. Apply to (x,y,z,1).
    pub enu_to_ecef_matrix: [f64; 16],
    pub units: String,
}

pub fn build_manifest(
    splat: &SplatCloud,
    frame: &EnuFrame,
    ply_path: &Path,
) -> DigitalEarthSplatManifest {
    DigitalEarthSplatManifest {
        format: DIGITAL_EARTH_MANIFEST_FORMAT.to_string(),
        ply_path: ply_path.to_string_lossy().into_owned(),
        splat_count: splat.len(),
        origin: frame.origin,
        origin_ecef: frame.origin_ecef.as_array(),
        enu_to_ecef_matrix: frame.enu_to_ecef_matrix(),
        units: "meters".to_string(),
    }
}

/// Writes the manifest JSON to `dest`. Does not copy or modify the PLY.
pub fn write_manifest(
    splat: &SplatCloud,
    frame: &EnuFrame,
    ply_path: &Path,
    dest: &Path,
) -> Result<DigitalEarthSplatManifest, String> {
    if splat.is_empty() {
        return Err("Cannot export an empty splat cloud to Digital Earth.".to_string());
    }
    let manifest = build_manifest(splat, frame, ply_path);
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    fs::write(dest, json).map_err(|e| e.to_string())?;
    Ok(manifest)
}

/// Reads a splat PLY and writes its Digital Earth placement manifest next to
/// it. `origin` is typically the AOI's ENU origin already computed by the
/// Geospatial suite (`aoiEnuFrame` on the TS side / `EnuFrame` here).
pub fn export_splat_for_digital_earth(
    ply_path: &Path,
    origin: Geodetic,
    dest: &Path,
) -> Result<DigitalEarthSplatManifest, String> {
    let splat = crate::splat::ply::read_ply(ply_path)?;
    let frame = EnuFrame::from_geodetic(origin);
    write_manifest(&splat, &frame, ply_path, dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_splat() -> SplatCloud {
        SplatCloud {
            positions: vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
            scales_log: vec![[0.0; 3]; 2],
            rot_wxyz: vec![[1.0, 0.0, 0.0, 0.0]; 2],
            opacity_logit: vec![0.0; 2],
            sh_dc: vec![[0.0; 3]; 2],
            sh_rest: Vec::new(),
            rest_per_channel: 0,
        }
    }

    #[test]
    fn manifest_matrix_recovers_origin_ecef() {
        let origin = Geodetic {
            lon_deg: 174.7633,
            lat_deg: -36.8485,
            height_m: 20.0,
        };
        let frame = EnuFrame::from_geodetic(origin);
        let splat = sample_splat();
        let manifest = build_manifest(&splat, &frame, Path::new("splat.ply"));

        assert_eq!(manifest.format, DIGITAL_EARTH_MANIFEST_FORMAT);
        assert_eq!(manifest.splat_count, 2);

        // Applying the matrix to local ENU (0,0,0,1) must recover origin_ecef
        // exactly, since the ENU origin's ECEF position is that point.
        let m = manifest.enu_to_ecef_matrix;
        let x = m[3];
        let y = m[7];
        let z = m[11];
        assert!((x - manifest.origin_ecef[0]).abs() < 1e-6);
        assert!((y - manifest.origin_ecef[1]).abs() < 1e-6);
        assert!((z - manifest.origin_ecef[2]).abs() < 1e-6);
    }

    #[test]
    fn manifest_matrix_places_offset_point_correctly() {
        let origin = Geodetic {
            lon_deg: 10.0,
            lat_deg: 50.0,
            height_m: 0.0,
        };
        let frame = EnuFrame::from_geodetic(origin);
        let splat = sample_splat();
        let manifest = build_manifest(&splat, &frame, Path::new("splat.ply"));

        // A splat point 100m east of the ENU origin should map via the
        // matrix to the same ECEF position `EnuFrame::enu_to_ecef` computes
        // directly (independent code path already covered by
        // transforms::tests, cross-checked here).
        let m = manifest.enu_to_ecef_matrix;
        let (east, north, up) = (100.0, 0.0, 0.0);
        let x = m[0] * east + m[1] * north + m[2] * up + m[3];
        let y = m[4] * east + m[5] * north + m[6] * up + m[7];
        let z = m[8] * east + m[9] * north + m[10] * up + m[11];

        let expected = frame.enu_to_ecef(crate::geospatial::transforms::Enu { east, north, up });
        assert!((x - expected.x).abs() < 1e-6);
        assert!((y - expected.y).abs() < 1e-6);
        assert!((z - expected.z).abs() < 1e-6);
    }

    #[test]
    fn refuses_empty_splat() {
        let origin = Geodetic {
            lon_deg: 0.0,
            lat_deg: 0.0,
            height_m: 0.0,
        };
        let frame = EnuFrame::from_geodetic(origin);
        let dest = std::env::temp_dir().join("digital_earth_export_test_empty.json");
        let result = write_manifest(&SplatCloud::default(), &frame, Path::new("x.ply"), &dest);
        assert!(result.is_err());
    }

    #[test]
    fn writes_manifest_file_to_disk() {
        let origin = Geodetic {
            lon_deg: 10.0,
            lat_deg: 50.0,
            height_m: 0.0,
        };
        let frame = EnuFrame::from_geodetic(origin);
        let splat = sample_splat();
        let dest = std::env::temp_dir().join("digital_earth_export_test_write.json");
        let manifest = write_manifest(&splat, &frame, Path::new("splat.ply"), &dest).unwrap();
        let read_back = fs::read_to_string(&dest).unwrap();
        let parsed: DigitalEarthSplatManifest = serde_json::from_str(&read_back).unwrap();
        assert_eq!(parsed.splat_count, manifest.splat_count);
        assert_eq!(parsed.format, DIGITAL_EARTH_MANIFEST_FORMAT);
        let _ = fs::remove_file(&dest);
    }
}
