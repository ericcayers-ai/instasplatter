//! Optional neural dense-init and post-polish sidecars.
//!
//! Default policy (v0.3.1): use the newest license-viable sidecar when present.
//! Neural densifiers **compose** with COLMAP MVS (see `dense::densify_after_sfm`).
//!
//! | Sidecar | License | Priority |
//! | --- | --- | --- |
//! | Depth Anything V2 Small / latest | Apache-2.0 | ON when installed |
//! | VGGT-1B-Commercial | Meta AUP (no military) | ON when installed + accepted |
//! | NVIDIA Fixer | NVIDIA Open Model (commercial OK) | ON when installed (`post_polish`) |
//! | VGGT-Ω (VGGT-Omega) | CC BY-NC-4.0 | research opt-in only (newest, best quality) |
//! | VGGT-1B (NC) / Difix research | CC BY-NC / gated | research opt-in only |
//! | MASt3R / DUSt3R / Pi3 weights | NC | not shipped |
//!
//! Why not VGGT-Ω by default: as of May 2026 the published Omega checkpoint
//! is CC BY-NC-4.0 on Hugging Face (`facebook/VGGT-Omega`). Until Meta ships
//! a commercial Omega weight, shipping it ON for a commercial product is a
//! license risk. Prefer Omega when the user enables Research sidecars.

use super::dense;
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
    /// Newest VGGT (Ω). Non-commercial weights; research path only.
    pub vggt_omega: bool,
    /// Present but non-commercial / research-only; never used unless opted in.
    pub vggt_research: bool,
    /// NVIDIA Fixer (commercial Open Model License) polish launcher.
    pub fixer: bool,
    /// Difix3D+ research launcher (gated; Research ON only).
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

pub fn status() -> SidecarStatus {
    let dav2 = launcher("depth-anything-v2").exists()
        || sidecars_dir().join("depth-anything-v2").join("weights.onnx").exists();
    let vggt_c = sidecars_dir()
        .join("vggt-commercial")
        .join("ACCEPTED")
        .exists()
        && launcher("vggt-commercial").exists();
    let vggt_omega = launcher("vggt-omega").exists();
    let vggt_r = launcher("vggt-research").exists();
    SidecarStatus {
        depth_anything_v2: dav2,
        vggt_commercial: vggt_c,
        vggt_omega,
        vggt_research: vggt_r,
        fixer: launcher("fixer").exists(),
        difix: launcher("difix").exists(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    images_dir: &'a str,
    workspace: &'a str,
    sparse_dir: Option<&'a str>,
    max_points: u32,
    /// Optional path for polish sidecars (Fixer / Difix).
    #[serde(skip_serializing_if = "Option::is_none")]
    splat_path: Option<&'a str>,
}

async fn invoke_launcher(
    ctx: &JobCtx,
    name: &str,
    images_dir: &Path,
    splat_path: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let launch = launcher(name);
    if !launch.exists() {
        return Ok(None);
    }
    ctx.log(format!("Running {name} sidecar…"));

    let sparse = crate::colmap::find_model_dir(&ctx.workspace);
    let splat_s = splat_path.map(|p| p.to_string_lossy().into_owned());
    let req = Request {
        images_dir: &images_dir.to_string_lossy(),
        workspace: &ctx.workspace.to_string_lossy(),
        sparse_dir: sparse.as_ref().map(|p| p.to_str().unwrap_or("")),
        max_points: 1_200_000,
        splat_path: splat_s.as_deref(),
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

/// Collect points from the highest-priority neural densifier that succeeds.
/// Does **not** write `init.ply`; the caller merges with MVS / sparse.
pub async fn try_neural_points(
    ctx: &JobCtx,
    images_dir: &Path,
) -> Result<Option<(Vec<[f32; 3]>, Vec<[u8; 3]>, String)>, String> {
    if !ctx.settings.use_neural_init {
        return Ok(None);
    }

    let st = status();
    let order: Vec<&str> = {
        let mut v = Vec::new();
        if ctx.settings.allow_research_sidecars && st.vggt_omega {
            v.push("vggt-omega");
        }
        if st.vggt_commercial {
            v.push("vggt-commercial");
        }
        if st.depth_anything_v2 {
            v.push("depth-anything-v2");
        }
        if ctx.settings.allow_research_sidecars && st.vggt_research {
            v.push("vggt-research");
        }
        v
    };

    for name in order {
        ctx.check_cancel()?;
        match invoke_launcher(ctx, name, images_dir, None).await {
            Ok(Some(ply_path)) => match consume_point_ply(&ply_path) {
                Ok((xyz, rgb)) if xyz.len() >= 32 => {
                    return Ok(Some((xyz, rgb, name.to_string())));
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
    Ok(None)
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

/// Post-train polish via NVIDIA Fixer (commercial) or Difix (research).
/// Returns `true` when `result_ply` was replaced with a polished splat.
pub async fn try_polish(ctx: &JobCtx, result_ply: &Path) -> Result<bool, String> {
    if !ctx.settings.post_polish {
        return Ok(false);
    }
    let st = status();
    let mut order: Vec<&str> = Vec::new();
    if st.fixer {
        order.push("fixer");
    }
    if ctx.settings.allow_research_sidecars && st.difix {
        order.push("difix");
    }
    if order.is_empty() {
        return Ok(false);
    }

    let images = ctx
        .workspace
        .join("images")
        .canonicalize()
        .unwrap_or_else(|_| ctx.workspace.join("images"));

    for name in order {
        ctx.check_cancel()?;
        ctx.stage_progress("finalize", 0.7, &format!("Polishing with {name}…"));
        match invoke_launcher(ctx, name, &images, Some(result_ply)).await {
            Ok(Some(out)) => {
                if out != result_ply {
                    std::fs::copy(&out, result_ply).map_err(|e| e.to_string())?;
                }
                ctx.log(format!("{name} polished the result splat."));
                return Ok(true);
            }
            Ok(None) => {
                ctx.log(format!("[{name}] produced no polished splat"));
            }
            Err(e) => {
                ctx.notice(format!("{name} polish skipped: {e}"));
            }
        }
    }
    Ok(false)
}
