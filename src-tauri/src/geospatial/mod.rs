//! Geospatial suite core (formats, catalog, DEM, hydro, registration).
//!
//! Scientific flood runs live in `hydro` (ANUGA/SWMM). Live WebGPU/CPU preview
//! lives in `preview` (checkpoint protocol) plus the frontend
//! `src/geospatial/preview` module; both share `events::SimEvent` / `sim://event`.

#![allow(dead_code)]

pub mod catalog;
pub mod data;
pub mod dem;
pub mod events;
pub mod exports;
pub mod hydro;
pub mod preview;
pub mod registration;
pub mod transforms;

use std::path::Path;

/// Ensure standard geo artifact directories exist under a project workspace.
pub fn prepare_workspace(workspace: &Path) -> Result<(), String> {
    crate::project::ensure_geo_workspace(workspace)
}
