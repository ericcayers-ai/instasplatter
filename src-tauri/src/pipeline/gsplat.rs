//! Optional NVIDIA train path via the gsplat Python sidecar.
//!
//! When CUDA is available and `engines/sidecars/gsplat-train` is installed,
//! Auto trainer selection prefers this over Brush. The sidecar speaks the same
//! export/`STEP` dialect the Brush watcher understands.

use super::brush::{exports_dir, init_ply_path};
use super::JobCtx;
use crate::colmap;
use crate::settings::app_data_dir;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    images_dir: &'a str,
    workspace: &'a str,
    sparse_dir: Option<&'a str>,
    max_steps: u32,
    max_splats: u32,
    max_resolution: u32,
    sh_degree: u32,
    export_every: u32,
    ssim_weight: f32,
    opac_loss_weight: f64,
    scale_loss_weight: f64,
    strategy: &'a str,
    absgrad: bool,
    antialiased: bool,
    app_opt: bool,
    bilateral_grid: bool,
    init_ply: Option<&'a str>,
    export_dir: &'a str,
}

fn launcher() -> PathBuf {
    let dir = app_data_dir().join("engines").join("sidecars").join("gsplat-train");
    #[cfg(windows)]
    {
        let bat = dir.join("run.bat");
        if bat.exists() {
            return bat;
        }
    }
    let py = dir.join("run.py");
    if py.exists() {
        return py;
    }
    dir.join("run")
}

pub fn is_installed() -> bool {
    launcher().exists()
}

/// Ensure COLMAP text files exist so the mini trainer can load poses.
fn ensure_sparse_txt(model_dir: &Path) -> Result<(), String> {
    let needs = !(model_dir.join("cameras.txt").exists() && model_dir.join("images.txt").exists());
    if !needs {
        return Ok(());
    }
    let model = colmap::read_model(model_dir)?;
    colmap::write_model_txt(model_dir, &model)
}

/// Train with gsplat. Returns the newest exported PLY.
pub async fn train(ctx: &JobCtx, resume: Option<(PathBuf, u32)>) -> Result<PathBuf, String> {
    let launch = launcher();
    if !launch.exists() {
        return Err(
            "gsplat-train sidecar is not installed. See tools/sidecars/gsplat-train/README.md."
                .into(),
        );
    }

    let model_dir = colmap::find_model_dir(&ctx.workspace)
        .ok_or("No sparse model for gsplat training.")?;
    ensure_sparse_txt(&model_dir)?;

    let exports = exports_dir(&ctx.workspace);
    std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;

    if let Some((checkpoint, iter)) = &resume {
        ctx.log(format!("gsplat resume from step {iter} (seeds init.ply)."));
        std::fs::copy(checkpoint, init_ply_path(&ctx.workspace))
            .map_err(|e| format!("Could not stage the checkpoint: {e}"))?;
    }

    let images = ctx.workspace.join("images");
    let images = if images.is_dir() {
        images
    } else {
        // ingest may leave frames at workspace root under a named folder.
        ctx.workspace.clone()
    };

    let s = &ctx.settings;
    let strategy = s.gsplat_strategy.as_str();
    let init = init_ply_path(&ctx.workspace);
    let init_s = init.exists().then(|| init.to_string_lossy().into_owned());
    let export_s = exports.to_string_lossy().into_owned();
    let sparse_s = model_dir.to_string_lossy().into_owned();
    // Map Brush-style opac/scale L1 (tiny) into gsplat MCMC-style regs (~0.01).
    let opac = (s.opac_loss_weight * 5e5).clamp(1e-4, 0.05);
    let scale = (s.scale_loss_weight * 5e4).clamp(1e-4, 0.05);

    let req = Request {
        images_dir: &images.to_string_lossy(),
        workspace: &ctx.workspace.to_string_lossy(),
        sparse_dir: Some(&sparse_s),
        max_steps: s.total_steps,
        max_splats: s.max_splats,
        max_resolution: s.max_resolution,
        sh_degree: s.sh_degree,
        export_every: s.export_every,
        ssim_weight: s.ssim_weight,
        opac_loss_weight: opac,
        scale_loss_weight: scale,
        strategy,
        absgrad: s.gsplat_absgrad,
        antialiased: s.gsplat_antialiased,
        app_opt: s.gsplat_appearance,
        bilateral_grid: s.gsplat_bilateral_grid,
        init_ply: init_s.as_deref(),
        export_dir: &export_s,
    };
    let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;

    ctx.log(format!(
        "Training with gsplat ({strategy}; absgrad={} antialiased={} appearance={} bilagrid={})",
        s.gsplat_absgrad, s.gsplat_antialiased, s.gsplat_appearance, s.gsplat_bilateral_grid
    ));
    if s.gsplat_strategy == "mcmc" && s.gsplat_absgrad {
        ctx.log("Note: AbsGrad disabled under MCMC (mutually exclusive strategies).");
    }

    let mut cmd = if launch.extension().and_then(|e| e.to_str()) == Some("py") {
        let mut c = tokio::process::Command::new("python");
        c.arg(&launch);
        c
    } else {
        tokio::process::Command::new(&launch)
    };
    #[cfg(windows)]
    cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(launch.parent().unwrap_or(Path::new(".")));

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start gsplat-train: {e}"))?;
    let pid = child.id();
    if let Some(pid) = pid {
        ctx.child_pids.lock().unwrap().push(pid);
    }
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }

    let mut watcher = ExportWatcher {
        // Reuse brush watcher internals via a local clone of the poll pattern.
        // We keep an identical scan of exports/.
        announced: resume.as_ref().map(|(_, i)| *i).unwrap_or(0),
        latest: None,
        dir: exports.clone(),
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let total_steps = s.total_steps;
    let log_app = ctx.app.clone();
    let log_job = ctx.job_id.clone();
    let stdout_task = tokio::spawn(async move {
        use tauri::Emitter;
        let mut lines = BufReader::new(stdout).lines();
        let mut last_path: Option<String> = None;
        while let Ok(Some(l)) = lines.next_line().await {
            let t = l.trim().to_string();
            if t.is_empty() {
                continue;
            }
            let _ = log_app.emit(
                "job://event",
                super::JobEvent::Log {
                    job_id: log_job.clone(),
                    line: t.clone(),
                },
            );
            // Final absolute PLY path is a lone path line from run.py.
            if t.ends_with(".ply") && (t.contains(':') || t.starts_with('/') || t.starts_with('\\'))
            {
                last_path = Some(t);
            }
        }
        last_path
    });
    let stderr_task = {
        let log_app = ctx.app.clone();
        let log_job = ctx.job_id.clone();
        tokio::spawn(async move {
            use tauri::Emitter;
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let t = l.trim().to_string();
                if t.is_empty() {
                    continue;
                }
                let _ = log_app.emit(
                    "job://event",
                    super::JobEvent::Log {
                        job_id: log_job.clone(),
                        line: format!("[gsplat] {t}"),
                    },
                );
            }
        })
    };

    let outcome = loop {
        if ctx.cancelled() {
            let _ = child.kill().await;
            break Err("__cancelled__".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            tokio::task::block_in_place(|| {
                let _ = poll_exports(ctx, &mut watcher, total_steps);
            });
            if !status.success() {
                break Err(format!(
                    "gsplat training failed (exit {:?}). Check the log; confirm CUDA PyTorch + gsplat are installed.",
                    status.code()
                ));
            }
            break Ok(());
        }
        tokio::task::block_in_place(|| {
            let _ = poll_exports(ctx, &mut watcher, total_steps);
        });
        tokio::time::sleep(Duration::from_millis(700)).await;
    };

    let _ = child.wait().await;
    if let Some(pid) = pid {
        ctx.child_pids.lock().unwrap().retain(|p| *p != pid);
    }
    let final_from_stdout = stdout_task.await.ok().flatten();
    let _ = stderr_task.await;
    outcome?;

    if let Some(p) = final_from_stdout {
        let path = PathBuf::from(&p);
        if path.exists() {
            return Ok(path);
        }
    }
    if let Some(p) = watcher.latest {
        return Ok(p);
    }
    // Prefer result_gsplat.ply / highest export.
    let staged = ctx.workspace.join("result_gsplat.ply");
    if staged.exists() {
        return Ok(staged);
    }
    Err("gsplat finished without a PLY export.".into())
}

struct ExportWatcher {
    dir: PathBuf,
    announced: u32,
    latest: Option<PathBuf>,
}

fn scan_exports(dir: &Path) -> std::collections::BTreeMap<u32, PathBuf> {
    let mut map = std::collections::BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            let name = match p.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if let Some(rest) = name.strip_prefix("export_") {
                if let Some(num) = rest.strip_suffix(".ply") {
                    if let Ok(iter) = num.parse::<u32>() {
                        map.insert(iter, p);
                    }
                }
            }
        }
    }
    map
}

fn poll_exports(ctx: &JobCtx, watcher: &mut ExportWatcher, total_steps: u32) -> Option<u32> {
    let map = scan_exports(&watcher.dir);
    let (iter, path) = map.iter().next_back()?;
    if *iter <= watcher.announced {
        return None;
    }
    watcher.announced = *iter;
    watcher.latest = Some(path.clone());
    let splat = path.to_string_lossy().into_owned();
    let at = *iter;
    ctx.update_project(|p| {
        p.latest_splat = Some(splat);
        p.latest_iter = at;
    });
    ctx.stage_progress(
        "train",
        *iter as f32 / total_steps.max(1) as f32,
        &format!("step {iter} / {total_steps}"),
    );
    ctx.emit(super::JobEvent::SplatReady {
        job_id: ctx.job_id.clone(),
        path: path.to_string_lossy().into_owned(),
        iter: *iter,
        total_steps,
    });
    Some(*iter)
}
