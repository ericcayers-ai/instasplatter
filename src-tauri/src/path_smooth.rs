//! Shared camera-path smoothing: Catmull-Rom → cubic Bézier + quaternion slerp.
//!
//! Used for visualization / flythrough / path export. Does **not** rewrite
//! SfM or live-tracking solver poses — call sites resample for display only.

use crate::math::{add, norm, scale, sub, V3};

/// Unit quaternion `(w, x, y, z)`.
pub type Quat = [f64; 4];

#[derive(Debug, Clone, Copy)]
pub struct PathPose {
    pub position: V3,
    pub orientation: Quat,
}

pub fn quat_identity() -> Quat {
    [1.0, 0.0, 0.0, 0.0]
}

pub fn quat_dot(a: Quat, b: Quat) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

pub fn quat_normalize(q: Quat) -> Quat {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n < 1e-12 {
        quat_identity()
    } else {
        [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
    }
}

/// Shortest-arc spherical linear interpolation.
pub fn quat_slerp(a: Quat, b: Quat, t: f64) -> Quat {
    let qa = quat_normalize(a);
    let mut qb = quat_normalize(b);
    let mut d = quat_dot(qa, qb);
    if d < 0.0 {
        qb = [-qb[0], -qb[1], -qb[2], -qb[3]];
        d = -d;
    }
    if d > 0.9995 {
        return quat_normalize([
            qa[0] + t * (qb[0] - qa[0]),
            qa[1] + t * (qb[1] - qa[1]),
            qa[2] + t * (qb[2] - qa[2]),
            qa[3] + t * (qb[3] - qa[3]),
        ]);
    }
    let th = d.clamp(-1.0, 1.0).acos();
    let s = th.sin();
    let w0 = ((1.0 - t) * th).sin() / s;
    let w1 = (t * th).sin() / s;
    [
        w0 * qa[0] + w1 * qb[0],
        w0 * qa[1] + w1 * qb[1],
        w0 * qa[2] + w1 * qb[2],
        w0 * qa[3] + w1 * qb[3],
    ]
}

pub fn eval_cubic_bezier(p0: V3, p1: V3, p2: V3, p3: V3, t: f64) -> V3 {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    let uuu = uu * u;
    let ttt = tt * t;
    add(
        add(scale(p0, uuu), scale(p1, 3.0 * uu * t)),
        add(scale(p2, 3.0 * u * tt), scale(p3, ttt)),
    )
}

/// Uniform Catmull-Rom segment P1→P2 as cubic Bézier controls.
pub fn catmull_rom_to_bezier(p0: V3, p1: V3, p2: V3, p3: V3) -> (V3, V3, V3, V3) {
    let b1 = [
        p1[0] + (p2[0] - p0[0]) / 6.0,
        p1[1] + (p2[1] - p0[1]) / 6.0,
        p1[2] + (p2[2] - p0[2]) / 6.0,
    ];
    let b2 = [
        p2[0] - (p3[0] - p1[0]) / 6.0,
        p2[1] - (p3[1] - p1[1]) / 6.0,
        p2[2] - (p3[2] - p1[2]) / 6.0,
    ];
    (p1, b1, b2, p2)
}

/// Resample a polyline through Catmull-Rom → Bézier. Endpoints preserved.
pub fn smooth_polyline(points: &[V3], samples_per_segment: usize) -> Vec<V3> {
    let n = points.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![points[0]];
    }
    if n == 2 {
        return vec![points[0], points[1]];
    }
    let samples = samples_per_segment.max(2);
    let mut out = Vec::with_capacity((n - 1) * (samples - 1) + 1);
    for i in 0..n - 1 {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(n - 1)];
        let (b0, b1, b2, b3) = catmull_rom_to_bezier(p0, p1, p2, p3);
        let last = i == n - 2;
        let count = if last { samples } else { samples - 1 };
        for s in 0..count {
            let t = s as f64 / (samples - 1) as f64;
            out.push(eval_cubic_bezier(b0, b1, b2, b3, t));
        }
    }
    out
}

fn path_tangent(points: &[V3], i: usize) -> V3 {
    if points.len() < 2 {
        return [0.0, 0.0, 1.0];
    }
    let a = points[i.saturating_sub(1)];
    let b = points[(i + 1).min(points.len() - 1)];
    let d = sub(b, a);
    let n = norm(d);
    if n < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        scale(d, 1.0 / n)
    }
}

fn quat_look_rotation(forward: V3, world_up: V3) -> Quat {
    let f_n = norm(forward);
    if f_n < 1e-12 {
        return quat_identity();
    }
    let f = scale(forward, 1.0 / f_n);
    let up_n = norm(world_up);
    let mut up = if up_n < 1e-12 {
        [0.0, -1.0, 0.0]
    } else {
        scale(world_up, 1.0 / up_n)
    };
    let mut right = [
        up[1] * f[2] - up[2] * f[1],
        up[2] * f[0] - up[0] * f[2],
        up[0] * f[1] - up[1] * f[0],
    ];
    let rn = norm(right);
    if rn < 1e-8 {
        let seed = if f[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        right = [
            seed[1] * f[2] - seed[2] * f[1],
            seed[2] * f[0] - seed[0] * f[2],
            seed[0] * f[1] - seed[1] * f[0],
        ];
        let rn2 = norm(right);
        right = scale(right, 1.0 / rn2.max(1e-12));
        up = [
            f[1] * right[2] - f[2] * right[1],
            f[2] * right[0] - f[0] * right[2],
            f[0] * right[1] - f[1] * right[0],
        ];
    } else {
        right = scale(right, 1.0 / rn);
        up = [
            f[1] * right[2] - f[2] * right[1],
            f[2] * right[0] - f[0] * right[2],
            f[0] * right[1] - f[1] * right[0],
        ];
    }
    // Columns = right, up, forward → quaternion via matrix trace method.
    let m = [
        [right[0], up[0], f[0]],
        [right[1], up[1], f[1]],
        [right[2], up[2], f[2]],
    ];
    quat_from_mat3(m)
}

fn quat_from_mat3(m: [[f64; 3]; 3]) -> Quat {
    let tr = m[0][0] + m[1][1] + m[2][2];
    if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        return quat_normalize([
            0.25 * s,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        ]);
    }
    if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        return quat_normalize([
            (m[2][1] - m[1][2]) / s,
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
        ]);
    }
    if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        return quat_normalize([
            (m[0][2] - m[2][0]) / s,
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
        ]);
    }
    let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
    quat_normalize([
        (m[1][0] - m[0][1]) / s,
        (m[0][2] + m[2][0]) / s,
        (m[1][2] + m[2][1]) / s,
        0.25 * s,
    ])
}

/// Smooth positions (Bézier) and orientations (slerp). Missing orients use tangents.
pub fn smooth_camera_path(
    positions: &[V3],
    orientations: Option<&[Quat]>,
    samples_per_segment: usize,
) -> Vec<PathPose> {
    if positions.is_empty() {
        return Vec::new();
    }
    if positions.len() == 1 {
        let o = orientations
            .and_then(|o| o.first().copied())
            .unwrap_or_else(quat_identity);
        return vec![PathPose {
            position: positions[0],
            orientation: quat_normalize(o),
        }];
    }
    let smoothed = smooth_polyline(positions, samples_per_segment);
    let mut filled: Vec<Quat> = (0..positions.len())
        .map(|i| {
            orientations
                .and_then(|o| o.get(i).copied())
                .unwrap_or_else(|| quat_look_rotation(path_tangent(positions, i), [0.0, -1.0, 0.0]))
        })
        .collect();
    for i in 1..filled.len() {
        if quat_dot(filled[i - 1], filled[i]) < 0.0 {
            filled[i] = [-filled[i][0], -filled[i][1], -filled[i][2], -filled[i][3]];
        }
    }

    let samples = samples_per_segment.max(2);
    let segs = positions.len() - 1;
    let mut out = Vec::with_capacity(smoothed.len());
    let mut idx = 0usize;
    for i in 0..segs {
        let last = i == segs - 1;
        let count = if last { samples } else { samples - 1 };
        for s in 0..count {
            let t = s as f64 / (samples - 1) as f64;
            let pos = smoothed.get(idx).copied().unwrap_or(positions[i]);
            idx += 1;
            out.push(PathPose {
                position: pos,
                orientation: quat_slerp(filled[i], filled[i + 1], t),
            });
        }
    }
    out
}

/// Flatten positions only (for path polyline / export).
pub fn smooth_path_positions(points: &[[f32; 3]], samples_per_segment: usize) -> Vec<[f32; 3]> {
    let pts: Vec<V3> = points
        .iter()
        .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
        .collect();
    smooth_polyline(&pts, samples_per_segment)
        .into_iter()
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_polyline_preserves_endpoints() {
        let pts = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [3.0, 1.0, 1.0],
        ];
        let s = smooth_polyline(&pts, 6);
        assert!(s.len() >= pts.len());
        assert!((s[0][0] - 0.0).abs() < 1e-9);
        let last = s.last().unwrap();
        assert!((last[0] - 3.0).abs() < 1e-9);
        assert!((last[2] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn slerp_stays_unit() {
        let a = quat_identity();
        let b = quat_look_rotation([1.0, 0.0, 0.0], [0.0, -1.0, 0.0]);
        let m = quat_slerp(a, b, 0.5);
        let n = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2] + m[3] * m[3]).sqrt();
        assert!((n - 1.0).abs() < 1e-6, "{n}");
    }

    #[test]
    fn smooth_camera_path_fills_orientations() {
        let pts = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 1.0]];
        let out = smooth_camera_path(&pts, None, 4);
        assert!(out.len() >= pts.len());
        for p in &out {
            let n = (p.orientation[0].powi(2)
                + p.orientation[1].powi(2)
                + p.orientation[2].powi(2)
                + p.orientation[3].powi(2))
            .sqrt();
            assert!((n - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn f32_path_helper_roundtrips_length() {
        let pts = [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 1.0, 0.0]];
        let s = smooth_path_positions(&pts, 5);
        assert!(s.len() >= pts.len());
    }
}
