//! Stage 2 - Structure-from-Motion via COLMAP (ROADMAP §3 stage 2).
//! feature_extractor -> matcher (sequential for video, exhaustive otherwise)
//! -> mapper, producing `sparse/0` in the Brush-compatible COLMAP layout.
//!
//! COLMAP 4.1 hooks: optional pose-prior / gravity mapper flags and LightGlue
//! matcher routing stubs when engines are present.

use super::JobCtx;
use crate::engines::colmap_exe;
use crate::pipeline::solver;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Options for [`run_sfm_with_options`].
#[derive(Debug, Clone, Default)]
pub struct SfmOptions {
    pub use_pose_priors: bool,
    pub use_gravity_prior: bool,
    /// "auto" | "sequential" | "exhaustive" | "lightglue" | "roma"
    pub matcher_front_end: String,
}

/// Spawn a COLMAP subcommand, stream stderr/stdout lines to the UI log and
/// a progress callback, honor cancellation.
pub async fn run_colmap_pub(
    ctx: &JobCtx,
    stage_progress: (f32, f32),
    args: &[&str],
    total_hint: usize,
) -> Result<(), String> {
    run_colmap(ctx, stage_progress, args, total_hint).await
}

async fn run_colmap(
    ctx: &JobCtx,
    stage_progress: (f32, f32),
    args: &[&str],
    total_hint: usize,
) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(colmap_exe());
    #[cfg(windows)]
    cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    let mut child = cmd
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start COLMAP: {e}"))?;

    if let Some(pid) = child.id() {
        ctx.child_pids.lock().unwrap().push(pid);
    }

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (lo, hi) = stage_progress;
    let mut seen = 0usize;

    let mut lines_out = BufReader::new(stdout).lines();
    let mut lines_err = BufReader::new(stderr).lines();
    loop {
        ctx.check_cancel()?;
        tokio::select! {
            line = lines_out.next_line() => {
                match line.map_err(|e| e.to_string())? {
                    Some(l) => {
                        if l.contains("Processed file") || l.contains("Registering image") || l.contains("Matching block") {
                            seen += 1;
                            let frac = (seen as f32 / total_hint.max(1) as f32).min(1.0);
                            ctx.stage_progress("sfm", lo + (hi - lo) * frac, l.trim());
                        }
                    }
                    None => break,
                }
            }
            line = lines_err.next_line() => {
                if let Ok(Some(l)) = line {
                    let t = l.trim();
                    if !t.is_empty() {
                        ctx.log(format!("[colmap] {t}"));
                    }
                }
            }
        }
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;
    ctx.check_cancel()?;
    if !status.success() {
        return Err(format!(
            "COLMAP {} failed (exit {:?})",
            args.first().unwrap_or(&"?"),
            status.code()
        ));
    }
    Ok(())
}

pub async fn run_sfm(ctx: &JobCtx, images_dir: &Path) -> Result<(), String> {
    run_sfm_with_options(ctx, images_dir, SfmOptions::default()).await
}

pub async fn run_sfm_with_options(
    ctx: &JobCtx,
    images_dir: &Path,
    opts: SfmOptions,
) -> Result<(), String> {
    let ws = &ctx.workspace;
    let db = ws.join("database.db");
    let sparse = ws.join("sparse");
    std::fs::create_dir_all(&sparse).map_err(|e| e.to_string())?;

    let n_images = std::fs::read_dir(images_dir)
        .map(|d| d.count())
        .unwrap_or(0);
    let db_s = db.to_string_lossy().into_owned();
    let img_s = images_dir.to_string_lossy().into_owned();
    let sparse_s = sparse.to_string_lossy().into_owned();
    let gpu = if ctx.settings.sift_gpu { "1" } else { "0" };

    // 1) Feature extraction. Single shared camera: frames come from one
    //    device in the common case; COLMAP still refines per-image poses.
    ctx.stage_progress("sfm", 0.0, "Extracting features…");
    run_colmap(
        ctx,
        (0.0, 0.35),
        &[
            "feature_extractor",
            "--database_path",
            &db_s,
            "--image_path",
            &img_s,
            "--ImageReader.single_camera",
            "1",
            "--ImageReader.camera_model",
            "OPENCV",
            "--FeatureExtraction.use_gpu",
            gpu,
            "--SiftExtraction.max_num_features",
            "8192",
        ],
        n_images,
    )
    .await?;

    // 2) Matching. LightGlue / RoMa are routing stubs until their engines land;
    //    sequential suits video; exhaustive for small unordered folders.
    let front = if opts.matcher_front_end.is_empty() {
        solver::matcher_front_end(&ctx.settings, false).to_string()
    } else {
        opts.matcher_front_end.clone()
    };
    if front == "lightglue" {
        ctx.log(
            "LightGlue routing requested — using COLMAP matcher until a LightGlue engine is installed.",
        );
    } else if front == "roma" {
        ctx.log(
            "RoMa matcher routing requested — densify path uses RoMa; SfM uses COLMAP matches.",
        );
    }

    let sequential = match front.as_str() {
        "sequential" => true,
        "exhaustive" | "lightglue" | "roma" => false,
        _ => match ctx.settings.matcher.as_str() {
            "sequential" => true,
            "exhaustive" => false,
            _ => n_images > 80,
        },
    };
    ctx.stage_progress("sfm", 0.35, "Matching features…");
    if sequential {
        run_colmap(
            ctx,
            (0.35, 0.55),
            &[
                "sequential_matcher",
                "--database_path",
                &db_s,
                "--SequentialMatching.overlap",
                "20",
                "--SequentialMatching.loop_detection",
                "1",
                "--FeatureMatching.use_gpu",
                gpu,
            ],
            n_images,
        )
        .await?;
    } else {
        run_colmap(
            ctx,
            (0.35, 0.55),
            &[
                "exhaustive_matcher",
                "--database_path",
                &db_s,
                "--FeatureMatching.use_gpu",
                gpu,
            ],
            n_images * n_images / 100 + 1,
        )
        .await?;
    }

    // 3) Incremental mapping → sparse/0
    // COLMAP 4.1: pose-prior / GPS covariance mapper when priors exist.
    ctx.stage_progress("sfm", 0.55, "Reconstructing camera poses…");
    let prior_path = [
        ws.join("pose_priors.txt"),
        ws.join("image_priors.txt"),
        sparse.join("pose_priors.txt"),
        ws.join("geo").join("pose_priors.txt"),
    ]
    .into_iter()
    .find(|p| p.exists());

    let mut mapper_args: Vec<String> = vec![
        "mapper".into(),
        "--database_path".into(),
        db_s.clone(),
        "--image_path".into(),
        img_s.clone(),
        "--output_path".into(),
        sparse_s.clone(),
        "--Mapper.min_num_matches".into(),
        "12".into(),
        "--Mapper.init_min_num_inliers".into(),
        "80".into(),
        "--Mapper.abs_pose_min_num_inliers".into(),
        "20".into(),
        "--Mapper.filter_max_reproj_error".into(),
        "3.5".into(),
        "--Mapper.ba_global_max_num_iterations".into(),
        "40".into(),
    ];

    if opts.use_pose_priors {
        if let Some(pp) = &prior_path {
            let pp_s = pp.to_string_lossy().into_owned();
            // COLMAP 3.9+/4.x pose prior path (best-effort; ignored if unsupported).
            mapper_args.push("--Mapper.use_pose_prior".into());
            mapper_args.push("1".into());
            mapper_args.push("--image_list_path".into());
            mapper_args.push(pp_s);
            ctx.log("Mapper: enabling pose-prior / GPS prior hooks.");
        } else {
            ctx.log("Pose-prior requested but no pose_priors.txt found; mapping without priors.");
        }
    }
    if opts.use_gravity_prior {
        let gravity = ws.join("gravity_priors.txt");
        if gravity.exists() {
            mapper_args.push("--Mapper.use_prior_rotation".into());
            mapper_args.push("1".into());
            ctx.log("Mapper: enabling gravity / orientation prior hook.");
        }
    }

    let mapper_refs: Vec<&str> = mapper_args.iter().map(|s| s.as_str()).collect();
    run_colmap(ctx, (0.55, 1.0), &mapper_refs, n_images).await?;

    if !sparse.join("0").join("cameras.bin").exists()
        && !sparse.join("0").join("cameras.txt").exists()
    {
        return Err(
            "COLMAP could not reconstruct the scene from these frames. This usually means too \
             little overlap between frames, motion blur, or a scene with too few distinct \
             features (a blank wall, open sky, water). Try a slower capture with more overlap, \
             better lighting, or more frames."
                .into(),
        );
    }
    ctx.stage_progress("sfm", 1.0, "Cameras solved");
    Ok(())
}
