//! Small dense linear algebra shared by SH rotation, pose solving and
//! bundle adjustment. Everything here is `f64` and sized for problems of a
//! few hundred unknowns at most.

/// Dense row-major matrix.
#[derive(Debug, Clone)]
pub struct Mat {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
}

impl Mat {
    pub fn zeros(rows: usize, cols: usize) -> Mat {
        Mat {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub fn identity(n: usize) -> Mat {
        let mut m = Mat::zeros(n, n);
        for i in 0..n {
            m[(i, i)] = 1.0;
        }
        m
    }
}

impl std::ops::Index<(usize, usize)> for Mat {
    type Output = f64;
    fn index(&self, (r, c): (usize, usize)) -> &f64 {
        &self.data[r * self.cols + c]
    }
}

impl std::ops::IndexMut<(usize, usize)> for Mat {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut f64 {
        &mut self.data[r * self.cols + c]
    }
}

/// Solve `A x = b` by Gaussian elimination with partial pivoting.
/// `a` is consumed. Returns `None` when `A` is singular to working precision.
pub fn solve(mut a: Mat, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = a.rows;
    debug_assert_eq!(a.cols, n);
    debug_assert_eq!(b.len(), n);

    for col in 0..n {
        let (pivot, mag) = (col..n).fold((col, 0.0f64), |(pi, pm), r| {
            let v = a[(r, col)].abs();
            if v > pm {
                (r, v)
            } else {
                (pi, pm)
            }
        });
        if mag < 1e-14 {
            return None;
        }
        if pivot != col {
            for c in 0..n {
                a.data.swap(pivot * n + c, col * n + c);
            }
            b.swap(pivot, col);
        }
        let d = a[(col, col)];
        for r in (col + 1)..n {
            let f = a[(r, col)] / d;
            if f == 0.0 {
                continue;
            }
            for c in col..n {
                a[(r, c)] -= f * a[(col, c)];
            }
            b[r] -= f * b[col];
        }
    }

    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for c in (r + 1)..n {
            s -= a[(r, c)] * x[c];
        }
        x[r] = s / a[(r, r)];
    }
    if x.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(x)
}

/// Solve `A X = B` for a square `A` and a matrix right-hand side.
pub fn solve_matrix(a: &Mat, b: &Mat) -> Option<Mat> {
    let n = a.rows;
    let mut out = Mat::zeros(n, b.cols);
    for c in 0..b.cols {
        let col: Vec<f64> = (0..n).map(|r| b[(r, c)]).collect();
        let x = solve(a.clone(), col)?;
        for (r, v) in x.into_iter().enumerate() {
            out[(r, c)] = v;
        }
    }
    Some(out)
}

pub fn transpose(a: &Mat) -> Mat {
    let mut o = Mat::zeros(a.cols, a.rows);
    for r in 0..a.rows {
        for c in 0..a.cols {
            o[(c, r)] = a[(r, c)];
        }
    }
    o
}

pub fn mat_mul(a: &Mat, b: &Mat) -> Mat {
    debug_assert_eq!(a.cols, b.rows);
    let mut o = Mat::zeros(a.rows, b.cols);
    for r in 0..a.rows {
        for c in 0..b.cols {
            o[(r, c)] = (0..a.cols).map(|k| a[(r, k)] * b[(k, c)]).sum();
        }
    }
    o
}

/// Least-squares solve of the overdetermined `A X = B` via the normal
/// equations. Prefer this over `solve_matrix` when `A` has more rows than
/// columns, or when a square `A` may be poorly conditioned.
pub fn solve_least_squares(a: &Mat, b: &Mat) -> Option<Mat> {
    let at = transpose(a);
    let ata = mat_mul(&at, a);
    let atb = mat_mul(&at, b);
    solve_matrix(&ata, &atb)
}

/// Solve the damped normal equations `(JᵀJ + λ·diag(JᵀJ)) δ = -Jᵀr`,
/// the Levenberg-Marquardt step. `jtj` and `jtr` are modified in place.
pub fn lm_step(jtj: &Mat, jtr: &[f64], lambda: f64) -> Option<Vec<f64>> {
    let n = jtj.rows;
    let mut a = jtj.clone();
    for i in 0..n {
        // Marquardt's scaling: damp proportional to the diagonal, with a
        // floor so zero-curvature directions still move.
        a[(i, i)] += lambda * jtj[(i, i)].max(1e-9);
    }
    let b: Vec<f64> = jtr.iter().map(|v| -v).collect();
    solve(a, b)
}

/// Symmetric eigen-decomposition of any size, by cyclic Jacobi rotations.
/// Returns `(eigenvalues ascending, eigenvectors as columns)`.
///
/// Used to take null spaces of the homogeneous systems that show up in the
/// eight-point algorithm, triangulation and DLT pose: the null vector is the
/// eigenvector of `AᵀA` with the smallest eigenvalue.
pub fn jacobi_eigen(a: &Mat) -> (Vec<f64>, Mat) {
    let n = a.rows;
    debug_assert_eq!(a.cols, n);
    let mut m = a.clone();
    let mut v = Mat::identity(n);

    for _ in 0..(64 * n) {
        // Largest off-diagonal magnitude.
        let (mut p, mut q, mut best) = (0usize, 0usize, 0.0f64);
        for i in 0..n {
            for j in (i + 1)..n {
                let t = m[(i, j)].abs();
                if t > best {
                    best = t;
                    p = i;
                    q = j;
                }
            }
        }
        if best < 1e-13 {
            break;
        }

        let theta = 0.5 * (m[(q, q)] - m[(p, p)]) / m[(p, q)];
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;

        // M <- Jᵀ M J, applied as two rank-2 updates on rows then columns.
        for k in 0..n {
            let mkp = m[(k, p)];
            let mkq = m[(k, q)];
            m[(k, p)] = c * mkp - s * mkq;
            m[(k, q)] = s * mkp + c * mkq;
        }
        for k in 0..n {
            let mpk = m[(p, k)];
            let mqk = m[(q, k)];
            m[(p, k)] = c * mpk - s * mqk;
            m[(q, k)] = s * mpk + c * mqk;
        }
        m[(p, q)] = 0.0;
        m[(q, p)] = 0.0;

        for k in 0..n {
            let vkp = v[(k, p)];
            let vkq = v[(k, q)];
            v[(k, p)] = c * vkp - s * vkq;
            v[(k, q)] = s * vkp + c * vkq;
        }
    }

    let mut idx: Vec<usize> = (0..n).collect();
    let vals: Vec<f64> = (0..n).map(|i| m[(i, i)]).collect();
    idx.sort_by(|&i, &j| vals[i].partial_cmp(&vals[j]).unwrap_or(std::cmp::Ordering::Equal));

    let sorted_vals: Vec<f64> = idx.iter().map(|&i| vals[i]).collect();
    let mut vecs = Mat::zeros(n, n);
    for (c, &src) in idx.iter().enumerate() {
        for r in 0..n {
            vecs[(r, c)] = v[(r, src)];
        }
    }
    (sorted_vals, vecs)
}

/// Unit null vector of `A`: the right singular vector of the smallest
/// singular value, found as the smallest eigenvector of `AᵀA`.
pub fn null_vector(a: &Mat) -> Vec<f64> {
    let ata = mat_mul(&transpose(a), a);
    let (_, vecs) = jacobi_eigen(&ata);
    (0..a.cols).map(|r| vecs[(r, 0)]).collect()
}

/// SVD of a 3x3 matrix: `m == u * diag(s) * vᵀ`, with `s` non-negative and
/// descending. `v` is always a proper rotation; `u` is orthogonal with
/// `det(u) == sign(det(m))`, which is the only way a non-negative `s` can
/// represent a matrix that flips orientation.
pub fn svd3(m: M3) -> (M3, [f64; 3], M3) {
    // V and S² are the eigenpairs of MᵀM.
    let mt = m3_transpose(m);
    let mtm = m3_mul(mt, m);
    let (vals, vecs) = eigen_sym3(mtm);

    // eigen_sym3 sorts ascending; SVD convention is descending.
    let order = [2usize, 1, 0];
    let mut v = [[0.0; 3]; 3];
    let mut s = [0.0; 3];
    for (c, &src) in order.iter().enumerate() {
        s[c] = vals[src].max(0.0).sqrt();
        for r in 0..3 {
            v[r][c] = vecs[r][src];
        }
    }
    // Keep V a right-handed frame.
    if m3_det(v) < 0.0 {
        for row in v.iter_mut() {
            row[2] = -row[2];
        }
    }

    // u_i = M v_i / s_i, completing rank-deficient columns by orthogonality.
    let mut u = [[0.0; 3]; 3];
    let col = |a: M3, c: usize| -> V3 { [a[0][c], a[1][c], a[2][c]] };
    let set_col = |a: &mut M3, c: usize, val: V3| {
        for r in 0..3 {
            a[r][c] = val[r];
        }
    };

    // Significance is relative: a singular value of 1e-9 is a hard zero for a
    // matrix whose largest is 4, and `M v / s` would then be pure roundoff.
    // `s` is sorted descending, so `s[0]` is the scale to compare against.
    let tol = (1e-9 * s[0]).max(1e-14);
    let mut valid = [false; 3];
    for c in 0..3 {
        if s[c] <= tol {
            continue;
        }
        let mut uc = scale(m3_mul_v(m, col(v, c)), 1.0 / s[c]);
        // Re-orthogonalize against the columns already accepted. In exact
        // arithmetic this changes nothing; numerically it is what stops a
        // rank-deficient direction from surviving as a near-zero column.
        for k in 0..c {
            if valid[k] {
                let uk = col(u, k);
                uc = sub(uc, scale(uk, dot(uk, uc)));
            }
        }
        if norm(uc) < 1e-6 {
            continue;
        }
        set_col(&mut u, c, normalize(uc));
        valid[c] = true;
    }
    for c in 0..3 {
        if valid[c] {
            continue;
        }
        // Fill from the cross product of the other two, or an arbitrary
        // orthogonal direction when two columns are degenerate. A zero
        // singular value leaves that column free, so any orthonormal
        // completion is a valid SVD.
        let others: Vec<usize> = (0..3).filter(|&k| k != c && valid[k]).collect();
        let filled = match others.len() {
            2 => cross(col(u, others[0]), col(u, others[1])),
            1 => {
                let a = col(u, others[0]);
                let seed = if a[0].abs() < 0.9 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                normalize(cross(a, seed))
            }
            _ => [1.0, 0.0, 0.0],
        };
        set_col(&mut u, c, normalize(filled));
        valid[c] = true;
    }

    (u, s, v)
}

/// Symmetric 3x3 eigen-decomposition by cyclic Jacobi rotations.
/// Returns `(eigenvalues ascending, eigenvectors as columns)`.
pub fn eigen_sym3(m: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut a = m;
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0f64]];

    for _ in 0..64 {
        // Largest off-diagonal magnitude.
        let (mut p, mut q, mut best) = (0usize, 1usize, 0.0f64);
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            if a[i][j].abs() > best {
                best = a[i][j].abs();
                p = i;
                q = j;
            }
        }
        if best < 1e-14 {
            break;
        }
        let theta = 0.5 * (a[q][q] - a[p][p]) / a[p][q];
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;

        let mut b = a;
        for k in 0..3 {
            b[k][p] = c * a[k][p] - s * a[k][q];
            b[k][q] = s * a[k][p] + c * a[k][q];
        }
        let mut d = b;
        for k in 0..3 {
            d[p][k] = c * b[p][k] - s * b[q][k];
            d[q][k] = s * b[p][k] + c * b[q][k];
        }
        a = d;
        a[p][q] = 0.0;
        a[q][p] = 0.0;

        let mut nv = v;
        for k in 0..3 {
            nv[k][p] = c * v[k][p] - s * v[k][q];
            nv[k][q] = s * v[k][p] + c * v[k][q];
        }
        v = nv;
    }

    let mut idx = [0usize, 1, 2];
    let vals = [a[0][0], a[1][1], a[2][2]];
    idx.sort_by(|&i, &j| vals[i].partial_cmp(&vals[j]).unwrap());
    let sorted_vals = [vals[idx[0]], vals[idx[1]], vals[idx[2]]];
    let mut sorted_vecs = [[0.0f64; 3]; 3];
    for (c, &src) in idx.iter().enumerate() {
        for r in 0..3 {
            sorted_vecs[r][c] = v[r][src];
        }
    }
    (sorted_vals, sorted_vecs)
}

// ---- 3D vector helpers -----------------------------------------------------

pub type V3 = [f64; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub fn normalize(a: V3) -> V3 {
    let n = norm(a);
    if n < 1e-15 {
        [0.0, 0.0, 0.0]
    } else {
        scale(a, 1.0 / n)
    }
}

pub type M3 = [[f64; 3]; 3];

pub fn m3_mul(a: M3, b: M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for (r, row) in o.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    o
}

pub fn m3_mul_v(a: M3, v: V3) -> V3 {
    [
        a[0][0] * v[0] + a[0][1] * v[1] + a[0][2] * v[2],
        a[1][0] * v[0] + a[1][1] * v[1] + a[1][2] * v[2],
        a[2][0] * v[0] + a[2][1] * v[1] + a[2][2] * v[2],
    ]
}

pub fn m3_transpose(a: M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for (r, row) in o.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = a[c][r];
        }
    }
    o
}

pub fn m3_det(a: M3) -> f64 {
    a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
}

/// Rodrigues: rotation vector (axis * angle) to a rotation matrix.
pub fn rodrigues(w: V3) -> M3 {
    let theta = norm(w);
    if theta < 1e-12 {
        // First-order term is enough below this angle and avoids 0/0.
        return [
            [1.0, -w[2], w[1]],
            [w[2], 1.0, -w[0]],
            [-w[1], w[0], 1.0],
        ];
    }
    let k = scale(w, 1.0 / theta);
    let (s, c) = (theta.sin(), theta.cos());
    let kx = [[0.0, -k[2], k[1]], [k[2], 0.0, -k[0]], [-k[1], k[0], 0.0]];
    let kx2 = m3_mul(kx, kx);
    let mut o = [[0.0; 3]; 3];
    for r in 0..3 {
        for c2 in 0..3 {
            let i = if r == c2 { 1.0 } else { 0.0 };
            o[r][c2] = i + s * kx[r][c2] + (1.0 - c) * kx2[r][c2];
        }
    }
    o
}

/// Inverse of `rodrigues`: rotation matrix to rotation vector.
#[cfg(test)]
pub fn rodrigues_inv(m: M3) -> V3 {
    let trace = m[0][0] + m[1][1] + m[2][2];
    let cos_t = ((trace - 1.0) * 0.5).clamp(-1.0, 1.0);
    let theta = cos_t.acos();
    if theta < 1e-9 {
        return [
            0.5 * (m[2][1] - m[1][2]),
            0.5 * (m[0][2] - m[2][0]),
            0.5 * (m[1][0] - m[0][1]),
        ];
    }
    if (std::f64::consts::PI - theta).abs() < 1e-6 {
        // Near pi the skew part vanishes; recover the axis from R + I.
        let d = [
            (m[0][0] + 1.0).max(0.0).sqrt(),
            (m[1][1] + 1.0).max(0.0).sqrt(),
            (m[2][2] + 1.0).max(0.0).sqrt(),
        ];
        let mut axis = normalize(d);
        // Fix the sign of the two smaller components against the skew part.
        if m[2][1] - m[1][2] < 0.0 {
            axis[0] = -axis[0];
        }
        if m[0][2] - m[2][0] < 0.0 {
            axis[1] = -axis[1];
        }
        if m[1][0] - m[0][1] < 0.0 {
            axis[2] = -axis[2];
        }
        return scale(axis, theta);
    }
    let s = 0.5 * theta / theta.sin();
    [
        s * (m[2][1] - m[1][2]),
        s * (m[0][2] - m[2][0]),
        s * (m[1][0] - m[0][1]),
    ]
}

/// Nearest rotation matrix (Gram-Schmidt re-orthonormalization).
pub fn orthonormalize(m: M3) -> M3 {
    let c0 = normalize([m[0][0], m[1][0], m[2][0]]);
    let mut c1 = [m[0][1], m[1][1], m[2][1]];
    c1 = normalize(sub(c1, scale(c0, dot(c0, c1))));
    let c2 = cross(c0, c1);
    [
        [c0[0], c1[0], c2[0]],
        [c0[1], c1[1], c2[1]],
        [c0[2], c1[2], c2[2]],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_recovers_a_known_solution() {
        let mut a = Mat::zeros(3, 3);
        // A deliberately non-symmetric, well-conditioned system.
        let vals = [[2.0, 1.0, -1.0], [-3.0, -1.0, 2.0], [-2.0, 1.0, 2.0]];
        for r in 0..3 {
            for c in 0..3 {
                a[(r, c)] = vals[r][c];
            }
        }
        let x = solve(a, vec![8.0, -11.0, -3.0]).unwrap();
        assert!((x[0] - 2.0).abs() < 1e-9, "{x:?}");
        assert!((x[1] - 3.0).abs() < 1e-9, "{x:?}");
        assert!((x[2] + 1.0).abs() < 1e-9, "{x:?}");
    }

    #[test]
    fn solve_needs_pivoting_when_the_first_pivot_is_zero() {
        let mut a = Mat::zeros(2, 2);
        a[(0, 0)] = 0.0;
        a[(0, 1)] = 1.0;
        a[(1, 0)] = 1.0;
        a[(1, 1)] = 0.0;
        let x = solve(a, vec![3.0, 7.0]).unwrap();
        assert!((x[0] - 7.0).abs() < 1e-12);
        assert!((x[1] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn singular_systems_return_none() {
        let mut a = Mat::zeros(2, 2);
        a[(0, 0)] = 1.0;
        a[(0, 1)] = 2.0;
        a[(1, 0)] = 2.0;
        a[(1, 1)] = 4.0;
        assert!(solve(a, vec![1.0, 2.0]).is_none());
    }

    #[test]
    fn rodrigues_roundtrips() {
        for w in [
            [0.0, 0.0, 0.0],
            [0.1, -0.2, 0.3],
            [1.0, 0.0, 0.0],
            [0.0, 2.5, 0.0],
            [0.3, 0.3, 3.0],
        ] {
            let m = rodrigues(w);
            let w2 = rodrigues_inv(m);
            let m2 = rodrigues(w2);
            for r in 0..3 {
                for c in 0..3 {
                    assert!((m[r][c] - m2[r][c]).abs() < 1e-7, "{w:?}: {m:?} {m2:?}");
                }
            }
        }
    }

    #[test]
    fn rodrigues_produces_rotations() {
        let m = rodrigues([0.3, -1.1, 0.7]);
        assert!((m3_det(m) - 1.0).abs() < 1e-9);
        let mt = m3_transpose(m);
        let i = m3_mul(m, mt);
        for r in 0..3 {
            for c in 0..3 {
                let want = if r == c { 1.0 } else { 0.0 };
                assert!((i[r][c] - want).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn rodrigues_inv_handles_a_half_turn() {
        // theta == pi is the branch where the skew-symmetric part vanishes.
        let axis = normalize([1.0, 2.0, -0.5]);
        let m = rodrigues(scale(axis, std::f64::consts::PI));
        let w = rodrigues_inv(m);
        let m2 = rodrigues(w);
        for r in 0..3 {
            for c in 0..3 {
                assert!((m[r][c] - m2[r][c]).abs() < 1e-5, "{m:?} {m2:?}");
            }
        }
    }

    #[test]
    fn eigen_sym3_finds_known_eigenpairs() {
        // diag(3,1,2) rotated: eigenvalues must come back sorted ascending.
        let r = rodrigues([0.4, -0.2, 0.9]);
        let d = [[3.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 2.0]];
        let a = m3_mul(m3_mul(r, d), m3_transpose(r));
        let (vals, vecs) = eigen_sym3(a);
        assert!((vals[0] - 1.0).abs() < 1e-8, "{vals:?}");
        assert!((vals[1] - 2.0).abs() < 1e-8, "{vals:?}");
        assert!((vals[2] - 3.0).abs() < 1e-8, "{vals:?}");
        // Each eigenvector satisfies A v = lambda v.
        for c in 0..3 {
            let v = [vecs[0][c], vecs[1][c], vecs[2][c]];
            let av = m3_mul_v(a, v);
            for k in 0..3 {
                assert!((av[k] - vals[c] * v[k]).abs() < 1e-7);
            }
        }
    }

    fn mat_from(rows: &[&[f64]]) -> Mat {
        let mut m = Mat::zeros(rows.len(), rows[0].len());
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                m[(r, c)] = *v;
            }
        }
        m
    }

    #[test]
    fn jacobi_eigen_diagonalizes_a_symmetric_matrix() {
        let a = mat_from(&[
            &[4.0, 1.0, -2.0, 2.0],
            &[1.0, 2.0, 0.0, 1.0],
            &[-2.0, 0.0, 3.0, -2.0],
            &[2.0, 1.0, -2.0, -1.0],
        ]);
        let (vals, vecs) = jacobi_eigen(&a);
        assert!(vals.windows(2).all(|w| w[0] <= w[1] + 1e-12), "{vals:?}");
        // A v = lambda v for every column.
        for c in 0..4 {
            for r in 0..4 {
                let av: f64 = (0..4).map(|k| a[(r, k)] * vecs[(k, c)]).sum();
                assert!((av - vals[c] * vecs[(r, c)]).abs() < 1e-8);
            }
        }
        // Eigenvectors are orthonormal.
        for c in 0..4 {
            let n: f64 = (0..4).map(|r| vecs[(r, c)] * vecs[(r, c)]).sum();
            assert!((n - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn null_vector_finds_the_kernel_of_a_rank_deficient_matrix() {
        // Rows all orthogonal to (1, 2, -1)/sqrt(6).
        let a = mat_from(&[&[2.0, 0.0, 2.0], &[1.0, -1.0, -1.0], &[0.0, 1.0, 2.0]]);
        let v = null_vector(&a);
        for r in 0..3 {
            let d: f64 = (0..3).map(|c| a[(r, c)] * v[c]).sum();
            assert!(d.abs() < 1e-8, "row {r}: {d}");
        }
        let n: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!((n - 1.0).abs() < 1e-9);
    }

    #[test]
    fn svd3_reconstructs_the_original_matrix() {
        let mats: [M3; 4] = [
            [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 10.0]],
            rodrigues([0.3, 0.2, -0.7]),
            // Rank 2.
            [[1.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 0.0]],
            // Rank 1.
            [[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [3.0, 6.0, 9.0]],
        ];
        for m in mats {
            let (u, s, v) = svd3(m);
            assert!(s[0] >= s[1] - 1e-12 && s[1] >= s[2] - 1e-12, "{s:?}");
            assert!(s.iter().all(|x| *x >= -1e-12));

            // V is a proper rotation; U is orthogonal and carries the sign of
            // det(m), because the singular values are non-negative.
            assert!((m3_det(v) - 1.0).abs() < 1e-7, "det V = {}", m3_det(v));
            assert!((m3_det(u).abs() - 1.0).abs() < 1e-7, "det U = {}", m3_det(u));
            let utu = m3_mul(m3_transpose(u), u);
            for r in 0..3 {
                for c in 0..3 {
                    let want = if r == c { 1.0 } else { 0.0 };
                    assert!((utu[r][c] - want).abs() < 1e-7, "U not orthogonal: {utu:?}");
                }
            }
            if m3_det(m).abs() > 1e-6 {
                assert!(
                    m3_det(u) * m3_det(m) > 0.0,
                    "det U must match sign(det m): {} vs {}",
                    m3_det(u),
                    m3_det(m)
                );
            }

            let sd = [[s[0], 0.0, 0.0], [0.0, s[1], 0.0], [0.0, 0.0, s[2]]];
            let rec = m3_mul(m3_mul(u, sd), m3_transpose(v));
            for r in 0..3 {
                for c in 0..3 {
                    assert!((rec[r][c] - m[r][c]).abs() < 1e-7, "{m:?} -> {rec:?}");
                }
            }
        }
    }

    #[test]
    fn svd3_handles_a_negative_determinant_matrix() {
        // A reflection: det = -1, so U must be improper while V stays proper.
        let m: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]];
        let (u, s, v) = svd3(m);
        for x in s {
            assert!((x - 1.0).abs() < 1e-9, "{s:?}");
        }
        assert!(m3_det(u) < 0.0);
        assert!((m3_det(v) - 1.0).abs() < 1e-9);
        let sd = [[s[0], 0.0, 0.0], [0.0, s[1], 0.0], [0.0, 0.0, s[2]]];
        let rec = m3_mul(m3_mul(u, sd), m3_transpose(v));
        for r in 0..3 {
            for c in 0..3 {
                assert!((rec[r][c] - m[r][c]).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn svd3_keeps_u_orthonormal_when_a_singular_value_is_only_relatively_small() {
        // A real essential matrix, fitted from noisy data: its third singular
        // value is 1e-9 in absolute terms but 1e-9 *relative* to the other
        // two, so it is a hard zero. Judging it against a fixed 1e-12 floor
        // made `u`'s last column the quotient of two roundoff quantities, and
        // the caller reads that column as the camera translation.
        let m: M3 = [
            [-0.000_361_103_994_373_267_5, -0.452_325_372_937_519_23, 0.004_504_128_731_563_889],
            [-0.274_090_319_024_212_16, 0.045_402_391_310_076_506, 4.537_543_149_981_465],
            [-0.003_611_039_943_736_724, -4.523_253_729_376_218_5, 0.045_041_287_315_702_894],
        ];
        let (u, s, v) = svd3(m);
        assert!(s[2] < 1e-6 * s[0], "expected a rank-2 matrix, got {s:?}");

        // Every column is a unit vector, and the frame is orthogonal.
        for c in 0..3 {
            let col: V3 = [u[0][c], u[1][c], u[2][c]];
            assert!((norm(col) - 1.0).abs() < 1e-9, "column {c} has length {}", norm(col));
        }
        let utu = m3_mul(m3_transpose(u), u);
        for r in 0..3 {
            for c in 0..3 {
                let want = if r == c { 1.0 } else { 0.0 };
                assert!((utu[r][c] - want).abs() < 1e-9, "u is not orthonormal: {utu:?}");
            }
        }
        assert!((m3_det(u).abs() - 1.0).abs() < 1e-9, "det(u) = {}", m3_det(u));
        assert!((m3_det(v) - 1.0).abs() < 1e-9);

        // The first two columns still reconstruct the matrix.
        let sd = [[s[0], 0.0, 0.0], [0.0, s[1], 0.0], [0.0, 0.0, 0.0]];
        let rec = m3_mul(m3_mul(u, sd), m3_transpose(v));
        for r in 0..3 {
            for c in 0..3 {
                assert!((rec[r][c] - m[r][c]).abs() < 1e-6, "{rec:?}");
            }
        }
    }

    #[test]
    fn svd3_of_a_rotation_has_unit_singular_values() {
        let r = rodrigues([0.9, -0.2, 0.4]);
        let (_, s, _) = svd3(r);
        for v in s {
            assert!((v - 1.0).abs() < 1e-7, "{s:?}");
        }
    }

    #[test]
    fn orthonormalize_repairs_a_drifted_rotation() {
        let mut m = rodrigues([0.2, 0.3, -0.1]);
        m[0][0] += 0.01;
        m[1][2] -= 0.02;
        let o = orthonormalize(m);
        assert!((m3_det(o) - 1.0).abs() < 1e-9);
    }
}
