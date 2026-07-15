//! Hydrodynamic prep and solver routing (stub).
//!
//! Standard path will use ANUGA + SWMM; live preview is WebGPU. Phase 1 only
//! defines job descriptors so the queue can tag geospatial work.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HydroEngine {
    Anuga,
    Swmm,
    WebGpuPreview,
    Experimental,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HydroJobSpec {
    pub scenario_id: String,
    pub engine: Option<HydroEngine>,
    pub preview: bool,
}

/// Validate that a scenario is ready for scientific mode (stub).
pub fn validate_for_scientific(_spec: &HydroJobSpec) -> Result<(), String> {
    Err("Flood solvers are not available in this build phase.".into())
}

/// Queue a preview or scientific run (stub — never starts a worker yet).
pub fn enqueue_hydro_job(_spec: HydroJobSpec) -> Result<String, String> {
    Err("Hydrodynamic jobs are not implemented yet.".into())
}
