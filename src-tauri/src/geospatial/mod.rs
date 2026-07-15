//! Geospatial suite core (formats, catalog, DEM/hydro stubs, registration).
//!
//! Phase 1 scaffolds compileable modules. Drone georegistration, flood solvers,
//! and MapLibre viewport land in later phases.

#![allow(dead_code)]

pub mod catalog;
pub mod data;
pub mod dem;
pub mod events;
pub mod exports;
pub mod hydro;
pub mod registration;

use std::path::Path;

/// Ensure standard geo artifact directories exist under a project workspace.
pub fn prepare_workspace(workspace: &Path) -> Result<(), String> {
    crate::project::ensure_geo_workspace(workspace)
}
