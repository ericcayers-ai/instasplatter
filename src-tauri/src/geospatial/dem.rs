//! DEM conditioning and terrain products (stub).
//!
//! Later: DTM/DSM derivation, flow paths via pyflwdir/Landlab workers, tile LOD.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DemProduct {
    pub dtm_path: Option<String>,
    pub dsm_path: Option<String>,
    pub orthomosaic_path: Option<String>,
    pub cell_size_m: Option<f64>,
    pub crs: Option<String>,
}

/// Condition a source DEM into flood-ready DTM products (stub).
pub fn condition_dem(source: &Path, _dest_dir: &Path) -> Result<DemProduct, String> {
    if !source.exists() {
        return Err(format!("DEM source not found: {}", source.display()));
    }
    Err("DEM conditioning is not implemented yet.".into())
}
