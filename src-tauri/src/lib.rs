mod engines;
mod pipeline;
mod profiler;
mod settings;

use pipeline::{JobCtx, JobEvent, JobHandle};
use profiler::HardwareProfile;
use settings::{ResolvedSettings, Settings};
use std::collections::HashMap;
use std::path::PathBuf;
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
    let cancel = Arc::new(AtomicBool::new(false));
    let child_pids = Arc::new(Mutex::new(Vec::new()));

    let handle = Arc::new(JobHandle {
        cancel: cancel.clone(),
        child_pids: child_pids.clone(),
        workspace: workspace.clone(),
    });
    state.jobs.lock().unwrap().insert(job_id.clone(), handle);

    let ctx = JobCtx {
        app: app.clone(),
        job_id: job_id.clone(),
        workspace,
        settings: resolved,
        cancel,
        child_pids,
    };

    let ret_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        match pipeline::run_job(&ctx, &input).await {
            Ok(_) => {}
            Err(e) if e == "__cancelled__" => {
                let _ = ctx.app.emit(
                    "job://event",
                    JobEvent::Cancelled {
                        job_id: ctx.job_id.clone(),
                    },
                );
            }
            Err(e) => {
                let _ = ctx.app.emit(
                    "job://event",
                    JobEvent::Error {
                        job_id: ctx.job_id.clone(),
                        message: e,
                    },
                );
            }
        }
    });

    Ok(ret_id)
}

#[tauri::command]
fn cancel_job(state: tauri::State<'_, AppState>, job_id: String) -> Result<(), String> {
    if let Some(handle) = state.jobs.lock().unwrap().get(&job_id) {
        handle.request_cancel();
        Ok(())
    } else {
        Err("unknown job".into())
    }
}

/// Dev/test hook: starts a job on launch. Reads INSTASPLATTER_AUTOSTART env
/// var, or a single-shot `autostart.txt` in the app data dir (consumed on read).
#[tauri::command]
fn get_autostart() -> Option<String> {
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

#[tauri::command]
fn export_splat(result_path: String, dest_path: String) -> Result<(), String> {
    std::fs::copy(&result_path, &dest_path)
        .map(|_| ())
        .map_err(|e| e.to_string())
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
            export_splat,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
