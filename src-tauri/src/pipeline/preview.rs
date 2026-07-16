//! Live reconstruction stage previews for the 3D viewport.
//!
//! After SfM / densify milestones we write XYZRGB PLYs under `previews/` and
//! emit [`super::JobEvent`] paths so the UI can stream sparse → dense → splat
//! layers without waiting for the first trainer checkpoint.

use super::{JobCtx, JobEvent};
use crate::colmap::{self, Camera, Image, Model};
use crate::math::{m3_mul_v, m3_transpose};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Cap so the WebGL point pass stays interactive on large MVS clouds.
const MAX_PREVIEW_POINTS: usize = 750_000;

pub fn preview_dir(workspace: &Path) -> PathBuf {
    workspace.join("previews")
}

pub fn sparse_ply_path(workspace: &Path) -> PathBuf {
    preview_dir(workspace).join("sparse.ply")
}

pub fn dense_ply_path(workspace: &Path) -> PathBuf {
    preview_dir(workspace).join("dense.ply")
}

/// Binary little-endian XYZRGB PLY (viewport point layers).
pub fn write_xyzrgb_ply(
    path: &Path,
    xyz: &[[f32; 3]],
    rgb: &[[u8; 3]],
) -> Result<usize, String> {
    if xyz.is_empty() || xyz.len() != rgb.len() {
        return Err("empty or mismatched point cloud".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let n = xyz.len().min(MAX_PREVIEW_POINTS);
    let step = if xyz.len() > n {
        xyz.len() as f64 / n as f64
    } else {
        1.0
    };
    let mut f = BufWriter::new(File::create(path).map_err(|e| e.to_string())?);
    let header = format!(
        "ply\nformat binary_little_endian 1.0\nelement vertex {n}\n\
         property float x\nproperty float y\nproperty float z\n\
         property uchar red\nproperty uchar green\nproperty uchar blue\n\
         end_header\n"
    );
    f.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
    let mut written = 0usize;
    let mut acc = 0.0f64;
    while written < n && (acc as usize) < xyz.len() {
        let i = acc as usize;
        let p = &xyz[i];
        let c = &rgb[i];
        f.write_all(&p[0].to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&p[1].to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&p[2].to_le_bytes()).map_err(|e| e.to_string())?;
        f.write_all(&[c[0], c[1], c[2]]).map_err(|e| e.to_string())?;
        written += 1;
        acc += step;
    }
    f.flush().map_err(|e| e.to_string())?;
    Ok(written)
}

/// Emit frame count + a simple sequential path preview after ingest/gating.
pub fn emit_ingest_preview(ctx: &JobCtx, frame_count: u32) {
    // Without poses yet, lay frames along +X so the viewport shows a capture
    // path skeleton that SfM frustums will replace.
    let spacing = 0.25f32;
    let path: Vec<[f32; 3]> = (0..frame_count)
        .map(|i| [i as f32 * spacing, 0.0, 0.0])
        .collect();
    ctx.emit(JobEvent::IngestPreview {
        job_id: ctx.job_id.clone(),
        frame_count,
        path,
    });
}

/// After SfM: export sparse cloud + batch-emit COLMAP cameras (for COLMAP path).
pub fn emit_sparse_stage(ctx: &JobCtx) -> Result<(), String> {
    let Some(model_dir) = colmap::find_model_dir(&ctx.workspace) else {
        return Ok(());
    };
    let model = colmap::read_model(&model_dir)?;
    emit_cameras_from_model(ctx, &model);
    let (xyz, rgb) = sparse_xyzrgb(&model);
    if xyz.is_empty() {
        ctx.log("Sparse preview: no points to stream.");
        return Ok(());
    }
    let dest = sparse_ply_path(&ctx.workspace);
    let n = write_xyzrgb_ply(&dest, &xyz, &rgb)?;
    ctx.emit(JobEvent::SparseCloudReady {
        job_id: ctx.job_id.clone(),
        path: dest.to_string_lossy().into_owned(),
        point_count: n as u32,
    });
    ctx.log(format!("Sparse preview: {n} points → {}", dest.display()));
    Ok(())
}

/// After densify: stream dense XYZRGB (separate from Gaussian `init.ply`).
pub fn emit_dense_stage(
    ctx: &JobCtx,
    xyz: &[[f32; 3]],
    rgb: &[[u8; 3]],
) -> Result<(), String> {
    if xyz.is_empty() {
        return Ok(());
    }
    let dest = dense_ply_path(&ctx.workspace);
    let n = write_xyzrgb_ply(&dest, xyz, rgb)?;
    ctx.emit(JobEvent::DenseCloudReady {
        job_id: ctx.job_id.clone(),
        path: dest.to_string_lossy().into_owned(),
        point_count: n as u32,
    });
    ctx.log(format!("Dense preview: {n} points → {}", dest.display()));
    Ok(())
}

fn sparse_xyzrgb(model: &Model) -> (Vec<[f32; 3]>, Vec<[u8; 3]>) {
    let mut xyz = Vec::with_capacity(model.points.len());
    let mut rgb = Vec::with_capacity(model.points.len());
    for p in &model.points {
        if p.track.len() < 2 || p.error > 4.0 {
            continue;
        }
        xyz.push([p.xyz[0] as f32, p.xyz[1] as f32, p.xyz[2] as f32]);
        rgb.push(p.rgb);
    }
    (xyz, rgb)
}

fn emit_cameras_from_model(ctx: &JobCtx, model: &Model) {
    let total = model.images.len() as u32;
    if total == 0 {
        return;
    }
    ctx.emit(JobEvent::CamerasReset {
        job_id: ctx.job_id.clone(),
    });
    let depth = scene_depth_hint(model);
    for (i, img) in model.images.iter().enumerate() {
        let Some(cam) = model.cameras.get(&img.camera_id) else {
            continue;
        };
        let (apex, corners) = image_frustum(img, cam, depth);
        ctx.emit(JobEvent::CameraRegistered {
            job_id: ctx.job_id.clone(),
            name: img.name.clone(),
            registered: (i + 1) as u32,
            total,
            confidence: 1.0,
            apex,
            corners,
        });
    }
}

fn scene_depth_hint(model: &Model) -> f64 {
    if model.points.is_empty() {
        return 0.35;
    }
    let mut dists: Vec<f64> = Vec::new();
    for img in model.images.iter().take(32) {
        let c = img.center();
        for p in model.points.iter().take(256) {
            let dx = p.xyz[0] - c[0];
            let dy = p.xyz[1] - c[1];
            let dz = p.xyz[2] - c[2];
            let d = (dx * dx + dy * dy + dz * dz).sqrt();
            if d.is_finite() && d > 1e-4 {
                dists.push(d);
            }
        }
    }
    if dists.is_empty() {
        return 0.35;
    }
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (dists[dists.len() / 2] * 0.12).clamp(0.05, 2.0)
}

fn image_frustum(img: &Image, cam: &Camera, depth: f64) -> ([f32; 3], [[f32; 3]; 4]) {
    let apex_v = img.center();
    let apex = [apex_v[0] as f32, apex_v[1] as f32, apex_v[2] as f32];
    let (fx, fy) = cam.focal();
    let (cx, cy) = cam.principal_point();
    let w = cam.width as f64;
    let h = cam.height as f64;
    let rt = m3_transpose(img.rotation());
    let corner = |u: f64, v: f64| -> [f32; 3] {
        let cam_pt = [
            (u - cx) / fx.max(1e-6) * depth,
            (v - cy) / fy.max(1e-6) * depth,
            depth,
        ];
        // p_world = R^T * p_cam + center
        let world = m3_mul_v(rt, cam_pt);
        [
            (world[0] + apex_v[0]) as f32,
            (world[1] + apex_v[1]) as f32,
            (world[2] + apex_v[2]) as f32,
        ]
    };
    (
        apex,
        [
            corner(0.0, 0.0),
            corner(w, 0.0),
            corner(w, h),
            corner(0.0, h),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn writes_readable_xyzrgb_ply() {
        let dir = temp_dir().join(format!("instasplatter-preview-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.ply");
        let xyz = vec![[0.0, 1.0, 2.0], [3.0, 4.0, 5.0]];
        let rgb = vec![[255, 0, 0], [0, 255, 0]];
        let n = write_xyzrgb_ply(&path, &xyz, &rgb).unwrap();
        assert_eq!(n, 2);
        let bytes = std::fs::read(&path).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("end_header"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
