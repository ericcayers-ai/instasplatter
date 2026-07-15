//! Pipeline orchestrator: ingestion, frame gating, camera solving, live
//! Gaussian training, export. Progress is streamed to the UI as it happens.
//!
//! Dual-mode (v0.5):
//! - **Standard**: VGGT-Commercial primary poses → COLMAP BA/fallback; RoMa∧DAV2∧MVS.
//! - **Experimental**: Ω→MASt3R→DUSt3R→VGGT-C→COLMAP; merge-all densify; Difix+Fixer.

pub mod brush;
pub mod colmap;
pub mod dense;
pub mod gating;
pub mod gsplat;
pub mod ingest;
pub mod sidecars;
pub mod solver;

use crate::project::Project;
use crate::settings::{app_data_dir, ResolvedSettings};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum JobEvent {
    #[serde(rename_all = "camelCase")]
    StageStarted { job_id: String, stage: String, label: String },
    #[serde(rename_all = "camelCase")]
    StageProgress {
        job_id: String,
        stage: String,
        progress: f32,
        detail: String,
    },
    #[serde(rename_all = "camelCase")]
    Log { job_id: String, line: String },
    /// A camera pose was solved and can be drawn in the viewport.
    #[serde(rename_all = "camelCase")]
    CameraRegistered {
        job_id: String,
        name: String,
        registered: u32,
        total: u32,
        /// Fraction of matched features that survived pose estimation.
        confidence: f32,
        apex: [f32; 3],
        corners: [[f32; 3]; 4],
    },
    /// Something worth telling the user plainly, without failing the job.
    #[serde(rename_all = "camelCase")]
    Notice { job_id: String, message: String },
    #[serde(rename_all = "camelCase")]
    SplatReady {
        job_id: String,
        path: String,
        iter: u32,
        total_steps: u32,
    },
    #[serde(rename_all = "camelCase")]
    Done {
        job_id: String,
        path: String,
        elapsed_secs: f64,
    },
    #[serde(rename_all = "camelCase")]
    Error { job_id: String, message: String },
    #[serde(rename_all = "camelCase")]
    Cancelled { job_id: String },
}

pub struct JobHandle {
    pub cancel: Arc<AtomicBool>,
    pub child_pids: Arc<Mutex<Vec<u32>>>,
}

impl JobHandle {
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        let pids = std::mem::take(&mut *self.child_pids.lock().unwrap());
        for pid in pids {
            // /T kills the whole child process tree; COLMAP spawns workers.
            let _ = crate::profiler::hidden_command("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output();
        }
    }
}

#[derive(Clone)]
pub struct JobCtx {
    pub app: tauri::AppHandle,
    pub job_id: String,
    pub workspace: PathBuf,
    pub settings: ResolvedSettings,
    pub cancel: Arc<AtomicBool>,
    pub child_pids: Arc<Mutex<Vec<u32>>>,
    /// Autosaved after every meaningful change so an interrupted run stays
    /// resumable (ROADMAP-V2 1.4).
    pub project: Arc<Mutex<Project>>,
}

impl JobCtx {
    pub fn emit(&self, event: JobEvent) {
        let _ = self.app.emit("job://event", event);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn check_cancel(&self) -> Result<(), String> {
        if self.cancelled() {
            Err("__cancelled__".into())
        } else {
            Ok(())
        }
    }

    pub fn stage_started(&self, stage: &str, label: &str) {
        self.emit(JobEvent::StageStarted {
            job_id: self.job_id.clone(),
            stage: stage.into(),
            label: label.into(),
        });
    }

    pub fn stage_progress(&self, stage: &str, progress: f32, detail: &str) {
        self.emit(JobEvent::StageProgress {
            job_id: self.job_id.clone(),
            stage: stage.into(),
            progress: progress.clamp(0.0, 1.0),
            detail: detail.into(),
        });
    }

    pub fn log(&self, line: impl Into<String>) {
        self.emit(JobEvent::Log {
            job_id: self.job_id.clone(),
            line: line.into(),
        });
    }

    pub fn notice(&self, message: impl Into<String>) {
        let message = message.into();
        self.log(message.clone());
        self.emit(JobEvent::Notice {
            job_id: self.job_id.clone(),
            message,
        });
    }

    /// Mutate and persist the project manifest. Save failures are logged but
    /// never abort a run that is otherwise going fine.
    pub fn update_project(&self, f: impl FnOnce(&mut Project)) {
        let mut p = self.project.lock().unwrap();
        f(&mut p);
        p.touch();
        if let Err(e) = p.save() {
            let _ = self.app.emit(
                "job://event",
                JobEvent::Log {
                    job_id: self.job_id.clone(),
                    line: format!("[warn] could not autosave the project: {e}"),
                },
            );
        }
    }
}

pub fn jobs_dir() -> PathBuf {
    app_data_dir().join("jobs")
}

/// Remove what a finished job no longer needs. Windows can hold file locks
/// briefly after a child dies, so deletion is retried.
///
/// The solved poses and the gated frames stay: mesh export reads both, and a
/// reopened project would otherwise show a splat with no cameras.
pub fn clean_intermediates(workspace: &Path) {
    for name in ["frames_raw", "exports"] {
        let p = workspace.join(name);
        for _ in 0..5 {
            if !p.exists() || std::fs::remove_dir_all(&p).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    for name in ["database.db", "init.ply"] {
        let _ = std::fs::remove_file(workspace.join(name));
    }
}

/// Delete a cancelled job's workspace entirely.
pub fn discard_workspace(workspace: &Path) {
    for _ in 0..5 {
        if !workspace.exists() || std::fs::remove_dir_all(workspace).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Solve camera poses.
/// Capture-aware candidate routing + scored hypotheses → COLMAP refine.
/// Live-init (when enabled) still runs first and falls back into this chain.
async fn solve_cameras(ctx: &JobCtx, images_dir: &Path) -> Result<(), String> {
    if ctx.settings.live_init {
        match crate::sfm::run_incremental(ctx, images_dir).await {
            Ok(()) => {
                ctx.notice(solver::camera_chip("live-init"));
                return Ok(());
            }
            Err(e) if e == "__cancelled__" => return Err(e),
            Err(reason) => {
                ctx.notice(format!(
                    "Live camera tracking stopped early ({reason}). Falling back to the batch camera solver."
                ));
                ctx.check_cancel()?;
                let _ = std::fs::remove_dir_all(ctx.workspace.join("sparse"));
                let _ = std::fs::remove_file(brush::init_ply_path(&ctx.workspace));
            }
        }
    }

    let st = sidecars::status();
    let profile = solver::detect_capture_profile(
        images_dir,
        &ctx.workspace,
        ctx.settings.experimental_mode,
    );
    ctx.log(format!("Capture profile: {profile:?}"));
    let chain: Vec<&str> = if ctx.settings.experimental_mode {
        solver::experimental_pose_candidates(profile, &st)
    } else {
        solver::standard_pose_candidates(profile, &st)
    };

    let prefer_pose_prior = chain.contains(&"colmap-pose-prior")
        || matches!(profile, solver::CaptureProfile::GpsRtkDrone);

    // GPS/RTK: try COLMAP pose-prior mapper before neural hypotheses.
    if prefer_pose_prior && solver::has_pose_priors(&ctx.workspace) {
        ctx.stage_progress("sfm", 0.05, "COLMAP pose-prior mapper…");
        match colmap::run_sfm_with_options(ctx, images_dir, colmap::SfmOptions {
            use_pose_priors: true,
            use_gravity_prior: true,
            matcher_front_end: solver::matcher_front_end(
                &ctx.settings,
                sidecars::lightglue_installed(),
            )
            .to_string(),
        })
        .await
        {
            Ok(()) => {
                ctx.notice(solver::camera_chip("colmap-pose-prior"));
                ctx.notice(format!("Trainer: {}", ctx.settings.trainer));
                return Ok(());
            }
            Err(e) => {
                ctx.notice(format!(
                    "Pose-prior mapper failed ({e}). Evaluating neural hypotheses…"
                ));
            }
        }
    }

    if !chain.is_empty() {
        ctx.stage_progress("sfm", 0.05, "Neural camera solver…");
        match sidecars::try_neural_poses(ctx, images_dir, &chain).await? {
            Some(name) => {
                // Guaranteed BA after neural initialization.
                let _ = sidecars::maybe_refine_poses_with_colmap(ctx, images_dir).await;
                ctx.log(format!("Camera solver: {name}"));
                ctx.notice(format!("Trainer: {}", ctx.settings.trainer));
                return Ok(());
            }
            None => {
                ctx.notice(if ctx.settings.experimental_mode {
                    "Neural pose hypotheses rejected. Falling back to COLMAP."
                } else {
                    "Commercial pose hypotheses unavailable or rejected. Falling back to COLMAP."
                });
            }
        }
    }

    colmap::run_sfm(ctx, images_dir).await?;
    if chain.is_empty() {
        ctx.notice(solver::camera_chip("colmap"));
    } else {
        ctx.notice(solver::camera_chip("colmap-fallback"));
    }
    ctx.notice(format!("Trainer: {}", ctx.settings.trainer));
    Ok(())
}

/// Run the full pipeline for one input. Returns the final .ply path.
pub async fn run_job(ctx: &JobCtx, input: &Path) -> Result<PathBuf, String> {
    let started = std::time::Instant::now();

    ctx.stage_started("ingest", "Reading input");
    let images_dir = ingest::ingest(ctx, input).await?;
    ctx.check_cancel()?;

    ctx.stage_started("sfm", "Solving cameras");
    solve_cameras(ctx, &images_dir).await?;
    ctx.check_cancel()?;
    // Dense geometry bootstrap: neural sidecar → COLMAP MVS → sparse seed.
    let _ = dense::densify_after_sfm(ctx, &images_dir).await?;
    ctx.check_cancel()?;
    let sparse = ctx.workspace.join("sparse");
    ctx.update_project(|p| p.sparse_dir = Some(sparse.to_string_lossy().into_owned()));

    ctx.stage_started("train", "Training splats");
    let final_ply = train_with_engine(ctx, None).await?;
    ctx.check_cancel()?;

    finalize(ctx, &final_ply, started).await
}

/// Resume an interrupted run from its saved checkpoint (ROADMAP-V2 1.4).
pub async fn resume_job(ctx: &JobCtx, checkpoint: PathBuf, start_iter: u32) -> Result<PathBuf, String> {
    let started = std::time::Instant::now();

    ctx.stage_started("ingest", "Reading input");
    ctx.stage_progress("ingest", 1.0, "Frames already prepared");
    ctx.stage_started("sfm", "Solving cameras");
    ctx.stage_progress("sfm", 1.0, "Cameras already solved");

    ctx.stage_started("train", "Training splats");
    let final_ply = train_with_engine(ctx, Some((checkpoint, start_iter))).await?;
    ctx.check_cancel()?;

    finalize(ctx, &final_ply, started).await
}

async fn train_with_engine(
    ctx: &JobCtx,
    resume: Option<(PathBuf, u32)>,
) -> Result<PathBuf, String> {
    match ctx.settings.trainer.as_str() {
        "gsplat" => gsplat::train(ctx, resume).await,
        "brush" => brush::train(ctx, resume).await,
        // "auto" already resolved in settings::resolve
        other => {
            ctx.notice(format!("Unknown trainer '{other}', using Brush."));
            brush::train(ctx, resume).await
        }
    }
}

async fn finalize(
    ctx: &JobCtx,
    final_ply: &Path,
    started: std::time::Instant,
) -> Result<PathBuf, String> {
    ctx.stage_started("finalize", "Finalizing");
    let result = ctx.workspace.join("result.ply");
    std::fs::copy(final_ply, &result).map_err(|e| e.to_string())?;

    // ROADMAP-V2 1.5: bake the Mip-Splatting filter into what the user keeps.
    let ctx_ref = ctx;
    let result_ref = result.clone();
    tokio::task::block_in_place(move || brush::bake_final_filter(ctx_ref, &result_ref))?;

    // NVIDIA Fixer (commercial) or Difix (research) polish when installed.
    let _ = sidecars::try_polish(ctx, &result).await;

    // ROADMAP-V2 1.4: autosave, so a completed reconstruction is never lost.
    // Recorded before the cleanup below deletes the exports it superseded.
    let result_str = result.to_string_lossy().into_owned();
    ctx.update_project(|p| {
        p.completed = true;
        p.latest_splat = Some(result_str.clone());
        p.latest_iter = p.total_steps;
    });

    if ctx.settings.keep_intermediates {
        let _ = std::fs::remove_file(ctx.workspace.join("database.db"));
    } else {
        clean_intermediates(&ctx.workspace);
    }

    ctx.emit(JobEvent::Done {
        job_id: ctx.job_id.clone(),
        path: result_str,
        elapsed_secs: started.elapsed().as_secs_f64(),
    });
    Ok(result)
}
