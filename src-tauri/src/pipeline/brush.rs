//! Stage 4 - live Gaussian Splat training via Brush.
//!
//! Brush is a separate executable, so everything here is orchestration around
//! its CLI rather than changes inside its training loop. Two features of
//! Brush 0.3 make the ROADMAP-V2 1.4 and 1.5 work possible:
//!
//!   * `--start-iter N` resumes the learning-rate and growth schedules at N.
//!   * an `init.ply` in the dataset directory is always used as the initial
//!     splat set.
//!
//! Together they let us stop training, transform the splats, and continue.
//! Checkpoint/resume drops a finished export in as `init.ply`; the
//! coarse-to-fine schedule does the same at each resolution step, applying the
//! Mip-Splatting 3D filter on the way through so the next stage's optimiser
//! can compensate for the widened Gaussians.
//!
//! Progressive resolution and the mip filter default ON in v0.3. Resuming via
//! CLI restarts Adam's moment estimates; that cost is accepted for the quality
//! win. A custom Brush build (see `tools/brush-fork/`) removes the restart once
//! densify / mip / appearance patches live inside the training loop.

use super::{JobCtx, JobEvent};
use crate::engines::brush_exe;
use crate::splat::{mipfilter, ply};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

/// One resolution step of the coarse-to-fine schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage {
    pub max_resolution: u32,
    /// Iteration this stage starts from (0 for a fresh run).
    pub start_iter: u32,
    /// Iteration at or beyond which the stage hands over. The final stage
    /// carries `end_iter == total_steps` and simply runs to completion.
    pub end_iter: u32,
}

/// Below this resolution a coarse stage buys nothing, so we do not add one.
const MIN_STAGE_RESOLUTION: u32 = 480;

/// Build the training schedule.
///
/// Following DashGaussian, most of the iteration budget is spent at reduced
/// resolution, where each step is far cheaper, and only the tail runs at the
/// target resolution. With `progressive` off, or when the target resolution
/// is already small, this collapses to a single stage and behaves exactly as
/// v0.1 did.
pub fn plan_stages(total_steps: u32, max_resolution: u32, progressive: bool) -> Vec<Stage> {
    let single = vec![Stage {
        max_resolution,
        start_iter: 0,
        end_iter: total_steps,
    }];
    if !progressive || total_steps < 1_000 || max_resolution <= MIN_STAGE_RESOLUTION {
        return single;
    }

    let quarter = (max_resolution / 4).max(MIN_STAGE_RESOLUTION / 2);
    let half = (max_resolution / 2).max(MIN_STAGE_RESOLUTION);
    // Nothing gained if the coarse steps are not actually coarser.
    if quarter >= half || half >= max_resolution {
        return single;
    }

    let a = (total_steps as f32 * 0.35) as u32;
    let b = (total_steps as f32 * 0.65) as u32;
    if a == 0 || a >= b || b >= total_steps {
        return single;
    }

    vec![
        Stage {
            max_resolution: quarter,
            start_iter: 0,
            end_iter: a,
        },
        Stage {
            max_resolution: half,
            start_iter: a,
            end_iter: b,
        },
        Stage {
            max_resolution,
            start_iter: b,
            end_iter: total_steps,
        },
    ]
}

/// Drop stages that a resumed run has already finished, and clip the stage
/// the run died inside so it restarts from `start_iter`.
pub fn stages_from(stages: &[Stage], start_iter: u32) -> Vec<Stage> {
    stages
        .iter()
        .filter(|s| s.end_iter > start_iter)
        .map(|s| Stage {
            start_iter: s.start_iter.max(start_iter),
            ..*s
        })
        .collect()
}

/// Export cadence for a stage: frequent enough that the stage boundary is
/// actually reached by some export, but no more often than requested.
fn stage_export_every(stage: &Stage, desired: u32) -> u32 {
    let span = stage.end_iter.saturating_sub(stage.start_iter).max(1);
    desired.clamp(50, (span / 2).max(50))
}

/// Scan the exports dir for `export_<iter>.ply`; returns iter -> path.
fn scan_exports(dir: &Path) -> BTreeMap<u32, PathBuf> {
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

/// A .ply is done being written when its size stops changing.
fn is_file_stable(p: &Path) -> bool {
    let s1 = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    std::thread::sleep(Duration::from_millis(150));
    let s2 = std::fs::metadata(p).map(|m| m.len()).unwrap_or(1);
    s1 == s2 && s1 > 0
}

pub fn init_ply_path(workspace: &Path) -> PathBuf {
    workspace.join("init.ply")
}

pub fn exports_dir(workspace: &Path) -> PathBuf {
    workspace.join("exports")
}

/// Seed the next run from `source`, optionally applying the Mip-Splatting 3D
/// filter first. Returns the number of Gaussians handed over.
fn seed_init_ply(
    ctx: &JobCtx,
    source: &Path,
    training_resolution: u32,
) -> Result<usize, String> {
    let dest = init_ply_path(&ctx.workspace);
    if !ctx.settings.mip_filter {
        std::fs::copy(source, &dest).map_err(|e| e.to_string())?;
        return Ok(0);
    }

    let model_dir = crate::colmap::find_model_dir(&ctx.workspace)
        .ok_or("Cannot apply the Mip-Splatting filter without solved cameras.")?;
    let model = crate::colmap::read_model(&model_dir)?;
    let mut cloud = ply::read_ply(source)?;
    let scale = mipfilter::focal_scale_for(&model, training_resolution);
    let stats = mipfilter::apply_3d_filter(&mut cloud, &model, mipfilter::DEFAULT_FILTER_SIZE, scale);
    ctx.log(format!(
        "Mip-Splatting filter: bounded {} of {} Gaussians (mean sigma {:.5}); {} were seen by no camera",
        stats.filtered,
        cloud.len(),
        stats.mean_sigma,
        stats.skipped_unobserved
    ));
    ply::write_ply(&dest, &cloud)?;
    Ok(cloud.len())
}

/// Bake the Mip-Splatting filter into a finished splat, in place.
pub fn bake_final_filter(ctx: &JobCtx, splat: &Path) -> Result<(), String> {
    if !ctx.settings.mip_filter {
        return Ok(());
    }
    let model_dir = match crate::colmap::find_model_dir(&ctx.workspace) {
        Some(d) => d,
        None => return Ok(()),
    };
    let model = crate::colmap::read_model(&model_dir)?;
    let mut cloud = ply::read_ply(splat)?;
    let scale = mipfilter::focal_scale_for(&model, ctx.settings.max_resolution);
    let stats = mipfilter::apply_3d_filter(&mut cloud, &model, mipfilter::DEFAULT_FILTER_SIZE, scale);
    ctx.log(format!(
        "Mip-Splatting bake: bounded {} of {} Gaussians; {} were seen by no camera",
        stats.filtered,
        cloud.len(),
        stats.skipped_unobserved
    ));
    ply::write_ply(splat, &cloud)
}

/// Tracks exports across every stage so iteration numbers stay monotonic and
/// each checkpoint is announced to the viewport exactly once.
struct ExportWatcher {
    dir: PathBuf,
    announced: u32,
    latest: Option<PathBuf>,
}

impl ExportWatcher {
    /// Announce any new stable export. Returns the newest announced iteration.
    fn poll(&mut self, ctx: &JobCtx, total_steps: u32) -> Option<u32> {
        let map = scan_exports(&self.dir);
        let (iter, path) = map.iter().next_back()?;
        if *iter <= self.announced {
            return None;
        }
        if !is_file_stable(path) {
            return None;
        }
        self.announced = *iter;
        self.latest = Some(path.clone());
        // ROADMAP-V2 1.4: the manifest must name a checkpoint that exists on
        // disk, or an interrupted run has nothing to resume from.
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
        ctx.emit(JobEvent::SplatReady {
            job_id: ctx.job_id.clone(),
            path: path.to_string_lossy().into_owned(),
            iter: *iter,
            total_steps,
        });
        Some(*iter)
    }
}

/// Spawn Brush for one stage. Returns `(exit_status_ok, stopped_at_boundary)`.
async fn run_stage(
    ctx: &JobCtx,
    stage: &Stage,
    watcher: &mut ExportWatcher,
    is_last: bool,
) -> Result<(), String> {
    let s = &ctx.settings;
    let export_every = stage_export_every(stage, s.export_every);

    let mut cmd = tokio::process::Command::new(brush_exe());
    #[cfg(windows)]
    cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    // cwd = workspace so Brush's autotune cache lands there, not in the app dir.
    cmd.current_dir(&ctx.workspace);
    cmd.arg(&ctx.workspace)
        .args(["--total-steps", &s.total_steps.to_string()])
        .args(["--start-iter", &stage.start_iter.to_string()])
        .args(["--max-splats", &s.max_splats.to_string()])
        .args(["--sh-degree", &s.sh_degree.to_string()])
        .args(["--max-resolution", &stage.max_resolution.to_string()])
        .args(["--refine-every", &s.refine_every.to_string()])
        .args(["--ssim-weight", &s.ssim_weight.to_string()])
        .args(["--opac-loss-weight", &format!("{:e}", s.opac_loss_weight)])
        .args(["--scale-loss-weight", &format!("{:e}", s.scale_loss_weight)])
        .args(["--mean-noise-weight", &s.mean_noise_weight.to_string()])
        .args(["--export-every", &export_every.to_string()])
        .args(["--export-path", &watcher.dir.to_string_lossy()])
        .args(["--export-name", "export_{iter}.ply"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start Brush: {e}"))?;
    let pid = child.id();
    if let Some(pid) = pid {
        ctx.child_pids.lock().unwrap().push(pid);
    }

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let total_steps = s.total_steps;

    // Forward Brush output to the UI log, parsing step numbers where present.
    let log_app = ctx.app.clone();
    let log_job = ctx.job_id.clone();
    let stdout_task = tokio::spawn(async move {
        use tauri::Emitter;
        let parse_step = |l: &str| -> Option<u32> {
            let lower = l.to_ascii_lowercase();
            if let Some(pos) = lower.find("step") {
                let tail: String = lower[pos + 4..]
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                return tail.parse().ok();
            }
            None
        };
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            let t = l.trim().to_string();
            if t.is_empty() {
                continue;
            }
            if let Some(step) = parse_step(&t) {
                let _ = log_app.emit(
                    "job://event",
                    JobEvent::StageProgress {
                        job_id: log_job.clone(),
                        stage: "train".into(),
                        progress: step as f32 / total_steps.max(1) as f32,
                        detail: t.clone(),
                    },
                );
            }
            let _ = log_app.emit(
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

    let outcome = loop {
        if ctx.cancelled() {
            let _ = child.kill().await;
            break Err("__cancelled__".to_string());
        }

        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            // Pick up whatever landed just before the process exited.
            tokio::task::block_in_place(|| watcher.poll(ctx, total_steps));
            if !status.success() {
                let code = status.code();
                // A crash before the first checkpoint usually means Brush
                // could not even start (a bad argument or a graphics driver
                // that would not initialize); a crash after some progress
                // usually means it ran out of VRAM partway through.
                let msg = if watcher.latest.is_some() {
                    format!(
                        "Training made progress before failing (exit {code:?}), which usually \
                         means it ran out of VRAM as the splat grew. Try a lower max splat \
                         count or a lower training resolution in Preferences."
                    )
                } else {
                    format!(
                        "Training failed before its first checkpoint (exit {code:?}). This \
                         usually means the graphics driver could not be initialized. Check that \
                         your GPU driver is current, and see the log below for what Brush printed."
                    )
                };
                break Err(msg);
            }
            break Ok(());
        }

        let reached = tokio::task::block_in_place(|| watcher.poll(ctx, total_steps));

        // Intermediate stages hand over as soon as a checkpoint reaches the
        // boundary; only the last stage waits for Brush to finish on its own.
        if !is_last {
            if let Some(iter) = reached {
                if iter >= stage.end_iter {
                    ctx.log(format!(
                        "Resolution stage complete at step {iter}; continuing at a higher resolution."
                    ));
                    let _ = child.kill().await;
                    break Ok(());
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(600)).await;
    };

    let _ = child.wait().await;
    if let Some(pid) = pid {
        ctx.child_pids.lock().unwrap().retain(|p| *p != pid);
    }
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    outcome
}

/// Train, optionally resuming from `(checkpoint, iteration)`.
/// Returns the newest exported .ply.
pub async fn train(ctx: &JobCtx, resume: Option<(PathBuf, u32)>) -> Result<PathBuf, String> {
    let s = &ctx.settings;
    let exports = exports_dir(&ctx.workspace);
    std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;

    let all_stages = plan_stages(s.total_steps, s.max_resolution, s.progressive_resolution);

    let (stages, start_iter) = match &resume {
        Some((checkpoint, iter)) => {
            ctx.log(format!("Resuming training from step {iter}."));
            std::fs::copy(checkpoint, init_ply_path(&ctx.workspace))
                .map_err(|e| format!("Could not stage the checkpoint: {e}"))?;
            (stages_from(&all_stages, *iter), *iter)
        }
        None => {
            // Fresh workspaces are unique per job. An init.ply here was written
            // by the dense-bootstrap stage and must be kept so Brush starts
            // from a filled cloud rather than inventing needle floaters.
            if init_ply_path(&ctx.workspace).exists() {
                ctx.log("Seeding Brush from densified init.ply.");
            }
            (all_stages.clone(), 0)
        }
    };

    if stages.len() > 1 {
        let res: Vec<String> = stages.iter().map(|s| s.max_resolution.to_string()).collect();
        ctx.log(format!(
            "Progressive resolution schedule: {} px",
            res.join(" then ")
        ));
    }

    let mut watcher = ExportWatcher {
        dir: exports.clone(),
        announced: start_iter,
        latest: resume.as_ref().map(|(p, _)| p.clone()),
    };

    let last = stages.len().saturating_sub(1);
    for (i, stage) in stages.iter().enumerate() {
        ctx.check_cancel()?;
        let is_last = i == last;
        if i > 0 {
            // Hand the previous stage's result forward as the initial splats.
            let prev = watcher
                .latest
                .clone()
                .ok_or("The previous resolution stage produced no checkpoint.")?;
            let n = tokio::task::block_in_place(|| seed_init_ply(ctx, &prev, stage.max_resolution))?;
            if n > 0 {
                ctx.log(format!("Carrying {n} Gaussians into the next stage."));
            }
        }
        run_stage(ctx, stage, &mut watcher, is_last).await?;
    }

    watcher
        .latest
        .ok_or_else(|| "Brush produced no exported splat.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_progressive_run_is_a_single_stage() {
        let s = plan_stages(30_000, 1600, false);
        assert_eq!(
            s,
            vec![Stage {
                max_resolution: 1600,
                start_iter: 0,
                end_iter: 30_000
            }]
        );
    }

    #[test]
    fn progressive_stages_ramp_resolution_and_cover_the_whole_budget() {
        let s = plan_stages(30_000, 1600, true);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].max_resolution, 400);
        assert_eq!(s[1].max_resolution, 800);
        assert_eq!(s[2].max_resolution, 1600);

        // Contiguous, monotonic, and finishing exactly at the budget.
        assert_eq!(s[0].start_iter, 0);
        assert_eq!(s[2].end_iter, 30_000);
        for w in s.windows(2) {
            assert_eq!(w[0].end_iter, w[1].start_iter);
            assert!(w[0].max_resolution < w[1].max_resolution);
        }
        // Most of the budget is spent below full resolution.
        assert!(s[1].end_iter > 30_000 / 2);
    }

    #[test]
    fn small_targets_and_short_runs_stay_single_stage() {
        assert_eq!(plan_stages(30_000, 480, true).len(), 1);
        assert_eq!(plan_stages(30_000, 320, true).len(), 1);
        assert_eq!(plan_stages(500, 1920, true).len(), 1);
    }

    #[test]
    fn progressive_stages_never_go_below_the_floor_resolution() {
        let s = plan_stages(30_000, 960, true);
        assert_eq!(s.len(), 3);
        assert!(s[0].max_resolution >= MIN_STAGE_RESOLUTION / 2);
        assert!(s[1].max_resolution >= MIN_STAGE_RESOLUTION);
        assert!(s[0].max_resolution < s[1].max_resolution);
    }

    #[test]
    fn resuming_drops_finished_stages_and_clips_the_current_one() {
        let all = plan_stages(30_000, 1600, true);
        // 12_000 lands inside the second stage (10_500 .. 19_500).
        let r = stages_from(&all, 12_000);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].max_resolution, 800);
        assert_eq!(r[0].start_iter, 12_000);
        assert_eq!(r[0].end_iter, 19_500);
        assert_eq!(r[1].max_resolution, 1600);
    }

    #[test]
    fn resuming_past_every_stage_leaves_nothing_to_do() {
        let all = plan_stages(30_000, 1600, true);
        assert!(stages_from(&all, 30_000).is_empty());
    }

    #[test]
    fn resuming_a_single_stage_run_clips_its_start() {
        let all = plan_stages(12_000, 1280, false);
        let r = stages_from(&all, 5_000);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].start_iter, 5_000);
        assert_eq!(r[0].end_iter, 12_000);
    }

    #[test]
    fn export_cadence_always_lands_inside_a_stage() {
        // A stage shorter than the requested cadence still gets two exports,
        // so the boundary is actually observed and the stage can hand over.
        let stage = Stage {
            max_resolution: 400,
            start_iter: 0,
            end_iter: 300,
        };
        let e = stage_export_every(&stage, 1000);
        assert!(e <= 150, "{e}");
        assert!(e >= 50);

        // A long stage honours the requested cadence.
        let stage = Stage {
            max_resolution: 1600,
            start_iter: 0,
            end_iter: 20_000,
        };
        assert_eq!(stage_export_every(&stage, 1000), 1000);
    }

    #[test]
    fn scan_exports_picks_up_iteration_numbers_and_ignores_other_files() {
        let dir = std::env::temp_dir().join("instasplatter_scan_exports_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["export_500.ply", "export_1000.ply", "init.ply", "export_x.ply", "notes.txt"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        let map = scan_exports(&dir);
        assert_eq!(map.len(), 2);
        assert_eq!(*map.keys().next_back().unwrap(), 1000);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
