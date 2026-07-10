//! Rendering depth and normals from a trained splat, on the CPU
//! (ROADMAP-V2 4.1).
//!
//! This is an EWA splat rasterizer. Each Gaussian's world covariance is
//! pushed through the camera and the perspective Jacobian into a 2D conic,
//! then composited front to back. The value read out is not colour but the
//! alpha-weighted mean depth, which is exactly what a TSDF wants.
//!
//! A pixel is only trusted once enough opacity has accumulated over it. Thin
//! or transparent regions therefore report nothing rather than a depth pulled
//! from a single faint Gaussian, and the volume leaves them unobserved.
//!
//! The normal of a Gaussian is its thinnest axis: a well-converged splat is
//! surfel-like, and that axis is the surface normal. Without the optional 2DGS
//! flattening regularizer these are noisier, which the TSDF averages out.

use crate::colmap::{Camera, Image};
use crate::splat::SplatCloud;
use rayon::prelude::*;

/// Pixels per tile edge. Splats are bucketed into tiles so a pixel only walks
/// the Gaussians that can actually cover it.
const TILE: usize = 16;

/// A splat wider than this many pixels is clipped rather than added to a huge
/// number of tiles. Very large Gaussians are background and contribute nothing
/// useful to a surface.
const MAX_RADIUS_PX: f32 = 96.0;

/// Screen-space low-pass, in pixels squared. Without it a Gaussian smaller
/// than a pixel has an unbounded conic.
const DILATION: f32 = 0.3;

/// Opacity a pixel must accumulate before its depth is believed.
const MIN_ALPHA: f32 = 0.5;

/// Depth and normals rendered from one camera.
#[derive(Debug, Clone)]
pub struct DepthMap {
    pub width: usize,
    pub height: usize,
    /// Metres along the camera's z axis, or 0 where nothing was seen.
    pub depth: Vec<f32>,
    /// World-space unit normal, or zero where nothing was seen.
    pub normal: Vec<[f32; 3]>,
    /// Intrinsics of this render, already scaled to `width` by `height`.
    pub fx: f32,
    pub fy: f32,
    pub cx: f32,
    pub cy: f32,
}

impl DepthMap {
    #[cfg(test)]
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.depth[y * self.width + x]
    }

    pub fn valid_pixels(&self) -> usize {
        self.depth.iter().filter(|d| **d > 0.0).count()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Longest side of the rendered map.
    pub max_dim: u32,
}

impl Default for RenderOptions {
    fn default() -> RenderOptions {
        RenderOptions { max_dim: 640 }
    }
}

/// One projected Gaussian, ready to composite.
struct Projected {
    depth: f32,
    cx: f32,
    cy: f32,
    /// Inverse of the 2D covariance: `[a, b, c]` for `[[a, b], [b, c]]`.
    conic: [f32; 3],
    radius: f32,
    opacity: f32,
    normal: [f32; 3],
}

/// Invert a symmetric 2x2, or `None` when it is singular.
fn invert2(m: [f32; 3]) -> Option<[f32; 3]> {
    let det = m[0] * m[2] - m[1] * m[1];
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    Some([m[2] * inv, -m[1] * inv, m[0] * inv])
}

/// Largest eigenvalue of a symmetric 2x2 `[[a, b], [b, c]]`.
fn max_eigenvalue(m: [f32; 3]) -> f32 {
    let (a, b, c) = (m[0], m[1], m[2]);
    let mid = 0.5 * (a + c);
    let off = (0.25 * (a - c) * (a - c) + b * b).sqrt();
    mid + off
}

/// The thinnest axis of Gaussian `i`, in world space, pointed at `eye`.
fn surfel_normal(cloud: &SplatCloud, i: usize, eye: [f32; 3]) -> [f32; 3] {
    let s = cloud.scale(i);
    let r = cloud.rot_matrix(i);
    let axis = (0..3).min_by(|&a, &b| s[a].partial_cmp(&s[b]).unwrap()).unwrap();
    let mut n = [r[0][axis], r[1][axis], r[2][axis]];
    let p = cloud.positions[i];
    let to_eye = [eye[0] - p[0], eye[1] - p[1], eye[2] - p[2]];
    if n[0] * to_eye[0] + n[1] * to_eye[1] + n[2] * to_eye[2] < 0.0 {
        n = [-n[0], -n[1], -n[2]];
    }
    n
}

/// Render depth and normals for `image`, whose intrinsics are `camera`.
pub fn render(
    cloud: &SplatCloud,
    camera: &Camera,
    image: &Image,
    opts: RenderOptions,
) -> DepthMap {
    let (w0, h0) = (camera.width as f32, camera.height as f32);
    let s = if opts.max_dim > 0 {
        (opts.max_dim as f32 / w0.max(h0)).min(1.0)
    } else {
        1.0
    };
    let width = ((w0 * s).round() as usize).max(1);
    let height = ((h0 * s).round() as usize).max(1);
    let (fx0, fy0) = camera.focal();
    let (cx0, cy0) = camera.principal_point();
    let (fx, fy) = ((fx0 as f32) * s, (fy0 as f32) * s);
    let (cx, cy) = ((cx0 as f32) * s, (cy0 as f32) * s);

    let r = image.rotation();
    let t = image.tvec;
    let rm: [[f32; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| r[i][j] as f32));
    let tv: [f32; 3] = [t[0] as f32, t[1] as f32, t[2] as f32];
    let centre = image.center();
    let eye = [centre[0] as f32, centre[1] as f32, centre[2] as f32];

    // ---- Project every Gaussian, keeping those that land on screen ---------
    let mut splats: Vec<Projected> = (0..cloud.len())
        .into_par_iter()
        .filter_map(|i| {
            let p = cloud.positions[i];
            let cam = [
                rm[0][0] * p[0] + rm[0][1] * p[1] + rm[0][2] * p[2] + tv[0],
                rm[1][0] * p[0] + rm[1][1] * p[1] + rm[1][2] * p[2] + tv[1],
                rm[2][0] * p[0] + rm[2][1] * p[1] + rm[2][2] * p[2] + tv[2],
            ];
            let z = cam[2];
            if z < 1e-3 {
                return None;
            }

            let opacity = cloud.opacity(i);
            if opacity < 0.02 {
                return None;
            }

            // Sigma_cam = R Sigma R^T, both symmetric.
            let c = cloud.covariance(i);
            let sigma = [[c[0], c[1], c[2]], [c[1], c[3], c[4]], [c[2], c[4], c[5]]];
            let mut rs = [[0.0f32; 3]; 3];
            for a in 0..3 {
                for b in 0..3 {
                    rs[a][b] = (0..3).map(|k| rm[a][k] * sigma[k][b]).sum();
                }
            }
            let mut cov_cam = [[0.0f32; 3]; 3];
            for a in 0..3 {
                for b in 0..3 {
                    cov_cam[a][b] = (0..3).map(|k| rs[a][k] * rm[b][k]).sum();
                }
            }

            // Perspective Jacobian at this point.
            let inv_z = 1.0 / z;
            let j = [
                [fx * inv_z, 0.0, -fx * cam[0] * inv_z * inv_z],
                [0.0, fy * inv_z, -fy * cam[1] * inv_z * inv_z],
            ];
            let mut jc = [[0.0f32; 3]; 2];
            for a in 0..2 {
                for b in 0..3 {
                    jc[a][b] = (0..3).map(|k| j[a][k] * cov_cam[k][b]).sum();
                }
            }
            let mut cov2 = [0.0f32; 3]; // [a, b, c]
            cov2[0] = (0..3).map(|k| jc[0][k] * j[0][k]).sum::<f32>() + DILATION;
            cov2[1] = (0..3).map(|k| jc[0][k] * j[1][k]).sum::<f32>();
            cov2[2] = (0..3).map(|k| jc[1][k] * j[1][k]).sum::<f32>() + DILATION;

            let conic = invert2(cov2)?;
            // Three standard deviations covers over 99% of the mass.
            let radius = 3.0 * max_eigenvalue(cov2).max(0.0).sqrt();
            if !radius.is_finite() || radius <= 0.0 || radius > MAX_RADIUS_PX {
                return None;
            }

            let px = fx * cam[0] * inv_z + cx;
            let py = fy * cam[1] * inv_z + cy;
            if px + radius < 0.0
                || py + radius < 0.0
                || px - radius >= width as f32
                || py - radius >= height as f32
            {
                return None;
            }

            Some(Projected {
                depth: z,
                cx: px,
                cy: py,
                conic,
                radius,
                opacity,
                normal: surfel_normal(cloud, i, eye),
            })
        })
        .collect();

    // Front to back, so compositing can stop once the pixel is opaque.
    splats.par_sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap_or(std::cmp::Ordering::Equal));

    // ---- Bucket into tiles, preserving depth order ------------------------
    let tiles_x = width.div_ceil(TILE);
    let tiles_y = height.div_ceil(TILE);
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); tiles_x * tiles_y];
    for (i, s) in splats.iter().enumerate() {
        let x0 = (((s.cx - s.radius) / TILE as f32).floor().max(0.0) as usize).min(tiles_x - 1);
        let x1 = (((s.cx + s.radius) / TILE as f32).floor().max(0.0) as usize).min(tiles_x - 1);
        let y0 = (((s.cy - s.radius) / TILE as f32).floor().max(0.0) as usize).min(tiles_y - 1);
        let y1 = (((s.cy + s.radius) / TILE as f32).floor().max(0.0) as usize).min(tiles_y - 1);
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                buckets[ty * tiles_x + tx].push(i as u32);
            }
        }
    }

    // ---- Composite --------------------------------------------------------
    let mut depth = vec![0.0f32; width * height];
    let mut normal = vec![[0.0f32; 3]; width * height];

    let rows: Vec<(usize, Vec<(f32, [f32; 3])>)> = (0..height)
        .into_par_iter()
        .map(|y| {
            let ty = y / TILE;
            let mut row = Vec::with_capacity(width);
            for x in 0..width {
                let tx = x / TILE;
                let mut transmittance = 1.0f32;
                let mut depth_acc = 0.0f32;
                let mut normal_acc = [0.0f32; 3];
                let mut weight = 0.0f32;

                for &si in &buckets[ty * tiles_x + tx] {
                    let s = &splats[si as usize];
                    let dx = x as f32 + 0.5 - s.cx;
                    let dy = y as f32 + 0.5 - s.cy;
                    let power = -0.5
                        * (s.conic[0] * dx * dx + 2.0 * s.conic[1] * dx * dy + s.conic[2] * dy * dy);
                    if power > 0.0 || power < -12.0 {
                        continue;
                    }
                    let a = (s.opacity * power.exp()).min(0.99);
                    if a < 1.0 / 255.0 {
                        continue;
                    }
                    let contribution = transmittance * a;
                    depth_acc += contribution * s.depth;
                    for k in 0..3 {
                        normal_acc[k] += contribution * s.normal[k];
                    }
                    weight += contribution;
                    transmittance *= 1.0 - a;
                    if transmittance < 1e-3 {
                        break;
                    }
                }

                let covered = 1.0 - transmittance;
                if covered < MIN_ALPHA || weight < 1e-6 {
                    row.push((0.0, [0.0; 3]));
                } else {
                    let n = [
                        normal_acc[0] / weight,
                        normal_acc[1] / weight,
                        normal_acc[2] / weight,
                    ];
                    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    let n = if len > 1e-6 {
                        [n[0] / len, n[1] / len, n[2] / len]
                    } else {
                        [0.0; 3]
                    };
                    row.push((depth_acc / weight, n));
                }
            }
            (y, row)
        })
        .collect();

    for (y, row) in rows {
        for (x, (d, n)) in row.into_iter().enumerate() {
            depth[y * width + x] = d;
            normal[y * width + x] = n;
        }
    }

    DepthMap {
        width,
        height,
        depth,
        normal,
        fx,
        fy,
        cx,
        cy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colmap::CameraModel;

    fn camera(w: u64, h: u64, f: f64) -> Camera {
        Camera {
            id: 1,
            model: CameraModel::Pinhole,
            width: w,
            height: h,
            params: vec![f, f, w as f64 / 2.0, h as f64 / 2.0],
        }
    }

    /// A camera at the origin looking down +z.
    fn identity_image() -> Image {
        Image {
            id: 1,
            qvec: [1.0, 0.0, 0.0, 0.0],
            tvec: [0.0, 0.0, 0.0],
            camera_id: 1,
            name: "a.png".into(),
            points2d: Vec::new(),
        }
    }

    /// One opaque, roughly isotropic Gaussian at `p`.
    fn one_splat(p: [f32; 3], scale: f32) -> SplatCloud {
        SplatCloud {
            positions: vec![p],
            scales_log: vec![[scale.ln(), scale.ln(), (scale * 0.05).ln()]],
            rot_wxyz: vec![[1.0, 0.0, 0.0, 0.0]],
            opacity_logit: vec![8.0], // essentially opaque
            sh_dc: vec![[1.0, 1.0, 1.0]],
            sh_rest: Vec::new(),
            rest_per_channel: 0,
        }
    }

    #[test]
    fn a_splat_in_front_of_the_camera_reports_its_depth() {
        let cloud = one_splat([0.0, 0.0, 3.0], 0.25);
        let map = render(&cloud, &camera(128, 128, 128.0), &identity_image(), RenderOptions { max_dim: 128 });
        assert_eq!((map.width, map.height), (128, 128));
        let d = map.at(64, 64);
        assert!((d - 3.0).abs() < 1e-3, "centre depth {d}");
        assert!(map.valid_pixels() > 20, "only {} pixels", map.valid_pixels());
    }

    #[test]
    fn a_splat_behind_the_camera_renders_nothing() {
        let cloud = one_splat([0.0, 0.0, -3.0], 0.25);
        let map = render(&cloud, &camera(64, 64, 64.0), &identity_image(), RenderOptions { max_dim: 64 });
        assert_eq!(map.valid_pixels(), 0);
        assert!(map.depth.iter().all(|d| *d == 0.0));
    }

    #[test]
    fn an_empty_cloud_renders_an_empty_map() {
        let cloud = SplatCloud::default();
        let map = render(&cloud, &camera(32, 32, 32.0), &identity_image(), RenderOptions { max_dim: 32 });
        assert_eq!(map.valid_pixels(), 0);
        assert_eq!(map.depth.len(), 32 * 32);
    }

    #[test]
    fn a_transparent_splat_is_not_trusted_as_a_surface() {
        let mut cloud = one_splat([0.0, 0.0, 3.0], 0.25);
        cloud.opacity_logit = vec![-3.0]; // about 0.05 opacity
        let map = render(&cloud, &camera(64, 64, 64.0), &identity_image(), RenderOptions { max_dim: 64 });
        assert_eq!(map.valid_pixels(), 0, "a faint splat must not define depth");
    }

    #[test]
    fn the_nearer_of_two_splats_wins() {
        let mut cloud = one_splat([0.0, 0.0, 5.0], 0.3);
        // A second, nearer splat covering the same pixels.
        cloud.positions.push([0.0, 0.0, 2.0]);
        cloud.scales_log.push([0.3f32.ln(), 0.3f32.ln(), 0.015f32.ln()]);
        cloud.rot_wxyz.push([1.0, 0.0, 0.0, 0.0]);
        cloud.opacity_logit.push(8.0);
        cloud.sh_dc.push([1.0, 1.0, 1.0]);

        let map = render(&cloud, &camera(128, 128, 128.0), &identity_image(), RenderOptions { max_dim: 128 });
        let d = map.at(64, 64);
        assert!((d - 2.0).abs() < 0.05, "occlusion not respected: depth {d}");
    }

    #[test]
    fn the_render_is_downscaled_and_the_intrinsics_follow() {
        let cloud = one_splat([0.0, 0.0, 3.0], 0.25);
        let map = render(&cloud, &camera(800, 600, 700.0), &identity_image(), RenderOptions { max_dim: 400 });
        assert_eq!((map.width, map.height), (400, 300));
        assert!((map.fx - 350.0).abs() < 1e-3);
        assert!((map.cx - 200.0).abs() < 1e-3);
        assert!((map.cy - 150.0).abs() < 1e-3);
    }

    #[test]
    fn the_normal_of_a_flat_splat_faces_the_camera() {
        // Thin along z, so the surfel normal is +-z; it must point at the eye.
        let cloud = one_splat([0.0, 0.0, 3.0], 0.4);
        let map = render(&cloud, &camera(64, 64, 64.0), &identity_image(), RenderOptions { max_dim: 64 });
        let n = map.normal[64 / 2 * 64 + 32];
        assert!(n[2] < -0.9, "normal {n:?} does not face the camera at the origin");
    }
}
