//! Scientific flood orchestration (ANUGA + SWMM).
//!
//! Standard path: prepare DEM → adaptive mesh plan → ANUGA sidecar (CPU lane) →
//! stream `sim://` / `geo://` checkpoints → persist `SimulationRun`.
//! When ANUGA is missing, returns a clear engine-missing error unless
//! `allow_demo` is set (labelled synthetic extents for UI continuity).

use crate::geospatial::dem::{self, DemProduct};
use crate::geospatial::events::{
    GeoEvent, SimEvent, GEO_EVENT_CHANNEL, SIM_EVENT_CHANNEL,
};
use crate::geospatial::registration::{plan_extent, ExtentPlan, ExtentPlanInput};
use crate::project::{FloodScenario, Project, SimulationRun};
use crate::settings::app_data_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HydroEngine {
    Anuga,
    Swmm,
    WebGpuPreview,
    /// Legacy catch-all — prefer specific experimental ids below.
    Experimental,
    Triton,
    Wflow,
    GeoClaw,
    Sfincs,
    Hipims,
    BgFlood,
    Itzi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HydroEngineTier {
    Standard,
    Preview,
    Experimental,
    /// GPL / restrictive — external install only, never shipped.
    ExternalPlugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HydroInstallKind {
    BundledWorker,
    ExternalPermissive,
    ExternalGplPlugin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HydroEngineDescriptor {
    pub id: HydroEngine,
    pub label: String,
    pub tier: HydroEngineTier,
    pub license: String,
    pub install_kind: HydroInstallKind,
    pub install_folder: String,
    pub bundled: bool,
    pub notes: String,
}

/// Checklist required before promoting an experimental hydro engine to Standard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HydroPromotionGates {
    pub lake_at_rest: bool,
    pub wet_dry_analytical: bool,
    pub dam_break_analytical: bool,
    pub rainfall_infiltration: bool,
    pub mesh_convergence: bool,
    pub mass_conservation: bool,
    pub urban_obstacles: bool,
    pub calibrated_historical: bool,
    pub anuga_cross_comparison: bool,
    pub cpu_gpu_tolerance: bool,
    pub reproducibility_hash: bool,
    pub license_cleared_for_standard: bool,
}

impl HydroPromotionGates {
    pub fn all_clear(&self) -> bool {
        self.lake_at_rest
            && self.wet_dry_analytical
            && self.dam_break_analytical
            && self.rainfall_infiltration
            && self.mesh_convergence
            && self.mass_conservation
            && self.urban_obstacles
            && self.calibrated_historical
            && self.anuga_cross_comparison
            && self.cpu_gpu_tolerance
            && self.reproducibility_hash
            && self.license_cleared_for_standard
    }

    pub fn failing(&self) -> Vec<&'static str> {
        let checks: [(&'static str, bool); 12] = [
            ("lake_at_rest", self.lake_at_rest),
            ("wet_dry_analytical", self.wet_dry_analytical),
            ("dam_break_analytical", self.dam_break_analytical),
            ("rainfall_infiltration", self.rainfall_infiltration),
            ("mesh_convergence", self.mesh_convergence),
            ("mass_conservation", self.mass_conservation),
            ("urban_obstacles", self.urban_obstacles),
            ("calibrated_historical", self.calibrated_historical),
            ("anuga_cross_comparison", self.anuga_cross_comparison),
            ("cpu_gpu_tolerance", self.cpu_gpu_tolerance),
            ("reproducibility_hash", self.reproducibility_hash),
            (
                "license_cleared_for_standard",
                self.license_cleared_for_standard,
            ),
        ];
        checks
            .into_iter()
            .filter_map(|(n, ok)| if ok { None } else { Some(n) })
            .collect()
    }
}

/// Full hydro engine registry (ANUGA/SWMM wired; experimental/GPL are adapters).
pub fn engine_registry() -> Vec<HydroEngineDescriptor> {
    vec![
        HydroEngineDescriptor {
            id: HydroEngine::Anuga,
            label: "ANUGA".into(),
            tier: HydroEngineTier::Standard,
            license: "Apache-2.0".into(),
            install_kind: HydroInstallKind::BundledWorker,
            install_folder: "anuga".into(),
            bundled: false,
            notes: "Authoritative 2D shallow-water scientific solver.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngine::Swmm,
            label: "EPA SWMM".into(),
            tier: HydroEngineTier::Standard,
            license: "Public Domain".into(),
            install_kind: HydroInstallKind::BundledWorker,
            install_folder: "swmm".into(),
            bundled: false,
            notes: "Urban drainage / network exchange.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngine::WebGpuPreview,
            label: "WebGPU live preview".into(),
            tier: HydroEngineTier::Preview,
            license: "Apache-2.0".into(),
            install_kind: HydroInstallKind::BundledWorker,
            install_folder: "webgpu-preview".into(),
            bundled: false,
            notes: "Display-rate interpolated preview; not authoritative.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngine::Triton,
            label: "TRITON / Kokkos".into(),
            tier: HydroEngineTier::Experimental,
            license: "BSD-style (verify)".into(),
            install_kind: HydroInstallKind::ExternalPermissive,
            install_folder: "triton".into(),
            bundled: false,
            notes: "Accelerated permissive rainfall / overland flow. Not bundled.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngine::Wflow,
            label: "Wflow.jl".into(),
            tier: HydroEngineTier::Experimental,
            license: "MIT (verify)".into(),
            install_kind: HydroInstallKind::ExternalPermissive,
            install_folder: "wflow".into(),
            bundled: false,
            notes: "Watershed / runoff workflows. External Julia install.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngine::GeoClaw,
            label: "GeoClaw".into(),
            tier: HydroEngineTier::Experimental,
            license: "BSD-3".into(),
            install_kind: HydroInstallKind::ExternalPermissive,
            install_folder: "geoclaw".into(),
            bundled: false,
            notes: "Coastal / surge specialization. External install.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngine::Sfincs,
            label: "SFINCS".into(),
            tier: HydroEngineTier::ExternalPlugin,
            license: "GPL".into(),
            install_kind: HydroInstallKind::ExternalGplPlugin,
            install_folder: "sfincs".into(),
            bundled: false,
            notes: "External GPL plugin — never ship in Apache installer.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngine::Hipims,
            label: "HiPIMS".into(),
            tier: HydroEngineTier::ExternalPlugin,
            license: "GPL".into(),
            install_kind: HydroInstallKind::ExternalGplPlugin,
            install_folder: "hipims".into(),
            bundled: false,
            notes: "External GPL plugin — never ship in Apache installer.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngine::BgFlood,
            label: "BG_Flood".into(),
            tier: HydroEngineTier::ExternalPlugin,
            license: "GPL".into(),
            install_kind: HydroInstallKind::ExternalGplPlugin,
            install_folder: "bg-flood".into(),
            bundled: false,
            notes: "External GPL plugin — never ship in Apache installer.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngine::Itzi,
            label: "Itzï".into(),
            tier: HydroEngineTier::ExternalPlugin,
            license: "GPL".into(),
            install_kind: HydroInstallKind::ExternalGplPlugin,
            install_folder: "itzi".into(),
            bundled: false,
            notes: "External GPL plugin — never ship in Apache installer.".into(),
        },
    ]
}

pub fn engine_descriptor(id: HydroEngine) -> Option<HydroEngineDescriptor> {
    engine_registry().into_iter().find(|d| d.id == id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalHydroInstallProtocol {
    pub engine: HydroEngine,
    pub expected_folder: String,
    pub accepted_marker: String,
    pub entrypoint: String,
    pub refuse_if_stub: bool,
    pub refuse_if_bundled_request: bool,
    pub instructions: String,
}

pub fn external_plugin_protocol(id: HydroEngine) -> Option<ExternalHydroInstallProtocol> {
    let d = engine_descriptor(id)?;
    if !matches!(
        d.install_kind,
        HydroInstallKind::ExternalGplPlugin | HydroInstallKind::ExternalPermissive
    ) {
        return None;
    }
    let gpl = matches!(d.install_kind, HydroInstallKind::ExternalGplPlugin);
    Some(ExternalHydroInstallProtocol {
        engine: id,
        expected_folder: format!("engines/hydro/{}", d.install_folder),
        accepted_marker: if gpl {
            "GPL_ACCEPTED".into()
        } else {
            "ACCEPTED".into()
        },
        entrypoint: "run".into(),
        refuse_if_stub: true,
        refuse_if_bundled_request: gpl,
        instructions: format!(
            "Install {} yourself under %LOCALAPPDATA%/InstaSplatter/engines/hydro/{}. \
             Place a {} marker after reviewing the license. \
             {} InstaSplatter never bundles this binary.",
            d.label,
            d.install_folder,
            if gpl { "GPL_ACCEPTED" } else { "ACCEPTED" },
            if gpl {
                "GPL engines are external plugins only."
            } else {
                "Experimental permissive engine."
            }
        ),
    })
}

pub fn refuse_gpl_bundle(id: HydroEngine) -> bool {
    engine_descriptor(id)
        .map(|d| matches!(d.install_kind, HydroInstallKind::ExternalGplPlugin))
        .unwrap_or(false)
}

/// Attempt to promote an experimental engine — requires full gate checklist.
pub fn try_promote_to_standard(
    id: HydroEngine,
    gates: &HydroPromotionGates,
) -> Result<(), String> {
    let d = engine_descriptor(id).ok_or_else(|| format!("Unknown hydro engine: {id:?}"))?;
    if matches!(d.install_kind, HydroInstallKind::ExternalGplPlugin) {
        return Err(format!(
            "{} is GPL — cannot promote into the Apache Standard installer.",
            d.label
        ));
    }
    if !matches!(d.tier, HydroEngineTier::Experimental) {
        return Err(format!(
            "{} is not an experimental hydro adapter ({:?}).",
            d.label, d.tier
        ));
    }
    if !gates.all_clear() {
        return Err(format!(
            "Promotion gates incomplete for {}: missing {:?}.",
            d.label,
            gates.failing()
        ));
    }
    Err(format!(
        "{} passed checklist scaffolding, but Standard promotion is not wired in this build.",
        d.label
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HydroJobSpec {
    pub workspace: String,
    pub scenario_id: String,
    pub engine: Option<HydroEngine>,
    pub preview: bool,
    /// When true (default for UI), allow labelled demo extents if ANUGA is missing.
    pub allow_demo: bool,
    pub dem_path: Option<String>,
    pub extent: Option<ExtentPlanInput>,
    pub ensemble: Option<EnsembleSpec>,
    pub enable_swmm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EnsembleSpec {
    /// Number of realizations (first cut runs 1).
    pub realizations: u32,
    pub rainfall_scale_range: Option<[f64; 2]>,
    pub roughness_scale_range: Option<[f64; 2]>,
    pub infiltration_scale_range: Option<[f64; 2]>,
    pub boundary_scale_range: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CalibrationTarget {
    pub gauge_id: Option<String>,
    pub observed_series_path: Option<String>,
    pub historical_extent_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MeshDensityPlan {
    pub site_max_area_m2: f64,
    pub regional_max_area_m2: f64,
    pub dem_resolution_m: f64,
    pub preview_cell_m: f64,
    pub bounds_enu: [f64; 4],
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FloodRunStatus {
    pub run_id: String,
    pub scenario_id: String,
    pub workspace: String,
    pub state: String,
    pub progress: f32,
    pub detail: String,
    pub mode: Option<String>,
    pub engine: Option<String>,
    pub engine_version: Option<String>,
    pub result_paths: Vec<String>,
    pub mass_balance: Option<f64>,
    pub label: Option<String>,
    pub created_unix: u64,
}

struct ActiveFloodRun {
    cancel: Arc<AtomicBool>,
    status: FloodRunStatus,
}

fn active_runs() -> &'static Mutex<HashMap<String, ActiveFloodRun>> {
    static RUNS: OnceLock<Mutex<HashMap<String, ActiveFloodRun>>> = OnceLock::new();
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Resolve anuga / swmm launcher: app engines tree first, then repo tools/.
pub fn resolve_geo_sidecar(name: &str) -> Option<PathBuf> {
    let candidates = [
        app_data_dir().join("engines").join("sidecars").join(name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tools")
            .join("sidecars")
            .join(name),
    ];
    for dir in &candidates {
        #[cfg(windows)]
        {
            let bat = dir.join("run.bat");
            if bat.exists() {
                return Some(bat);
            }
            let py = dir.join("run.py");
            if py.exists() {
                return Some(py);
            }
        }
        #[cfg(not(windows))]
        {
            let sh = dir.join("run.sh");
            if sh.exists() {
                return Some(sh);
            }
            let py = dir.join("run.py");
            if py.exists() {
                return Some(py);
            }
        }
    }
    None
}

pub fn anuga_installed() -> bool {
    resolve_geo_sidecar("anuga").is_some()
}

/// Map extent planner output → scientific mesh density knobs.
pub fn mesh_plan_from_extent(plan: &ExtentPlan) -> MeshDensityPlan {
    MeshDensityPlan {
        site_max_area_m2: plan.scientific_mesh_max_area_m2,
        regional_max_area_m2: plan.regional_mesh_max_area_m2,
        dem_resolution_m: plan.dem_resolution_m,
        preview_cell_m: plan.preview_cell_m,
        bounds_enu: plan.bounds_enu,
        notes: plan.notes.clone(),
    }
}

/// Validate scenario readiness for scientific mode (metric scale preferred).
pub fn validate_for_scientific(
    project: &Project,
    scenario: &FloodScenario,
    allow_demo: bool,
) -> Result<(), String> {
    if scenario.id.is_empty() {
        return Err("Scenario id is empty.".into());
    }
    let scale = project
        .geo_reference
        .as_ref()
        .and_then(|g| g.scale_status.as_deref())
        .unwrap_or("unscaled");
    if scale == "unscaled" && !allow_demo {
        return Err(
            "Project is unscaled — scientific flood needs metric GeoReference (or allow demo mode)."
                .into(),
        );
    }
    if !anuga_installed() && !allow_demo {
        return Err(
            "ANUGA sidecar not found. Install under engines/sidecars/anuga or enable demo mode."
                .into(),
        );
    }
    Ok(())
}

fn default_scenario(id: &str) -> FloodScenario {
    // Flood lab defaults — labelled draft / non-authoritative until calibrated.
    FloodScenario {
        id: id.to_string(),
        name: "Flood lab — site rain".into(),
        terrain_layer_id: None,
        rainfall: Some(serde_json::json!({
            "template": "chicago_6h",
            "rateMmPerHour": 25.0,
            "hyetograph": [
                { "hours": 0.0, "mmPerHour": 2.0 },
                { "hours": 1.0, "mmPerHour": 8.0 },
                { "hours": 2.0, "mmPerHour": 28.0 },
                { "hours": 3.0, "mmPerHour": 42.0 },
                { "hours": 4.0, "mmPerHour": 22.0 },
                { "hours": 5.0, "mmPerHour": 10.0 },
                { "hours": 6.0, "mmPerHour": 4.0 },
                { "hours": 12.0, "mmPerHour": 1.0 }
            ],
            "authority": "draft-template"
        })),
        inflows: Some(serde_json::json!({
            "northEdgeCms": 12.0,
            "note": "Distributed north-edge inflow proxy for Flood lab"
        })),
        infiltration: Some(serde_json::json!({
            "rateMmPerHour": 2.0,
            "method": "constant"
        })),
        roughness: Some(serde_json::json!({
            "preset": "mixed_urban",
            "manningN": 0.035,
            "presets": {
                "channel": 0.03,
                "floodplain": 0.045,
                "mixed_urban": 0.035,
                "forest": 0.08
            }
        })),
        structures: None,
        drains: None,
        boundary_conditions: Some(serde_json::json!({
            "outlet": {
                "type": "stage",
                "stageM": 0.4,
                "edge": "south",
                "note": "Open outlet stage BC — draft Flood lab default"
            },
            "walls": "reflective"
        })),
        solver_settings: Some(serde_json::json!({
            "cfl": 0.9,
            "durationHours": 12.0,
            "handStageM": 1.5,
            "previewPath": "soft+hand",
            "authority": "draft"
        })),
        validation_state: Some("draft".into()),
        aoi_wgs84: None,
    }
}

/// Convert a WGS84 AOI box to approximate local ENU metres about its centre.
pub fn aoi_wgs84_to_enu_box(aoi: [f64; 4]) -> ([f64; 3], [f64; 4]) {
    let [west, south, east, north] = aoi;
    let lon0 = 0.5 * (west + east);
    let lat0 = 0.5 * (south + north);
    let m_per_deg_lat = 111_320.0;
    let m_per_deg_lon = 111_320.0 * (lat0 * std::f64::consts::PI / 180.0).cos();
    let min_e = (west - lon0) * m_per_deg_lon;
    let max_e = (east - lon0) * m_per_deg_lon;
    let min_n = (south - lat0) * m_per_deg_lat;
    let max_n = (north - lat0) * m_per_deg_lat;
    ([lon0, lat0, 0.0], [min_e, min_n, max_e, max_n])
}

/// Optional splat AABB in ENU metres from the project's latest PLY (+ manual TRS).
/// Used so extent planning can tighten regional mesh around the reconstruction footprint.
pub fn splat_bounds_enu_from_project(project: &Project) -> Option<[f64; 6]> {
    let path = project.latest_splat.as_ref()?;
    let cloud = crate::splat::ply::read_ply(Path::new(path)).ok()?;
    let mut b = cloud.axis_aligned_bounds()?;
    if let Some(tf) = project.model_transform.as_ref() {
        let sx = tf.scale[0].abs().max(1e-4) as f64;
        let sy = tf.scale[1].abs().max(1e-4) as f64;
        let sz = tf.scale[2].abs().max(1e-4) as f64;
        let tx = tf.translation[0] as f64;
        let ty = tf.translation[1] as f64;
        let tz = tf.translation[2] as f64;
        // Conservative AABB under anisotropic scale + translation (rotation ignored).
        let cx = 0.5 * (b[0] + b[3]);
        let cy = 0.5 * (b[1] + b[4]);
        let cz = 0.5 * (b[2] + b[5]);
        let hx = 0.5 * (b[3] - b[0]) * sx;
        let hy = 0.5 * (b[4] - b[1]) * sy;
        let hz = 0.5 * (b[5] - b[2]) * sz;
        b = [
            cx - hx + tx,
            cy - hy + ty,
            cz - hz + tz,
            cx + hx + tx,
            cy + hy + ty,
            cz + hz + tz,
        ];
    }
    Some(b)
}

/// Persist AOI on a flood scenario and return an extent plan for soft/scientific solvers.
pub fn commit_flood_aoi(
    workspace: &Path,
    scenario_id: &str,
    aoi_wgs84: [f64; 4],
) -> Result<(FloodScenario, ExtentPlan, Option<crate::project::GeoReference>), String> {
    let [west, south, east, north] = aoi_wgs84;
    if !(west < east && south < north) {
        return Err("AOI must have west < east and south < north".into());
    }
    if (east - west) < 1e-5 || (north - south) < 1e-5 {
        return Err("AOI is too small".into());
    }

    let mut project = Project::load(workspace)?;
    crate::geospatial::prepare_workspace(workspace)?;
    let mut scenario = ensure_scenario(&mut project, scenario_id);
    scenario.aoi_wgs84 = Some(aoi_wgs84);
    if let Some(slot) = project
        .flood_scenarios
        .iter_mut()
        .find(|s| s.id == scenario_id)
    {
        *slot = scenario.clone();
    }

    let (origin, dem_bounds_enu) = aoi_wgs84_to_enu_box(aoi_wgs84);
    let splat_bounds_enu = splat_bounds_enu_from_project(&project);
    // Seed / refresh geo origin at AOI centre so scientific + preview share a frame.
    let geo = crate::geospatial::registration::compute_geo_reference(workspace, Some(origin))?;
    let extent_input = ExtentPlanInput {
        camera_enu: Vec::new(),
        splat_bounds_enu,
        dem_bounds_enu: Some(dem_bounds_enu),
        dem_accuracy_m: Some(2.0),
        preview_budget_cells: Some(1024),
        enu_origin: Some(origin),
        geo_reference: Some(geo.geo_reference.clone()),
    };
    let plan = plan_extent(&extent_input);
    project.geo_reference = Some(geo.geo_reference.clone());
    project.save()?;
    Ok((scenario, plan, Some(geo.geo_reference)))
}

fn ensure_scenario(project: &mut Project, scenario_id: &str) -> FloodScenario {
    if let Some(s) = project
        .flood_scenarios
        .iter()
        .find(|s| s.id == scenario_id)
    {
        return s.clone();
    }
    let s = default_scenario(scenario_id);
    project.flood_scenarios.push(s.clone());
    s
}

fn duration_hours(scenario: &FloodScenario) -> f64 {
    scenario
        .solver_settings
        .as_ref()
        .and_then(|v| v.get("durationHours"))
        .and_then(|v| v.as_f64())
        .unwrap_or(12.0)
}

fn ensemble_scales(spec: &EnsembleSpec, index: u32) -> (f64, f64, f64) {
    let n = spec.realizations.max(1);
    let t = if n <= 1 {
        0.5
    } else {
        index as f64 / (n - 1) as f64
    };
    let lerp = |range: Option<[f64; 2]>, default: f64| match range {
        Some([a, b]) => a + (b - a) * t,
        None => default,
    };
    (
        lerp(spec.rainfall_scale_range, 1.0),
        lerp(spec.roughness_scale_range, 1.0),
        lerp(spec.infiltration_scale_range, 1.0),
    )
}

/// List in-memory + persisted flood run statuses for a workspace (optional filter).
pub fn list_run_status(workspace: Option<&str>) -> Vec<FloodRunStatus> {
    let mut out: Vec<FloodRunStatus> = active_runs()
        .lock()
        .unwrap()
        .values()
        .map(|r| r.status.clone())
        .collect();

    if let Some(ws) = workspace {
        out.retain(|s| s.workspace == ws);
        if let Ok(proj) = Project::load(Path::new(ws)) {
            for run in &proj.simulation_runs {
                if out.iter().any(|s| s.run_id == run.id) {
                    continue;
                }
                out.push(FloodRunStatus {
                    run_id: run.id.clone(),
                    scenario_id: run.scenario_id.clone(),
                    workspace: ws.to_string(),
                    state: run
                        .status
                        .clone()
                        .unwrap_or_else(|| "done".into()),
                    progress: if run.status.as_deref() == Some("failed") {
                        0.0
                    } else {
                        1.0
                    },
                    detail: run
                        .mode
                        .clone()
                        .unwrap_or_else(|| "persisted".into()),
                    mode: run.mode.clone(),
                    engine: run.engine.clone(),
                    engine_version: run.engine_version.clone(),
                    result_paths: run.result_paths.clone(),
                    mass_balance: run.mass_balance,
                    label: None,
                    created_unix: run.created_unix,
                });
            }
        }
    }
    out.sort_by(|a, b| b.created_unix.cmp(&a.created_unix));
    out
}

pub fn cancel_run(run_id: &str) -> Result<(), String> {
    let guard = active_runs().lock().unwrap();
    let Some(run) = guard.get(run_id) else {
        return Err(format!("No active flood run {run_id}"));
    };
    run.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

fn emit_geo(app: &AppHandle, event: GeoEvent) {
    let _ = app.emit(GEO_EVENT_CHANNEL, event);
}

fn emit_sim(app: &AppHandle, event: SimEvent) {
    let _ = app.emit(SIM_EVENT_CHANNEL, event);
}

fn update_active(run_id: &str, f: impl FnOnce(&mut FloodRunStatus)) {
    if let Some(run) = active_runs().lock().unwrap().get_mut(run_id) {
        f(&mut run.status);
    }
}

fn persist_run(workspace: &Path, run: &SimulationRun) -> Result<(), String> {
    let mut proj = Project::load(workspace)?;
    if let Some(existing) = proj.simulation_runs.iter_mut().find(|r| r.id == run.id) {
        *existing = run.clone();
    } else {
        proj.simulation_runs.push(run.clone());
    }
    proj.touch();
    proj.save()
}

/// Intrinsic demo when no Python sidecar is available at all.
fn run_intrinsic_demo(
    app: &AppHandle,
    cancel: &AtomicBool,
    run_id: &str,
    out_dir: &Path,
    mesh: &MeshDensityPlan,
    duration_h: f64,
) -> Result<(Vec<String>, f64), String> {
    fs::create_dir_all(out_dir.join("checkpoints")).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let n = 12u32;
    for i in 0..=n {
        if cancel.load(Ordering::SeqCst) {
            return Err("__cancelled__".into());
        }
        let u = i as f64 / n as f64;
        let t_h = duration_h * u;
        let wet = if u < 0.45 {
            (u / 0.45 * std::f64::consts::FRAC_PI_2).sin()
        } else {
            (-(u - 0.45) / 0.4).exp()
        }
        .clamp(0.05, 0.95);
        let ck = out_dir.join("checkpoints").join(format!("t{i:05}.geojson"));
        let [min_e, min_n, max_e, max_n] = mesh.bounds_enu;
        let cx = 0.5 * (min_e + max_e);
        let cy = 0.5 * (min_n + max_n);
        let rx = 0.5 * (max_e - min_e) * (0.15 + 0.75 * wet);
        let ry = 0.5 * (max_n - min_n) * (0.12 + 0.7 * wet);
        let mut ring = Vec::new();
        for k in 0..=32 {
            let a = (k as f64 / 32.0) * std::f64::consts::TAU;
            let wobble = 1.0 + 0.07 * (a * 3.0 + t_h).sin();
            ring.push(serde_json::json!([
                cx + a.cos() * rx * wobble,
                cy + a.sin() * ry * wobble
            ]));
        }
        let fc = serde_json::json!({
            "type": "FeatureCollection",
            "features": [{
                "type": "Feature",
                "properties": {
                    "simTimeHours": t_h,
                    "wetFraction": wet,
                    "maxDepthM": 0.2 + wet * 2.2,
                    "mode": "demo"
                },
                "geometry": { "type": "Polygon", "coordinates": [ring] }
            }]
        });
        fs::write(&ck, serde_json::to_string_pretty(&fc).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let progress = u as f32;
        update_active(run_id, |s| {
            s.progress = progress;
            s.detail = format!("demo t={t_h:.1} h (no ANUGA sidecar)");
            s.mode = Some("demo".into());
        });
        emit_geo(
            app,
            GeoEvent::RunProgress {
                run_id: run_id.to_string(),
                progress,
                detail: format!("demo t={t_h:.1} h"),
            },
        );
        emit_sim(
            app,
            SimEvent::Checkpoint {
                run_id: run_id.to_string(),
                progress,
                sim_time_hours: t_h,
                checkpoint_path: Some(ck.to_string_lossy().into_owned()),
                detail: format!("demo t={t_h:.1} h"),
                mode: "demo".into(),
                max_depth_m: Some((0.2 + wet * 2.2) as f32),
                wet_fraction: Some(wet as f32),
                mass_m3: None,
            },
        );
        std::thread::sleep(std::time::Duration::from_millis(40));
    }

    let hydro = out_dir.join("hydrograph.json");
    let mut samples = Vec::new();
    for i in 0..=12 {
        let hours = duration_h * (i as f64 / 12.0);
        let u = i as f64 / 12.0;
        let env = if u < 0.4 {
            (u / 0.4 * std::f64::consts::FRAC_PI_2).sin()
        } else {
            (-(u - 0.4) / 0.35).exp()
        };
        samples.push(serde_json::json!({
            "hours": hours,
            "stageM": 0.3 + 1.8 * env,
            "dischargeCms": 5.0 + 80.0 * env
        }));
    }
    fs::write(
        &hydro,
        serde_json::to_string_pretty(&serde_json::json!({ "samples": samples }))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    results.push(hydro.to_string_lossy().into_owned());
    emit_sim(
        app,
        SimEvent::Hydrograph {
            run_id: run_id.to_string(),
            path: hydro.to_string_lossy().into_owned(),
        },
    );

    let manifest = out_dir.join("manifest.json");
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&serde_json::json!({
            "mode": "demo",
            "label": "Demo mode — ANUGA sidecar missing; extents are synthetic",
            "engine": "intrinsic-demo"
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    results.push(manifest.to_string_lossy().into_owned());
    Ok((results, 0.02))
}

fn invoke_anuga_sidecar(
    app: &AppHandle,
    cancel: &AtomicBool,
    launch: &Path,
    request: &serde_json::Value,
    run_id: &str,
) -> Result<(Vec<String>, f64, String, Option<String>), String> {
    let body = serde_json::to_string(request).map_err(|e| e.to_string())?;

    let mut cmd = if launch.extension().and_then(|e| e.to_str()) == Some("py") {
        let mut c = Command::new("python");
        c.arg(launch);
        c
    } else {
        Command::new(launch)
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(launch.parent().unwrap_or(Path::new(".")));

    let mut child = cmd.spawn().map_err(|e| format!("Failed to start ANUGA sidecar: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body.as_bytes())
            .map_err(|e| format!("Failed to write ANUGA request: {e}"))?;
    }

    let stdout = child.stdout.take().ok_or("ANUGA sidecar stdout missing")?;
    let reader = BufReader::new(stdout);
    let mut result_paths = Vec::new();
    let mut mass_balance = 0.0_f64;
    let mut mode = "scaffold".to_string();
    let mut engine_version: Option<String> = None;
    let mut label: Option<String> = None;
    let mut done = false;

    for line in reader.lines() {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            return Err("__cancelled__".into());
        }
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let kind = msg.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        match kind {
            "progress" => {
                let progress = msg.get("progress").and_then(|p| p.as_f64()).unwrap_or(0.0) as f32;
                let detail = msg
                    .get("detail")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                let sim_t = msg
                    .get("simTimeHours")
                    .and_then(|t| t.as_f64())
                    .unwrap_or(0.0);
                let ck = msg
                    .get("checkpoint")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string());
                let m = msg
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("scaffold")
                    .to_string();
                mode = m.clone();
                update_active(run_id, |s| {
                    s.progress = progress;
                    s.detail = detail.clone();
                    s.mode = Some(m.clone());
                });
                emit_geo(
                    app,
                    GeoEvent::RunProgress {
                        run_id: run_id.to_string(),
                        progress,
                        detail: detail.clone(),
                    },
                );
                emit_sim(
                    app,
                    SimEvent::Checkpoint {
                        run_id: run_id.to_string(),
                        progress,
                        sim_time_hours: sim_t,
                        checkpoint_path: ck,
                        detail,
                        mode: m,
                        max_depth_m: None,
                        wet_fraction: None,
                        mass_m3: None,
                    },
                );
            }
            "done" => {
                done = true;
                mode = msg
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or(&mode)
                    .to_string();
                mass_balance = msg
                    .get("massBalance")
                    .and_then(|m| m.as_f64())
                    .unwrap_or(0.0);
                engine_version = msg
                    .get("engineVersion")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                label = msg
                    .get("label")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(arr) = msg.get("resultPaths").and_then(|p| p.as_array()) {
                    for p in arr {
                        if let Some(s) = p.as_str() {
                            result_paths.push(s.to_string());
                        }
                    }
                }
            }
            "error" => {
                let message = msg
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("ANUGA error")
                    .to_string();
                return Err(message);
            }
            _ => {}
        }
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    if cancel.load(Ordering::SeqCst) {
        return Err("__cancelled__".into());
    }
    if !status.success() && !done {
        let code = status.code().unwrap_or(-1);
        if code == 2 {
            return Err("__engine_missing__".into());
        }
        return Err(format!("ANUGA sidecar exited with code {code}"));
    }
    Ok((result_paths, mass_balance, mode, engine_version.or(label)))
}

fn maybe_run_swmm(
    run_id: &str,
    out_dir: &Path,
    scenario: &FloodScenario,
    demo: bool,
) -> Vec<String> {
    let Some(launch) = resolve_geo_sidecar("swmm") else {
        return Vec::new();
    };
    let swmm_out = out_dir.join("swmm");
    let _ = fs::create_dir_all(&swmm_out);
    let network = scenario
        .drains
        .as_ref()
        .and_then(|d| d.get("networkPath"))
        .and_then(|p| p.as_str())
        .map(|s| s.to_string());
    let req = serde_json::json!({
        "schemaVersion": 1,
        "runId": run_id,
        "outputDir": swmm_out.to_string_lossy(),
        "networkPath": network,
        "surfaceExchange": {
            "fromAnuga": out_dir.join("checkpoints").to_string_lossy(),
            "toAnuga": swmm_out.join("outfalls.json").to_string_lossy()
        },
        "durationHours": duration_hours(scenario),
        "demoMode": demo
    });
    let body = match serde_json::to_string(&req) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };
    let mut cmd = if launch.extension().and_then(|e| e.to_str()) == Some("py") {
        let mut c = Command::new("python");
        c.arg(&launch);
        c
    } else {
        Command::new(&launch)
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(launch.parent().unwrap_or(Path::new(".")));
    let Ok(mut child) = cmd.spawn() else {
        return Vec::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(body.as_bytes());
    }
    let mut paths = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().flatten() {
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(arr) = msg.get("couplingPaths").and_then(|a| a.as_array()) {
                    for p in arr {
                        if let Some(s) = p.as_str() {
                            paths.push(s.to_string());
                        }
                    }
                }
            }
        }
    }
    let _ = child.wait();
    paths
}

/// Start a scientific (or labelled demo) flood run on the CPU lane.
pub fn start_scientific_flood(
    app: AppHandle,
    mut spec: HydroJobSpec,
) -> Result<FloodRunStatus, String> {
    if spec.workspace.is_empty() || spec.scenario_id.is_empty() {
        return Err("workspace and scenarioId are required".into());
    }
    spec.engine = Some(spec.engine.unwrap_or(HydroEngine::Anuga));
    // Default allow_demo to true for UI continuity when caller omits the field
    // via JSON null/default — HydroJobSpec.allow_demo defaults false with Default,
    // so Tauri commands should pass the intended value explicitly.

    let ws = PathBuf::from(&spec.workspace);
    let mut project = Project::load(&ws)?;
    crate::geospatial::prepare_workspace(&ws)?;

    let scenario = ensure_scenario(&mut project, &spec.scenario_id);
    project.save()?;

    validate_for_scientific(&project, &scenario, spec.allow_demo)?;

    let run_id = format!(
        "run_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let out_dir = ws.join("geo").join("runs").join(&run_id);
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let cancel = Arc::new(AtomicBool::new(false));
    let status = FloodRunStatus {
        run_id: run_id.clone(),
        scenario_id: spec.scenario_id.clone(),
        workspace: spec.workspace.clone(),
        state: "running".into(),
        progress: 0.0,
        detail: "Preparing DEM…".into(),
        mode: None,
        engine: Some("anuga".into()),
        engine_version: None,
        result_paths: Vec::new(),
        mass_balance: None,
        label: None,
        created_unix: now_unix(),
    };

    {
        let mut guard = active_runs().lock().unwrap();
        // One scientific CPU run at a time per process (CPU lane).
        if guard.values().any(|r| r.status.state == "running") {
            return Err("A scientific flood run is already active on the CPU lane.".into());
        }
        guard.insert(
            run_id.clone(),
            ActiveFloodRun {
                cancel: Arc::clone(&cancel),
                status: status.clone(),
            },
        );
    }

    let app_bg = app.clone();
    let cancel_bg = Arc::clone(&cancel);
    let run_id_bg = run_id.clone();
    let spec_bg = spec.clone();
    let scenario_bg = scenario.clone();
    let ws_bg = ws.clone();
    let out_bg = out_dir.clone();
    let geo_ref = project.geo_reference.clone();

    std::thread::Builder::new()
        .name("flood-anuga-cpu".into())
        .spawn(move || {
            let result = (|| -> Result<(SimulationRun, Option<String>), String> {
                let splat_bounds_enu = Project::load(&ws_bg)
                    .ok()
                    .and_then(|p| splat_bounds_enu_from_project(&p));
                let extent_input = spec_bg.extent.clone().unwrap_or_else(|| {
                    if let Some(aoi) = scenario_bg.aoi_wgs84 {
                        let (origin, dem_bounds_enu) = aoi_wgs84_to_enu_box(aoi);
                        ExtentPlanInput {
                            camera_enu: Vec::new(),
                            splat_bounds_enu: splat_bounds_enu.clone(),
                            dem_bounds_enu: Some(dem_bounds_enu),
                            dem_accuracy_m: Some(2.0),
                            preview_budget_cells: Some(1024),
                            enu_origin: Some(
                                geo_ref
                                    .as_ref()
                                    .and_then(|g| g.local_origin)
                                    .unwrap_or(origin),
                            ),
                            geo_reference: geo_ref.clone(),
                        }
                    } else {
                        ExtentPlanInput {
                            camera_enu: Vec::new(),
                            splat_bounds_enu: splat_bounds_enu.clone(),
                            dem_bounds_enu: Some([0.0, 0.0, 400.0, 300.0]),
                            dem_accuracy_m: Some(2.0),
                            preview_budget_cells: Some(1024),
                            enu_origin: geo_ref.as_ref().and_then(|g| g.local_origin),
                            geo_reference: geo_ref.clone(),
                        }
                    }
                });
                let extent_plan = plan_extent(&extent_input);
                let mesh = mesh_plan_from_extent(&extent_plan);

                update_active(&run_id_bg, |s| {
                    s.detail = "Preparing DEM…".into();
                });
                let dem_src = spec_bg.dem_path.as_ref().map(PathBuf::from);
                let aoi_for_dem = scenario_bg.aoi_wgs84;
                let dem: DemProduct = dem::prepare_flood_dem_with_opts(
                    &ws_bg,
                    &dem::DemStageOpts {
                        source: dem_src.map(|p| p.to_string_lossy().into_owned()),
                        cell_size_m: Some(mesh.dem_resolution_m.max(1.0)),
                        crs: geo_ref
                            .as_ref()
                            .and_then(|g| g.working_crs.clone())
                            .or_else(|| Some("local-ENU-m".into())),
                        aoi_wgs84: aoi_for_dem,
                        nodata: Some(-9999.0),
                    },
                )?;
                // Honest Demo when DEM is synthetic even if ANUGA is present.
                if dem.synthetic {
                    emit_geo(
                        &app_bg,
                        GeoEvent::EngineMissing {
                            engine: "dem".into(),
                            message: "No real DEM staged — scientific path will label Demo extents when falling back; fetch USGS 3DEP / Copernicus first."
                                .into(),
                            demo_available: true,
                        },
                    );
                }

                let ensemble = spec_bg.ensemble.clone().unwrap_or(EnsembleSpec {
                    realizations: 1,
                    ..Default::default()
                });
                // First cut: single realization (hooks present for ensembles).
                let (rain_s, rough_s, infil_s) = ensemble_scales(&ensemble, 0);
                let duration = duration_hours(&scenario_bg);

                let request = serde_json::json!({
                    "schemaVersion": 1,
                    "runId": run_id_bg,
                    "workspace": ws_bg.to_string_lossy(),
                    "outputDir": out_bg.to_string_lossy(),
                    "demoMode": spec_bg.allow_demo || dem.synthetic,
                    "dem": {
                        "path": dem.dtm_path,
                        "crs": dem.crs,
                        "cellSizeM": dem.cell_size_m.unwrap_or(mesh.dem_resolution_m),
                        "synthetic": dem.synthetic,
                        "bedSource": dem.bed_source
                    },
                    "extent": {
                        "boundsEnu": mesh.bounds_enu,
                        "meshMaxAreaM2": mesh.site_max_area_m2,
                        "regionalMeshMaxAreaM2": mesh.regional_max_area_m2
                    },
                    "scenario": {
                        "id": scenario_bg.id,
                        "name": scenario_bg.name,
                        "durationHours": duration,
                        "rainfall": scenario_bg.rainfall,
                        "inflows": scenario_bg.inflows,
                        "infiltration": scenario_bg.infiltration,
                        "roughness": scenario_bg.roughness,
                        "structures": scenario_bg.structures,
                        "drains": scenario_bg.drains,
                        "boundaryConditions": scenario_bg.boundary_conditions,
                        "solverSettings": scenario_bg.solver_settings
                    },
                    "ensemble": {
                        "realizationIndex": 0,
                        "totalRealizations": ensemble.realizations.max(1),
                        "rainfallScale": rain_s,
                        "roughnessScale": rough_s,
                        "infiltrationScale": infil_s
                    },
                    "swmm": {
                        "enabled": spec_bg.enable_swmm,
                        "networkPath": scenario_bg.drains.as_ref()
                            .and_then(|d| d.get("networkPath"))
                            .cloned()
                    },
                    "checkpointEveryS": 600
                });

                let (paths, mass, mut mode, mut version_or_label) =
                    if let Some(launch) = resolve_geo_sidecar("anuga") {
                        match invoke_anuga_sidecar(
                            &app_bg,
                            &cancel_bg,
                            &launch,
                            &request,
                            &run_id_bg,
                        ) {
                            Ok(v) => v,
                            Err(e) if e == "__engine_missing__" && spec_bg.allow_demo => {
                                emit_geo(
                                    &app_bg,
                                    GeoEvent::EngineMissing {
                                        engine: "anuga".into(),
                                        message: "ANUGA not importable — falling back to demo mode."
                                            .into(),
                                        demo_available: true,
                                    },
                                );
                                let (p, m) = run_intrinsic_demo(
                                    &app_bg,
                                    &cancel_bg,
                                    &run_id_bg,
                                    &out_bg,
                                    &mesh,
                                    duration,
                                )?;
                                (p, m, "demo".into(), Some(
                                    "Demo mode — ANUGA engine missing; extents are synthetic".into(),
                                ))
                            }
                            Err(e) => return Err(e),
                        }
                    } else if spec_bg.allow_demo {
                        emit_geo(
                            &app_bg,
                            GeoEvent::EngineMissing {
                                engine: "anuga".into(),
                                message: "ANUGA sidecar not found — running labelled demo.".into(),
                                demo_available: true,
                            },
                        );
                        let (p, m) = run_intrinsic_demo(
                            &app_bg,
                            &cancel_bg,
                            &run_id_bg,
                            &out_bg,
                            &mesh,
                            duration,
                        )?;
                        (
                            p,
                            m,
                            "demo".into(),
                            Some(
                                "Demo mode — ANUGA sidecar missing; extents are synthetic".into(),
                            ),
                        )
                    } else {
                        emit_geo(
                            &app_bg,
                            GeoEvent::EngineMissing {
                                engine: "anuga".into(),
                                message: "ANUGA sidecar not found.".into(),
                                demo_available: true,
                            },
                        );
                        return Err(
                            "ANUGA sidecar not found. Install engines/sidecars/anuga or allow demo."
                                .into(),
                        );
                    };

                // Real DEM required for Scientific badge — synthetic bed stays Demo.
                if dem.synthetic && mode != "demo" {
                    mode = "demo".into();
                    version_or_label = Some(
                        "Demo mode — DEM is synthetic; fetch USGS 3DEP / Copernicus for scientific bed."
                            .into(),
                    );
                }

                let mut all_paths = paths;
                if spec_bg.enable_swmm || scenario_bg.drains.is_some() {
                    let swmm_paths =
                        maybe_run_swmm(&run_id_bg, &out_bg, &scenario_bg, mode == "demo");
                    all_paths.extend(swmm_paths);
                }

                let (engine_version, label) = if mode == "demo" {
                    (None, version_or_label)
                } else {
                    (version_or_label, None)
                };

                let sim = SimulationRun {
                    id: run_id_bg.clone(),
                    scenario_id: spec_bg.scenario_id.clone(),
                    engine: Some("anuga".into()),
                    engine_version,
                    grid_or_mesh: Some(format!(
                        "tri max_area_m2={:.1} regional={:.1}",
                        mesh.site_max_area_m2, mesh.regional_max_area_m2
                    )),
                    timestep_s: None,
                    cfl: scenario_bg
                        .solver_settings
                        .as_ref()
                        .and_then(|s| s.get("cfl"))
                        .and_then(|c| c.as_f64()),
                    mass_balance: Some(mass),
                    result_paths: all_paths.clone(),
                    checkpoint_paths: all_paths
                        .iter()
                        .filter(|p| p.contains("checkpoints"))
                        .cloned()
                        .collect(),
                    hardware: Some("cpu".into()),
                    reproducibility_hash: None,
                    created_unix: now_unix(),
                    status: Some("done".into()),
                    mode: Some(mode.clone()),
                };
                persist_run(&ws_bg, &sim)?;
                Ok((sim, label))
            })();

            match result {
                Ok((sim, label)) => {
                    update_active(&run_id_bg, |s| {
                        s.state = "done".into();
                        s.progress = 1.0;
                        s.detail = label
                            .clone()
                            .unwrap_or_else(|| format!("mode={}", sim.mode.as_deref().unwrap_or("?")));
                        s.mode = sim.mode.clone();
                        s.engine_version = sim.engine_version.clone();
                        s.result_paths = sim.result_paths.clone();
                        s.mass_balance = sim.mass_balance;
                        s.label = label.clone();
                    });
                    emit_geo(
                        &app_bg,
                        GeoEvent::RunDone {
                            run_id: run_id_bg.clone(),
                            result_paths: sim.result_paths.clone(),
                            mode: sim.mode.clone(),
                            mass_balance: sim.mass_balance,
                        },
                    );
                    emit_sim(
                        &app_bg,
                        SimEvent::Done {
                            run_id: run_id_bg.clone(),
                            mode: sim.mode.clone().unwrap_or_else(|| "anuga".into()),
                            result_paths: sim.result_paths,
                            mass_balance: sim.mass_balance,
                            label,
                        },
                    );
                }
                Err(e) if e == "__cancelled__" => {
                    update_active(&run_id_bg, |s| {
                        s.state = "cancelled".into();
                        s.detail = "Cancelled".into();
                    });
                    let mut failed = SimulationRun {
                        id: run_id_bg.clone(),
                        scenario_id: spec_bg.scenario_id.clone(),
                        engine: Some("anuga".into()),
                        created_unix: now_unix(),
                        status: Some("cancelled".into()),
                        mode: Some("cancelled".into()),
                        ..Default::default()
                    };
                    failed.hardware = Some("cpu".into());
                    let _ = persist_run(&ws_bg, &failed);
                    emit_geo(
                        &app_bg,
                        GeoEvent::RunCancelled {
                            run_id: run_id_bg.clone(),
                        },
                    );
                }
                Err(e) => {
                    update_active(&run_id_bg, |s| {
                        s.state = "failed".into();
                        s.detail = e.clone();
                    });
                    let failed = SimulationRun {
                        id: run_id_bg.clone(),
                        scenario_id: spec_bg.scenario_id.clone(),
                        engine: Some("anuga".into()),
                        created_unix: now_unix(),
                        status: Some("failed".into()),
                        mode: Some("failed".into()),
                        hardware: Some("cpu".into()),
                        ..Default::default()
                    };
                    let _ = persist_run(&ws_bg, &failed);
                    emit_geo(
                        &app_bg,
                        GeoEvent::Error {
                            message: e,
                            run_id: Some(run_id_bg.clone()),
                        },
                    );
                }
            }
        })
        .map_err(|e| format!("Failed to spawn flood worker: {e}"))?;

    Ok(status)
}

/// Queue-compatible enqueue (returns run id). Prefer `start_scientific_flood`.
pub fn enqueue_hydro_job(app: AppHandle, spec: HydroJobSpec) -> Result<String, String> {
    if let Some(id) = spec.engine {
        if refuse_gpl_bundle(id) {
            let proto = external_plugin_protocol(id);
            return Err(format!(
                "Refusing to run bundled GPL hydro engine {:?}. {:?}",
                id,
                proto.map(|p| p.instructions)
            ));
        }
        if matches!(
            id,
            HydroEngine::Triton
                | HydroEngine::Wflow
                | HydroEngine::GeoClaw
                | HydroEngine::Experimental
        ) {
            return Err(format!(
                "Experimental hydro engine {:?} is registered but not executed in this build. \
                 Install via external protocol under engines/hydro/.",
                id
            ));
        }
    }
    let status = start_scientific_flood(app, spec)?;
    Ok(status.run_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aoi_domain_is_not_wellington_locked() {
        // Auckland CBD-ish box — centre must follow the AOI, not a Wellington default.
        let aoi = [174.75, -36.86, 174.78, -36.84];
        let (origin, bounds) = aoi_wgs84_to_enu_box(aoi);
        assert!((origin[0] - 174.765).abs() < 1e-6);
        assert!((origin[1] - (-36.85)).abs() < 1e-6);
        assert!(bounds[0] < 0.0 && bounds[2] > 0.0);
        assert!(bounds[1] < 0.0 && bounds[3] > 0.0);
        let width = bounds[2] - bounds[0];
        let height = bounds[3] - bounds[1];
        assert!(width > 1000.0 && width < 5000.0, "width={width}");
        assert!(height > 1000.0 && height < 5000.0, "height={height}");
    }

    #[test]
    fn commit_flood_aoi_persists_domain_and_plans_extent() {
        use crate::profiler::Preset;
        use crate::project::Suite;
        use crate::settings::ResolvedSettings;

        let ws = std::env::temp_dir().join("instasplatter_aoi_commit_test");
        let _ = fs::remove_dir_all(&ws);
        fs::create_dir_all(&ws).unwrap();
        let settings = ResolvedSettings {
            preset: Preset::Balanced,
            max_frames: 100,
            max_resolution: 1280,
            blur_reject_fraction: 0.15,
            matcher: "auto".into(),
            sift_gpu: true,
            total_steps: 1000,
            max_splats: 1_000_000,
            sh_degree: 3,
            refine_every: 200,
            ssim_weight: 0.2,
            export_every: 500,
            progressive_resolution: false,
            mip_filter: false,
            live_init: false,
            dense_init: true,
            use_neural_init: true,
            allow_research_sidecars: false,
            experimental_mode: false,
            experimental_license_acked: false,
            post_polish: true,
            trainer: "brush".into(),
            gsplat_strategy: "mcmc".into(),
            gsplat_absgrad: true,
            gsplat_antialiased: true,
            gsplat_appearance: true,
            gsplat_bilateral_grid: true,
            roma_quality: "base".into(),
            strictness: 0.5,
            export_format: "ply".into(),
            keep_intermediates: false,
            opac_loss_weight: 1e-9,
            scale_loss_weight: 1e-8,
            mean_noise_weight: 40.0,
        };
        let p = Project::new_with_suite(
            "job_aoi",
            Path::new("C:/in/survey"),
            &ws,
            &settings,
            Suite::Geospatial,
        );
        p.save().unwrap();

        let aoi = [-122.42, 37.77, -122.40, 37.79]; // SF-ish
        let (scenario, plan, geo) = commit_flood_aoi(&ws, "default", aoi).unwrap();
        assert_eq!(scenario.aoi_wgs84, Some(aoi));
        assert!(plan.bounds_enu[2] > plan.bounds_enu[0]);
        assert!(plan.bounds_enu[3] > plan.bounds_enu[1]);
        let origin = geo.as_ref().and_then(|g| g.local_origin).unwrap();
        assert!((origin[0] - (-122.41)).abs() < 1e-3);
        assert!((origin[1] - 37.78).abs() < 1e-3);

        let back = Project::load(&ws).unwrap();
        assert_eq!(
            back.flood_scenarios
                .iter()
                .find(|s| s.id == "default")
                .and_then(|s| s.aoi_wgs84),
            Some(aoi)
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn mesh_plan_copies_extent() {
        let plan = ExtentPlan {
            scientific_mesh_max_area_m2: 16.0,
            regional_mesh_max_area_m2: 100.0,
            dem_resolution_m: 2.0,
            preview_cell_m: 4.0,
            bounds_enu: [0.0, 0.0, 100.0, 80.0],
            notes: vec!["ok".into()],
            ..Default::default()
        };
        let m = mesh_plan_from_extent(&plan);
        assert_eq!(m.site_max_area_m2, 16.0);
        assert_eq!(m.bounds_enu[2], 100.0);
    }

    #[test]
    fn ensemble_midpoint_default() {
        let (r, n, i) = ensemble_scales(&EnsembleSpec::default(), 0);
        assert!((r - 1.0).abs() < 1e-9);
        assert!((n - 1.0).abs() < 1e-9);
        assert!((i - 1.0).abs() < 1e-9);
    }

    #[test]
    fn resolve_anuga_from_repo_tools() {
        assert!(
            resolve_geo_sidecar("anuga").is_some(),
            "expected tools/sidecars/anuga launcher in repo"
        );
    }

    #[test]
    fn registry_lists_standard_experimental_and_gpl() {
        let regs = engine_registry();
        assert!(regs.iter().any(|d| d.id == HydroEngine::Anuga));
        assert!(regs.iter().any(|d| d.id == HydroEngine::Triton));
        assert!(regs.iter().any(|d| d.id == HydroEngine::Wflow));
        assert!(regs.iter().any(|d| d.id == HydroEngine::GeoClaw));
        assert!(regs
            .iter()
            .any(|d| d.id == HydroEngine::Sfincs && !d.bundled));
    }

    #[test]
    fn gpl_engines_refuse_bundle_and_need_protocol() {
        for id in [
            HydroEngine::Sfincs,
            HydroEngine::Hipims,
            HydroEngine::BgFlood,
            HydroEngine::Itzi,
        ] {
            assert!(refuse_gpl_bundle(id));
            let p = external_plugin_protocol(id).expect("protocol");
            assert!(p.refuse_if_bundled_request);
            assert!(p.accepted_marker.contains("GPL"));
        }
    }

    #[test]
    fn promotion_gates_block_incomplete_checklist() {
        let mut gates = HydroPromotionGates::default();
        let err = try_promote_to_standard(HydroEngine::Triton, &gates).unwrap_err();
        assert!(err.contains("Promotion gates incomplete"));
        gates = HydroPromotionGates {
            lake_at_rest: true,
            wet_dry_analytical: true,
            dam_break_analytical: true,
            rainfall_infiltration: true,
            mesh_convergence: true,
            mass_conservation: true,
            urban_obstacles: true,
            calibrated_historical: true,
            anuga_cross_comparison: true,
            cpu_gpu_tolerance: true,
            reproducibility_hash: true,
            license_cleared_for_standard: true,
        };
        assert!(gates.all_clear());
        let err = try_promote_to_standard(HydroEngine::Triton, &gates).unwrap_err();
        assert!(err.contains("not wired"));
    }

    #[test]
    fn gpl_cannot_promote_even_with_gates() {
        let gates = HydroPromotionGates {
            lake_at_rest: true,
            wet_dry_analytical: true,
            dam_break_analytical: true,
            rainfall_infiltration: true,
            mesh_convergence: true,
            mass_conservation: true,
            urban_obstacles: true,
            calibrated_historical: true,
            anuga_cross_comparison: true,
            cpu_gpu_tolerance: true,
            reproducibility_hash: true,
            license_cleared_for_standard: true,
        };
        let err = try_promote_to_standard(HydroEngine::Sfincs, &gates).unwrap_err();
        assert!(err.contains("GPL"));
    }
}
