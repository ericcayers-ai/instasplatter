//! Baking a rigid rotation into a splat cloud (ROADMAP-V2 1.3).
//!
//! Rotating a splat set is not just rotating its positions. Each Gaussian's
//! own orientation composes with the rotation, and the view-dependent
//! spherical-harmonic coefficients live in world space, so they have to be
//! rotated into the new frame or every specular highlight ends up pointing
//! the wrong way.
//!
//! Rather than deriving Wigner-D matrices for the exact SH convention 3DGS
//! uses, we build the per-band rotation matrix numerically: a rotation maps
//! each degree-l subspace to itself, so evaluating the basis at 2l+1
//! independent directions gives a square system whose solution is that
//! band's rotation matrix, exactly, for whatever basis and sign convention
//! the evaluator happens to use.

use super::{mat3_to_quat, quat_to_mat3, SplatCloud};
use crate::math::{m3_mul_v, m3_transpose, Mat, M3};

/// The direction of "up" for a scene, used by up-axis alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl Axis {
    pub fn vector(self) -> [f64; 3] {
        match self {
            Axis::PosX => [1.0, 0.0, 0.0],
            Axis::NegX => [-1.0, 0.0, 0.0],
            Axis::PosY => [0.0, 1.0, 0.0],
            Axis::NegY => [0.0, -1.0, 0.0],
            Axis::PosZ => [0.0, 0.0, 1.0],
            Axis::NegZ => [0.0, 0.0, -1.0],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Axis::PosX => "+x",
            Axis::NegX => "-x",
            Axis::PosY => "+y",
            Axis::NegY => "-y",
            Axis::PosZ => "+z",
            Axis::NegZ => "-z",
        }
    }

    pub fn parse(s: &str) -> Option<Axis> {
        Some(match s {
            "+x" | "x" => Axis::PosX,
            "-x" => Axis::NegX,
            "+y" | "y" => Axis::PosY,
            "-y" => Axis::NegY,
            "+z" | "z" => Axis::PosZ,
            "-z" => Axis::NegZ,
            _ => return None,
        })
    }

    /// The signed axis closest to `v`.
    pub fn nearest(v: [f64; 3]) -> Axis {
        let all = [
            Axis::PosX,
            Axis::NegX,
            Axis::PosY,
            Axis::NegY,
            Axis::PosZ,
            Axis::NegZ,
        ];
        let mut best = Axis::PosY;
        let mut best_dot = f64::NEG_INFINITY;
        for a in all {
            let av = a.vector();
            let d = av[0] * v[0] + av[1] * v[1] + av[2] * v[2];
            if d > best_dot {
                best_dot = d;
                best = a;
            }
        }
        best
    }
}

/// Rotation taking unit vector `from` onto unit vector `to`, choosing the
/// shortest arc. Handles the antiparallel case, where the axis is ambiguous.
pub fn rotation_between(from: [f64; 3], to: [f64; 3]) -> M3 {
    use crate::math::{cross, dot, normalize, rodrigues};
    let a = normalize(from);
    let b = normalize(to);
    let d = dot(a, b).clamp(-1.0, 1.0);
    if d > 1.0 - 1e-9 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    if d < -1.0 + 1e-9 {
        // Any axis perpendicular to `a` works; pick a stable one.
        let seed = if a[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let axis = normalize(cross(a, seed));
        return rodrigues(crate::math::scale(axis, std::f64::consts::PI));
    }
    let axis = normalize(cross(a, b));
    rodrigues(crate::math::scale(axis, d.acos()))
}

// ---- Spherical harmonics ---------------------------------------------------

const SH_C1: f64 = 0.488_602_511_902_919_9;
const SH_C2: [f64; 5] = [
    1.092_548_430_592_079_2,
    -1.092_548_430_592_079_2,
    0.315_391_565_252_520_05,
    -1.092_548_430_592_079_2,
    0.546_274_215_296_039_6,
];
const SH_C3: [f64; 7] = [
    -0.590_043_589_926_643_5,
    2.890_611_442_640_554,
    -0.457_045_799_464_465_8,
    0.373_176_332_590_115_4,
    -0.457_045_799_464_465_8,
    1.445_305_721_320_277,
    -0.590_043_589_926_643_5,
];

/// Real SH basis for one band, in the coefficient order 3DGS stores.
fn sh_band(l: usize, d: [f64; 3]) -> Vec<f64> {
    let (x, y, z) = (d[0], d[1], d[2]);
    let (xx, yy, zz) = (x * x, y * y, z * z);
    match l {
        1 => vec![-SH_C1 * y, SH_C1 * z, -SH_C1 * x],
        2 => vec![
            SH_C2[0] * x * y,
            SH_C2[1] * y * z,
            SH_C2[2] * (2.0 * zz - xx - yy),
            SH_C2[3] * x * z,
            SH_C2[4] * (xx - yy),
        ],
        3 => vec![
            SH_C3[0] * y * (3.0 * xx - yy),
            SH_C3[1] * x * y * z,
            SH_C3[2] * y * (4.0 * zz - xx - yy),
            SH_C3[3] * z * (2.0 * zz - 3.0 * xx - 3.0 * yy),
            SH_C3[4] * x * (4.0 * zz - xx - yy),
            SH_C3[5] * z * (xx - yy),
            SH_C3[6] * x * (xx - 3.0 * yy),
        ],
        _ => unreachable!("sh_band only covers l = 1..3"),
    }
}

/// Directions used to pin down each band's rotation matrix, laid out on a
/// golden-angle spiral. Taken exactly `2l+1` at a time these can come out
/// nearly coplanar, so we oversample and solve in least squares instead.
fn probe_dirs(count: usize) -> Vec<[f64; 3]> {
    let golden = std::f64::consts::PI * (3.0 - 5.0f64.sqrt());
    (0..count)
        .map(|i| {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / count as f64;
            let r = (1.0 - z * z).max(0.0).sqrt();
            let theta = golden * i as f64;
            [r * theta.cos(), r * theta.sin(), z]
        })
        .collect()
}

/// Matrix `M` with `c' = M c`, rotating band-`l` coefficients by `r`.
///
/// We need `sum_j c'_j Y_j(d) = sum_j c_j Y_j(Rᵀ d)` for every `d`. Sampling
/// directions gives `A M = B`, which is consistent because rotations preserve
/// each degree-l subspace, so a least-squares solve recovers `M` exactly.
fn sh_band_rotation(l: usize, r: M3) -> Option<Mat> {
    let n = 2 * l + 1;
    let dirs = probe_dirs(8 * n);
    let rt = m3_transpose(r);

    let mut a = Mat::zeros(dirs.len(), n);
    let mut b = Mat::zeros(dirs.len(), n);
    for (i, d) in dirs.iter().enumerate() {
        let ya = sh_band(l, *d);
        let yb = sh_band(l, m3_mul_v(rt, *d));
        for j in 0..n {
            a[(i, j)] = ya[j];
            b[(i, j)] = yb[j];
        }
    }
    crate::math::solve_least_squares(&a, &b)
}

/// Rotate a splat's `f_rest` coefficients in place. `rest_per_channel` is 3,
/// 8 or 15 for SH degree 1, 2 and 3.
fn rotate_sh_rest(cloud: &mut SplatCloud, r: M3) -> Result<(), String> {
    let k = cloud.rest_per_channel;
    if k == 0 {
        return Ok(());
    }
    let degree = cloud.sh_degree() as usize;
    let mut bands: Vec<(usize, Mat)> = Vec::new();
    for l in 1..=degree {
        let m = sh_band_rotation(l, r)
            .ok_or_else(|| format!("Could not build the SH rotation for band {l}."))?;
        bands.push((l, m));
    }

    let n = cloud.len();
    let mut out = vec![0.0f32; cloud.sh_rest.len()];
    for i in 0..n {
        let base = i * k * 3;
        for ch in 0..3 {
            let off = base + ch * k;
            let mut band_start = 0usize;
            for (l, m) in &bands {
                let w = 2 * l + 1;
                for row in 0..w {
                    let mut acc = 0.0f64;
                    for col in 0..w {
                        acc += m[(row, col)] * cloud.sh_rest[off + band_start + col] as f64;
                    }
                    out[off + band_start + row] = acc as f32;
                }
                band_start += w;
            }
        }
    }
    cloud.sh_rest = out;
    Ok(())
}

/// Rotate the whole cloud about `pivot` by `r` (row-major, world space).
pub fn rotate_cloud(cloud: &mut SplatCloud, r: M3, pivot: [f64; 3]) -> Result<(), String> {
    let rf = [
        [r[0][0] as f32, r[0][1] as f32, r[0][2] as f32],
        [r[1][0] as f32, r[1][1] as f32, r[1][2] as f32],
        [r[2][0] as f32, r[2][1] as f32, r[2][2] as f32],
    ];
    let pf = [pivot[0] as f32, pivot[1] as f32, pivot[2] as f32];

    for i in 0..cloud.len() {
        let p = cloud.positions[i];
        let rel = [p[0] - pf[0], p[1] - pf[1], p[2] - pf[2]];
        let rot = super::mat3_mul_vec(rf, rel);
        cloud.positions[i] = [rot[0] + pf[0], rot[1] + pf[1], rot[2] + pf[2]];

        // The splat's own frame composes on the left: R_new = R * R_splat.
        let rs = quat_to_mat3(cloud.unit_rot(i));
        cloud.rot_wxyz[i] = mat3_to_quat(super::mat3_mul(rf, rs));
    }

    rotate_sh_rest(cloud, r)
}

/// Rotation that carries `up` (a direction in the current frame) onto the
/// chosen world axis. Used by both "snap to nearest axis" and the
/// ground-plane alignment, which only differ in how `up` is obtained.
pub fn align_up(up: [f64; 3], target: Axis) -> M3 {
    rotation_between(up, target.vector())
}

/// Estimate the scene's ground plane and return its upward normal.
///
/// A plane is fitted by RANSAC over splat positions, then the normal is
/// oriented to point away from the bulk of the scene, which is the direction
/// a viewer calls "up".
pub fn estimate_ground_normal(cloud: &SplatCloud) -> Option<[f64; 3]> {
    use crate::math::{cross, dot, normalize, sub};

    let n = cloud.len();
    if n < 32 {
        return None;
    }

    // Deterministic sampling: a fixed-stride LCG so repeated runs on the same
    // cloud produce the same plane.
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = |m: usize| -> usize {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 33) as usize) % m
    };

    let pts: Vec<[f64; 3]> = cloud
        .positions
        .iter()
        .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
        .collect();

    let (_, radius) = cloud.robust_bounds(0.9);
    let inlier_dist = (radius as f64) * 0.01;

    let mut best_normal = None;
    let mut best_count = 0usize;
    for _ in 0..256 {
        let (a, b, c) = (pts[next(n)], pts[next(n)], pts[next(n)]);
        let nrm = cross(sub(b, a), sub(c, a));
        if crate::math::norm(nrm) < 1e-9 {
            continue;
        }
        let nrm = normalize(nrm);
        let d = dot(nrm, a);
        // Count inliers on a strided subset; the full set is overkill here.
        let stride = (n / 4000).max(1);
        let count = pts
            .iter()
            .step_by(stride)
            .filter(|p| (dot(nrm, **p) - d).abs() < inlier_dist)
            .count();
        if count > best_count {
            best_count = count;
            best_normal = Some(nrm);
        }
    }

    let nrm = best_normal?;
    let stride = (n / 4000).max(1);
    let sampled: Vec<&[f64; 3]> = pts.iter().step_by(stride).collect();
    if best_count * 5 < sampled.len() {
        // Fewer than 20% of points lie on the best plane: no credible ground.
        return None;
    }

    // Orient the normal so the scene's centre of mass sits on its positive
    // side, i.e. the ground is below everything else.
    let centroid = sampled.iter().fold([0.0; 3], |acc, p| {
        [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
    });
    let centroid = crate::math::scale(centroid, 1.0 / sampled.len() as f64);
    let plane_pt = sampled
        .iter()
        .min_by(|a, b| dot(nrm, ***a).partial_cmp(&dot(nrm, ***b)).unwrap())?;
    if dot(nrm, sub(centroid, **plane_pt)) < 0.0 {
        Some(crate::math::scale(nrm, -1.0))
    } else {
        Some(nrm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{dot, m3_mul_v, normalize, rodrigues};

    fn eval_sh(l: usize, coeffs: &[f64], d: [f64; 3]) -> f64 {
        sh_band(l, d).iter().zip(coeffs).map(|(y, c)| y * c).sum()
    }

    #[test]
    fn sh_band_rotation_matches_evaluating_the_rotated_direction() {
        let r = rodrigues([0.3, -0.7, 0.45]);
        for l in 1..=3 {
            let n = 2 * l + 1;
            let m = sh_band_rotation(l, r).expect("band rotation");
            // Arbitrary coefficients.
            let c: Vec<f64> = (0..n).map(|i| 0.7 - 0.3 * i as f64).collect();
            let c2: Vec<f64> = (0..n)
                .map(|row| (0..n).map(|col| m[(row, col)] * c[col]).sum())
                .collect();

            // For test directions unrelated to the probe set, the rotated
            // coefficients must reproduce the original field at R^T d.
            for d in [
                normalize([0.2, 0.9, -0.3]),
                normalize([-0.7, 0.1, 0.6]),
                normalize([0.0, 0.0, 1.0]),
                normalize([1.0, 1.0, 1.0]),
            ] {
                let lhs = eval_sh(l, &c2, d);
                let rhs = eval_sh(l, &c, m3_mul_v(crate::math::m3_transpose(r), d));
                assert!((lhs - rhs).abs() < 1e-9, "l={l} {lhs} vs {rhs}");
            }
        }
    }

    #[test]
    fn identity_rotation_leaves_sh_untouched() {
        let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        for l in 1..=3 {
            let m = sh_band_rotation(l, id).unwrap();
            for r in 0..(2 * l + 1) {
                for c in 0..(2 * l + 1) {
                    let want = if r == c { 1.0 } else { 0.0 };
                    assert!((m[(r, c)] - want).abs() < 1e-9, "l={l}");
                }
            }
        }
    }

    #[test]
    fn rotating_a_cloud_moves_positions_and_composes_orientations() {
        let mut cloud = SplatCloud {
            positions: vec![[1.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
            scales_log: vec![[0.0; 3]; 2],
            rot_wxyz: vec![[1.0, 0.0, 0.0, 0.0]; 2],
            opacity_logit: vec![0.0; 2],
            sh_dc: vec![[0.0; 3]; 2],
            sh_rest: vec![],
            rest_per_channel: 0,
        };
        // 90 degrees about +z takes +x to +y.
        let r = rodrigues([0.0, 0.0, std::f64::consts::FRAC_PI_2]);
        rotate_cloud(&mut cloud, r, [0.0, 0.0, 0.0]).unwrap();
        assert!(cloud.positions[0][0].abs() < 1e-6, "{:?}", cloud.positions[0]);
        assert!((cloud.positions[0][1] - 1.0).abs() < 1e-6);
        // The pivot point does not move.
        assert!(cloud.positions[1].iter().all(|v| v.abs() < 1e-6));
        // Identity splat orientation becomes the applied rotation.
        let m = super::quat_to_mat3(cloud.unit_rot(0));
        assert!((m[0][1] + 1.0).abs() < 1e-5, "{m:?}");
        assert!((m[1][0] - 1.0).abs() < 1e-5, "{m:?}");
    }

    #[test]
    fn rotation_about_a_pivot_keeps_the_pivot_fixed() {
        let pivot = [3.0, -1.0, 2.0];
        let mut cloud = SplatCloud {
            positions: vec![[3.0, -1.0, 2.0]],
            scales_log: vec![[0.0; 3]],
            rot_wxyz: vec![[1.0, 0.0, 0.0, 0.0]],
            opacity_logit: vec![0.0],
            sh_dc: vec![[0.0; 3]],
            sh_rest: vec![],
            rest_per_channel: 0,
        };
        rotate_cloud(&mut cloud, rodrigues([0.4, 0.2, -0.9]), pivot).unwrap();
        for k in 0..3 {
            assert!((cloud.positions[0][k] as f64 - pivot[k]).abs() < 1e-4);
        }
    }

    #[test]
    fn align_up_maps_the_source_direction_onto_the_axis() {
        let up = normalize([0.2, -0.9, 0.35]);
        let r = align_up(up, Axis::PosY);
        let mapped = m3_mul_v(r, up);
        assert!((mapped[1] - 1.0).abs() < 1e-9, "{mapped:?}");
    }

    #[test]
    fn rotation_between_handles_antiparallel_vectors() {
        let r = rotation_between([0.0, 1.0, 0.0], [0.0, -1.0, 0.0]);
        let m = m3_mul_v(r, [0.0, 1.0, 0.0]);
        assert!((m[1] + 1.0).abs() < 1e-6, "{m:?}");
        assert!((crate::math::m3_det(r) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn nearest_axis_snaps_to_the_dominant_component() {
        assert_eq!(Axis::nearest([0.1, -0.9, 0.2]), Axis::NegY);
        assert_eq!(Axis::nearest([0.8, 0.1, 0.2]), Axis::PosX);
        assert_eq!(Axis::nearest([0.0, 0.0, -1.0]), Axis::NegZ);
    }

    #[test]
    fn ground_normal_is_found_for_a_plane_with_scene_above_it() {
        let mut cloud = SplatCloud::default();
        // A dense floor at y = 0 plus a sparse column of points above it,
        // in a world where "up" is +y.
        for i in 0..40 {
            for j in 0..40 {
                cloud.positions.push([i as f32 * 0.1, 0.0, j as f32 * 0.1]);
            }
        }
        for i in 0..200 {
            cloud.positions.push([2.0, 0.2 + i as f32 * 0.005, 2.0]);
        }
        let n = cloud.len();
        cloud.scales_log = vec![[0.0; 3]; n];
        cloud.rot_wxyz = vec![[1.0, 0.0, 0.0, 0.0]; n];
        cloud.opacity_logit = vec![0.0; n];
        cloud.sh_dc = vec![[0.0; 3]; n];

        let up = estimate_ground_normal(&cloud).expect("ground plane");
        assert!(dot(up, [0.0, 1.0, 0.0]) > 0.99, "{up:?}");
    }

    #[test]
    fn ground_normal_is_none_when_there_is_no_plane() {
        let mut cloud = SplatCloud::default();
        let mut s: u64 = 12345;
        for _ in 0..500 {
            let mut r = || {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((s >> 33) as f32 / (1u32 << 31) as f32) - 0.5
            };
            cloud.positions.push([r(), r(), r()]);
        }
        let n = cloud.len();
        cloud.scales_log = vec![[0.0; 3]; n];
        cloud.rot_wxyz = vec![[1.0, 0.0, 0.0, 0.0]; n];
        cloud.opacity_logit = vec![0.0; n];
        cloud.sh_dc = vec![[0.0; 3]; n];
        assert!(estimate_ground_normal(&cloud).is_none());
    }

    #[test]
    fn full_cloud_rotation_preserves_rendered_colour_direction() {
        // A splat with degree-1 SH: after rotating the cloud, evaluating its
        // colour along the rotated view direction must match the original.
        let mut cloud = SplatCloud {
            positions: vec![[0.0; 3]],
            scales_log: vec![[0.0; 3]],
            rot_wxyz: vec![[1.0, 0.0, 0.0, 0.0]],
            opacity_logit: vec![0.0],
            sh_dc: vec![[0.0; 3]],
            sh_rest: vec![0.9, -0.4, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            rest_per_channel: 3,
        };
        let dir = normalize([0.3, 0.5, -0.8]);
        let before: f64 = eval_sh(
            1,
            &cloud.sh_rest[0..3].iter().map(|v| *v as f64).collect::<Vec<_>>(),
            dir,
        );

        let r = rodrigues([0.1, 0.8, -0.3]);
        rotate_cloud(&mut cloud, r, [0.0, 0.0, 0.0]).unwrap();

        let after: f64 = eval_sh(
            1,
            &cloud.sh_rest[0..3].iter().map(|v| *v as f64).collect::<Vec<_>>(),
            m3_mul_v(r, dir),
        );
        assert!((before - after).abs() < 1e-5, "{before} vs {after}");
    }
}
