//! Geospatial export scaffolding (COG, GeoPackage, Zarr, COPC, 3D Tiles, …).

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GeoExportKind {
    CogDepth,
    CogVelocity,
    CogHazard,
    GeoPackage,
    GeoJson,
    Zarr,
    NetCdf,
    Copc,
    Spz,
    GltfGaussian,
    Tiles3d,
    ScenarioReport,
}

impl GeoExportKind {
    pub fn id(self) -> &'static str {
        match self {
            GeoExportKind::CogDepth => "cogDepth",
            GeoExportKind::CogVelocity => "cogVelocity",
            GeoExportKind::CogHazard => "cogHazard",
            GeoExportKind::GeoPackage => "geoPackage",
            GeoExportKind::GeoJson => "geoJson",
            GeoExportKind::Zarr => "zarr",
            GeoExportKind::NetCdf => "netCdf",
            GeoExportKind::Copc => "copc",
            GeoExportKind::Spz => "spz",
            GeoExportKind::GltfGaussian => "gltfGaussian",
            GeoExportKind::Tiles3d => "tiles3d",
            GeoExportKind::ScenarioReport => "scenarioReport",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GeoExportKind::CogDepth => "COG max depth",
            GeoExportKind::CogVelocity => "COG velocity",
            GeoExportKind::CogHazard => "COG hazard",
            GeoExportKind::GeoPackage => "GeoPackage",
            GeoExportKind::GeoJson => "GeoJSON",
            GeoExportKind::Zarr => "Zarr time series",
            GeoExportKind::NetCdf => "CF-NetCDF",
            GeoExportKind::Copc => "COPC point cloud",
            GeoExportKind::Spz => "SPZ splat",
            GeoExportKind::GltfGaussian => "glTF Gaussian",
            GeoExportKind::Tiles3d => "3D Tiles",
            GeoExportKind::ScenarioReport => "Scenario report",
        }
    }
}

/// Formats the UI can advertise before writers exist.
pub fn list_export_kinds() -> Vec<GeoExportKind> {
    vec![
        GeoExportKind::CogDepth,
        GeoExportKind::CogVelocity,
        GeoExportKind::CogHazard,
        GeoExportKind::GeoPackage,
        GeoExportKind::GeoJson,
        GeoExportKind::Zarr,
        GeoExportKind::NetCdf,
        GeoExportKind::Copc,
        GeoExportKind::Spz,
        GeoExportKind::GltfGaussian,
        GeoExportKind::Tiles3d,
        GeoExportKind::ScenarioReport,
    ]
}

/// Write a geospatial product (stub).
pub fn export(_kind: GeoExportKind, _src: &Path, _dest: &Path) -> Result<(), String> {
    Err("Geospatial exports are not implemented yet.".into())
}
