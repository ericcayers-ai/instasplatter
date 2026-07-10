//! Shared Gaussian-splat data model.
//!
//! Everything downstream of training reads and writes through `SplatCloud`:
//! the orientation bake (ROADMAP-V2 1.3), the export formats (1.6), the
//! Mip-Splatting 3D smoothing filter (1.5) and the mesh extractor (Phase 4).

pub mod export;
pub mod mipfilter;
pub mod ply;
pub mod transform;

/// Zeroth-order spherical-harmonic basis constant.
pub const SH_C0: f32 = 0.282_094_79;

/// A trained 3D Gaussian splat set, stored exactly as it appears in a 3DGS
/// PLY: scales are natural-log, opacity is a logit and rotations are
/// unnormalized quaternions in `(w, x, y, z)` order.
#[derive(Debug, Clone, Default)]
pub struct SplatCloud {
    pub positions: Vec<[f32; 3]>,
    pub scales_log: Vec<[f32; 3]>,
    pub rot_wxyz: Vec<[f32; 4]>,
    pub opacity_logit: Vec<f32>,
    pub sh_dc: Vec<[f32; 3]>,
    /// Higher-order SH laid out exactly as the `f_rest_*` PLY properties:
    /// for each splat, `rest_per_channel` values for red, then green, then
    /// blue. Empty when `rest_per_channel == 0`.
    pub sh_rest: Vec<f32>,
    pub rest_per_channel: usize,
}

impl SplatCloud {
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// SH degree implied by the number of higher-order coefficients.
    pub fn sh_degree(&self) -> u32 {
        match self.rest_per_channel {
            0 => 0,
            3 => 1,
            8 => 2,
            _ => 3,
        }
    }

    /// Linear scale (the PLY stores `ln(scale)`).
    pub fn scale(&self, i: usize) -> [f32; 3] {
        let s = self.scales_log[i];
        [s[0].exp(), s[1].exp(), s[2].exp()]
    }

    /// Opacity in `0..1` (the PLY stores the logit).
    pub fn opacity(&self, i: usize) -> f32 {
        1.0 / (1.0 + (-self.opacity_logit[i]).exp())
    }

    /// Unit quaternion `(w, x, y, z)`; falls back to identity if degenerate.
    pub fn unit_rot(&self, i: usize) -> [f32; 4] {
        let q = self.rot_wxyz[i];
        let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        if n < 1e-12 {
            [1.0, 0.0, 0.0, 0.0]
        } else {
            [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
        }
    }

    /// Row-major 3x3 rotation matrix for splat `i`.
    pub fn rot_matrix(&self, i: usize) -> [[f32; 3]; 3] {
        quat_to_mat3(self.unit_rot(i))
    }

    /// Upper-triangle of the world-space 3D covariance
    /// `[c00, c01, c02, c11, c12, c22]`.
    pub fn covariance(&self, i: usize) -> [f32; 6] {
        let r = self.rot_matrix(i);
        let s = self.scale(i);
        // M = R * diag(s); Sigma = M * M^T
        let m = [
            [r[0][0] * s[0], r[0][1] * s[1], r[0][2] * s[2]],
            [r[1][0] * s[0], r[1][1] * s[1], r[1][2] * s[2]],
            [r[2][0] * s[0], r[2][1] * s[1], r[2][2] * s[2]],
        ];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        [
            dot(m[0], m[0]),
            dot(m[0], m[1]),
            dot(m[0], m[2]),
            dot(m[1], m[1]),
            dot(m[1], m[2]),
            dot(m[2], m[2]),
        ]
    }

    /// Centroid and the given quantile of the radial distance from it. The
    /// quantile keeps distant floaters from blowing up scene bounds.
    pub fn robust_bounds(&self, quantile: f32) -> ([f32; 3], f32) {
        if self.is_empty() {
            return ([0.0; 3], 1.0);
        }
        let n = self.len() as f64;
        let mut c = [0.0f64; 3];
        for p in &self.positions {
            for k in 0..3 {
                c[k] += p[k] as f64;
            }
        }
        let center = [(c[0] / n) as f32, (c[1] / n) as f32, (c[2] / n) as f32];
        let mut d: Vec<f32> = self
            .positions
            .iter()
            .map(|p| {
                let v = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
                (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
            })
            .collect();
        d.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((d.len() - 1) as f32 * quantile.clamp(0.0, 1.0)) as usize;
        (center, d[idx].max(1e-4))
    }
}

/// Row-major 3x3 from a unit quaternion `(w, x, y, z)`.
pub fn quat_to_mat3(q: [f32; 4]) -> [[f32; 3]; 3] {
    let (w, x, y, z) = (q[0], q[1], q[2], q[3]);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
        ],
        [
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

/// Unit quaternion `(w, x, y, z)` from a row-major 3x3 rotation matrix.
/// Uses the largest-diagonal branch so it stays stable for any rotation.
pub fn mat3_to_quat(m: [[f32; 3]; 3]) -> [f32; 4] {
    let trace = m[0][0] + m[1][1] + m[2][2];
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [
            0.25 * s,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [
            (m[2][1] - m[1][2]) / s,
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [
            (m[0][2] - m[2][0]) / s,
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [
            (m[1][0] - m[0][1]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
        ]
    }
}

pub fn mat3_mul(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut o = [[0.0f32; 3]; 3];
    for (r, row) in o.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    o
}

pub fn mat3_mul_vec(a: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        a[0][0] * v[0] + a[0][1] * v[1] + a[0][2] * v[2],
        a[1][0] * v[0] + a[1][1] * v[1] + a[1][2] * v[2],
        a[2][0] * v[0] + a[2][1] * v[1] + a[2][2] * v[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn quat_matrix_roundtrip() {
        // A handful of rotations including ones that hit each mat3_to_quat branch.
        let quats = [
            [1.0, 0.0, 0.0, 0.0f32],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.7071068, 0.7071068, 0.0, 0.0],
            [0.5, 0.5, 0.5, 0.5],
            [0.183, -0.365, 0.548, 0.730],
        ];
        for q in quats {
            let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
            let q = [q[0] / n, q[1] / n, q[2] / n, q[3] / n];
            let m = quat_to_mat3(q);
            let q2 = mat3_to_quat(m);
            // q and -q are the same rotation; compare via the matrix.
            let m2 = quat_to_mat3(q2);
            for r in 0..3 {
                for c in 0..3 {
                    assert!(approx(m[r][c], m2[r][c], 1e-5), "{m:?} vs {m2:?}");
                }
            }
        }
    }

    #[test]
    fn rotation_matrices_are_orthonormal() {
        let q = [0.183, -0.365, 0.548, 0.730f32];
        let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        let m = quat_to_mat3([q[0] / n, q[1] / n, q[2] / n, q[3] / n]);
        for r in 0..3 {
            let len: f32 = (0..3).map(|c| m[r][c] * m[r][c]).sum::<f32>().sqrt();
            assert!(approx(len, 1.0, 1e-5));
        }
        let d: f32 = (0..3).map(|c| m[0][c] * m[1][c]).sum();
        assert!(approx(d, 0.0, 1e-5));
    }

    #[test]
    fn covariance_of_axis_aligned_splat() {
        let cloud = SplatCloud {
            positions: vec![[0.0; 3]],
            scales_log: vec![[0.0, 1.0f32.ln(), 2.0f32.ln()]],
            rot_wxyz: vec![[1.0, 0.0, 0.0, 0.0]],
            opacity_logit: vec![0.0],
            sh_dc: vec![[0.0; 3]],
            sh_rest: vec![],
            rest_per_channel: 0,
        };
        // Identity rotation => Sigma = diag(s^2) = diag(1, 1, 4).
        let c = cloud.covariance(0);
        assert!(approx(c[0], 1.0, 1e-5));
        assert!(approx(c[3], 1.0, 1e-5));
        assert!(approx(c[5], 4.0, 1e-5));
        assert!(approx(c[1], 0.0, 1e-6));
        assert!(approx(c[2], 0.0, 1e-6));
        assert!(approx(c[4], 0.0, 1e-6));
    }
}
