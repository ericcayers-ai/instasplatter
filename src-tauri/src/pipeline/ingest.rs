//! Stage 0/1 - ingestion and frame selection (ROADMAP §3 stages 0-1).
//! Video goes through adaptive ffmpeg frame extraction; a folder is read as
//! a validated image list. Both paths then go through blur gating and even
//! subsampling to max_frames.

use super::{gating, JobCtx};
use crate::engines::{ffmpeg_exe, ffprobe_exe};
use crate::profiler::hidden_command;
use std::fs;
use std::path::{Path, PathBuf};

const VIDEO_EXTS: &[&str] = &["mp4", "mov", "avi", "mkv", "webm", "m4v", "mts", "3gp"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"];

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn list_images(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .collect();
    v.sort();
    v
}

fn video_duration_secs(path: &Path) -> Option<f64> {
    let out = hidden_command(&ffprobe_exe())
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Extract candidate frames from a video with ffmpeg at an adaptive fps.
async fn extract_video_frames(ctx: &JobCtx, video: &Path, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let duration = video_duration_secs(video).unwrap_or(0.0);
    // Extract ~2x the target frame count so gating has options to reject.
    let candidates = (ctx.settings.max_frames * 2).max(30) as f64;
    let fps = if duration > 0.5 {
        (candidates / duration).clamp(0.2, 10.0)
    } else {
        4.0
    };
    ctx.log(format!(
        "Video: {:.1}s, extracting candidates at {:.2} fps",
        duration, fps
    ));

    let pattern = out_dir.join("frame_%05d.jpg");
    let mut cmd = tokio::process::Command::new(ffmpeg_exe());
    #[cfg(windows)]
    cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    let child = cmd
        .arg("-y")
        .arg("-i")
        .arg(video)
        .args(["-vf", &format!("fps={fps:.4}"), "-qscale:v", "2"])
        .arg(&pattern)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;

    if let Some(pid) = child.id() {
        ctx.child_pids.lock().unwrap().push(pid);
    }
    let status = child
        .wait_with_output()
        .await
        .map_err(|e| e.to_string())?
        .status;
    if !status.success() {
        return Err(format!(
            "FFmpeg could not read this video (exit {:?}). It may be corrupt, or in a codec \
             FFmpeg does not support. Try re-exporting it, or drop an image folder instead.",
            status.code()
        ));
    }
    Ok(())
}

/// Runs ingestion + gating. Returns the workspace `images/` dir ready for SfM.
pub async fn ingest(ctx: &JobCtx, input: &Path) -> Result<PathBuf, String> {
    let images_dir = ctx.workspace.join("images");
    fs::create_dir_all(&images_dir).map_err(|e| e.to_string())?;

    let candidates: Vec<PathBuf> = if input.is_file() && is_video(input) {
        ctx.stage_progress("ingest", 0.05, "Extracting frames from video…");
        let raw_dir = ctx.workspace.join("frames_raw");
        extract_video_frames(ctx, input, &raw_dir).await?;
        ctx.check_cancel()?;
        let frames = list_images(&raw_dir);
        if frames.is_empty() {
            return Err("No frames could be extracted from the video.".into());
        }
        frames
    } else if input.is_dir() {
        let imgs = list_images(input);
        if imgs.is_empty() {
            return Err(format!(
                "No images found in this folder. Supported types are {}.",
                IMAGE_EXTS.join(", ")
            ));
        }
        if imgs.len() < 3 {
            return Err(format!(
                "Found only {} usable image(s), need at least 3 (ideally 20 or more).",
                imgs.len()
            ));
        }
        imgs
    } else {
        return Err("Input must be a video file or a folder of images.".into());
    };

    ctx.stage_progress(
        "ingest",
        0.4,
        &format!("Scoring {} candidate frames…", candidates.len()),
    );

    // Stage 1 - blur gating + even subsampling to max_frames.
    let (selected, report) = tokio::task::block_in_place(|| {
        gating::select_frames(
            &candidates,
            ctx.settings.max_frames as usize,
            ctx.settings.blur_reject_fraction,
        )
    });
    ctx.check_cancel()?;

    if report.unreadable > 0 {
        ctx.notice(format!(
            "{} of {} frames could not be read and were skipped. The rest of the input looks fine.",
            report.unreadable, report.total
        ));
    }
    ctx.log(format!(
        "Frame gating: {} unreadable, {} rejected as too blurry, {} kept of {} candidates",
        report.unreadable, report.blur_rejected, report.kept, report.total
    ));

    if selected.len() < 3 {
        return Err(format!(
            "Only {} usable frame(s) remain after gating ({} unreadable, {} too blurry). \
             Need at least 3. Try a slower capture, better lighting, or lowering blur rejection \
             in Preferences.",
            selected.len(),
            report.unreadable,
            report.blur_rejected
        ));
    }

    ctx.stage_progress("ingest", 0.8, "Preparing dataset…");
    for (i, src) in selected.iter().enumerate() {
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg")
            .to_ascii_lowercase();
        let dest = images_dir.join(format!("img_{:05}.{ext}", i));
        fs::copy(src, &dest).map_err(|e| e.to_string())?;
    }

    // Clean up raw extracted frames.
    let raw_dir = ctx.workspace.join("frames_raw");
    if raw_dir.exists() && !ctx.settings.keep_intermediates {
        let _ = fs::remove_dir_all(&raw_dir);
    }

    ctx.stage_progress("ingest", 1.0, &format!("{} frames ready", selected.len()));
    super::preview::emit_ingest_preview(ctx, selected.len() as u32);
    Ok(images_dir)
}
