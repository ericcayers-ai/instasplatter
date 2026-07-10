//! Two-view and single-view geometry: the essential matrix, triangulation and
//! pose from 3D-2D correspondences, each wrapped in RANSAC.
//!
//! Everything works in calibrated (normalized) image coordinates, so the
//! intrinsics appear exactly once, in `Intrinsics::normalize`.

use crate::math::{
    add, dot, lm_step, m3_det, m3_mul, m3_mul_v, m3_transpose, normalize, null_vector, rodrigues,
    scale, sub, svd3, Mat, M3, V3,
};
#[cfg(test)]
use crate::math::{norm, rodrigues_inv};

/// World-to-camera rigid transform: `x_cam = R * x_world + t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub r: M3,
    pub t: V3,
}

impl Pose {
    pub fn identity() -> Pose {
        Pose {
            r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            t: [0.0; 3],
        }
    }

    pub fn apply(&self, p: V3) -> V3 {
        add(m3_mul_v(self.r, p), self.t)
    }

    /// Camera centre in world space.
    pub fn center(&self) -> V3 {
        scale(m3_mul_v(m3_transpose(self.r), self.t), -1.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Intrinsics {
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
}

impl Intrinsics {
    /// Pixel to calibrated bearing `(x, y)` with implicit `z = 1`.
    pub fn normalize(&self, px: f64, py: f64) -> [f64; 2] {
        [(px - self.cx) / self.fx, (py - self.cy) / self.fy]
    }

    /// Camera-space point to pixels, or `None` when behind the camera. The
    /// engine works in calibrated coordinates; the tests project to check them.
    #[cfg(test)]
    pub fn project(&self, cam: V3) -> Option<[f64; 2]> {
        if cam[2] <= 1e-9 {
            return None;
        }
        Some([
            self.fx * cam[0] / cam[2] + self.cx,
            self.fy * cam[1] / cam[2] + self.cy,
        ])
    }

    /// Average focal, used to turn pixel thresholds into calibrated ones.
    pub fn mean_focal(&self) -> f64 {
        0.5 * (self.fx + self.fy)
    }
}

/// A small deterministic generator. SfM must be reproducible: the same input
/// has to yield the same reconstruction on every run and every machine.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
    /// `k` distinct indices below `n`, or `None` when `n < k`.
    pub fn sample(&mut self, n: usize, k: usize, out: &mut Vec<usize>) -> bool {
        if n < k {
            return false;
        }
        out.clear();
        let mut guard = 0;
        while out.len() < k {
            let v = self.below(n);
            if !out.contains(&v) {
                out.push(v);
            }
            guard += 1;
            if guard > 100 * k {
                return false;
            }
        }
        true
    }
}

// ---- Essential matrix ------------------------------------------------------

/// Hartley normalization: centre the points and scale them to a mean radius of
/// sqrt(2). Without it the eight-point system is badly conditioned.
fn hartley(pts: &[[f64; 2]]) -> ([[f64; 3]; 3], Vec<[f64; 2]>) {
    let n = pts.len() as f64;
    let cx = pts.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = pts.iter().map(|p| p[1]).sum::<f64>() / n;
    let mean_d = pts
        .iter()
        .map(|p| ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt())
        .sum::<f64>()
        / n;
    let s = if mean_d > 1e-12 { 2f64.sqrt() / mean_d } else { 1.0 };
    let t = [[s, 0.0, -s * cx], [0.0, s, -s * cy], [0.0, 0.0, 1.0]];
    let out = pts.iter().map(|p| [s * (p[0] - cx), s * (p[1] - cy)]).collect();
    (t, out)
}

/// Force the two non-zero singular values equal and the third to zero, which
/// is what makes a 3x3 matrix an essential matrix.
pub fn enforce_essential(e: M3) -> M3 {
    let (u, s, v) = svd3(e);
    let m = 0.5 * (s[0] + s[1]);
    let sd = [[m, 0.0, 0.0], [0.0, m, 0.0], [0.0, 0.0, 0.0]];
    m3_mul(m3_mul(u, sd), m3_transpose(v))
}

/// Eight-point essential matrix from calibrated correspondences.
/// `x2ᵀ E x1 == 0` for every inlier.
pub fn essential_eight_point(x1: &[[f64; 2]], x2: &[[f64; 2]]) -> Option<M3> {
    if x1.len() < 8 || x1.len() != x2.len() {
        return None;
    }
    let (t1, p1) = hartley(x1);
    let (t2, p2) = hartley(x2);

    let mut a = Mat::zeros(p1.len(), 9);
    for i in 0..p1.len() {
        let (u1, v1) = (p1[i][0], p1[i][1]);
        let (u2, v2) = (p2[i][0], p2[i][1]);
        let row = [u2 * u1, u2 * v1, u2, v2 * u1, v2 * v1, v2, u1, v1, 1.0];
        for (c, val) in row.iter().enumerate() {
            a[(i, c)] = *val;
        }
    }
    let e = null_vector(&a);
    let e_norm: M3 = [
        [e[0], e[1], e[2]],
        [e[3], e[4], e[5]],
        [e[6], e[7], e[8]],
    ];
    // Undo the normalizing transforms: E = T2ᵀ E' T1.
    let e_raw = m3_mul(m3_mul(m3_transpose(t2), e_norm), t1);
    let e = enforce_essential(e_raw);
    if e.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    Some(e)
}

/// First-order geometric distance of a correspondence to the epipolar
/// constraint, in calibrated units.
pub fn sampson_distance(e: M3, x1: [f64; 2], x2: [f64; 2]) -> f64 {
    let p1 = [x1[0], x1[1], 1.0];
    let p2 = [x2[0], x2[1], 1.0];
    let ex1 = m3_mul_v(e, p1);
    let etx2 = m3_mul_v(m3_transpose(e), p2);
    let num = dot(p2, ex1);
    let den = ex1[0] * ex1[0] + ex1[1] * ex1[1] + etx2[0] * etx2[0] + etx2[1] * etx2[1];
    if den < 1e-15 {
        return f64::INFINITY;
    }
    num * num / den
}

/// The four rigid transforms consistent with an essential matrix.
pub fn decompose_essential(e: M3) -> [Pose; 4] {
    let (mut u, _, mut v) = svd3(e);
    // Both factors must be proper rotations here; E's overall sign is free.
    if m3_det(u) < 0.0 {
        for row in u.iter_mut() {
            row[2] = -row[2];
        }
    }
    if m3_det(v) < 0.0 {
        for row in v.iter_mut() {
            row[2] = -row[2];
        }
    }
    let w: M3 = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let r1 = m3_mul(m3_mul(u, w), m3_transpose(v));
    let r2 = m3_mul(m3_mul(u, m3_transpose(w)), m3_transpose(v));
    let t: V3 = [u[0][2], u[1][2], u[2][2]];
    let tn = scale(t, -1.0);
    [
        Pose { r: r1, t },
        Pose { r: r1, t: tn },
        Pose { r: r2, t },
        Pose { r: r2, t: tn },
    ]
}

/// Triangulate one point by the linear DLT, from two calibrated views.
pub fn triangulate(p1: &Pose, p2: &Pose, x1: [f64; 2], x2: [f64; 2]) -> Option<V3> {
    let rows_of = |p: &Pose| -> [[f64; 4]; 3] {
        [
            [p.r[0][0], p.r[0][1], p.r[0][2], p.t[0]],
            [p.r[1][0], p.r[1][1], p.r[1][2], p.t[1]],
            [p.r[2][0], p.r[2][1], p.r[2][2], p.t[2]],
        ]
    };
    let m1 = rows_of(p1);
    let m2 = rows_of(p2);

    let mut a = Mat::zeros(4, 4);
    for c in 0..4 {
        a[(0, c)] = x1[0] * m1[2][c] - m1[0][c];
        a[(1, c)] = x1[1] * m1[2][c] - m1[1][c];
        a[(2, c)] = x2[0] * m2[2][c] - m2[0][c];
        a[(3, c)] = x2[1] * m2[2][c] - m2[1][c];
    }
    let x = null_vector(&a);
    if x[3].abs() < 1e-12 {
        return None; // point at infinity
    }
    let p = [x[0] / x[3], x[1] / x[3], x[2] / x[3]];
    p.iter().all(|v| v.is_finite()).then_some(p)
}

/// Angle at the point between the two viewing rays. Small parallax means the
/// depth is poorly constrained no matter how good the matches are.
pub fn parallax(p1: &Pose, p2: &Pose, point: V3) -> f64 {
    let a = normalize(sub(point, p1.center()));
    let b = normalize(sub(point, p2.center()));
    dot(a, b).clamp(-1.0, 1.0).acos()
}

/// Choose the pose whose triangulations put the most points in front of both
/// cameras, and return those points.
pub fn select_by_cheirality(
    candidates: &[Pose; 4],
    x1: &[[f64; 2]],
    x2: &[[f64; 2]],
) -> Option<(Pose, Vec<Option<V3>>)> {
    let first = Pose::identity();
    let mut best: Option<(usize, Pose, Vec<Option<V3>>)> = None;

    for cand in candidates {
        let mut points = Vec::with_capacity(x1.len());
        let mut good = 0usize;
        for i in 0..x1.len() {
            let p = triangulate(&first, cand, x1[i], x2[i]);
            let keep = p.filter(|p| {
                let z1 = first.apply(*p)[2];
                let z2 = cand.apply(*p)[2];
                z1 > 1e-6 && z2 > 1e-6
            });
            if keep.is_some() {
                good += 1;
            }
            points.push(keep);
        }
        if best.as_ref().map(|(g, _, _)| good > *g).unwrap_or(true) {
            best = Some((good, *cand, points));
        }
    }

    let (good, pose, points) = best?;
    (good > 0).then_some((pose, points))
}

/// Result of a robust two-view estimate.
pub struct TwoView {
    pub pose: Pose,
    pub inliers: Vec<usize>,
}

/// RANSAC over the eight-point algorithm. `threshold_px` is converted to
/// calibrated units with `focal`.
pub fn ransac_essential(
    x1: &[[f64; 2]],
    x2: &[[f64; 2]],
    focal: f64,
    threshold_px: f64,
    iterations: usize,
    rng: &mut Rng,
) -> Option<TwoView> {
    let n = x1.len();
    if n < 8 {
        return None;
    }
    // Sampson distance is squared, so square the calibrated threshold too.
    let thresh = (threshold_px / focal).powi(2);

    let mut best_inliers: Vec<usize> = Vec::new();
    let mut idx = Vec::with_capacity(8);
    for _ in 0..iterations {
        if !rng.sample(n, 8, &mut idx) {
            return None;
        }
        let s1: Vec<[f64; 2]> = idx.iter().map(|&i| x1[i]).collect();
        let s2: Vec<[f64; 2]> = idx.iter().map(|&i| x2[i]).collect();
        let e = match essential_eight_point(&s1, &s2) {
            Some(e) => e,
            None => continue,
        };
        let inliers: Vec<usize> = (0..n)
            .filter(|&i| sampson_distance(e, x1[i], x2[i]) < thresh)
            .collect();
        if inliers.len() > best_inliers.len() {
            best_inliers = inliers;
        }
    }

    if best_inliers.len() < 12 {
        return None;
    }

    // Refit on all inliers, then re-score.
    let s1: Vec<[f64; 2]> = best_inliers.iter().map(|&i| x1[i]).collect();
    let s2: Vec<[f64; 2]> = best_inliers.iter().map(|&i| x2[i]).collect();
    let e = essential_eight_point(&s1, &s2)?;
    let inliers: Vec<usize> = (0..n)
        .filter(|&i| sampson_distance(e, x1[i], x2[i]) < thresh)
        .collect();
    if inliers.len() < 12 {
        return None;
    }

    let in1: Vec<[f64; 2]> = inliers.iter().map(|&i| x1[i]).collect();
    let in2: Vec<[f64; 2]> = inliers.iter().map(|&i| x2[i]).collect();
    let (pose, _) = select_by_cheirality(&decompose_essential(e), &in1, &in2)?;
    Some(TwoView { pose, inliers })
}

// ---- Pose from 3D-2D -------------------------------------------------------

/// Linear DLT pose from at least six 3D-2D correspondences in calibrated
/// coordinates. The result is refined, not used directly.
pub fn pnp_dlt(points: &[V3], obs: &[[f64; 2]]) -> Option<Pose> {
    let n = points.len();
    if n < 6 || n != obs.len() {
        return None;
    }
    let mut a = Mat::zeros(2 * n, 12);
    for i in 0..n {
        let x = [points[i][0], points[i][1], points[i][2], 1.0];
        let (u, v) = (obs[i][0], obs[i][1]);
        for c in 0..4 {
            a[(2 * i, c)] = -x[c];
            a[(2 * i, 8 + c)] = u * x[c];
            a[(2 * i + 1, 4 + c)] = -x[c];
            a[(2 * i + 1, 8 + c)] = v * x[c];
        }
    }
    let p = null_vector(&a);
    let mut m: M3 = [
        [p[0], p[1], p[2]],
        [p[4], p[5], p[6]],
        [p[8], p[9], p[10]],
    ];
    let mut t: V3 = [p[3], p[7], p[11]];

    // The null vector's sign is free; choose the one that puts the first
    // point in front of the camera.
    let z = dot([m[2][0], m[2][1], m[2][2]], points[0]) + t[2];
    if z < 0.0 {
        for row in m.iter_mut() {
            for v in row.iter_mut() {
                *v = -*v;
            }
        }
        t = scale(t, -1.0);
    }

    // M is a rotation times a positive scale. Recover both.
    let (u, s, v) = svd3(m);
    let mean_s = (s[0] + s[1] + s[2]) / 3.0;
    if mean_s < 1e-12 {
        return None;
    }
    let mut r = m3_mul(u, m3_transpose(v));
    if m3_det(r) < 0.0 {
        // Flip the least significant axis rather than the whole matrix.
        let mut u2 = u;
        for row in u2.iter_mut() {
            row[2] = -row[2];
        }
        r = m3_mul(u2, m3_transpose(v));
    }
    let t = scale(t, 1.0 / mean_s);
    (r.iter().flatten().all(|v| v.is_finite()) && t.iter().all(|v| v.is_finite()))
        .then_some(Pose { r, t })
}

/// Squared reprojection error in calibrated units, or `None` behind the camera.
pub fn reprojection_error(pose: &Pose, point: V3, obs: [f64; 2]) -> Option<f64> {
    let c = pose.apply(point);
    if c[2] <= 1e-9 {
        return None;
    }
    let du = c[0] / c[2] - obs[0];
    let dv = c[1] / c[2] - obs[1];
    Some(du * du + dv * dv)
}

/// Apply the SE(3) left increment `delta = [v, w]`: `T <- exp(w, v) * T`.
///
/// This is the update the `[I | -skew(c)]` Jacobian linearizes, which is why
/// the translation is rotated before the offset is added. The rotation is
/// re-orthonormalized so drift cannot accumulate over many iterations.
pub fn apply_se3_increment(pose: &Pose, delta: &[f64]) -> Pose {
    let rot = rodrigues([delta[3], delta[4], delta[5]]);
    Pose {
        r: crate::math::orthonormalize(m3_mul(rot, pose.r)),
        t: add(m3_mul_v(rot, pose.t), [delta[0], delta[1], delta[2]]),
    }
}

/// Motion-only bundle adjustment: refine one pose against fixed 3D points,
/// with a Huber-style robust weight so a few surviving outliers cannot drag
/// the solution.
pub fn refine_pose(pose: &Pose, points: &[V3], obs: &[[f64; 2]], huber: f64) -> Pose {
    let mut current = *pose;
    let mut lambda = 1e-4;

    for _ in 0..12 {
        let mut jtj = Mat::zeros(6, 6);
        let mut jtr = vec![0.0; 6];
        let mut cost = 0.0;
        let mut used = 0usize;

        for i in 0..points.len() {
            let c = current.apply(points[i]);
            if c[2] <= 1e-9 {
                continue;
            }
            used += 1;
            let inv_z = 1.0 / c[2];
            let res = [c[0] * inv_z - obs[i][0], c[1] * inv_z - obs[i][1]];
            let r2 = res[0] * res[0] + res[1] * res[1];
            // Huber weight on the residual norm.
            let r = r2.sqrt();
            let w = if r > huber { huber / r } else { 1.0 };
            cost += w * r2;

            // d(projection)/d(camera point)
            let dp = [
                [inv_z, 0.0, -c[0] * inv_z * inv_z],
                [0.0, inv_z, -c[1] * inv_z * inv_z],
            ];
            // Left perturbation on SE(3): c' = exp(w) * c + v, so
            // d(camera point)/d(v, w) = [I | -skew(c)]. The update below must
            // rotate t to match, or the Jacobian describes a different step.
            let dc: [[f64; 6]; 3] = [
                [1.0, 0.0, 0.0, 0.0, c[2], -c[1]],
                [0.0, 1.0, 0.0, -c[2], 0.0, c[0]],
                [0.0, 0.0, 1.0, c[1], -c[0], 0.0],
            ];
            let mut j = [[0.0f64; 6]; 2];
            for row in 0..2 {
                for k in 0..6 {
                    j[row][k] = (0..3).map(|d| dp[row][d] * dc[d][k]).sum();
                }
            }
            for row in 0..2 {
                for a in 0..6 {
                    jtr[a] += w * j[row][a] * res[row];
                    for b in 0..6 {
                        jtj[(a, b)] += w * j[row][a] * j[row][b];
                    }
                }
            }
        }

        if used < 4 {
            return current;
        }

        let delta = match lm_step(&jtj, &jtr, lambda) {
            Some(d) => d,
            None => return current,
        };

        let candidate = apply_se3_increment(&current, &delta);

        let new_cost: f64 = points
            .iter()
            .zip(obs)
            .filter_map(|(p, o)| reprojection_error(&candidate, *p, *o))
            .map(|r2| {
                let r = r2.sqrt();
                if r > huber {
                    huber * (2.0 * r - huber)
                } else {
                    r2
                }
            })
            .sum();

        if new_cost < cost {
            current = candidate;
            lambda = (lambda * 0.5).max(1e-9);
        } else {
            lambda *= 4.0;
            if lambda > 1e6 {
                break;
            }
        }
    }
    current
}

/// RANSAC over `pnp_dlt`, returning the refined pose and its inliers.
pub fn ransac_pnp(
    points: &[V3],
    obs: &[[f64; 2]],
    focal: f64,
    threshold_px: f64,
    iterations: usize,
    rng: &mut Rng,
) -> Option<(Pose, Vec<usize>)> {
    let n = points.len();
    if n < 6 {
        return None;
    }
    let thresh = (threshold_px / focal).powi(2);

    let mut best: Vec<usize> = Vec::new();
    let mut idx = Vec::with_capacity(6);
    for _ in 0..iterations {
        if !rng.sample(n, 6, &mut idx) {
            return None;
        }
        let sp: Vec<V3> = idx.iter().map(|&i| points[i]).collect();
        let so: Vec<[f64; 2]> = idx.iter().map(|&i| obs[i]).collect();
        let pose = match pnp_dlt(&sp, &so) {
            Some(p) => p,
            None => continue,
        };
        let inliers: Vec<usize> = (0..n)
            .filter(|&i| reprojection_error(&pose, points[i], obs[i]).map(|e| e < thresh).unwrap_or(false))
            .collect();
        if inliers.len() > best.len() {
            best = inliers;
            // An overwhelming consensus will not improve; stop early.
            if best.len() * 10 > n * 9 {
                break;
            }
        }
    }

    if best.len() < 6 {
        return None;
    }

    let sp: Vec<V3> = best.iter().map(|&i| points[i]).collect();
    let so: Vec<[f64; 2]> = best.iter().map(|&i| obs[i]).collect();
    let seed = pnp_dlt(&sp, &so)?;
    let pose = refine_pose(&seed, &sp, &so, threshold_px / focal);

    let inliers: Vec<usize> = (0..n)
        .filter(|&i| reprojection_error(&pose, points[i], obs[i]).map(|e| e < thresh).unwrap_or(false))
        .collect();
    (inliers.len() >= 6).then_some((pose, inliers))
}

/// Angle between two rotations, in radians. Only the tests need this, but
/// every one of them that checks a recovered pose needs it.
#[cfg(test)]
pub fn rotation_angle_between(a: M3, b: M3) -> f64 {
    norm(rodrigues_inv(m3_mul(m3_transpose(a), b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intr() -> Intrinsics {
        Intrinsics {
            fx: 800.0,
            fy: 800.0,
            cx: 320.0,
            cy: 240.0,
        }
    }

    /// A repeatable cloud of points spread in front of the origin.
    fn scene(n: usize) -> Vec<V3> {
        let mut rng = Rng::new(7);
        (0..n)
            .map(|_| {
                let f = |lo: f64, hi: f64, r: &mut Rng| {
                    lo + (r.next_u64() % 10_000) as f64 / 10_000.0 * (hi - lo)
                };
                [
                    f(-2.0, 2.0, &mut rng),
                    f(-1.5, 1.5, &mut rng),
                    f(4.0, 9.0, &mut rng),
                ]
            })
            .collect()
    }

    fn project_all(pose: &Pose, pts: &[V3]) -> Vec<[f64; 2]> {
        pts.iter()
            .map(|p| {
                let c = pose.apply(*p);
                [c[0] / c[2], c[1] / c[2]]
            })
            .collect()
    }

    #[test]
    fn intrinsics_normalize_and_project_are_inverses() {
        let k = intr();
        let px = [400.0, 180.0];
        let n = k.normalize(px[0], px[1]);
        let back = k.project([n[0], n[1], 1.0]).unwrap();
        assert!((back[0] - px[0]).abs() < 1e-9 && (back[1] - px[1]).abs() < 1e-9);
        assert!(k.project([0.0, 0.0, -1.0]).is_none());
    }

    #[test]
    fn triangulation_recovers_known_points() {
        let pts = scene(30);
        let p1 = Pose::identity();
        let p2 = Pose {
            r: rodrigues([0.02, 0.15, -0.01]),
            t: [-1.0, 0.05, 0.1],
        };
        let x1 = project_all(&p1, &pts);
        let x2 = project_all(&p2, &pts);
        for i in 0..pts.len() {
            let p = triangulate(&p1, &p2, x1[i], x2[i]).unwrap();
            for k in 0..3 {
                assert!((p[k] - pts[i][k]).abs() < 1e-6, "{p:?} vs {:?}", pts[i]);
            }
        }
    }

    #[test]
    fn essential_matrix_satisfies_the_epipolar_constraint() {
        let pts = scene(20);
        let p1 = Pose::identity();
        let p2 = Pose {
            r: rodrigues([0.03, 0.2, 0.01]),
            t: [-1.2, 0.0, 0.15],
        };
        let x1 = project_all(&p1, &pts);
        let x2 = project_all(&p2, &pts);
        let e = essential_eight_point(&x1, &x2).unwrap();
        for i in 0..pts.len() {
            assert!(sampson_distance(e, x1[i], x2[i]) < 1e-14, "{i}");
        }
    }

    #[test]
    fn enforce_essential_zeroes_the_third_singular_value() {
        let m: M3 = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 10.0]];
        let e = enforce_essential(m);
        let (_, s, _) = svd3(e);
        // The bounds are relative because `svd3` recovers singular values as
        // square roots of the eigenvalues of `EᵀE`. A true zero comes back as
        // the root of a roundoff-sized eigenvalue, so it is only ever small
        // compared to `s[0]`, never small in absolute terms.
        assert!(s[2] < 1e-6 * s[0], "third singular value not zeroed: {s:?}");
        assert!((s[0] - s[1]).abs() < 1e-9 * s[0], "{s:?}");
    }

    #[test]
    fn two_view_pose_is_recovered_up_to_scale() {
        let pts = scene(60);
        let p1 = Pose::identity();
        let truth = Pose {
            r: rodrigues([0.02, 0.18, -0.03]),
            t: [-1.0, 0.1, 0.2],
        };
        let x1 = project_all(&p1, &pts);
        let x2 = project_all(&truth, &pts);

        let e = essential_eight_point(&x1, &x2).unwrap();
        let (pose, points) = select_by_cheirality(&decompose_essential(e), &x1, &x2).unwrap();

        // Rotation is recovered exactly; translation only up to scale.
        assert!(rotation_angle_between(pose.r, truth.r) < 1e-6);
        let a = normalize(pose.t);
        let b = normalize(truth.t);
        assert!(dot(a, b) > 0.999999, "{a:?} vs {b:?}");
        assert!(points.iter().filter(|p| p.is_some()).count() >= 55);
    }

    #[test]
    fn ransac_essential_ignores_gross_outliers() {
        let pts = scene(80);
        let p1 = Pose::identity();
        let truth = Pose {
            r: rodrigues([0.01, 0.16, 0.0]),
            t: [-1.0, 0.0, 0.1],
        };
        let x1 = project_all(&p1, &pts);
        let mut x2 = project_all(&truth, &pts);

        // Corrupt a quarter of the correspondences.
        let mut rng = Rng::new(3);
        let mut corrupted = Vec::new();
        for i in 0..20 {
            let k = i * 4;
            x2[k] = [
                (rng.below(1000) as f64 / 1000.0) - 0.5,
                (rng.below(1000) as f64 / 1000.0) - 0.5,
            ];
            corrupted.push(k);
        }

        let mut rng = Rng::new(11);
        let tv = ransac_essential(&x1, &x2, 800.0, 1.5, 500, &mut rng).unwrap();
        assert!(tv.inliers.len() >= 55, "only {} inliers", tv.inliers.len());
        // None of the corrupted correspondences should be trusted.
        for k in corrupted {
            assert!(!tv.inliers.contains(&k), "outlier {k} kept");
        }
        assert!(rotation_angle_between(tv.pose.r, truth.r) < 1e-3);
    }

    #[test]
    fn ransac_essential_needs_eight_points() {
        let mut rng = Rng::new(1);
        assert!(ransac_essential(&[[0.0, 0.0]; 5], &[[0.0, 0.0]; 5], 800.0, 1.0, 10, &mut rng).is_none());
    }

    #[test]
    fn pnp_dlt_recovers_a_known_pose() {
        let pts = scene(20);
        let truth = Pose {
            r: rodrigues([0.2, -0.35, 0.1]),
            t: [0.4, -0.2, 0.6],
        };
        let obs = project_all(&truth, &pts);
        let pose = pnp_dlt(&pts, &obs).unwrap();
        assert!(rotation_angle_between(pose.r, truth.r) < 1e-6, "rotation off");
        for k in 0..3 {
            assert!((pose.t[k] - truth.t[k]).abs() < 1e-6, "{:?}", pose.t);
        }
    }

    #[test]
    fn refine_pose_pulls_a_perturbed_estimate_back() {
        let pts = scene(40);
        let truth = Pose {
            r: rodrigues([0.1, 0.2, -0.05]),
            t: [0.3, 0.1, 0.2],
        };
        let obs = project_all(&truth, &pts);
        let start = Pose {
            r: m3_mul(rodrigues([0.05, -0.04, 0.03]), truth.r),
            t: add(truth.t, [0.08, -0.06, 0.05]),
        };
        assert!(rotation_angle_between(start.r, truth.r) > 0.05);

        let refined = refine_pose(&start, &pts, &obs, 0.01);
        assert!(rotation_angle_between(refined.r, truth.r) < 1e-5, "rotation not refined");
        for k in 0..3 {
            assert!((refined.t[k] - truth.t[k]).abs() < 1e-5, "{:?}", refined.t);
        }
    }

    #[test]
    fn ransac_pnp_survives_a_third_of_outliers() {
        let pts = scene(60);
        let truth = Pose {
            r: rodrigues([0.15, 0.25, -0.08]),
            t: [0.2, -0.3, 0.5],
        };
        let mut obs = project_all(&truth, &pts);
        let mut rng = Rng::new(5);
        let mut bad = Vec::new();
        for i in 0..20 {
            let k = i * 3;
            obs[k] = [
                (rng.below(1000) as f64 / 1000.0) - 0.5,
                (rng.below(1000) as f64 / 1000.0) - 0.5,
            ];
            bad.push(k);
        }

        let mut rng = Rng::new(23);
        let (pose, inliers) = ransac_pnp(&pts, &obs, 800.0, 2.0, 800, &mut rng).unwrap();
        assert!(inliers.len() >= 38, "only {} inliers", inliers.len());
        assert!(rotation_angle_between(pose.r, truth.r) < 1e-3);
        for k in bad {
            assert!(!inliers.contains(&k));
        }
    }

    #[test]
    fn parallax_is_small_for_a_distant_point_and_large_for_a_near_one() {
        let p1 = Pose::identity();
        let p2 = Pose {
            r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            t: [-1.0, 0.0, 0.0],
        };
        let near = parallax(&p1, &p2, [0.0, 0.0, 2.0]);
        let far = parallax(&p1, &p2, [0.0, 0.0, 500.0]);
        assert!(near > far);
        assert!(far < 0.01);
        assert!(near > 0.2);
    }

    #[test]
    fn pose_centre_maps_to_the_camera_origin() {
        let p = Pose { r: rodrigues([0.4, -0.1, 0.2]), t: [0.5, -1.0, 2.0] };
        let o = p.apply(p.center());
        assert!(o.iter().all(|v| v.abs() < 1e-12), "{o:?}");
    }

    #[test]
    fn rng_sampling_is_deterministic_and_distinct() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        let (mut ia, mut ib) = (Vec::new(), Vec::new());
        assert!(a.sample(50, 8, &mut ia));
        assert!(b.sample(50, 8, &mut ib));
        assert_eq!(ia, ib);
        assert_eq!(ia.len(), 8);
        for i in 0..8 {
            for j in (i + 1)..8 {
                assert_ne!(ia[i], ia[j]);
            }
        }
        assert!(!a.sample(3, 8, &mut ia));
    }
}

