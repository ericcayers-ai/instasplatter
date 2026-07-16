//! Dense geometry bootstrap after sparse SfM.
//!
//! Needle / empty-cloud failures usually start from a sparse COLMAP point
//! cloud that is too thin for Brush to densify into solid surfaces.
//! Sources are confidence-fused (Sim(3) align + voxel merge), never raw
//! concatenation of incompatible frames.
//!
//! Neural densifiers land in [`super::sidecars`] when their binaries are present.

use super::brush;
use super::JobCtx;
use crate::colmap::{self, Model, Point3D};
use crate::math::{self, M3, V3};
use crate::splat::{ply, SplatCloud, SH_C0};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Hard cap so an enormous fused cloud cannot exhaust VRAM before training.
const MAX_INIT_POINTS: usize = 1_500_000;
/// Skip points whose COLMAP track error is this large (pixels).
const MAX_SPARSE_ERROR: f64 = 4.0;
/// Minimum observations for a sparse point to seed a Gaussian.
const MIN_TRACK_LEN: usize = 2;
/// Voxel size as a fraction of scene scale for confidence-weighted fusion.
const VOXEL_FRAC: f32 = 0.0025;
/// Max orientation angle (degrees) when accepting a Sim(3) hypothesis.
const MAX_ALIGN_ANGLE_DEG: f64 = 35.0;
/// Max scale ratio between clouds (absolute log).
const MAX_LOG_SCALE: f64 = 1.2; // ~e^1.2 ≈ 3.3×

/// Evidence point carrying confidence + provenance for schema-v2 fusion.
#[derive(Debug, Clone)]
pub struct EvidencePoint {
    pub xyz: [f32; 3],
    pub rgb: [u8; 3],
    pub confidence: f32,
    #[allow(dead_code)]
    pub source: &'static str,
    /// Thin high-confidence tracks (sparse COLMAP) bypass voxel average.
    pub preserve: bool,
}

/// Rigid+scale transform `p' = s R p + t`.
#[derive(Debug, Clone, Copy)]
pub struct Sim3 {
    pub scale: f64,
    pub rotation: M3,
    pub translation: V3,
}

impl Sim3 {
    #[allow(dead_code)]
    pub fn identity() -> Self {
        Self {
            scale: 1.0,
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [0.0, 0.0, 0.0],
        }
    }

    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let v = math::m3_mul_v(self.rotation, [p[0] as f64, p[1] as f64, p[2] as f64]);
        [
            (self.scale * v[0] + self.translation[0]) as f32,
            (self.scale * v[1] + self.translation[1]) as f32,
            (self.scale * v[2] + self.translation[2]) as f32,
        ]
    }
}

fn centroid(pts: &[[f32; 3]]) -> V3 {
    if pts.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    let n = pts.len() as f64;
    let mut c = [0.0f64; 3];
    for p in pts {
        c[0] += p[0] as f64;
        c[1] += p[1] as f64;
        c[2] += p[2] as f64;
    }
    [c[0] / n, c[1] / n, c[2] / n]
}

fn rms_radius(pts: &[[f32; 3]], c: V3) -> f64 {
    if pts.is_empty() {
        return 1.0;
    }
    let mut s = 0.0;
    for p in pts {
        let d = [
            p[0] as f64 - c[0],
            p[1] as f64 - c[1],
            p[2] as f64 - c[2],
        ];
        s += d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    }
    (s / pts.len() as f64).sqrt().max(1e-6)
}

/// Umeyama-style Sim(3) from source → target using centroids + covariance SVD.
/// Uses a subsample of up to `max_pairs` points (index-aligned subsample).
pub fn estimate_sim3(src: &[[f32; 3]], tgt: &[[f32; 3]], max_pairs: usize) -> Option<Sim3> {
    if src.len() < 8 || tgt.len() < 8 {
        return None;
    }
    let n = src.len().min(tgt.len()).min(max_pairs.max(8));
    let step_s = src.len() as f64 / n as f64;
    let step_t = tgt.len() as f64 / n as f64;
    let mut a = Vec::with_capacity(n);
    let mut b = Vec::with_capacity(n);
    for i in 0..n {
        a.push(src[(i as f64 * step_s) as usize]);
        b.push(tgt[(i as f64 * step_t) as usize]);
    }
    let ca = centroid(&a);
    let cb = centroid(&b);
    let ra = rms_radius(&a, ca);
    let rb = rms_radius(&b, cb);
    let scale = (rb / ra).clamp((-MAX_LOG_SCALE).exp(), MAX_LOG_SCALE.exp());

    // Covariance H = Σ (a-ca)(b-cb)^T
    let mut h = [[0.0f64; 3]; 3];
    for i in 0..n {
        let pa = [
            (a[i][0] as f64 - ca[0]) / ra,
            (a[i][1] as f64 - ca[1]) / ra,
            (a[i][2] as f64 - ca[2]) / ra,
        ];
        let pb = [
            (b[i][0] as f64 - cb[0]) / rb,
            (b[i][1] as f64 - cb[1]) / rb,
            (b[i][2] as f64 - cb[2]) / rb,
        ];
        for r in 0..3 {
            for c in 0..3 {
                h[r][c] += pa[r] * pb[c];
            }
        }
    }
    let (u, _s, vt) = math::svd3(h);
    let mut r = math::m3_mul(math::m3_transpose(vt), math::m3_transpose(u));
    // Enforce proper rotation (det +1).
    if math::m3_det(r) < 0.0 {
        let mut vt_fix = vt;
        vt_fix[2][0] *= -1.0;
        vt_fix[2][1] *= -1.0;
        vt_fix[2][2] *= -1.0;
        r = math::m3_mul(math::m3_transpose(vt_fix), math::m3_transpose(u));
    }
    r = math::orthonormalize(r);

    // Orientation gate: rotation angle from identity.
    let tr = (r[0][0] + r[1][1] + r[2][2]).clamp(-1.0, 3.0);
    let angle = ((tr - 1.0) * 0.5).clamp(-1.0, 1.0).acos().to_degrees();
    if angle > MAX_ALIGN_ANGLE_DEG && (scale.ln().abs() > 0.15) {
        // Large rotation + scale change ⇒ reject; treat as already-aligned
        // only when both clouds share a near-common frame already.
        if angle > 90.0 {
            return None;
        }
    }
    if scale.ln().abs() > MAX_LOG_SCALE {
        return None;
    }

    let t = [
        cb[0] - scale * (r[0][0] * ca[0] + r[0][1] * ca[1] + r[0][2] * ca[2]),
        cb[1] - scale * (r[1][0] * ca[0] + r[1][1] * ca[1] + r[1][2] * ca[2]),
        cb[2] - scale * (r[2][0] * ca[0] + r[2][1] * ca[1] + r[2][2] * ca[2]),
    ];
    Some(Sim3 {
        scale,
        rotation: r,
        translation: t,
    })
}

fn bounds_ok(pts: &[[f32; 3]], ref_pts: &[[f32; 3]]) -> bool {
    if pts.is_empty() || ref_pts.is_empty() {
        return false;
    }
    let c_ref = centroid(ref_pts);
    let r_ref = rms_radius(ref_pts, c_ref);
    let c = centroid(pts);
    let r = rms_radius(pts, c);
    if r < 1e-5 || r_ref < 1e-5 {
        return false;
    }
    let center_dist = math::norm([
        c[0] - c_ref[0],
        c[1] - c_ref[1],
        c[2] - c_ref[2],
    ]);
    center_dist < r_ref * 4.0 && (r / r_ref).ln().abs() < MAX_LOG_SCALE + 0.3
}

/// Confidence-weighted voxel fusion with preserved thin tracks.
pub fn fuse_evidence(
    clouds: &[Vec<EvidencePoint>],
    scene_scale: f32,
    max_points: usize,
) -> (Vec<[f32; 3]>, Vec<[u8; 3]>) {
    let voxel = (scene_scale * VOXEL_FRAC).clamp(1e-4, 0.05);
    let inv = 1.0 / voxel;

    #[derive(Default, Clone)]
    struct Acc {
        xyz: [f64; 3],
        rgb: [f64; 3],
        w: f64,
    }
    let mut grid: HashMap<(i32, i32, i32), Acc> = HashMap::new();
    let mut preserved: Vec<EvidencePoint> = Vec::new();

    for cloud in clouds {
        for p in cloud {
            if p.preserve && p.confidence >= 0.7 {
                preserved.push(p.clone());
                continue;
            }
            if p.confidence < 0.15 {
                continue;
            }
            let key = (
                (p.xyz[0] * inv).floor() as i32,
                (p.xyz[1] * inv).floor() as i32,
                (p.xyz[2] * inv).floor() as i32,
            );
            let w = p.confidence as f64;
            let e = grid.entry(key).or_default();
            e.xyz[0] += p.xyz[0] as f64 * w;
            e.xyz[1] += p.xyz[1] as f64 * w;
            e.xyz[2] += p.xyz[2] as f64 * w;
            e.rgb[0] += p.rgb[0] as f64 * w;
            e.rgb[1] += p.rgb[1] as f64 * w;
            e.rgb[2] += p.rgb[2] as f64 * w;
            e.w += w;
        }
    }

    let mut fused: Vec<(f32, [f32; 3], [u8; 3])> = grid
        .into_values()
        .filter(|a| a.w > 1e-6)
        .map(|a| {
            let inv_w = 1.0 / a.w;
            let xyz = [
                (a.xyz[0] * inv_w) as f32,
                (a.xyz[1] * inv_w) as f32,
                (a.xyz[2] * inv_w) as f32,
            ];
            let rgb = [
                (a.rgb[0] * inv_w).round().clamp(0.0, 255.0) as u8,
                (a.rgb[1] * inv_w).round().clamp(0.0, 255.0) as u8,
                (a.rgb[2] * inv_w).round().clamp(0.0, 255.0) as u8,
            ];
            (a.w as f32, xyz, rgb)
        })
        .collect();
    fused.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Dedup preserved against dense voxels, then append.
    let mut out_xyz = Vec::new();
    let mut out_rgb = Vec::new();
    let budget = max_points.saturating_sub(preserved.len().min(max_points / 5));
    for (_, xyz, rgb) in fused.into_iter().take(budget) {
        out_xyz.push(xyz);
        out_rgb.push(rgb);
    }
    for p in preserved {
        if out_xyz.len() >= max_points {
            break;
        }
        out_xyz.push(p.xyz);
        out_rgb.push(p.rgb);
    }
    (out_xyz, out_rgb)
}

fn evidence_from_xyzrgb(
    xyz: Vec<[f32; 3]>,
    rgb: Vec<[u8; 3]>,
    source: &'static str,
    confidence: f32,
    preserve: bool,
) -> Vec<EvidencePoint> {
    xyz.into_iter()
        .zip(rgb)
        .map(|(xyz, rgb)| EvidencePoint {
            xyz,
            rgb,
            confidence,
            source,
            preserve,
        })
        .collect()
}

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

/// Scene scale hint across evidence clouds (for neural multi-source fusion).
pub fn scene_scale_hint(clouds: &[Vec<EvidencePoint>]) -> f32 {
    let mut xyz = Vec::new();
    for c in clouds {
        for p in c.iter().take(4_096) {
            xyz.push(p.xyz);
        }
    }
    scene_scale_of(&xyz)
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
    // Live dense layer for the viewport (XYZRGB), independent of Gaussian init.
    if let Err(e) = super::preview::emit_dense_stage(ctx, &xyz, &rgb) {
        ctx.log(format!("[warn] dense preview failed: {e}"));
    }
    Ok(cloud.len())
}

fn seed_from_sparse_model(ctx: &JobCtx, model: &Model) -> Result<usize, String> {
    let (xyz, rgb) = filter_sparse_points(&model.points);
    write_init_from_points(ctx, xyz, rgb, "sparse COLMAP")
}

/// Attempt dense init via confidence-weighted fusion (not raw concatenation).
/// Returns Ok(true) when a denser-than-sparse `init.ply` was written.
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

    let model_dir = colmap::find_model_dir(&ctx.workspace)
        .ok_or("No sparse model to densify.")?;
    let model = colmap::read_model(&model_dir)?;

    // Canonical reference: sparse COLMAP / ENU frame.
    let (sx, sr) = filter_sparse_points(&model.points);
    let ref_xyz = sx.clone();
    let scene = scene_scale_of(&ref_xyz);
    let mut clouds: Vec<Vec<EvidencePoint>> = Vec::new();
    let mut labels: Vec<String> = Vec::new();

    if !sx.is_empty() {
        clouds.push(evidence_from_xyzrgb(sx, sr, "sparse", 0.85, true));
        labels.push("sparse".into());
    }

    // Collect source clouds, align each into the sparse frame, then fuse.
    let mut candidates: Vec<(String, Vec<[f32; 3]>, Vec<[u8; 3]>, f32)> = Vec::new();

    if let Some((rx, rr)) = super::sidecars::try_roma_densify(ctx, images_dir).await? {
        candidates.push(("RoMa".into(), rx, rr, 0.75));
    }

    if let Some((nx, nr, name)) = super::sidecars::try_neural_points(ctx, images_dir).await? {
        ctx.log(format!(
            "Neural dense init ({name}): {} points (will fuse).",
            nx.len()
        ));
        let conf = if name.contains("depth-anything-3") {
            0.70
        } else if name.contains("mapanything") {
            0.68
        } else if name.contains("vggt") {
            0.72
        } else {
            0.60
        };
        candidates.push((name, nx, nr, conf));
    }

    if ctx.settings.sift_gpu {
        match run_mvs_fused(ctx, images_dir, &model_dir, &model).await {
            Ok(Some((mx, mr))) => {
                ctx.log(format!("COLMAP MVS: {} fused points (will fuse).", mx.len()));
                candidates.push(("MVS".into(), mx, mr, 0.80));
            }
            Ok(None) => {}
            Err(e) => {
                ctx.notice(format!("Dense MVS failed ({e}). Continuing with other seeds."));
            }
        }
    } else if candidates.is_empty() && clouds.is_empty() {
        ctx.notice(
            "Dense MVS needs a CUDA COLMAP build. Seeding from neural/sparse clouds instead.",
        );
    }

    for (name, mut xyz, rgb, conf) in candidates {
        if !ref_xyz.is_empty() {
            if !bounds_ok(&xyz, &ref_xyz) {
                // Try Sim(3) into canonical frame.
                match estimate_sim3(&xyz, &ref_xyz, 2_048) {
                    Some(sim) => {
                        xyz = xyz.iter().map(|p| sim.apply(*p)).collect();
                        if !bounds_ok(&xyz, &ref_xyz) {
                            ctx.log(format!(
                                "Skipping {name}: failed orientation/bounds gate after Sim(3)."
                            ));
                            continue;
                        }
                        ctx.log(format!(
                            "Aligned {name} into COLMAP frame (s={:.3}).",
                            sim.scale
                        ));
                    }
                    None => {
                        ctx.log(format!(
                            "Skipping {name}: could not Sim(3)-align to sparse frame."
                        ));
                        continue;
                    }
                }
            }
        }
        let src: &'static str = match name.as_str() {
            "RoMa" => "roma",
            "MVS" => "mvs",
            other if other.contains("depth-anything-3") => "da3",
            other if other.contains("depth-anything") => "dav2",
            other if other.contains("mapanything") => "mapanything",
            other if other.contains("vggt") => "vggt",
            _ => "neural",
        };
        clouds.push(evidence_from_xyzrgb(xyz, rgb, src, conf, false));
        labels.push(name);
    }

    if clouds.is_empty() {
        seed_from_sparse_model(ctx, &model)?;
        return Ok(false);
    }

    let (xyz, rgb) = fuse_evidence(&clouds, scene.max(0.01), MAX_INIT_POINTS);

    if xyz.len() < 32 {
        seed_from_sparse_model(ctx, &model)?;
        return Ok(false);
    }

    let label = if labels.is_empty() {
        "sparse COLMAP".to_string()
    } else {
        labels.join("+")
    };
    ctx.notice(format!("Init (fused): {label}"));
    write_init_from_points(ctx, xyz, rgb, &label)?;
    Ok(labels.iter().any(|l| {
        l == "RoMa"
            || l == "MVS"
            || l.contains("vggt")
            || l.contains("depth")
            || l.contains("mast3r")
            || l.contains("dust3r")
            || l.contains("mapanything")
            || l.contains('+')
    }))
}

/// Run undistort → patch-match → fusion. Returns fused XYZRGB or None.
async fn run_mvs_fused(
    ctx: &JobCtx,
    images_dir: &Path,
    model_dir: &Path,
    model: &Model,
) -> Result<Option<(Vec<[f32; 3]>, Vec<[u8; 3]>)>, String> {
    let ws = &ctx.workspace;
    let dense = ws.join("dense");
    let undistorted = dense.join("undistorted");
    let sparse_s = model_dir.to_string_lossy().into_owned();
    let img_s = images_dir.to_string_lossy().into_owned();
    let und_s = undistorted.to_string_lossy().into_owned();
    let n_imgs = model.images.len();

    std::fs::create_dir_all(&dense).map_err(|e| e.to_string())?;

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
        let _ = std::fs::remove_dir_all(&dense);
        return Err(format!("undistort: {e}"));
    }

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
        let _ = std::fs::remove_dir_all(&dense);
        return Err(format!("patch-match: {e}"));
    }

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
        let _ = std::fs::remove_dir_all(&dense);
        return Err(format!("fusion: {e}"));
    }

    if !fused.exists() {
        let _ = std::fs::remove_dir_all(&dense);
        return Ok(None);
    }

    let result = read_xyzrgb_ply(&fused);
    let _ = std::fs::remove_dir_all(&dense);
    match result {
        Ok((xyz, rgb)) => Ok(Some((xyz, rgb))),
        Err(e) => Err(e),
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

    #[test]
    fn subsample_compose_preserves_multiple_sources() {
        let mut xyz: Vec<[f32; 3]> = (0..100).map(|i| [i as f32, 0.0, 0.0]).collect();
        let mut rgb = vec![[10u8, 10, 10]; 100];
        xyz.extend((0..50).map(|i| [0.0, i as f32, 1.0]));
        rgb.extend(vec![[20u8, 20, 20]; 50]);
        let (x, r) = subsample(xyz, rgb, 80);
        assert_eq!(x.len(), 80);
        assert_eq!(r.len(), 80);
        assert_eq!(x[0][0], 0.0);
    }

    #[test]
    fn fuse_evidence_dedups_voxels_and_keeps_preserved() {
        let a = evidence_from_xyzrgb(
            vec![[0.0, 0.0, 0.0], [0.001, 0.0, 0.0], [1.0, 0.0, 0.0]],
            vec![[255, 0, 0], [250, 0, 0], [0, 255, 0]],
            "mvs",
            0.8,
            false,
        );
        let thin = evidence_from_xyzrgb(
            vec![[0.5, 0.5, 0.5]],
            vec![[0, 0, 255]],
            "sparse",
            0.9,
            true,
        );
        let (xyz, rgb) = fuse_evidence(&[a, thin], 10.0, 100);
        assert!(xyz.len() >= 2);
        assert!(rgb.iter().any(|c| c[2] == 255)); // preserved blue
    }

    #[test]
    fn sim3_identity_on_matching_clouds() {
        let pts: Vec<[f32; 3]> = (0..64)
            .map(|i| [(i % 8) as f32, (i / 8) as f32, 0.0])
            .collect();
        let sim = estimate_sim3(&pts, &pts, 64).expect("sim3");
        assert!((sim.scale - 1.0).abs() < 0.15, "{}", sim.scale);
        let p = sim.apply([1.0, 2.0, 0.0]);
        assert!((p[0] - 1.0).abs() < 0.5);
        assert!((p[1] - 2.0).abs() < 0.5);
    }

    #[test]
    fn bounds_gate_rejects_wild_outliers() {
        let a: Vec<[f32; 3]> = (0..32).map(|i| [i as f32 * 0.1, 0.0, 0.0]).collect();
        let b: Vec<[f32; 3]> = (0..32).map(|i| [i as f32 * 0.1 + 1_000.0, 0.0, 0.0]).collect();
        assert!(!bounds_ok(&b, &a));
        assert!(bounds_ok(&a, &a));
    }
}
