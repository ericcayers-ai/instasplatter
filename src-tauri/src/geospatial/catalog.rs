//! Managed data connectors and local catalog (stub).
//!
//! Later: USGS 3DEP, Copernicus, HydroSHEDS, STAC/Sentinel, gauges, attribution.

use crate::geospatial::data::{GeoBounds, GeoFormat};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub format: GeoFormat,
    pub license: Option<String>,
    pub bounds: Option<GeoBounds>,
    pub resolution_m: Option<f64>,
    pub url: Option<String>,
    pub stale: bool,
}

/// Built-in connector names shown in the UI before download workers exist.
pub fn connector_names() -> Vec<&'static str> {
    vec![
        "USGS 3DEP",
        "Copernicus DEM",
        "HydroSHEDS",
        "Earth Search STAC",
        "Overture / OSM",
        "USGS gauges",
        "NOAA MRMS / NWM",
        "ECMWF Open Data",
    ]
}

/// List cached catalog entries for a workspace (empty until connectors land).
pub fn list_cached(_workspace_geo_sources: &str) -> Vec<CatalogEntry> {
    Vec::new()
}

/// Fetch a remote asset into the local hash cache (stub).
pub fn fetch_asset(_entry_id: &str, _dest_dir: &str) -> Result<CatalogEntry, String> {
    Err("Data connectors are not implemented yet.".into())
}
