//! Geospatial data types and format scaffolding.
//!
//! Project-level GeoReference / GeoLayer live in `crate::project`. This module
//! holds shared helpers and format identifiers used by catalog, DEM, and export
//! paths once those land.

use serde::{Deserialize, Serialize};

#[allow(unused_imports)]
pub use crate::project::{
    ensure_geo_workspace, FloodScenario, GeoLayer, GeoLayerKind, GeoReference, SimulationRun,
    GEO_WORKSPACE_DIRS,
};

/// Supported raster / vector / cloud interchange formats (catalog scaffolding).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GeoFormat {
    GeoTiff,
    Cog,
    GeoPackage,
    GeoJson,
    FlatGeobuf,
    GeoParquet,
    Las,
    Laz,
    Copc,
    Zarr,
    NetCdf,
    PmTiles,
    Spz,
    Unknown,
}

impl GeoFormat {
    pub fn from_extension(ext: &str) -> GeoFormat {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "tif" | "tiff" | "geotiff" => GeoFormat::GeoTiff,
            "cog" => GeoFormat::Cog,
            "gpkg" => GeoFormat::GeoPackage,
            "geojson" | "json" => GeoFormat::GeoJson,
            "fgb" => GeoFormat::FlatGeobuf,
            "parquet" => GeoFormat::GeoParquet,
            "las" => GeoFormat::Las,
            "laz" => GeoFormat::Laz,
            "copc" => GeoFormat::Copc,
            "zarr" => GeoFormat::Zarr,
            "nc" | "netcdf" => GeoFormat::NetCdf,
            "pmtiles" => GeoFormat::PmTiles,
            "spz" => GeoFormat::Spz,
            _ => GeoFormat::Unknown,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            GeoFormat::GeoTiff => "geotiff",
            GeoFormat::Cog => "cog",
            GeoFormat::GeoPackage => "geopackage",
            GeoFormat::GeoJson => "geojson",
            GeoFormat::FlatGeobuf => "flatgeobuf",
            GeoFormat::GeoParquet => "geoparquet",
            GeoFormat::Las => "las",
            GeoFormat::Laz => "laz",
            GeoFormat::Copc => "copc",
            GeoFormat::Zarr => "zarr",
            GeoFormat::NetCdf => "netcdf",
            GeoFormat::PmTiles => "pmtiles",
            GeoFormat::Spz => "spz",
            GeoFormat::Unknown => "unknown",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GeoFormat::GeoTiff => "GeoTIFF",
            GeoFormat::Cog => "Cloud Optimized GeoTIFF",
            GeoFormat::GeoPackage => "GeoPackage",
            GeoFormat::GeoJson => "GeoJSON",
            GeoFormat::FlatGeobuf => "FlatGeobuf",
            GeoFormat::GeoParquet => "GeoParquet",
            GeoFormat::Las => "LAS",
            GeoFormat::Laz => "LAZ",
            GeoFormat::Copc => "COPC",
            GeoFormat::Zarr => "Zarr",
            GeoFormat::NetCdf => "NetCDF",
            GeoFormat::PmTiles => "PMTiles",
            GeoFormat::Spz => "SPZ",
            GeoFormat::Unknown => "Unknown",
        }
    }
}

/// Lightweight CRS label (full PROJ transforms land with registration).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CrsRef {
    pub epsg: Option<u32>,
    pub proj: Option<String>,
    pub vertical_epsg: Option<u32>,
}

/// Axis-aligned geographic / projected bounds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GeoBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl GeoBounds {
    pub fn as_array(self) -> [f64; 4] {
        [self.min_x, self.min_y, self.max_x, self.max_y]
    }

    pub fn from_array(a: [f64; 4]) -> Self {
        Self {
            min_x: a[0],
            min_y: a[1],
            max_x: a[2],
            max_y: a[3],
        }
    }
}
