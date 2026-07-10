//! Local bundle adjustment over a sliding window of keyframes.
//!
//! The normal equations of a bundle problem are enormous but sparse: no two
//! 3D points interact directly, so the point block is block-diagonal and can
//! be eliminated exactly. That is the Schur complement, and it reduces a
//! system of `6C + 3P` unknowns to one of `6C`. With a window of a handful of
//! keyframes and a few thousand points, the reduced system is small enough to
//! solve densely every frame, which is what keeps pose refinement running
//! alongside training rather than blocking it.
//!
//! Gauge freedom is removed by holding the oldest keyframes in the window
//! fixed. They anchor both the coordinate frame and the scale.

use super::geometry::{apply_se3_increment, Pose};
use crate::math::{m3_mul_v, Mat};

/// One image measurement of one point.
#[derive(Debug, Clone, Copy)]
pub struct Observation {
    pub cam: usize,
    pub point: usize,
    /// Calibrated (normalized) image coordinates.
    pub obs: [f64; 2],
}

#[derive(Debug, Clone, Copy)]
pub struct BaOptions {
    pub iterations: usize,
    /// Huber threshold on the residual norm, in calibrated units.
    pub huber: f64,
    /// Number of leading poses held fixed. Must be at least 1.
    pub fixed_cams: usize,
}

impl Default for BaOptions {
    fn default() -> BaOptions {
        BaOptions {
            iterations: 8,
            huber: 2.0 / 800.0,
            fixed_cams: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BaReport {
    pub final_cost: f64,
    pub iterations_run: usize,
    /// Root-mean-square reprojection residual, in calibrated units.
    pub rms: f64,
}

fn huber_cost(r2: f64, huber: f64) -> f64 {
    let r = r2.sqrt();
    if r > huber {
        huber * (2.0 * r - huber)
    } else {
        r2
    }
}

/// Total robust cost and RMS residual.
fn evaluate(poses: &[Pose], points: &[[f64; 3]], obs: &[Observation], huber: f64) -> (f64, f64) {
    let mut cost = 0.0;
    let mut sq = 0.0;
    let mut n = 0usize;
    for o in obs {
        let c = poses[o.cam].apply(points[o.point]);
        if c[2] <= 1e-9 {
            // A point behind the camera is a real error; charge it the cap.
            cost += huber * huber;
            continue;
        }
        let du = c[0] / c[2] - o.obs[0];
        let dv = c[1] / c[2] - o.obs[1];
        let r2 = du * du + dv * dv;
        cost += huber_cost(r2, huber);
        sq += r2;
        n += 1;
    }
    (cost, if n > 0 { (sq / n as f64).sqrt() } else { 0.0 })
}

/// Invert a symmetric positive-definite 3x3, with a fallback for singular
/// blocks (a point seen by only one camera).
fn invert3(m: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-18 {
        return None;
    }
    let inv_det = 1.0 / det;
    let mut o = [[0.0; 3]; 3];
    o[0][0] = (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det;
    o[0][1] = (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det;
    o[0][2] = (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det;
    o[1][0] = (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det;
    o[1][1] = (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det;
    o[1][2] = (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det;
    o[2][0] = (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det;
    o[2][1] = (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det;
    o[2][2] = (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det;
    Some(o)
}

/// Refine `poses[fixed_cams..]` and every point against the observations.
///
/// Returns a report; `poses` and `points` are updated in place only when the
/// cost actually improves, so a failed adjustment is a no-op rather than a
/// corruption.
pub fn bundle_adjust(
    poses: &mut [Pose],
    points: &mut [[f64; 3]],
    obs: &[Observation],
    opts: BaOptions,
) -> BaReport {
    let n_cams = poses.len();
    let n_points = points.len();
    let fixed = opts.fixed_cams.max(1).min(n_cams);
    let free = n_cams.saturating_sub(fixed);

    let (initial_cost, initial_rms) = evaluate(poses, points, obs, opts.huber);
    let mut report = BaReport {
        final_cost: initial_cost,
        rms: initial_rms,
        iterations_run: 0,
    };
    if free == 0 && n_points == 0 {
        return report;
    }
    if obs.is_empty() {
        return report;
    }

    // Which observations touch each point, for the Schur sums.
    let mut per_point: Vec<Vec<usize>> = vec![Vec::new(); n_points];
    for (k, o) in obs.iter().enumerate() {
        per_point[o.point].push(k);
    }

    let mut lambda = 1e-4;
    let dim = 6 * free;

    for _ in 0..opts.iterations {
        // Accumulators.
        let mut u = vec![[[0.0f64; 6]; 6]; free.max(1)];
        let mut rc = vec![[0.0f64; 6]; free.max(1)];
        let mut v = vec![[[0.0f64; 3]; 3]; n_points];
        let mut rp = vec![[0.0f64; 3]; n_points];
        // W blocks, keyed by observation index (cam, point) pairs are unique
        // enough that per-observation storage is simplest and exact.
        let mut w: Vec<Option<[[f64; 3]; 6]>> = vec![None; obs.len()];

        for (k, o) in obs.iter().enumerate() {
            let pose = &poses[o.cam];
            let c = pose.apply(points[o.point]);
            if c[2] <= 1e-9 {
                continue;
            }
            let inv_z = 1.0 / c[2];
            let res = [c[0] * inv_z - o.obs[0], c[1] * inv_z - o.obs[1]];
            let r2 = res[0] * res[0] + res[1] * res[1];
            let r = r2.sqrt();
            let weight = if r > opts.huber { opts.huber / r } else { 1.0 };

            // d(projection) / d(camera point)
            let dp = [
                [inv_z, 0.0, -c[0] * inv_z * inv_z],
                [0.0, inv_z, -c[1] * inv_z * inv_z],
            ];

            // Point jacobian: d(camera point)/d(world point) = R.
            let mut b = [[0.0f64; 3]; 2];
            for row in 0..2 {
                for col in 0..3 {
                    b[row][col] = (0..3).map(|d| dp[row][d] * pose.r[d][col]).sum();
                }
            }

            // Pose jacobian, only for free cameras.
            let free_idx = o.cam.checked_sub(fixed);
            let mut a = [[0.0f64; 6]; 2];
            if free_idx.is_some() {
                // d(camera point)/d(se3 left increment) = [I | -skew(c)]
                let dc: [[f64; 6]; 3] = [
                    [1.0, 0.0, 0.0, 0.0, c[2], -c[1]],
                    [0.0, 1.0, 0.0, -c[2], 0.0, c[0]],
                    [0.0, 0.0, 1.0, c[1], -c[0], 0.0],
                ];
                for row in 0..2 {
                    for col in 0..6 {
                        a[row][col] = (0..3).map(|d| dp[row][d] * dc[d][col]).sum();
                    }
                }
            }

            // V_j += B^T B, rp_j += B^T res
            for i in 0..3 {
                for j in 0..3 {
                    v[o.point][i][j] += weight * (b[0][i] * b[0][j] + b[1][i] * b[1][j]);
                }
                rp[o.point][i] += weight * (b[0][i] * res[0] + b[1][i] * res[1]);
            }

            if let Some(ci) = free_idx {
                for i in 0..6 {
                    for j in 0..6 {
                        u[ci][i][j] += weight * (a[0][i] * a[0][j] + a[1][i] * a[1][j]);
                    }
                    rc[ci][i] += weight * (a[0][i] * res[0] + a[1][i] * res[1]);
                }
                let mut wb = [[0.0f64; 3]; 6];
                for i in 0..6 {
                    for j in 0..3 {
                        wb[i][j] = weight * (a[0][i] * b[0][j] + a[1][i] * b[1][j]);
                    }
                }
                w[k] = Some(wb);
            }
        }

        // Damped point blocks, inverted once.
        let mut v_inv: Vec<Option<[[f64; 3]; 3]>> = Vec::with_capacity(n_points);
        for vj in v.iter() {
            let mut d = *vj;
            for i in 0..3 {
                d[i][i] += lambda * vj[i][i].max(1e-9);
            }
            v_inv.push(invert3(d));
        }

        // Reduced camera system S dc = -b.
        let mut s = Mat::zeros(dim.max(1), dim.max(1));
        let mut b_vec = vec![0.0f64; dim.max(1)];
        for ci in 0..free {
            for i in 0..6 {
                for j in 0..6 {
                    s[(6 * ci + i, 6 * ci + j)] = u[ci][i][j];
                }
                s[(6 * ci + i, 6 * ci + i)] += lambda * u[ci][i][i].max(1e-9);
                b_vec[6 * ci + i] = rc[ci][i];
            }
        }

        for p in 0..n_points {
            let vi = match v_inv[p] {
                Some(m) => m,
                None => continue,
            };
            // Observations of this point that touch a free camera.
            let touching: Vec<(usize, [[f64; 3]; 6])> = per_point[p]
                .iter()
                .filter_map(|&k| {
                    let wb = w[k]?;
                    let ci = obs[k].cam.checked_sub(fixed)?;
                    Some((ci, wb))
                })
                .collect();

            // b_i -= W_ij * Vinv_j * rp_j
            for (ci, wb) in &touching {
                let vr = m3_mul_v(vi, rp[p]);
                for i in 0..6 {
                    b_vec[6 * ci + i] -= (0..3).map(|d| wb[i][d] * vr[d]).sum::<f64>();
                }
            }
            // S_ik -= W_ij * Vinv_j * W_kj^T
            for (ci, wi) in &touching {
                // Y = W_ij * Vinv_j   (6x3)
                let mut y = [[0.0f64; 3]; 6];
                for i in 0..6 {
                    for j in 0..3 {
                        y[i][j] = (0..3).map(|d| wi[i][d] * vi[d][j]).sum();
                    }
                }
                for (ck, wk) in &touching {
                    for i in 0..6 {
                        for j in 0..6 {
                            let val: f64 = (0..3).map(|d| y[i][d] * wk[j][d]).sum();
                            s[(6 * ci + i, 6 * ck + j)] -= val;
                        }
                    }
                }
            }
        }

        // Solve for the camera increments.
        let dc = if free > 0 {
            match crate::math::solve(s, b_vec.iter().map(|v| -v).collect()) {
                Some(d) => d,
                None => {
                    lambda *= 8.0;
                    if lambda > 1e8 {
                        break;
                    }
                    continue;
                }
            }
        } else {
            Vec::new()
        };

        // Back-substitute the point increments.
        let mut dp_all = vec![[0.0f64; 3]; n_points];
        for p in 0..n_points {
            let vi = match v_inv[p] {
                Some(m) => m,
                None => continue,
            };
            let mut acc = rp[p];
            for &k in &per_point[p] {
                if let (Some(wb), Some(ci)) = (w[k], obs[k].cam.checked_sub(fixed)) {
                    for j in 0..3 {
                        acc[j] += (0..6).map(|i| wb[i][j] * dc[6 * ci + i]).sum::<f64>();
                    }
                }
            }
            let d = m3_mul_v(vi, acc);
            dp_all[p] = [-d[0], -d[1], -d[2]];
        }

        // Trial step.
        let mut trial_poses = poses.to_vec();
        for ci in 0..free {
            trial_poses[fixed + ci] = apply_se3_increment(&poses[fixed + ci], &dc[6 * ci..6 * ci + 6]);
        }
        let mut trial_points = points.to_vec();
        for p in 0..n_points {
            for k in 0..3 {
                trial_points[p][k] += dp_all[p][k];
            }
        }

        let (cost, rms) = evaluate(&trial_poses, &trial_points, obs, opts.huber);
        report.iterations_run += 1;
        if cost < report.final_cost {
            poses.copy_from_slice(&trial_poses);
            points.copy_from_slice(&trial_points);
            report.final_cost = cost;
            report.rms = rms;
            lambda = (lambda * 0.4).max(1e-10);
        } else {
            lambda *= 6.0;
            if lambda > 1e8 {
                break;
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{m3_mul, rodrigues};
    use crate::sfm::geometry::rotation_angle_between;

    struct Problem {
        poses: Vec<Pose>,
        points: Vec<[f64; 3]>,
        obs: Vec<Observation>,
    }

    /// Four cameras looking at a grid of points from slightly different spots.
    fn synthetic() -> Problem {
        let mut points = Vec::new();
        for i in 0..5 {
            for j in 0..5 {
                for k in 0..2 {
                    points.push([
                        -1.0 + i as f64 * 0.5,
                        -1.0 + j as f64 * 0.5,
                        5.0 + k as f64 * 1.5,
                    ]);
                }
            }
        }
        let poses: Vec<Pose> = (0..4)
            .map(|c| {
                let a = c as f64 * 0.06;
                Pose {
                    r: rodrigues([0.01 * c as f64, a, -0.005 * c as f64]),
                    t: [-0.4 * c as f64, 0.05 * c as f64, 0.02 * c as f64],
                }
            })
            .collect();

        let mut obs = Vec::new();
        for (ci, pose) in poses.iter().enumerate() {
            for (pi, p) in points.iter().enumerate() {
                let c = pose.apply(*p);
                if c[2] > 1e-6 {
                    obs.push(Observation {
                        cam: ci,
                        point: pi,
                        obs: [c[0] / c[2], c[1] / c[2]],
                    });
                }
            }
        }
        Problem { poses, points, obs }
    }

    #[test]
    fn invert3_matches_a_known_inverse() {
        let m = [[4.0, 1.0, 0.0], [1.0, 3.0, 1.0], [0.0, 1.0, 2.0]];
        let inv = invert3(m).unwrap();
        for r in 0..3 {
            for c in 0..3 {
                let v: f64 = (0..3).map(|k| m[r][k] * inv[k][c]).sum();
                let want = if r == c { 1.0 } else { 0.0 };
                assert!((v - want).abs() < 1e-12);
            }
        }
        assert!(invert3([[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [1.0, 1.0, 1.0]]).is_none());
    }

    #[test]
    fn a_perfect_reconstruction_has_zero_cost_and_is_left_alone() {
        let p = synthetic();
        let mut poses = p.poses.clone();
        let mut points = p.points.clone();
        let (initial, _) = evaluate(&poses, &points, &p.obs, BaOptions::default().huber);
        let report = bundle_adjust(&mut poses, &mut points, &p.obs, BaOptions::default());
        assert!(initial < 1e-18, "{initial}");
        assert!(report.rms < 1e-9);
        for (a, b) in poses.iter().zip(&p.poses) {
            assert!(rotation_angle_between(a.r, b.r) < 1e-9);
        }
    }

    #[test]
    fn bundle_adjustment_recovers_perturbed_poses_and_points() {
        let p = synthetic();
        let mut poses = p.poses.clone();
        let mut points = p.points.clone();

        // Perturb the free cameras (all but the first two) and every point.
        for c in 2..poses.len() {
            poses[c] = Pose {
                r: m3_mul(rodrigues([0.02, -0.015, 0.01]), poses[c].r),
                t: [poses[c].t[0] + 0.05, poses[c].t[1] - 0.03, poses[c].t[2] + 0.02],
            };
        }
        for (i, pt) in points.iter_mut().enumerate() {
            let s = ((i % 7) as f64 - 3.0) * 0.01;
            pt[0] += s;
            pt[1] -= s;
            pt[2] += s * 0.5;
        }

        let opts = BaOptions {
            iterations: 30,
            huber: 1.0, // effectively pure least squares on clean data
            fixed_cams: 2,
        };
        let (initial, _) = evaluate(&poses, &points, &p.obs, opts.huber);
        let report = bundle_adjust(&mut poses, &mut points, &p.obs, opts);

        assert!(report.final_cost < initial * 1e-6, "{initial} -> {report:?}");
        assert!(report.rms < 1e-6, "rms {}", report.rms);
        for c in 2..poses.len() {
            assert!(
                rotation_angle_between(poses[c].r, p.poses[c].r) < 1e-4,
                "camera {c} rotation not recovered"
            );
            for k in 0..3 {
                assert!(
                    (poses[c].t[k] - p.poses[c].t[k]).abs() < 1e-3,
                    "camera {c} translation {:?} vs {:?}",
                    poses[c].t,
                    p.poses[c].t
                );
            }
        }
    }

    #[test]
    fn fixed_cameras_never_move() {
        let p = synthetic();
        let mut poses = p.poses.clone();
        let mut points = p.points.clone();
        for pt in points.iter_mut() {
            pt[2] += 0.1;
        }
        let before: Vec<Pose> = poses[..2].to_vec();
        bundle_adjust(&mut poses, &mut points, &p.obs, BaOptions { iterations: 10, ..Default::default() });
        for (a, b) in poses[..2].iter().zip(&before) {
            assert_eq!(a.t, b.t);
            assert_eq!(a.r, b.r);
        }
    }

    #[test]
    fn the_huber_weight_limits_the_pull_of_an_outlier() {
        let p = synthetic();

        // One badly wrong measurement.
        let mut obs = p.obs.clone();
        obs[10].obs = [5.0, -4.0];

        let run = |huber: f64| {
            let mut poses = p.poses.clone();
            let mut points = p.points.clone();
            bundle_adjust(
                &mut poses,
                &mut points,
                &obs,
                BaOptions { iterations: 20, huber, fixed_cams: 2 },
            );
            // How far the outlier dragged the point it belongs to.
            let pi = obs[10].point;
            (0..3)
                .map(|k| (points[pi][k] - p.points[pi][k]).abs())
                .fold(0.0f64, f64::max)
        };

        let robust = run(1e-3);
        let least_squares = run(1e9);
        assert!(
            robust < least_squares * 0.5,
            "huber did not suppress the outlier: {robust} vs {least_squares}"
        );
    }

    #[test]
    fn an_empty_problem_is_a_no_op() {
        let mut poses = vec![Pose::identity()];
        let mut points: Vec<[f64; 3]> = vec![];
        let r = bundle_adjust(&mut poses, &mut points, &[], BaOptions::default());
        assert_eq!(r.iterations_run, 0);
        assert_eq!(r.final_cost, 0.0);
    }

    #[test]
    fn a_window_where_every_camera_is_fixed_still_refines_points() {
        let p = synthetic();
        let mut poses = p.poses.clone();
        let mut points = p.points.clone();
        for pt in points.iter_mut() {
            pt[0] += 0.05;
        }
        let opts = BaOptions { iterations: 20, huber: 1.0, fixed_cams: poses.len() };
        let (initial, _) = evaluate(&poses, &points, &p.obs, opts.huber);
        let r = bundle_adjust(&mut poses, &mut points, &p.obs, opts);
        assert!(r.final_cost < initial * 1e-6, "{initial} -> {r:?}");
        for (a, b) in points.iter().zip(&p.points) {
            for k in 0..3 {
                assert!((a[k] - b[k]).abs() < 1e-5);
            }
        }
    }
}
