mod colmap;
mod engines;
mod math;
mod mesh;
mod pipeline;
mod profiler;
mod project;
mod settings;
mod sfm;
mod splat;

use pipeline::{JobCtx, JobEvent, JobHandle};
use profiler::HardwareProfile;
use project::{Project, ProjectSummary};
use settings::{ResolvedSettings, Settings};
use splat::{export, ply, transform};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Default)]
struct AppState {
    jobs: Mutex<HashMap<String, Arc<JobHandle>>>,
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
    if !st.colmap || !st.brush {
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
    if !st.brush {
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

/// Dev/test hook: starts a job on launch. Reads INSTASPLATTER_AUTOSTART, or a
/// single-shot `autostart.txt` in the app data dir (consumed on read). Both are
/// ignored unless INSTASPLATTER_DEV is set.
#[tauri::command]
fn get_autostart() -> Option<String> {
    if !dev_mode() {
        return None;
    }
    // Single-shot per app process: frontend reloads (HMR) must not re-trigger.
    static CONSUMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if CONSUMED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    if let Ok(v) = std::env::var("INSTASPLATTER_AUTOSTART") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let marker = settings::app_data_dir().join("autostart.txt");
    if let Ok(v) = std::fs::read_to_string(&marker) {
        let _ = std::fs::remove_file(&marker);
        let v = v.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
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
/// The common case (PLY, no rotation) is a byte copy, so exporting what was
/// just trained cannot introduce a rounding difference.
#[tauri::command]
fn export_splat(
    result_path: String,
    dest_path: String,
    rotation: Option<[f32; 9]>,
) -> Result<(), String> {
    let src = PathBuf::from(&result_path);
    let dest = PathBuf::from(&dest_path);
    let format = export::Format::from_path(&dest);

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
        let opts = mesh::MeshOptions {
            resolution: resolution.unwrap_or(384).clamp(64, 1024),
            textured: textured.unwrap_or(true) && images.is_dir(),
            ..Default::default()
        };

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
        Ok(m.triangle_count())
    })
    .await
    .map_err(|e| format!("Mesh extraction did not finish: {e}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
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
            resume_project,
            list_projects,
            delete_project,
            save_project_orientation,
            estimate_up_axis,
            list_export_formats,
            export_splat,
            export_mesh,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
