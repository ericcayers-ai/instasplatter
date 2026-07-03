//! Pipeline orchestrator (ROADMAP §3): ingestion → frame gating → SfM →
//! live Gaussian training → export, streaming progress events to the UI.

pub mod brush;
pub mod colmap;
pub mod gating;
pub mod ingest;

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
    #[allow(dead_code)]
    pub workspace: PathBuf,
}

impl JobHandle {
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        let pids = self.child_pids.lock().unwrap().clone();
        for pid in pids {
            // /T kills the whole child process tree (COLMAP spawns workers).
            let _ = crate::profiler::hidden_command("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output();
        }
    }
}

pub struct JobCtx {
    pub app: tauri::AppHandle,
    pub job_id: String,
    pub workspace: PathBuf,
    pub settings: ResolvedSettings,
    pub cancel: Arc<AtomicBool>,
    pub child_pids: Arc<Mutex<Vec<u32>>>,
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
}

pub fn jobs_dir() -> PathBuf {
    app_data_dir().join("jobs")
}

/// Run the full pipeline for one input. Returns the final .ply path.
pub async fn run_job(ctx: &JobCtx, input: &Path) -> Result<PathBuf, String> {
    let started = std::time::Instant::now();

    // Stage 0/1 — ingest + frame gating
    ctx.stage_started("ingest", "Reading input");
    let images_dir = ingest::ingest(ctx, input).await?;
    ctx.check_cancel()?;

    // Stage 2 — SfM (COLMAP)
    ctx.stage_started("sfm", "Solving cameras");
    colmap::run_sfm(ctx, &images_dir).await?;
    ctx.check_cancel()?;

    // Stage 4 — live Gaussian training (Brush)
    ctx.stage_started("train", "Training splats");
    let final_ply = brush::train(ctx).await?;
    ctx.check_cancel()?;

    // Stage 6/7 — finalize
    ctx.stage_started("finalize", "Finalizing");
    let result = ctx.workspace.join("result.ply");
    std::fs::copy(&final_ply, &result).map_err(|e| e.to_string())?;

    if !ctx.settings.keep_intermediates {
        // Keep images + result; drop the COLMAP database to save space.
        let _ = std::fs::remove_file(ctx.workspace.join("database.db"));
    }

    ctx.emit(JobEvent::Done {
        job_id: ctx.job_id.clone(),
        path: result.to_string_lossy().into_owned(),
        elapsed_secs: started.elapsed().as_secs_f64(),
    });
    Ok(result)
}
