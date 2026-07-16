mod colmap;
mod engines;
mod geospatial;
mod math;
mod mesh;
mod pipeline;
mod profiler;
mod project;
mod queue;
mod settings;
mod sfm;
mod splat;

use pipeline::{JobCtx, JobEvent, JobHandle};
use profiler::HardwareProfile;
use project::{Project, ProjectSummary, Suite};
use settings::{ResolvedSettings, Settings};
use splat::{export, ply, transform};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Manager;

#[derive(Clone, Default)]
struct AppState {
    jobs: Arc<Mutex<HashMap<String, Arc<JobHandle>>>>,
}

static PROFILE: OnceLock<HardwareProfile> = OnceLock::new();

fn cached_profile() -> &'static HardwareProfile {
    PROFILE.get_or_init(profiler::profile)
}

/// Developer hooks are off unless `INSTASPLATTER_DEV` says otherwise. The
/// autostart hook drives a job without any user action, so it must never be
/// reachable in a normal install.
fn dev_mode() -> bool {
    std::env::var("INSTASPLATTER_DEV")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Below this much free space on the workspace volume, a job is refused
/// rather than left to fail deep inside COLMAP or Brush with a confusing
/// "disk full" error from a tool the user has never heard of. Frame
/// extraction, the COLMAP database and a run of training checkpoints
/// together commonly reach a few gigabytes.
const MIN_FREE_DISK_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Free space on the volume holding `path`, or `None` if it cannot be
/// determined (an unmounted or exotic filesystem). A run is never blocked on
/// a diagnostic that could not be made.
fn free_disk_bytes(path: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    disks
        .list()
        .iter()
        .filter(|d| target.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

#[tauri::command]
fn get_hardware_profile() -> HardwareProfile {
    cached_profile().clone()
}

#[tauri::command]
fn get_settings() -> Settings {
    Settings::load()
}

#[tauri::command]
fn set_settings(settings: Settings) -> Result<(), String> {
    settings.save()
}

#[tauri::command]
fn get_resolved_settings() -> ResolvedSettings {
    settings::resolve(&Settings::load(), cached_profile())
}

#[tauri::command]
fn get_engine_status() -> engines::EngineStatus {
    engines::status()
}

#[tauri::command]
async fn install_engines(app: tauri::AppHandle) -> Result<engines::EngineStatus, String> {
    let has_cuda = cached_profile().has_cuda;
    engines::ensure_engines(app, has_cuda).await
}

/// Register a job with the app state and drive it to completion on the async
/// runtime. `discard_on_cancel` is false for resumed runs, whose workspace
/// already holds a reconstruction worth keeping.
fn spawn_job(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
    ctx: JobCtx,
    discard_on_cancel: bool,
    run: impl std::future::Future<Output = Result<PathBuf, String>> + Send + 'static,
) -> String {
    let handle = Arc::new(JobHandle {
        cancel: ctx.cancel.clone(),
        child_pids: ctx.child_pids.clone(),
    });
    state
        .jobs
        .lock()
        .unwrap()
        .insert(ctx.job_id.clone(), handle);

    let job_id = ctx.job_id.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        match run.await {
            Ok(_) => {}
            Err(e) if e == "__cancelled__" => {
                if discard_on_cancel {
                    // The children are dead by now, so their file handles are
                    // gone and the workspace can go with them.
                    tokio::task::block_in_place(|| pipeline::discard_workspace(&ctx.workspace));
                }
                let _ = app.emit(
                    "job://event",
                    JobEvent::Cancelled {
                        job_id: ctx.job_id.clone(),
                    },
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "job://event",
                    JobEvent::Error {
                        job_id: ctx.job_id.clone(),
                        message: e,
                    },
                );
            }
        }
    });
    job_id
}

#[tauri::command]
async fn start_job(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input_path: String,
) -> Result<String, String> {
    let input = PathBuf::from(&input_path);
    if !input.exists() {
        return Err("Input path does not exist.".into());
    }

    // Engines must be present before we start.
    let st = engines::status();
    if !st.colmap || !(st.brush || st.gsplat) {
        return Err("__engines_missing__".into());
    }
    if !st.ffmpeg && input.is_file() {
        return Err(
            "FFmpeg was not found. Install it (winget install ffmpeg) or drop an image folder instead."
                .into(),
        );
    }

    let job_id = format!(
        "job_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let workspace = pipeline::jobs_dir().join(&job_id);
    std::fs::create_dir_all(&workspace).map_err(|e| e.to_string())?;

    if let Some(free) = free_disk_bytes(&workspace) {
        if free < MIN_FREE_DISK_BYTES {
            let _ = std::fs::remove_dir_all(&workspace);
            return Err(format!(
                "Only {:.1} GB free on this drive. InstaSplatter needs a few gigabytes for \
                 extracted frames, the camera database and training checkpoints. Free up space \
                 and try again.",
                free as f64 / (1024.0 * 1024.0 * 1024.0)
            ));
        }
    }

    let resolved = settings::resolve(&Settings::load(), cached_profile());
    let proj = Project::new(&job_id, &input, &workspace, &resolved);
    proj.save()?;

    let ctx = JobCtx {
        app: app.clone(),
        job_id: job_id.clone(),
        workspace,
        settings: resolved,
        cancel: Arc::new(AtomicBool::new(false)),
        child_pids: Arc::new(Mutex::new(Vec::new())),
        project: Arc::new(Mutex::new(proj)),
    };

    let run_ctx = ctx.clone();
    Ok(spawn_job(&app, &state, ctx, true, async move {
        pipeline::run_job(&run_ctx, &input).await
    }))
}

/// Pick up an interrupted run where its last checkpoint left off.
#[tauri::command]
async fn resume_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    workspace: String,
) -> Result<String, String> {
    let ws = PathBuf::from(&workspace);
    let proj = Project::load(&ws)?;
    if !proj.is_resumable() {
        return Err("This project cannot be resumed.".into());
    }
    let st = engines::status();
    if !(st.brush || st.gsplat) {
        return Err("__engines_missing__".into());
    }

    let checkpoint = PathBuf::from(
        proj.latest_splat
            .clone()
            .ok_or("This project has no checkpoint to resume from.")?,
    );
    let start_iter = proj.latest_iter;
    let job_id = proj.job_id.clone();

    // A resumed run keeps the settings it was started with. Changing them mid
    // run would mean the schedule no longer matches the checkpoint.
    let ctx = JobCtx {
        app: app.clone(),
        job_id: job_id.clone(),
        workspace: ws,
        settings: proj.settings.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        child_pids: Arc::new(Mutex::new(Vec::new())),
        project: Arc::new(Mutex::new(proj)),
    };

    let run_ctx = ctx.clone();
    Ok(spawn_job(&app, &state, ctx, false, async move {
        pipeline::resume_job(&run_ctx, checkpoint, start_iter).await
    }))
}

#[tauri::command]
fn cancel_job(state: tauri::State<'_, AppState>, job_id: String) -> Result<(), String> {
    if let Some(handle) = state.jobs.lock().unwrap().get(&job_id) {
        // Kills the child tree. The workspace is removed by the job task once
        // it unwinds, so nothing is deleted while a process still holds it.
        handle.request_cancel();
        Ok(())
    } else {
        Err("unknown job".into())
    }
}

#[tauri::command]
fn enqueue_jobs(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
    suite: Option<String>,
) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Err("No inputs to enqueue.".into());
    }
    let suite = suite
        .as_deref()
        .and_then(Suite::parse)
        .unwrap_or_else(|| Settings::load().default_suite());
    let q = queue::global();
    let ids = q.enqueue(paths, suite);
    q.try_start_next(&app, Arc::clone(&state.jobs));
    queue::emit_now(&app);
    Ok(ids)
}

/// Active product suite for the shell (reconstruction | geospatial).
#[tauri::command]
fn get_suite() -> String {
    Settings::load().default_suite().as_str().to_string()
}

#[tauri::command]
fn set_suite(suite: String) -> Result<String, String> {
    let parsed = Suite::parse(&suite).ok_or_else(|| {
        format!("{suite} is not a suite. Use reconstruction or geospatial.")
    })?;
    let mut s = Settings::load();
    s.default_suite = Some(parsed.as_str().to_string());
    s.save()?;
    Ok(parsed.as_str().to_string())
}

/// Formats and data connectors advertised by the geospatial suite.
#[tauri::command]
fn get_geo_catalog_info() -> serde_json::Value {
    let formats: Vec<_> = [
        geospatial::data::GeoFormat::GeoTiff,
        geospatial::data::GeoFormat::Cog,
        geospatial::data::GeoFormat::GeoPackage,
        geospatial::data::GeoFormat::GeoJson,
        geospatial::data::GeoFormat::FlatGeobuf,
        geospatial::data::GeoFormat::Las,
        geospatial::data::GeoFormat::Laz,
        geospatial::data::GeoFormat::Copc,
        geospatial::data::GeoFormat::Zarr,
        geospatial::data::GeoFormat::NetCdf,
        geospatial::data::GeoFormat::PmTiles,
    ]
    .iter()
    .map(|f| {
        serde_json::json!({
            "id": f.id(),
            "label": f.label(),
        })
    })
    .collect();
    let exports: Vec<_> = geospatial::exports::list_export_kinds()
        .into_iter()
        .map(|k| {
            serde_json::json!({
                "id": k.id(),
                "label": k.label(),
                "worksOffline": k.works_offline(),
            })
        })
        .collect();
    serde_json::json!({
        "connectors": geospatial::catalog::connector_names(),
        "formats": formats,
        "exports": exports,
    })
}

/// Import flight / survey telemetry into a project and write pose priors.
#[tauri::command]
fn import_geo_telemetry(
    workspace: String,
    paths: Vec<String>,
) -> Result<geospatial::registration::RegistrationResult, String> {
    let ws = PathBuf::from(&workspace);
    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    geospatial::registration::import_telemetry_into_project(&ws, &path_bufs)
}

/// Set / refine ground control points on a project. When `refine` is true and
/// enough local picks exist, run robust Sim(3) and update geo residuals.
#[tauri::command]
fn set_geo_gcps(
    workspace: String,
    gcps: Vec<geospatial::registration::GcpPoint>,
    refine: Option<bool>,
) -> Result<serde_json::Value, String> {
    let ws = PathBuf::from(&workspace);
    let (geo, report) =
        geospatial::registration::set_project_gcps(&ws, gcps, refine.unwrap_or(false))?;
    Ok(serde_json::json!({
        "geoReference": geo,
        "residualReport": report,
    }))
}

/// Recompute ENU/ECEF GeoReference (optional origin override) and pose priors.
#[tauri::command]
fn compute_geo_reference(
    workspace: String,
    origin_lon_lat_h: Option<[f64; 3]>,
) -> Result<geospatial::registration::RegistrationResult, String> {
    geospatial::registration::compute_geo_reference(&PathBuf::from(workspace), origin_lon_lat_h)
}

/// Adaptive CRS / tile / mesh / preview resolution plan from scene bounds.
#[tauri::command]
fn plan_geo_extent(
    input: geospatial::registration::ExtentPlanInput,
) -> geospatial::registration::ExtentPlan {
    geospatial::registration::plan_extent(&input)
}

/// Load current GeoReference from a project (if any).
#[tauri::command]
fn get_geo_reference(workspace: String) -> Result<Option<project::GeoReference>, String> {
    let proj = Project::load(&PathBuf::from(workspace))?;
    Ok(proj.geo_reference)
}

/// Start an ANUGA scientific flood (CPU lane). Falls back to labelled demo when
/// the engine is missing and `allow_demo` is true (default).
#[tauri::command]
fn start_scientific_flood(
    app: tauri::AppHandle,
    workspace: String,
    scenario_id: String,
    allow_demo: Option<bool>,
    dem_path: Option<String>,
    enable_swmm: Option<bool>,
) -> Result<geospatial::hydro::FloodRunStatus, String> {
    let spec = geospatial::hydro::HydroJobSpec {
        workspace,
        scenario_id,
        engine: Some(geospatial::hydro::HydroEngine::Anuga),
        preview: false,
        allow_demo: allow_demo.unwrap_or(true),
        dem_path,
        extent: None,
        ensemble: None,
        enable_swmm: enable_swmm.unwrap_or(false),
    };
    geospatial::hydro::start_scientific_flood(app, spec)
}

/// Cancel an in-flight scientific flood run.
#[tauri::command]
fn cancel_scientific_flood(run_id: String) -> Result<(), String> {
    geospatial::hydro::cancel_run(&run_id)
}

/// List active + persisted flood run statuses (optional workspace filter).
#[tauri::command]
fn list_flood_run_status(workspace: Option<String>) -> Vec<geospatial::hydro::FloodRunStatus> {
    geospatial::hydro::list_run_status(workspace.as_deref())
}

/// Whether the ANUGA sidecar launcher is discoverable (app engines or repo tools).
#[tauri::command]
fn get_flood_engine_status() -> serde_json::Value {
    let anuga = geospatial::hydro::resolve_geo_sidecar("anuga");
    let swmm = geospatial::hydro::resolve_geo_sidecar("swmm");
    serde_json::json!({
        "anugaLauncher": anuga.as_ref().map(|p| p.to_string_lossy().into_owned()),
        "swmmLauncher": swmm.as_ref().map(|p| p.to_string_lossy().into_owned()),
        "anugaReady": anuga.is_some(),
        "swmmReady": swmm.is_some(),
        "cpuLane": "scientific flood / ANUGA",
        "demoAvailable": true,
    })
}

/// Export flood rasters/vectors/time series/3D stubs + scenario manifest.
#[tauri::command]
fn export_flood_products(
    workspace: String,
    run_id: Option<String>,
) -> Result<geospatial::exports::FloodExportResult, String> {
    let ws = PathBuf::from(&workspace);
    geospatial::exports::export_flood_products(&ws, run_id.as_deref())
}

/// Export a single geospatial product kind (optionally copy to `destPath`).
#[tauri::command]
fn export_geo_layer(
    workspace: String,
    kind: String,
    run_id: Option<String>,
    dest_path: Option<String>,
) -> Result<geospatial::exports::LayerExportResult, String> {
    let parsed = geospatial::exports::GeoExportKind::parse(&kind)
        .ok_or_else(|| format!("Unknown export kind '{kind}'"))?;
    let ws = PathBuf::from(&workspace);
    let dest = dest_path.as_ref().map(PathBuf::from);
    geospatial::exports::export_geo_layer(&ws, parsed, run_id.as_deref(), dest.as_deref())
}

#[tauri::command]
fn list_queue() -> serde_json::Value {
    let q = queue::global();
    serde_json::json!({
        "items": q.list(),
        "paused": q.is_paused(),
    })
}

#[tauri::command]
fn pause_queue(app: tauri::AppHandle, paused: bool) {
    queue::global().set_paused(paused);
    queue::emit_now(&app);
}

#[tauri::command]
fn cancel_queue_item(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) {
    queue::global().cancel_item(&id, &state.jobs);
    queue::emit_now(&app);
}

#[tauri::command]
fn clear_finished_queue(app: tauri::AppHandle) {
    queue::global().clear_finished();
    queue::emit_now(&app);
}

#[tauri::command]
fn resume_queue(app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    let q = queue::global();
    q.set_paused(false);
    q.try_start_next(&app, Arc::clone(&state.jobs));
    queue::emit_now(&app);
}

#[tauri::command]
fn list_projects() -> Vec<ProjectSummary> {
    project::list_projects(&pipeline::jobs_dir())
}

#[tauri::command]
fn delete_project(workspace: String) -> Result<(), String> {
    let ws = PathBuf::from(&workspace);
    // Only ever delete inside our own jobs directory.
    if !ws.starts_with(pipeline::jobs_dir()) || Project::load(&ws).is_err() {
        return Err("That is not an InstaSplatter project.".into());
    }
    pipeline::discard_workspace(&ws);
    Ok(())
}

/// Remember the orientation the user set in the viewport, so a reopened
/// project and a later export both come out the same way up.
#[tauri::command]
fn save_project_orientation(workspace: String, rotation: [f32; 9]) -> Result<(), String> {
    let ws = PathBuf::from(&workspace);
    let mut proj = Project::load(&ws)?;
    proj.model_rotation = Some(rotation);
    proj.touch();
    proj.save()
}

/// Consume one-shot developer input paths (single or batch). Ignored unless
/// `INSTASPLATTER_DEV` is set. Shared by Rust setup and the frontend hook so a
/// path is never started twice (HMR / double mount).
fn take_dev_inputs() -> Vec<String> {
    if !dev_mode() {
        return Vec::new();
    }
    static CONSUMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if CONSUMED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Vec::new();
    }

    let mut paths = Vec::new();

    // Semi-colon / newline separated list, or a path to a batch file.
    if let Ok(v) = std::env::var("INSTASPLATTER_BATCH") {
        let v = v.trim().to_string();
        if !v.is_empty() {
            let as_file = PathBuf::from(&v);
            if as_file.is_file() {
                if let Ok(body) = std::fs::read_to_string(&as_file) {
                    paths.extend(parse_path_list(&body));
                }
            } else {
                paths.extend(parse_path_list(&v));
            }
        }
    }

    if let Ok(v) = std::env::var("INSTASPLATTER_AUTOSTART") {
        let v = v.trim().to_string();
        if !v.is_empty() {
            paths.push(v);
        }
    }

    let batch_marker = settings::app_data_dir().join("batch.txt");
    if let Ok(body) = std::fs::read_to_string(&batch_marker) {
        let _ = std::fs::remove_file(&batch_marker);
        paths.extend(parse_path_list(&body));
    }

    let marker = settings::app_data_dir().join("autostart.txt");
    if let Ok(v) = std::fs::read_to_string(&marker) {
        let _ = std::fs::remove_file(&marker);
        let v = v.trim().to_string();
        if !v.is_empty() {
            paths.push(v);
        }
    }

    // De-dupe while preserving order.
    let mut seen = std::collections::HashSet::new();
    paths.retain(|p| seen.insert(p.clone()));
    paths
}

fn parse_path_list(body: &str) -> Vec<String> {
    let body = body.trim_start_matches('\u{feff}');
    body.split(|c| c == '\n' || c == '\r' || c == ';')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Kept for frontend compatibility. Batch / autostart is started from Rust
/// `setup` so multi-file smoke does not depend on WebView boot order.
#[tauri::command]
fn get_autostart() -> Option<String> {
    None
}

/// Start enqueued smoke / batch inputs from Rust so runs do not depend on the
/// WebView finishing boot (agent and CI smoke path).
fn maybe_start_dev_batch(app: &tauri::AppHandle, state: &AppState) {
    let paths = take_dev_inputs();
    if paths.is_empty() {
        return;
    }
    log::info!("dev batch: enqueueing {} input(s)", paths.len());
    let q = queue::global();
    let _ids = q.enqueue(paths, Suite::Reconstruction);
    q.try_start_next(app, Arc::clone(&state.jobs));
    queue::emit_now(app);
}

/// A splat's ground plane, and the rotation that stands the scene upright on
/// it. The viewport offers this as "align up from the ground plane".
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GroundPlane {
    /// Unit normal of the plane, in the splat's own coordinates.
    normal: [f32; 3],
    /// The signed world axis that normal is closest to, as `+y`, `-z` and so on.
    nearest_axis: String,
    /// Row-major 3x3 taking the normal onto the requested up axis.
    rotation: [f32; 9],
}

/// `target` names the axis the ground normal should end up along, as `+y`,
/// `-z` and so on. It defaults to `-y`: COLMAP's world is y-down, so screen up
/// is world `-y`, and standing the ground on it is what makes a scene upright.
/// A caller exporting to a z-up tool passes `+z` instead.
#[tauri::command]
fn estimate_up_axis(splat_path: String, target: Option<String>) -> Result<Option<GroundPlane>, String> {
    let target = match target.as_deref() {
        None => transform::Axis::NegY,
        Some(s) => transform::Axis::parse(s)
            .ok_or_else(|| format!("{s} is not an axis. Use +x, -x, +y, -y, +z or -z."))?,
    };
    let cloud = ply::read_ply(Path::new(&splat_path))?;
    let Some(n) = transform::estimate_ground_normal(&cloud) else {
        return Ok(None);
    };
    let r = transform::align_up(n, target);
    Ok(Some(GroundPlane {
        normal: [n[0] as f32, n[1] as f32, n[2] as f32],
        nearest_axis: transform::Axis::nearest(n).name().to_string(),
        rotation: [
            r[0][0] as f32, r[0][1] as f32, r[0][2] as f32,
            r[1][0] as f32, r[1][1] as f32, r[1][2] as f32,
            r[2][0] as f32, r[2][1] as f32, r[2][2] as f32,
        ],
    }))
}

/// What the export pickers offer, so the extensions live in one place.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FormatChoice {
    extension: String,
    label: String,
}

#[tauri::command]
fn list_export_formats() -> (Vec<FormatChoice>, Vec<FormatChoice>) {
    let splats = [export::Format::Ply, export::Format::Splat, export::Format::Spz]
        .iter()
        .map(|f| FormatChoice {
            extension: f.extension().to_string(),
            label: f.label().to_string(),
        })
        .collect();
    let meshes = [
        (mesh::export::MeshFormat::Glb, "glTF binary"),
        (mesh::export::MeshFormat::Obj, "Wavefront OBJ"),
        (mesh::export::MeshFormat::Ply, "Mesh PLY"),
    ]
    .iter()
    .map(|(f, label)| FormatChoice {
        extension: f.extension().to_string(),
        label: label.to_string(),
    })
    .collect();
    (splats, meshes)
}

/// Write the finished splat where the user asked, in the format their chosen
/// extension implies, with the viewport's orientation baked in.
///
/// The orientation comes from the project file when `workspace` is given and
/// `rotation` is not: the viewport saves it there every time the user turns
/// the model, so export picks up whatever the user last saw without the
/// caller having to track a live renderer state. The common case (PLY, no
/// rotation) is a byte copy, so exporting what was just trained cannot
/// introduce a rounding difference.
#[tauri::command]
fn export_splat(
    result_path: String,
    dest_path: String,
    workspace: Option<String>,
    rotation: Option<[f32; 9]>,
) -> Result<(), String> {
    let src = PathBuf::from(&result_path);
    let dest = PathBuf::from(&dest_path);
    let format = export::Format::from_path(&dest);

    let rotation = rotation.or_else(|| {
        workspace
            .as_deref()
            .and_then(|ws| Project::load(Path::new(ws)).ok())
            .and_then(|p| p.model_rotation)
    });

    let identity = |r: &[f32; 9]| {
        const I: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        r.iter().zip(I).all(|(a, b)| (a - b).abs() < 1e-6)
    };
    let rot = rotation.filter(|r| !identity(r));

    if rot.is_none() && format == export::Format::Ply {
        std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
        return Ok(());
    }

    let mut cloud = ply::read_ply(&src)?;
    if let Some(r) = rot {
        let m = [
            [r[0] as f64, r[1] as f64, r[2] as f64],
            [r[3] as f64, r[4] as f64, r[5] as f64],
            [r[6] as f64, r[7] as f64, r[8] as f64],
        ];
        // Rotate about the scene centre, which is what the viewport does, so
        // the export matches what the user was looking at.
        let (c, _) = cloud.robust_bounds(0.95);
        transform::rotate_cloud(&mut cloud, m, [c[0] as f64, c[1] as f64, c[2] as f64])?;
    }
    export::write(&dest, &cloud, format)
}

/// Progress of a mesh extraction, which runs long enough to need reporting.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshProgress {
    progress: f32,
    detail: String,
}

/// Extract a mesh from a finished reconstruction (ROADMAP-V2 4.3).
///
/// This is an action the user takes after training, never a pipeline stage.
/// It needs the workspace's solved cameras, and the source frames when the
/// mesh is to be coloured.
#[tauri::command]
async fn export_mesh(
    app: tauri::AppHandle,
    workspace: String,
    splat_path: String,
    dest_path: String,
    resolution: Option<u32>,
    textured: Option<bool>,
    quality: Option<String>,
) -> Result<usize, String> {
    use tauri::Emitter;
    let ws = PathBuf::from(&workspace);
    let dest = PathBuf::from(&dest_path);

    let model_dir = colmap::find_model_dir(&ws)
        .ok_or("This project has no solved cameras, so a mesh cannot be built.")?;

    tauri::async_runtime::spawn_blocking(move || {
        let model = colmap::read_model(&model_dir)?;
        let cloud = ply::read_ply(Path::new(&splat_path))?;

        let images = ws.join("images");
        let q = quality.as_deref().unwrap_or("high");
        let mut opts = match q {
            "draft" => mesh::MeshOptions::draft(),
            "max" => mesh::MeshOptions::max(),
            _ => mesh::MeshOptions::default(),
        };
        if let Some(r) = resolution {
            opts.resolution = r.clamp(64, 1024);
        }
        opts.textured = textured.unwrap_or(true) && images.is_dir();
        let opts = opts;

        let m = mesh::extract(&cloud, &model, images.is_dir().then_some(images.as_path()), opts, |p, detail| {
            let _ = app.emit(
                "mesh://progress",
                MeshProgress {
                    progress: p,
                    detail: detail.to_string(),
                },
            );
            Ok(())
        })?;

        mesh::export::write(&dest, &m, mesh::export::MeshFormat::from_path(&dest))?;
        let triangles = m.triangle_count();
        let job_id = ws
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("mesh-export")
            .to_string();
        let _ = app.emit(
            "job://event",
            pipeline::JobEvent::MeshReady {
                job_id,
                path: dest.to_string_lossy().into_owned(),
                triangle_count: triangles as u32,
            },
        );
        Ok(triangles)
    })
    .await
    .map_err(|e| format!("Mesh extraction did not finish: {e}"))?
}

/// Everything needed to debug a stuck or failed run without asking the user
/// to describe their machine over chat (ROADMAP-V2 5.7). `recent_logs` comes
/// from the frontend, which is the only place a full run's log survives;
/// Rust only ever emits events, it does not buffer them.
#[tauri::command]
fn export_diagnostics(
    workspace: Option<String>,
    recent_logs: Vec<String>,
    dest_path: String,
) -> Result<(), String> {
    let mut out = String::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    out.push_str(&format!(
        "InstaSplatter diagnostics\nversion {}\nunix time {now}\n\n",
        env!("CARGO_PKG_VERSION")
    ));

    out.push_str("## Hardware\n");
    let profile = cached_profile();
    out.push_str(&format!(
        "GPU: {} ({:?}), {} MB VRAM, CUDA: {}\nCPU: {} ({} threads)\nRAM: {} MB\nAuto preset: {:?}\n\n",
        profile.gpu_name,
        profile.gpu_vendor,
        profile.vram_mb,
        profile.has_cuda,
        profile.cpu_name,
        profile.cpu_threads,
        profile.ram_mb,
        profile.auto_preset,
    ));

    out.push_str("## Engines\n");
    let st = engines::status();
    out.push_str(&format!(
        "colmap: {}, brush: {}{}, ffmpeg: {}, da3: {}, dav2: {}, mapanything: {}, vggt-commercial: {}, vggt-omega: {}, fixer: {}, gsplat: {}\n\n",
        st.colmap,
        st.brush,
        if st.brush_custom { " (custom)" } else { "" },
        st.ffmpeg,
        st.depth_anything_3,
        st.depth_anything_v2,
        st.mapanything,
        st.vggt_commercial,
        st.vggt_omega,
        st.fixer,
        st.gsplat
    ));

    out.push_str("## Settings\n");
    let raw = Settings::load();
    let resolved = settings::resolve(&raw, profile);
    out.push_str(&format!("{raw:#?}\n\nResolved:\n{resolved:#?}\n\n"));

    if let Some(ws) = &workspace {
        out.push_str("## Project\n");
        match Project::load(Path::new(ws)) {
            Ok(p) => {
                out.push_str(&format!(
                    "job_id: {}\ninput: {}\ncompleted: {}\nresumable: {}\nlatest_iter: {} / {}\n\n",
                    p.job_id,
                    p.input_path,
                    p.completed,
                    p.is_resumable(),
                    p.latest_iter,
                    p.total_steps,
                ));
            }
            Err(e) => out.push_str(&format!("Could not read project.json: {e}\n\n")),
        }
    }

    if !recent_logs.is_empty() {
        out.push_str("## Recent log\n");
        for line in &recent_logs {
            out.push_str(line);
            out.push('\n');
        }
    }

    std::fs::write(&dest_path, out).map_err(|e| format!("Cannot write {dest_path}: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<AppState>().inner().clone();
            // Defer one tick so plugins / window are live before GPU jobs start.
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                maybe_start_dev_batch(&handle, &state);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_hardware_profile,
            get_settings,
            set_settings,
            get_resolved_settings,
            get_engine_status,
            install_engines,
            get_autostart,
            start_job,
            cancel_job,
            enqueue_jobs,
            list_queue,
            pause_queue,
            resume_queue,
            cancel_queue_item,
            clear_finished_queue,
            get_suite,
            set_suite,
            get_geo_catalog_info,
            import_geo_telemetry,
            set_geo_gcps,
            compute_geo_reference,
            plan_geo_extent,
            get_geo_reference,
            start_scientific_flood,
            cancel_scientific_flood,
            list_flood_run_status,
            get_flood_engine_status,
            export_flood_products,
            export_geo_layer,
            resume_project,
            list_projects,
            delete_project,
            save_project_orientation,
            estimate_up_axis,
            list_export_formats,
            export_splat,
            export_mesh,
            export_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
