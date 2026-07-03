//! Stage 4 — live Gaussian Splat training via Brush (ROADMAP §3 stage 4).
//! Spawns `brush_app.exe` headless on the COLMAP workspace, watches the
//! export directory and streams every intermediate .ply to the viewport.

use super::{JobCtx, JobEvent};
use crate::engines::brush_exe;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Scan the exports dir for `export_<iter>.ply` files; returns iter → path.
fn scan_exports(dir: &PathBuf) -> BTreeMap<u32, PathBuf> {
    let mut map = BTreeMap::new();
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

fn is_file_stable(p: &PathBuf) -> bool {
    // A ply is "done writing" if its size is unchanged across a short window.
    let s1 = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    std::thread::sleep(Duration::from_millis(150));
    let s2 = std::fs::metadata(p).map(|m| m.len()).unwrap_or(1);
    s1 == s2 && s1 > 0
}

pub async fn train(ctx: &JobCtx) -> Result<PathBuf, String> {
    let s = &ctx.settings;
    let exports = ctx.workspace.join("exports");
    std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;

    let mut cmd = tokio::process::Command::new(brush_exe());
    #[cfg(windows)]
    cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    // cwd = workspace so Brush's autotune cache lands there, not in the app dir.
    cmd.current_dir(&ctx.workspace);
    cmd.arg(&ctx.workspace)
        .args(["--total-steps", &s.total_steps.to_string()])
        .args(["--max-splats", &s.max_splats.to_string()])
        .args(["--sh-degree", &s.sh_degree.to_string()])
        .args(["--max-resolution", &s.max_resolution.to_string()])
        .args(["--refine-every", &s.refine_every.to_string()])
        .args(["--ssim-weight", &s.ssim_weight.to_string()])
        .args(["--opac-loss-weight", &format!("{:e}", s.opac_loss_weight)])
        .args(["--scale-loss-weight", &format!("{:e}", s.scale_loss_weight)])
        .args(["--mean-noise-weight", &s.mean_noise_weight.to_string()])
        .args(["--export-every", &s.export_every.to_string()])
        .args(["--export-path", &exports.to_string_lossy()])
        .args(["--export-name", "export_{iter}.ply"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to start Brush: {e}"))?;
    if let Some(pid) = child.id() {
        ctx.child_pids.lock().unwrap().push(pid);
    }

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Forward Brush output to the UI log and opportunistically parse steps.
    let log_ctx_app = ctx.app.clone();
    let log_job = ctx.job_id.clone();
    let total_steps = s.total_steps;
    let stdout_task = tokio::spawn(async move {
        use tauri::Emitter;
        let step_re = |l: &str| -> Option<u32> {
            // Match "step 1234" / "Step: 1234" / "1234/30000"-style output.
            let lower = l.to_ascii_lowercase();
            if let Some(pos) = lower.find("step") {
                let tail: String = lower[pos + 4..]
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                return tail.parse().ok();
            }
            if let Some((a, b)) = l.split_once('/') {
                let a: String = a.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
                let a: String = a.chars().rev().collect();
                if b.trim_start().starts_with(&total_steps.to_string()) {
                    return a.parse().ok();
                }
            }
            None
        };
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            let t = l.trim().to_string();
            if t.is_empty() {
                continue;
            }
            if let Some(step) = step_re(&t) {
                let _ = log_ctx_app.emit(
                    "job://event",
                    JobEvent::StageProgress {
                        job_id: log_job.clone(),
                        stage: "train".into(),
                        progress: step as f32 / total_steps as f32,
                        detail: t.clone(),
                    },
                );
            }
            let _ = log_ctx_app.emit(
                "job://event",
                JobEvent::Log {
                    job_id: log_job.clone(),
                    line: format!("[brush] {t}"),
                },
            );
        }
    });
    let err_app = ctx.app.clone();
    let err_job = ctx.job_id.clone();
    let stderr_task = tokio::spawn(async move {
        use tauri::Emitter;
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            let t = l.trim().to_string();
            if !t.is_empty() {
                let _ = err_app.emit(
                    "job://event",
                    JobEvent::Log {
                        job_id: err_job.clone(),
                        line: format!("[brush] {t}"),
                    },
                );
            }
        }
    });

    // Watch the export dir; emit SplatReady for each new stable checkpoint.
    let mut announced: u32 = 0;
    let mut last_path: Option<PathBuf> = None;
    loop {
        if ctx.cancelled() {
            let _ = child.kill().await;
            return Err("__cancelled__".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            // Process ended — pick up any remaining exports below.
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            let map = scan_exports(&exports);
            if let Some((iter, path)) = map.iter().next_back() {
                if *iter > announced {
                    ctx.emit(JobEvent::SplatReady {
                        job_id: ctx.job_id.clone(),
                        path: path.to_string_lossy().into_owned(),
                        iter: *iter,
                        total_steps,
                    });
                }
                last_path = Some(path.clone());
            }
            if !status.success() {
                return Err(format!("Brush training failed (exit {:?})", status.code()));
            }
            break;
        }

        let map = tokio::task::block_in_place(|| scan_exports(&exports));
        if let Some((iter, path)) = map.iter().next_back() {
            if *iter > announced {
                let stable = {
                    let p = path.clone();
                    tokio::task::block_in_place(move || is_file_stable(&p))
                };
                if stable {
                    announced = *iter;
                    last_path = Some(path.clone());
                    ctx.stage_progress(
                        "train",
                        *iter as f32 / total_steps as f32,
                        &format!("step {iter} / {total_steps}"),
                    );
                    ctx.emit(JobEvent::SplatReady {
                        job_id: ctx.job_id.clone(),
                        path: path.to_string_lossy().into_owned(),
                        iter: *iter,
                        total_steps,
                    });
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(600)).await;
    }

    last_path.ok_or_else(|| "Brush produced no export .ply".into())
}
