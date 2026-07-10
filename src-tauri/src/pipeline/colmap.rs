//! Stage 2 - Structure-from-Motion via COLMAP (ROADMAP §3 stage 2).
//! feature_extractor -> matcher (sequential for video, exhaustive otherwise)
//! -> mapper, producing `sparse/0` in the Brush-compatible COLMAP layout.

use super::JobCtx;
use crate::engines::colmap_exe;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Spawn a COLMAP subcommand, stream stderr/stdout lines to the UI log and
/// a progress callback, honor cancellation.
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
        ],
        n_images,
    )
    .await?;

    // 2) Matching. Sequential suits video and orbit captures; exhaustive is
    //    more robust for small unordered folders.
    let sequential = match ctx.settings.matcher.as_str() {
        "sequential" => true,
        "exhaustive" => false,
        _ => n_images > 100, // auto
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
                "15",
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
    ctx.stage_progress("sfm", 0.55, "Reconstructing camera poses…");
    run_colmap(
        ctx,
        (0.55, 1.0),
        &[
            "mapper",
            "--database_path",
            &db_s,
            "--image_path",
            &img_s,
            "--output_path",
            &sparse_s,
        ],
        n_images,
    )
    .await?;

    if !sparse.join("0").join("cameras.bin").exists() {
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
