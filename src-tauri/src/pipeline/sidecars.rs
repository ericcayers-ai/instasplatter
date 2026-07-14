//! Optional neural dense-init, pose, and post-polish sidecars.
//!
//! Dual-mode policy (v0.5):
//! - **Standard**: VGGT-Commercial / DAV2 / RoMa v2 / Fixer (commercial-safe).
//! - **Experimental** (license ack): + Ω / MASt3R / DUSt3R / Difix; merge all densifiers.
//!
//! Do **not** vendor GPL Lichtfeld densify plugin sources — RoMa orchestration is
//! clean-room (MIT RoMa v2 APIs + our filters).

use super::dense;
use super::solver;
use super::JobCtx;
use crate::settings::app_data_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarStatus {
    pub depth_anything_v2: bool,
    pub vggt_commercial: bool,
    /// Newest VGGT (Ω). Non-commercial weights; Experimental only.
    pub vggt_omega: bool,
    /// Present but non-commercial / research-only; Experimental only.
    pub vggt_research: bool,
    pub mast3r: bool,
    pub dust3r: bool,
    /// RoMa v2 densify (MIT densifier + user-installed DINOv3 weights).
    pub roma_v2: bool,
    /// NVIDIA Fixer (commercial Open Model License) polish launcher.
    pub fixer: bool,
    /// Difix3D+ research launcher (gated; Experimental only).
    pub difix: bool,
}

fn sidecars_dir() -> PathBuf {
    app_data_dir().join("engines").join("sidecars")
}

fn launcher(name: &str) -> PathBuf {
    let dir = sidecars_dir().join(name);
    #[cfg(windows)]
    {
        let bat = dir.join("run.bat");
        if bat.exists() {
            return bat;
        }
        let exe = dir.join("run.exe");
        if exe.exists() {
            return exe;
        }
        let py = dir.join("run.py");
        if py.exists() {
            return py;
        }
    }
    #[cfg(not(windows))]
    {
        let sh = dir.join("run.sh");
        if sh.exists() {
            return sh;
        }
        let py = dir.join("run.py");
        if py.exists() {
            return py;
        }
    }
    dir.join("run")
}

/// Template stubs ship a `.stub` marker so we never report them as "ready".
fn is_stub_sidecar(name: &str) -> bool {
    sidecars_dir().join(name).join(".stub").exists()
}

fn launcher_ready(name: &str) -> bool {
    launcher(name).exists() && !is_stub_sidecar(name)
}

pub fn status() -> SidecarStatus {
    let dav2 = launcher_ready("depth-anything-v2")
        || sidecars_dir().join("depth-anything-v2").join("weights.onnx").exists();
    let vggt_c = sidecars_dir()
        .join("vggt-commercial")
        .join("ACCEPTED")
        .exists()
        && launcher_ready("vggt-commercial");
    SidecarStatus {
        depth_anything_v2: dav2,
        vggt_commercial: vggt_c,
        vggt_omega: launcher_ready("vggt-omega"),
        vggt_research: launcher_ready("vggt-research"),
        mast3r: launcher_ready("mast3r"),
        dust3r: launcher_ready("dust3r"),
        roma_v2: launcher_ready("roma-v2"),
        fixer: launcher_ready("fixer"),
        difix: launcher_ready("difix"),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    images_dir: &'a str,
    workspace: &'a str,
    sparse_dir: Option<&'a str>,
    max_points: u32,
    /// "sfm" | "densify" | "polish" — launchers that support multi-role.
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<&'a str>,
    /// RoMa quality: "fast" | "base" | "high" | "precise".
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<&'a str>,
    /// Optional path for polish sidecars (Fixer / Difix).
    #[serde(skip_serializing_if = "Option::is_none")]
    splat_path: Option<&'a str>,
    /// Lichtfeld-recipe style knobs (ignored by launchers that do not need them).
    reference_fraction: f32,
    neighbors_per_ref: u32,
}

async fn invoke_launcher(
    ctx: &JobCtx,
    name: &str,
    images_dir: &Path,
    splat_path: Option<&Path>,
    task: Option<&str>,
    quality: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    if solver::is_research_sidecar(name) && !ctx.settings.experimental_mode {
        ctx.log(format!(
            "Skipping {name}: research/NC sidecar requires Experimental Mode."
        ));
        return Ok(None);
    }

    let launch = launcher(name);
    if !launch.exists() {
        return Ok(None);
    }
    if is_stub_sidecar(name) {
        ctx.notice(format!(
            "{name} is still a stub — install real weights and delete the .stub marker."
        ));
        return Err(format!("{name} stub (not wired)"));
    }
    ctx.log(format!("Running {name} sidecar…"));

    let sparse = crate::colmap::find_model_dir(&ctx.workspace);
    let splat_s = splat_path.map(|p| p.to_string_lossy().into_owned());
    let quality_owned = quality.map(|s| s.to_string());
    let req = Request {
        images_dir: &images_dir.to_string_lossy(),
        workspace: &ctx.workspace.to_string_lossy(),
        sparse_dir: sparse.as_ref().map(|p| p.to_str().unwrap_or("")),
        max_points: if ctx.settings.experimental_mode {
            2_000_000
        } else {
            1_200_000
        },
        task,
        quality: quality_owned.as_deref(),
        splat_path: splat_s.as_deref(),
        // Lichtfeld densify plugin defaults (recipe only — not their GPL code).
        reference_fraction: 0.3,
        neighbors_per_ref: 8,
    };
    let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;

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
        .map_err(|e| format!("Could not start {name}: {e}"))?;
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
    let out = child
        .wait_with_output()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(pid) = pid {
        ctx.child_pids.lock().unwrap().retain(|p| *p != pid);
    }

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        ctx.log(format!("[{name}] {err}"));
        return Err(format!("{name} failed"));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let path = stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(PathBuf::from);
    match path {
        Some(p) if p.exists() => Ok(Some(p)),
        Some(p) if p.to_string_lossy().eq_ignore_ascii_case("ok") => {
            // Pose sidecars may print "OK" after writing workspace/sparse.
            Ok(Some(p))
        }
        _ => {
            ctx.log(format!("[{name}] produced no output path"));
            Ok(None)
        }
    }
}

fn consume_point_ply(ply_path: &Path) -> Result<(Vec<[f32; 3]>, Vec<[u8; 3]>), String> {
    dense::read_xyzrgb_ply(ply_path).or_else(|_| {
        crate::splat::ply::read_ply(ply_path).map(|c| {
            let rgb: Vec<[u8; 3]> = c
                .sh_dc
                .iter()
                .map(|dc| {
                    let to_u8 = |v: f32| {
                        ((0.5 + crate::splat::SH_C0 * v) * 255.0).clamp(0.0, 255.0) as u8
                    };
                    [to_u8(dc[0]), to_u8(dc[1]), to_u8(dc[2])]
                })
                .collect();
            (c.positions, rgb)
        })
    })
}

fn sparse_model_usable(workspace: &Path) -> bool {
    crate::colmap::find_model_dir(workspace)
        .and_then(|d| crate::colmap::read_model(&d).ok())
        .map(|m| m.images.len() >= 2)
        .unwrap_or(false)
}

/// Run a pose/SfM sidecar that writes a COLMAP sparse model under `workspace/sparse`.
/// Returns the solver name on success.
pub async fn try_neural_poses(
    ctx: &JobCtx,
    images_dir: &Path,
    chain: &[&str],
) -> Result<Option<String>, String> {
    for name in chain {
        ctx.check_cancel()?;
        // Clear partial sparse so a failed neural write cannot confuse COLMAP.
        let sparse = ctx.workspace.join("sparse");
        let _ = std::fs::remove_dir_all(&sparse);
        let _ = std::fs::create_dir_all(sparse.join("0"));

        match invoke_launcher(ctx, name, images_dir, None, Some("sfm"), None).await {
            Ok(Some(path)) => {
                // Accept: printed model dir, printed cameras.txt parent, or "OK" + usable sparse.
                let looks_ok = if path.to_string_lossy().eq_ignore_ascii_case("ok") {
                    sparse_model_usable(&ctx.workspace)
                } else if path.is_dir() {
                    crate::colmap::read_model(&path)
                        .map(|m| m.images.len() >= 2)
                        .unwrap_or(false)
                        || sparse_model_usable(&ctx.workspace)
                } else if path.extension().and_then(|e| e.to_str()) == Some("txt")
                    || path.file_name().and_then(|f| f.to_str()) == Some("cameras.bin")
                {
                    sparse_model_usable(&ctx.workspace)
                } else if path.extension().and_then(|e| e.to_str()) == Some("ply") {
                    // Some launchers emit poses + dense points; poses land in sparse/.
                    sparse_model_usable(&ctx.workspace)
                } else {
                    sparse_model_usable(&ctx.workspace)
                };

                if looks_ok {
                    // If the sidecar wrote elsewhere, copy into workspace/sparse/0.
                    if path.is_dir() && path != sparse.join("0") && path != sparse {
                        if let Ok(model) = crate::colmap::read_model(&path) {
                            let dest = sparse.join("0");
                            let _ = std::fs::create_dir_all(&dest);
                            let _ = crate::colmap::write_model_txt(&dest, &model);
                        }
                    }
                    if sparse_model_usable(&ctx.workspace) {
                        ctx.notice(solver::camera_chip(name));
                        return Ok(Some((*name).to_string()));
                    }
                }
                ctx.log(format!("[{name}] SfM output was not a usable COLMAP model."));
            }
            Ok(None) => continue,
            Err(e) => {
                ctx.notice(format!("{name} pose solver skipped: {e}"));
            }
        }
    }
    Ok(None)
}

/// Optional light COLMAP triangulation / bundle adjust on an existing sparse model.
pub async fn maybe_refine_poses_with_colmap(
    ctx: &JobCtx,
    images_dir: &Path,
) -> Result<(), String> {
    let model_dir = match crate::colmap::find_model_dir(&ctx.workspace) {
        Some(d) => d,
        None => return Ok(()),
    };
    let model = crate::colmap::read_model(&model_dir)?;
    // Only refine when frame count is high — matches Standard plan A1.
    if model.images.len() < 80 && !ctx.settings.experimental_mode {
        return Ok(());
    }
    let n = model.images.len();
    let img_s = images_dir.to_string_lossy().into_owned();
    let model_s = model_dir.to_string_lossy().into_owned();
    let db = ctx.workspace.join("database.db");
    if !db.exists() {
        // Without a feature DB, skip BA rather than re-running full SfM.
        ctx.log("Skipping COLMAP BA refine (no feature database yet).");
        return Ok(());
    }
    let db_s = db.to_string_lossy().into_owned();
    ctx.log("Refining neural poses with limited COLMAP bundle adjustment…");
    match super::colmap::run_colmap_pub(
        ctx,
        (0.7, 0.82),
        &[
            "point_triangulator",
            "--database_path",
            &db_s,
            "--image_path",
            &img_s,
            "--input_path",
            &model_s,
            "--output_path",
            &model_s,
        ],
        n,
    )
    .await
    {
        Ok(()) => {
            let _ = super::colmap::run_colmap_pub(
                ctx,
                (0.82, 0.88),
                &[
                    "bundle_adjuster",
                    "--input_path",
                    &model_s,
                    "--output_path",
                    &model_s,
                ],
                n,
            )
            .await;
        }
        Err(e) => {
            ctx.log(format!("COLMAP refine skipped: {e}"));
        }
    }
    Ok(())
}

/// Collect points from neural densifiers.
/// Standard: first usable success. Experimental: merge every available source.
pub async fn try_neural_points(
    ctx: &JobCtx,
    images_dir: &Path,
) -> Result<Option<(Vec<[f32; 3]>, Vec<[u8; 3]>, String)>, String> {
    if !ctx.settings.use_neural_init {
        return Ok(None);
    }

    let st = status();
    let order = solver::densify_neural_order(&ctx.settings, &st);
    if order.is_empty() {
        return Ok(None);
    }

    let merge_all = ctx.settings.experimental_mode;
    let mut xyz: Vec<[f32; 3]> = Vec::new();
    let mut rgb: Vec<[u8; 3]> = Vec::new();
    let mut labels: Vec<String> = Vec::new();

    for name in order {
        ctx.check_cancel()?;
        match invoke_launcher(ctx, name, images_dir, None, Some("densify"), None).await {
            Ok(Some(ply_path)) => match consume_point_ply(&ply_path) {
                Ok((px, pr)) if px.len() >= 32 => {
                    if merge_all {
                        xyz.extend(px);
                        rgb.extend(pr);
                        labels.push(name.to_string());
                    } else {
                        return Ok(Some((px, pr, name.to_string())));
                    }
                }
                Ok(_) => ctx.log(format!("[{name}] too few points")),
                Err(e) => ctx.log(format!("[{name}] could not consume output: {e}")),
            },
            Ok(None) => continue,
            Err(e) => {
                ctx.notice(format!("{name} sidecar skipped: {e}"));
            }
        }
    }

    if merge_all && xyz.len() >= 32 {
        return Ok(Some((xyz, rgb, labels.join("+"))));
    }
    Ok(None)
}

/// RoMa v2 densify (Lichtfeld-style recipe via clean-room sidecar).
pub async fn try_roma_densify(
    ctx: &JobCtx,
    images_dir: &Path,
) -> Result<Option<(Vec<[f32; 3]>, Vec<[u8; 3]>)>, String> {
    if !ctx.settings.dense_init || !status().roma_v2 {
        return Ok(None);
    }
    let q = ctx.settings.roma_quality.as_str();
    match invoke_launcher(ctx, "roma-v2", images_dir, None, Some("densify"), Some(q)).await
    {
        Ok(Some(ply_path)) => match consume_point_ply(&ply_path) {
            Ok((xyz, rgb)) if xyz.len() >= 32 => {
                ctx.log(format!(
                    "RoMa v2 densify ({q}): {} points (will merge).",
                    xyz.len()
                ));
                Ok(Some((xyz, rgb)))
            }
            Ok(_) => {
                ctx.log("[roma-v2] too few points");
                Ok(None)
            }
            Err(e) => {
                ctx.log(format!("[roma-v2] could not consume output: {e}"));
                Ok(None)
            }
        },
        Ok(None) => Ok(None),
        Err(e) => {
            ctx.notice(format!("RoMa densify skipped: {e}"));
            Ok(None)
        }
    }
}

/// Try neural densifiers and write `init.ply` (kept for diagnostics / future callers).
#[allow(dead_code)]
pub async fn try_dense_init(ctx: &JobCtx, images_dir: &Path) -> Result<Option<usize>, String> {
    match try_neural_points(ctx, images_dir).await? {
        Some((xyz, rgb, name)) => {
            let n = dense::write_init_from_points(ctx, xyz, rgb, &name)?;
            Ok(Some(n))
        }
        None => Ok(None),
    }
}

/// Post-train polish. Standard: Fixer. Experimental: Difix then Fixer (both if present).
pub async fn try_polish(ctx: &JobCtx, result_ply: &Path) -> Result<bool, String> {
    if !ctx.settings.post_polish {
        return Ok(false);
    }
    let st = status();
    let order = solver::polish_order(&ctx.settings, &st);
    if order.is_empty() {
        return Ok(false);
    }

    let images = ctx
        .workspace
        .join("images")
        .canonicalize()
        .unwrap_or_else(|_| ctx.workspace.join("images"));

    let mut any = false;
    let mut labels = Vec::new();
    for name in order {
        ctx.check_cancel()?;
        ctx.stage_progress("finalize", 0.7, &format!("Polishing with {name}…"));
        match invoke_launcher(ctx, name, &images, Some(result_ply), Some("polish"), None).await
        {
            Ok(Some(out)) => {
                if out != result_ply {
                    std::fs::copy(&out, result_ply).map_err(|e| e.to_string())?;
                }
                ctx.log(format!("{name} polished the result splat."));
                labels.push(name.to_string());
                any = true;
                // Experimental: continue so Fixer can run after Difix.
                if !ctx.settings.experimental_mode {
                    break;
                }
            }
            Ok(None) => {
                ctx.log(format!("[{name}] produced no polished splat"));
            }
            Err(e) => {
                ctx.notice(format!("{name} polish skipped: {e}"));
            }
        }
    }
    if any {
        ctx.notice(format!("Polish: {}", labels.join(" → ")));
    }
    Ok(any)
}
