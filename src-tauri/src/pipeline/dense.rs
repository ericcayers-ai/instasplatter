//! Dense geometry bootstrap after sparse SfM.
//!
//! Needle / empty-cloud failures usually start from a sparse COLMAP point
//! cloud that is too thin for Brush to densify into solid surfaces. When
//! CUDA is available we run COLMAP's patch-match MVS and fuse a dense
//! cloud into `init.ply` as isotropic Gaussians. Otherwise we still seed
//! Brush from the sparse `points3D` cloud, which is denser than leaving
//! Brush to invent its own cold start.
//!
//! Neural densifiers (Depth Anything V2, VGGT commercial) land in
//! [`super::sidecars`] when their binaries are present.

use super::brush;
use super::JobCtx;
use crate::colmap::{self, Model, Point3D};
use crate::splat::{ply, SplatCloud, SH_C0};
use std::path::{Path, PathBuf};

/// Hard cap so an enormous fused cloud cannot exhaust VRAM before training.
const MAX_INIT_POINTS: usize = 1_500_000;
/// Skip points whose COLMAP track error is this large (pixels).
const MAX_SPARSE_ERROR: f64 = 4.0;
/// Minimum observations for a sparse point to seed a Gaussian.
const MIN_TRACK_LEN: usize = 2;

/// Convert coloured XYZ points into small isotropic Gaussians Brush can train.
pub fn points_to_gaussians(xyz: &[[f32; 3]], rgb: &[[u8; 3]], scene_scale: f32) -> SplatCloud {
    let n = xyz.len().min(rgb.len());
    // Scale ≈ median nearest-neighbour spacing surrogate from scene radius.
    let base = (scene_scale / (n.max(1) as f32).cbrt()).clamp(1e-4, 0.05);
    let scale_log = base.ln();
    let opac = 0.5f32.ln() - (1.0 - 0.5f32).ln(); // logit(0.5) ≈ 0
    let opac = (opac).clamp(-2.0, 2.0);

    let mut cloud = SplatCloud {
        positions: Vec::with_capacity(n),
        scales_log: Vec::with_capacity(n),
        rot_wxyz: Vec::with_capacity(n),
        opacity_logit: Vec::with_capacity(n),
        sh_dc: Vec::with_capacity(n),
        sh_rest: Vec::new(),
        rest_per_channel: 0,
    };
    for i in 0..n {
        let c = rgb[i];
        let dc = |v: u8| (v as f32 / 255.0 - 0.5) / SH_C0;
        cloud.positions.push(xyz[i]);
        cloud.scales_log.push([scale_log, scale_log, scale_log]);
        cloud.rot_wxyz.push([1.0, 0.0, 0.0, 0.0]);
        cloud.opacity_logit.push(opac);
        cloud.sh_dc.push([dc(c[0]), dc(c[1]), dc(c[2])]);
    }
    cloud
}

fn filter_sparse_points(points: &[Point3D]) -> (Vec<[f32; 3]>, Vec<[u8; 3]>) {
    let mut xyz = Vec::new();
    let mut rgb = Vec::new();
    for p in points {
        if p.track.len() < MIN_TRACK_LEN || p.error > MAX_SPARSE_ERROR {
            continue;
        }
        xyz.push([p.xyz[0] as f32, p.xyz[1] as f32, p.xyz[2] as f32]);
        rgb.push(p.rgb);
    }
    (xyz, rgb)
}

fn subsample(
    xyz: Vec<[f32; 3]>,
    rgb: Vec<[u8; 3]>,
    max_n: usize,
) -> (Vec<[f32; 3]>, Vec<[u8; 3]>) {
    if xyz.len() <= max_n {
        return (xyz, rgb);
    }
    let step = xyz.len() as f64 / max_n as f64;
    let mut ox = Vec::with_capacity(max_n);
    let mut or = Vec::with_capacity(max_n);
    let mut acc = 0.0;
    while ox.len() < max_n && (acc as usize) < xyz.len() {
        let i = acc as usize;
        ox.push(xyz[i]);
        or.push(rgb[i]);
        acc += step;
    }
    (ox, or)
}

fn scene_scale_of(xyz: &[[f32; 3]]) -> f32 {
    if xyz.is_empty() {
        return 1.0;
    }
    let n = xyz.len() as f64;
    let mut c = [0.0f64; 3];
    for p in xyz {
        c[0] += p[0] as f64;
        c[1] += p[1] as f64;
        c[2] += p[2] as f64;
    }
    c[0] /= n;
    c[1] /= n;
    c[2] /= n;
    let mut d: Vec<f32> = xyz
        .iter()
        .map(|p| {
            let dx = p[0] as f64 - c[0];
            let dy = p[1] as f64 - c[1];
            let dz = p[2] as f64 - c[2];
            ((dx * dx + dy * dy + dz * dz).sqrt()) as f32
        })
        .collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    d[(d.len() as f32 * 0.9) as usize].max(1e-3)
}

/// Read a fused / point PLY (xyz + rgb) into parallel arrays.
pub fn read_xyzrgb_ply(path: &Path) -> Result<(Vec<[f32; 3]>, Vec<[u8; 3]>), String> {
    // Prefer the splat reader when the file already looks like a Gaussian PLY.
    if let Ok(cloud) = ply::read_ply(path) {
        if cloud.len() > 0 && !cloud.scales_log.is_empty() {
            let xyz = cloud.positions.clone();
            let rgb: Vec<[u8; 3]> = cloud
                .sh_dc
                .iter()
                .map(|dc| {
                    let to_u8 = |v: f32| {
                        ((0.5 + SH_C0 * v) * 255.0).clamp(0.0, 255.0) as u8
                    };
                    [to_u8(dc[0]), to_u8(dc[1]), to_u8(dc[2])]
                })
                .collect();
            return Ok((xyz, rgb));
        }
    }

    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let probe = &bytes[..bytes.len().min(128 * 1024)];
    let text = String::from_utf8_lossy(probe);
    let end = text
        .find("end_header")
        .ok_or_else(|| "Not a PLY file.".to_string())?;
    let nl = text[end..]
        .find('\n')
        .ok_or_else(|| "Malformed PLY header.".to_string())?;
    let data_start = end + nl + 1;
    let header = &text[..end];

    let mut vertex_count = 0usize;
    let mut props: Vec<(String, usize)> = Vec::new();
    let mut offset = 0usize;
    let mut in_vertex = false;
    let size_of = |t: &str| -> usize {
        match t {
            "float" | "float32" | "uint" | "int" | "uint32" | "int32" => 4,
            "double" | "float64" => 8,
            "uchar" | "uint8" | "char" | "int8" => 1,
            "ushort" | "uint16" | "short" | "int16" => 2,
            _ => 4,
        }
    };
    for line in header.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.first() == Some(&"element") {
            in_vertex = parts.get(1) == Some(&"vertex");
            if in_vertex {
                vertex_count = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            }
        } else if parts.first() == Some(&"property") && in_vertex && parts.len() >= 3 {
            let sz = size_of(parts[1]);
            props.push((parts[2].to_string(), offset));
            offset += sz;
        }
    }
    let stride = offset;
    if stride == 0 || vertex_count == 0 {
        return Err("Empty point cloud.".into());
    }
    let find = |name: &str| props.iter().find(|(n, _)| n == name).map(|(_, o)| *o);
    let ox = find("x").ok_or("PLY missing x")?;
    let oy = find("y").ok_or("PLY missing y")?;
    let oz = find("z").ok_or("PLY missing z")?;
    let or = find("red").or_else(|| find("r"));
    let og = find("green").or_else(|| find("g"));
    let ob = find("blue").or_else(|| find("b"));

    let data = &bytes[data_start..];
    let mut xyz = Vec::with_capacity(vertex_count);
    let mut rgb = Vec::with_capacity(vertex_count);
    for i in 0..vertex_count {
        let base = i * stride;
        if base + stride > data.len() {
            break;
        }
        let read_f32 = |off: usize| {
            f32::from_le_bytes([
                data[base + off],
                data[base + off + 1],
                data[base + off + 2],
                data[base + off + 3],
            ])
        };
        xyz.push([read_f32(ox), read_f32(oy), read_f32(oz)]);
        let mut c = [180u8, 180, 180];
        if let (Some(r), Some(g), Some(b)) = (or, og, ob) {
            // COLOUR can be uchar or float; try uchar first.
            if r + 1 <= stride {
                c = [data[base + r], data[base + g], data[base + b]];
                // If they look like float halves of colour in 0..1, reinterpret.
                if c[0] == 0 && c[1] == 0 && read_f32(r) > 0.0 && read_f32(r) <= 1.0 {
                    c = [
                        (read_f32(r) * 255.0) as u8,
                        (read_f32(g) * 255.0) as u8,
                        (read_f32(b) * 255.0) as u8,
                    ];
                }
            }
        }
        rgb.push(c);
    }
    Ok((xyz, rgb))
}

pub fn write_init_from_points(
    ctx: &JobCtx,
    xyz: Vec<[f32; 3]>,
    rgb: Vec<[u8; 3]>,
    label: &str,
) -> Result<usize, String> {
    let (xyz, rgb) = subsample(xyz, rgb, MAX_INIT_POINTS);
    if xyz.len() < 32 {
        return Err(format!("{label} produced too few points ({})", xyz.len()));
    }
    let scale = scene_scale_of(&xyz);
    let cloud = points_to_gaussians(&xyz, &rgb, scale);
    let dest = brush::init_ply_path(&ctx.workspace);
    ply::write_ply(&dest, &cloud)?;
    ctx.log(format!(
        "Dense init ({label}): wrote {} Gaussians to init.ply (scene scale {:.3})",
        cloud.len(),
        scale
    ));
    Ok(cloud.len())
}

fn seed_from_sparse_model(ctx: &JobCtx, model: &Model) -> Result<usize, String> {
    let (xyz, rgb) = filter_sparse_points(&model.points);
    write_init_from_points(ctx, xyz, rgb, "sparse COLMAP")
}

/// Attempt COLMAP patch-match stereo densification. Returns Ok(true) when a
/// denser init.ply was written, Ok(false) when the sparse fallback was used.
pub async fn densify_after_sfm(ctx: &JobCtx, images_dir: &Path) -> Result<bool, String> {
    if !ctx.settings.dense_init {
        // Still seed from sparse points when densify is off: cold Brush starts
        // are what produce the classic needle/floater cloud.
        if let Some(model_dir) = colmap::find_model_dir(&ctx.workspace) {
            let model = colmap::read_model(&model_dir)?;
            let _ = seed_from_sparse_model(ctx, &model);
        }
        return Ok(false);
    }

    ctx.stage_progress("sfm", 0.85, "Densifying geometry…");

    // Neural sidecars first: they produce denser pointmaps when installed.
    if let Some(n) = super::sidecars::try_dense_init(ctx, images_dir).await? {
        ctx.log(format!("Neural dense init produced {n} Gaussians."));
        return Ok(true);
    }

    let model_dir = colmap::find_model_dir(&ctx.workspace)
        .ok_or("No sparse model to densify.")?;
    let model = colmap::read_model(&model_dir)?;

    // CUDA MVS is the primary densifier on NVIDIA; without it, skip straight
    // to the sparse seed (still better than no init.ply).
    if !ctx.settings.sift_gpu {
        ctx.notice(
            "Dense MVS needs a CUDA COLMAP build. Seeding training from the sparse cloud instead.",
        );
        seed_from_sparse_model(ctx, &model)?;
        return Ok(false);
    }

    let ws = &ctx.workspace;
    let dense = ws.join("dense");
    let undistorted = dense.join("undistorted");
    let sparse_s = model_dir.to_string_lossy().into_owned();
    let img_s = images_dir.to_string_lossy().into_owned();
    let und_s = undistorted.to_string_lossy().into_owned();
    let n_imgs = model.images.len();

    std::fs::create_dir_all(&dense).map_err(|e| e.to_string())?;

    // 1) Undistort into a workspace patch-match understands.
    if let Err(e) = super::colmap::run_colmap_pub(
        ctx,
        (0.82, 0.88),
        &[
            "image_undistorter",
            "--image_path",
            &img_s,
            "--input_path",
            &sparse_s,
            "--output_path",
            &und_s,
            "--output_type",
            "COLMAP",
        ],
        n_imgs,
    )
    .await
    {
        ctx.notice(format!(
            "Dense undistort failed ({e}). Seeding from the sparse cloud."
        ));
        seed_from_sparse_model(ctx, &model)?;
        return Ok(false);
    }

    // 2) Patch-match stereo. Cap by training resolution so Draft/Eco stays
    // interactive; geom_consistency is reserved for High/Max budgets.
    let mvs_size = ctx.settings.max_resolution.clamp(480, 1600).to_string();
    let use_geom = matches!(
        ctx.settings.preset,
        crate::profiler::Preset::High | crate::profiler::Preset::Max
    );
    let geom = if use_geom { "true" } else { "false" };
    if let Err(e) = super::colmap::run_colmap_pub(
        ctx,
        (0.88, 0.95),
        &[
            "patch_match_stereo",
            "--workspace_path",
            &und_s,
            "--workspace_format",
            "COLMAP",
            "--PatchMatchStereo.geom_consistency",
            geom,
            "--PatchMatchStereo.max_image_size",
            &mvs_size,
        ],
        n_imgs,
    )
    .await
    {
        ctx.notice(format!(
            "Patch-match stereo failed ({e}). Seeding from the sparse cloud."
        ));
        seed_from_sparse_model(ctx, &model)?;
        return Ok(false);
    }

    // 3) Fuse depth maps into a coloured point cloud.
    let fused = PathBuf::from(&und_s).join("fused.ply");
    let fused_s = fused.to_string_lossy().into_owned();
    if let Err(e) = super::colmap::run_colmap_pub(
        ctx,
        (0.95, 0.99),
        &[
            "stereo_fusion",
            "--workspace_path",
            &und_s,
            "--workspace_format",
            "COLMAP",
            "--input_type",
            "geometric",
            "--output_path",
            &fused_s,
        ],
        n_imgs,
    )
    .await
    {
        ctx.notice(format!(
            "Stereo fusion failed ({e}). Seeding from the sparse cloud."
        ));
        seed_from_sparse_model(ctx, &model)?;
        return Ok(false);
    }

    if !fused.exists() {
        ctx.notice("Stereo fusion produced no cloud. Seeding from the sparse cloud.");
        seed_from_sparse_model(ctx, &model)?;
        return Ok(false);
    }

    match read_xyzrgb_ply(&fused) {
        Ok((xyz, rgb)) => {
            // Merge sparse high-confidence points so thin structures survive.
            let (sx, sr) = filter_sparse_points(&model.points);
            let mut xyz = xyz;
            let mut rgb = rgb;
            xyz.extend(sx);
            rgb.extend(sr);
            write_init_from_points(ctx, xyz, rgb, "MVS+sparse")?;
            // Drop the bulky undistorted workspace; training only needs init.ply
            // and the original sparse poses/images.
            let _ = std::fs::remove_dir_all(&dense);
            Ok(true)
        }
        Err(e) => {
            ctx.notice(format!(
                "Could not read fused.ply ({e}). Seeding from the sparse cloud."
            ));
            seed_from_sparse_model(ctx, &model)?;
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_become_isotropic_gaussians_with_matching_colour() {
        let xyz = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let rgb = vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]];
        let cloud = points_to_gaussians(&xyz, &rgb, 1.0);
        assert_eq!(cloud.len(), 3);
        assert!(cloud.sh_dc[0][0] > cloud.sh_dc[0][1]);
        // Absolute scale stays on a sensible order for unit scenes.
        let s = cloud.scale(0)[0];
        assert!(s > 1e-5 && s < 0.2, "{s}");
    }

    #[test]
    fn subsample_keeps_span_endpoints() {
        let xyz: Vec<[f32; 3]> = (0..100).map(|i| [i as f32, 0.0, 0.0]).collect();
        let rgb = vec![[10u8, 10, 10]; 100];
        let (x, _) = subsample(xyz, rgb, 10);
        assert_eq!(x.len(), 10);
        assert_eq!(x[0][0], 0.0);
        assert!(x.last().unwrap()[0] >= 90.0);
    }
}
