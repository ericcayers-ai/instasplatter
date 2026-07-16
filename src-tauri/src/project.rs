//! Project bundles (ROADMAP-V2 1.4 / geospatial suite v2).
//!
//! Every job writes a `project.json` next to its workspace holding the input
//! reference, the resolved settings, where the solved poses live and which
//! splat is current. That is everything needed to reopen a finished scene or
//! resume an interrupted run, so closing the app mid-training is not fatal.
//!
//! Format v2 adds a suite tag and geospatial placeholders (GeoReference,
//! layers, flood scenarios, simulation runs) while still loading v1 manifests.

use crate::settings::ResolvedSettings;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "project.json";
const PROJECT_VERSION: u32 = 2;

/// Relative paths under a workspace used by the geospatial suite.
pub const GEO_WORKSPACE_DIRS: &[&str] = &[
    "geo/sources",
    "geo/derived",
    "geo/tiles",
    "geo/scenarios",
    "geo/runs",
    "geo/exports",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Suite {
    #[default]
    Reconstruction,
    Geospatial,
}

impl Suite {
    pub fn as_str(self) -> &'static str {
        match self {
            Suite::Reconstruction => "reconstruction",
            Suite::Geospatial => "geospatial",
        }
    }

    pub fn parse(s: &str) -> Option<Suite> {
        match s.trim().to_ascii_lowercase().as_str() {
            "reconstruction" | "recon" => Some(Suite::Reconstruction),
            "geospatial" | "geo" => Some(Suite::Geospatial),
            _ => None,
        }
    }
}

/// Metric frame tying the local scene to a geodetic CRS.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GeoReference {
    /// EPSG or PROJ string for the horizontal CRS (e.g. "EPSG:4979").
    pub source_crs: Option<String>,
    /// Vertical datum or compound CRS label.
    pub vertical_datum: Option<String>,
    /// Linear units of the working frame ("m", "ft", …).
    pub units: Option<String>,
    /// Working projected / ENU CRS label (e.g. "local-ENU-m").
    pub working_crs: Option<String>,
    /// 4×4 row-major ECEF → local ENU.
    pub ecef_to_enu: Option<[f64; 16]>,
    /// 4×4 row-major local ENU → ECEF (inverse of `ecef_to_enu`).
    pub enu_to_ecef: Option<[f64; 16]>,
    /// Lon/lat/ellipsoidal height (degrees, degrees, metres) for the ENU origin.
    pub local_origin: Option<[f64; 3]>,
    /// ECEF metres of the ENU origin.
    pub local_origin_ecef: Option<[f64; 3]>,
    /// Typical horizontal uncertainty in metres when known.
    pub uncertainty_m: Option<f64>,
    /// Mean GCP residual (metres) after a Sim(3) solve.
    pub gcp_residual_m: Option<f64>,
    /// Max GCP residual (metres) after a Sim(3) solve.
    pub gcp_residual_max_m: Option<f64>,
    /// Free-form provenance (telemetry source, GPS quality, etc.).
    pub provenance: Option<String>,
    /// `"metric"` | `"unscaled"` | `"approx"` — flood science requires metric.
    pub scale_status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum GeoLayerKind {
    #[default]
    Raster,
    Vector,
    PointCloud,
    Splat,
    Mesh,
    Network,
    TimeSeries,
}

/// Layer entry for the geospatial catalog / viewport tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoLayer {
    pub id: String,
    pub name: String,
    pub kind: GeoLayerKind,
    pub path: Option<String>,
    pub source_license: Option<String>,
    pub content_hash: Option<String>,
    /// Axis-aligned bounds [min_x, min_y, max_x, max_y] in layer CRS.
    pub bounds: Option<[f64; 4]>,
    pub crs: Option<String>,
    pub visible: bool,
    pub style: Option<serde_json::Value>,
    pub lod: Option<u32>,
}

impl Default for GeoLayer {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: GeoLayerKind::Raster,
            path: None,
            source_license: None,
            content_hash: None,
            bounds: None,
            crs: None,
            visible: true,
            style: None,
            lod: None,
        }
    }
}

/// Placeholder flood scenario (terrain + forcing + structures).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FloodScenario {
    pub id: String,
    pub name: String,
    pub terrain_layer_id: Option<String>,
    pub rainfall: Option<serde_json::Value>,
    pub inflows: Option<serde_json::Value>,
    pub infiltration: Option<serde_json::Value>,
    pub roughness: Option<serde_json::Value>,
    pub structures: Option<serde_json::Value>,
    pub drains: Option<serde_json::Value>,
    pub boundary_conditions: Option<serde_json::Value>,
    pub solver_settings: Option<serde_json::Value>,
    /// "draft" | "calibrated" | "validated" | …
    pub validation_state: Option<String>,
    /// Area of interest in WGS84: `[west, south, east, north]` degrees.
    pub aoi_wgs84: Option<[f64; 4]>,
}

/// One scientific or preview simulation attempt for a scenario.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SimulationRun {
    pub id: String,
    pub scenario_id: String,
    pub engine: Option<String>,
    pub engine_version: Option<String>,
    pub grid_or_mesh: Option<String>,
    pub timestep_s: Option<f64>,
    pub cfl: Option<f64>,
    pub mass_balance: Option<f64>,
    pub result_paths: Vec<String>,
    pub checkpoint_paths: Vec<String>,
    pub hardware: Option<String>,
    pub reproducibility_hash: Option<String>,
    pub created_unix: u64,
    /// `"queued"` | `"running"` | `"done"` | `"failed"` | `"cancelled"`.
    pub status: Option<String>,
    /// `"anuga"` | `"demo"` | `"preview"` | … — demo is not scientifically authoritative.
    pub mode: Option<String>,
}

/// Manual override of auto geo pose for the splat layer (translate / rotate / scale).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelTransform {
    pub translation: [f32; 3],
    /// Row-major 3×3.
    pub rotation: [f32; 9],
    pub scale: [f32; 3],
}

impl ModelTransform {
    pub fn identity() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub version: u32,
    #[serde(default)]
    pub suite: Suite,
    pub job_id: String,
    pub created_unix: u64,
    pub updated_unix: u64,
    pub input_path: String,
    pub workspace: String,
    pub settings: ResolvedSettings,

    /// Directory holding `cameras.*` and `images.*`, once SfM has run.
    pub sparse_dir: Option<String>,
    /// Most recent splat written by training, absolute path.
    pub latest_splat: Option<String>,
    pub latest_iter: u32,
    pub total_steps: u32,
    pub completed: bool,
    /// Orientation the user set in the viewport, row-major 3x3.
    pub model_rotation: Option<[f32; 9]>,
    /// Manual geo splat TRS override (ENU metres); supersedes auto registration pose when set.
    #[serde(default)]
    pub model_transform: Option<ModelTransform>,

    // ---- Geospatial (v2; empty for reconstruction-only projects) ----
    #[serde(default)]
    pub geo_reference: Option<GeoReference>,
    #[serde(default)]
    pub geo_layers: Vec<GeoLayer>,
    #[serde(default)]
    pub flood_scenarios: Vec<FloodScenario>,
    #[serde(default)]
    pub simulation_runs: Vec<SimulationRun>,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Create `workspace/geo/{sources,derived,tiles,scenarios,runs,exports}`.
pub fn ensure_geo_workspace(workspace: &Path) -> Result<(), String> {
    for rel in GEO_WORKSPACE_DIRS {
        fs::create_dir_all(workspace.join(rel)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl Project {
    pub fn new(
        job_id: &str,
        input_path: &Path,
        workspace: &Path,
        settings: &ResolvedSettings,
    ) -> Project {
        Self::new_with_suite(
            job_id,
            input_path,
            workspace,
            settings,
            Suite::Reconstruction,
        )
    }

    pub fn new_with_suite(
        job_id: &str,
        input_path: &Path,
        workspace: &Path,
        settings: &ResolvedSettings,
        suite: Suite,
    ) -> Project {
        let t = now_unix();
        let mut proj = Project {
            version: PROJECT_VERSION,
            suite,
            job_id: job_id.to_string(),
            created_unix: t,
            updated_unix: t,
            input_path: input_path.to_string_lossy().into_owned(),
            workspace: workspace.to_string_lossy().into_owned(),
            settings: settings.clone(),
            sparse_dir: None,
            latest_splat: None,
            latest_iter: 0,
            total_steps: settings.total_steps,
            completed: false,
            model_rotation: None,
            model_transform: None,
            geo_reference: None,
            geo_layers: Vec::new(),
            flood_scenarios: Vec::new(),
            simulation_runs: Vec::new(),
        };
        if suite == Suite::Geospatial {
            proj.geo_reference = Some(GeoReference::default());
            let _ = ensure_geo_workspace(workspace);
        }
        proj
    }

    pub fn path(workspace: &Path) -> PathBuf {
        workspace.join(PROJECT_FILE)
    }

    /// Write atomically: a crash mid-save must not truncate an existing
    /// manifest, or a resumable job becomes unresumable.
    pub fn save(&self) -> Result<(), String> {
        let ws = PathBuf::from(&self.workspace);
        fs::create_dir_all(&ws).map_err(|e| e.to_string())?;
        if self.suite == Suite::Geospatial {
            ensure_geo_workspace(&ws)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = ws.join(format!("{PROJECT_FILE}.tmp"));
        fs::write(&tmp, json).map_err(|e| e.to_string())?;
        let dest = Project::path(&ws);
        // Windows rename fails if the destination exists.
        let _ = fs::remove_file(&dest);
        fs::rename(&tmp, &dest).map_err(|e| e.to_string())
    }

    pub fn load(workspace: &Path) -> Result<Project, String> {
        let p = Project::path(workspace);
        let text = fs::read_to_string(&p)
            .map_err(|e| format!("Cannot read {}: {e}", p.display()))?;
        let mut proj: Project = serde_json::from_str(&text)
            .map_err(|e| format!("{} is not a valid project file: {e}", p.display()))?;
        if proj.version > PROJECT_VERSION {
            return Err(format!(
                "This project was written by a newer version of InstaSplatter (format {}).",
                proj.version
            ));
        }
        // v1 → v2: suite/geo fields already defaulted via serde; bump version.
        if proj.version < PROJECT_VERSION {
            proj.version = PROJECT_VERSION;
        }
        Ok(proj)
    }

    pub fn touch(&mut self) {
        self.updated_unix = now_unix();
    }

    /// True when training stopped part way and enough state survives to pick
    /// it up again: poses on disk and at least one exported checkpoint.
    pub fn is_resumable(&self) -> bool {
        if self.suite != Suite::Reconstruction {
            return false;
        }
        if self.completed || self.latest_iter == 0 || self.latest_iter >= self.total_steps {
            return false;
        }
        let splat_ok = self
            .latest_splat
            .as_ref()
            .map(|p| Path::new(p).exists())
            .unwrap_or(false);
        let poses_ok = self
            .sparse_dir
            .as_ref()
            .map(|p| crate::colmap::find_model_dir(Path::new(p)).is_some())
            .unwrap_or(false);
        splat_ok && poses_ok
    }
}

/// Summary shown in the "reopen a project" list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub job_id: String,
    pub workspace: String,
    pub input_name: String,
    pub updated_unix: u64,
    pub completed: bool,
    pub resumable: bool,
    pub latest_iter: u32,
    pub total_steps: u32,
    pub result_path: Option<String>,
    pub suite: Suite,
}

impl From<&Project> for ProjectSummary {
    fn from(p: &Project) -> ProjectSummary {
        let result = Path::new(&p.workspace).join("result.ply");
        ProjectSummary {
            job_id: p.job_id.clone(),
            workspace: p.workspace.clone(),
            input_name: Path::new(&p.input_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.input_path.clone()),
            updated_unix: p.updated_unix,
            completed: p.completed,
            resumable: p.is_resumable(),
            latest_iter: p.latest_iter,
            total_steps: p.total_steps,
            result_path: result.exists().then(|| result.to_string_lossy().into_owned()),
            suite: p.suite,
        }
    }
}

/// All projects under the jobs directory, newest first.
pub fn list_projects(jobs_dir: &Path) -> Vec<ProjectSummary> {
    let mut out: Vec<ProjectSummary> = fs::read_dir(jobs_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|d| Project::load(&d).ok())
        .map(|p| ProjectSummary::from(&p))
        .collect();
    out.sort_by(|a, b| b.updated_unix.cmp(&a.updated_unix));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::Preset;

    fn settings() -> ResolvedSettings {
        ResolvedSettings {
            preset: Preset::Balanced,
            max_frames: 100,
            max_resolution: 1280,
            blur_reject_fraction: 0.15,
            matcher: "auto".into(),
            sift_gpu: true,
            total_steps: 12000,
            max_splats: 3_000_000,
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
        }
    }

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("instasplatter_project_{name}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn project_roundtrips_through_disk() {
        let ws = temp("roundtrip");
        let mut p = Project::new("job_1", Path::new("C:/in/clip.mp4"), &ws, &settings());
        p.latest_iter = 500;
        p.save().unwrap();

        let back = Project::load(&ws).unwrap();
        assert_eq!(back.job_id, "job_1");
        assert_eq!(back.latest_iter, 500);
        assert_eq!(back.total_steps, 12000);
        assert_eq!(back.settings.preset, Preset::Balanced);
        assert_eq!(back.suite, Suite::Reconstruction);
        assert_eq!(back.version, PROJECT_VERSION);
        assert!(!back.completed);
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn v1_manifest_loads_as_v2_reconstruction() {
        let ws = temp("v1_migrate");
        let v1 = serde_json::json!({
            "version": 1,
            "jobId": "job_old",
            "createdUnix": 1,
            "updatedUnix": 2,
            "inputPath": "C:/in/clip.mp4",
            "workspace": ws.to_string_lossy(),
            "settings": settings(),
            "sparseDir": null,
            "latestSplat": null,
            "latestIter": 0,
            "totalSteps": 12000,
            "completed": false,
            "modelRotation": null
        });
        fs::write(Project::path(&ws), serde_json::to_string_pretty(&v1).unwrap()).unwrap();
        let back = Project::load(&ws).unwrap();
        assert_eq!(back.version, PROJECT_VERSION);
        assert_eq!(back.suite, Suite::Reconstruction);
        assert!(back.geo_layers.is_empty());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn geospatial_project_creates_geo_dirs() {
        let ws = temp("geo_dirs");
        let p = Project::new_with_suite(
            "job_geo",
            Path::new("C:/in/survey"),
            &ws,
            &settings(),
            Suite::Geospatial,
        );
        p.save().unwrap();
        for rel in GEO_WORKSPACE_DIRS {
            assert!(ws.join(rel).is_dir(), "missing {rel}");
        }
        assert!(p.geo_reference.is_some());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn saving_twice_replaces_rather_than_failing() {
        let ws = temp("resave");
        let mut p = Project::new("job_2", Path::new("in"), &ws, &settings());
        p.save().unwrap();
        p.latest_iter = 999;
        p.save().unwrap();
        assert_eq!(Project::load(&ws).unwrap().latest_iter, 999);
        // No temp file is left behind.
        assert!(!ws.join(format!("{PROJECT_FILE}.tmp")).exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn a_future_format_version_is_refused() {
        let ws = temp("future");
        let mut p = Project::new("job_3", Path::new("in"), &ws, &settings());
        p.version = PROJECT_VERSION + 1;
        p.save().unwrap();
        let err = Project::load(&ws).unwrap_err();
        assert!(err.contains("newer version"), "{err}");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn resumable_requires_poses_a_checkpoint_and_unfinished_training() {
        let ws = temp("resumable");
        let sparse = ws.join("sparse").join("0");
        fs::create_dir_all(&sparse).unwrap();
        fs::write(sparse.join("cameras.txt"), "").unwrap();
        let splat = ws.join("exports").join("export_500.ply");
        fs::create_dir_all(splat.parent().unwrap()).unwrap();
        fs::write(&splat, b"x").unwrap();

        let mut p = Project::new("job_4", Path::new("in"), &ws, &settings());
        assert!(!p.is_resumable(), "no checkpoint yet");

        p.latest_iter = 500;
        p.latest_splat = Some(splat.to_string_lossy().into_owned());
        p.sparse_dir = Some(ws.join("sparse").to_string_lossy().into_owned());
        assert!(p.is_resumable());

        // A finished job is not resumable.
        p.completed = true;
        assert!(!p.is_resumable());
        p.completed = false;

        // Neither is one that already reached the last step.
        p.latest_iter = p.total_steps;
        assert!(!p.is_resumable());
        p.latest_iter = 500;

        // Nor one whose checkpoint has been deleted.
        fs::remove_file(&splat).unwrap();
        assert!(!p.is_resumable());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn listing_projects_sorts_newest_first_and_skips_junk() {
        let jobs = temp("list");
        for (i, id) in ["job_a", "job_b"].iter().enumerate() {
            let ws = jobs.join(id);
            fs::create_dir_all(&ws).unwrap();
            let mut p = Project::new(id, Path::new("C:/in/clip.mp4"), &ws, &settings());
            p.updated_unix = 1000 + i as u64;
            p.save().unwrap();
        }
        // A directory with no manifest must be ignored, not crash the listing.
        fs::create_dir_all(jobs.join("not_a_project")).unwrap();

        let list = list_projects(&jobs);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].job_id, "job_b");
        assert_eq!(list[0].input_name, "clip.mp4");
        assert!(list[0].result_path.is_none());
        assert_eq!(list[0].suite, Suite::Reconstruction);
        let _ = fs::remove_dir_all(&jobs);
    }
}
