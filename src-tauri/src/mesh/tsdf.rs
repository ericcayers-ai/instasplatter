//! A truncated signed-distance volume and the marching-cubes surface it
//! carries (ROADMAP-V2 4.1).
//!
//! Fusing many noisy depth maps into a TSDF and extracting its zero level set
//! is the standard, deterministic route from posed depth to a mesh. It is what
//! 2DGS, PGSR and RaDe-GS all do underneath, and it needs nothing but the
//! depth the splat already renders.
//!
//! ## Why the triangle table is built at runtime
//!
//! The familiar 256-entry marching-cubes table encodes one fixed choice for
//! the faces where all four corners alternate in sign. Neighbouring cubes can
//! make opposite choices for the face they share, and the mesh then has a hole
//! there. Rather than copy a table and inherit that, this module derives the
//! topology per cube:
//!
//!   * on each face, the active edges are paired into segments,
//!   * a face with four active edges is disambiguated by the asymptotic
//!     decider, which reads only that face's four corner values,
//!   * the segments are walked into closed loops and fanned into triangles.
//!
//! Because the ambiguous choice depends only on the shared face's corners, two
//! cubes that share a face always agree, and the surface closes. Vertices are
//! keyed by the global grid edge they sit on, so the two cubes meeting at an
//! edge reference the same vertex rather than two coincident copies. That is
//! what makes `every_edge_is_shared_by_exactly_two_triangles` pass.

use crate::colmap::Image;
use crate::mesh::raster::DepthMap;
use crate::mesh::Mesh;
use rayon::prelude::*;
use std::collections::HashMap;

/// Corner `i` of a cube, as an offset in grid cells.
const CORNER: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1],
];

/// The two corners each of the twelve cube edges joins.
const EDGE: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// The six faces, as four corners in cyclic order.
const FACE_CORNERS: [[usize; 4]; 6] = [
    [0, 1, 2, 3], // z = 0
    [4, 5, 6, 7], // z = 1
    [0, 1, 5, 4], // y = 0
    [3, 2, 6, 7], // y = 1
    [0, 3, 7, 4], // x = 0
    [1, 2, 6, 5], // x = 1
];

/// The edge joining face corner `i` to face corner `i + 1`. Every cube edge
/// appears in exactly two faces, which is what lets the segments be walked.
const FACE_EDGES: [[usize; 4]; 6] = [
    [0, 1, 2, 3],
    [4, 5, 6, 7],
    [0, 9, 4, 8],
    [2, 10, 6, 11],
    [3, 11, 7, 8],
    [1, 10, 5, 9],
];

/// A scalar field sampled on a regular grid.
#[derive(Debug, Clone)]
pub struct Field {
    /// Samples per axis.
    pub dims: [usize; 3],
    /// World position of sample `(0, 0, 0)`.
    pub origin: [f32; 3],
    /// Edge length of one cell.
    pub voxel: f32,
    pub values: Vec<f32>,
    /// A sample no camera ever saw. Cubes touching one are skipped, so an
    /// unobserved region leaves a boundary rather than an invented surface.
    pub valid: Vec<bool>,
    /// Empty, or one colour per sample.
    pub colors: Vec<[f32; 3]>,
}

impl Field {
    pub fn new(dims: [usize; 3], origin: [f32; 3], voxel: f32) -> Field {
        let n = dims[0] * dims[1] * dims[2];
        Field {
            dims,
            origin,
            voxel,
            values: vec![0.0; n],
            valid: vec![false; n],
            colors: Vec::new(),
        }
    }

    #[inline]
    pub fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dims[1] + y) * self.dims[0] + x
    }

    #[inline]
    pub fn position(&self, x: usize, y: usize, z: usize) -> [f32; 3] {
        [
            self.origin[0] + x as f32 * self.voxel,
            self.origin[1] + y as f32 * self.voxel,
            self.origin[2] + z as f32 * self.voxel,
        ]
    }

    /// Sample a field analytically, for the tests.
    #[cfg(test)]
    pub fn from_fn(
        dims: [usize; 3],
        origin: [f32; 3],
        voxel: f32,
        f: impl Fn([f32; 3]) -> f32,
    ) -> Field {
        let mut g = Field::new(dims, origin, voxel);
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let i = g.index(x, y, z);
                    g.values[i] = f(g.position(x, y, z));
                    g.valid[i] = true;
                }
            }
        }
        g
    }
}

/// A surface seen edge on tells you almost nothing about where it is: one
/// pixel of error along the ray moves it a long way. Views below this cosine
/// of incidence are ignored, and the rest are weighted by it.
///
/// Without this, a voxel near a silhouette is free space to one camera and
/// just behind the surface to another. Their mean crosses zero somewhere that
/// is not the surface, and marching cubes dutifully meshes the pocket.
const MIN_INCIDENCE_COS: f32 = 0.2;

/// A truncated signed-distance volume, fused from posed depth maps.
///
/// Each sample stores the running weighted mean of the signed distance to the
/// nearest observed surface, clamped to `+-truncation`. Averaging is what
/// removes the per-view noise that raw splat depth carries.
#[derive(Debug)]
pub struct Tsdf {
    pub field: Field,
    pub weight: Vec<f32>,
    /// Distance beyond which a sample carries no information, in world units.
    pub truncation: f32,
    colour_sum: Vec<[f32; 3]>,
}

impl Tsdf {
    /// A volume covering `min..max` with cells of `voxel` units, padded by the
    /// truncation band so the surface never touches the boundary.
    pub fn new(min: [f32; 3], max: [f32; 3], voxel: f32) -> Result<Tsdf, String> {
        if !(voxel > 0.0) || !min.iter().chain(&max).all(|v| v.is_finite()) {
            return Err("The mesh volume has no usable extent.".into());
        }
        let truncation = 3.0 * voxel;
        let pad = 2.0 * truncation;
        let origin: [f32; 3] = std::array::from_fn(|k| min[k] - pad);
        let dims: [usize; 3] = std::array::from_fn(|k| {
            (((max[k] + pad) - origin[k]) / voxel).ceil() as usize + 1
        });
        let total = dims[0].saturating_mul(dims[1]).saturating_mul(dims[2]);
        if total == 0 || dims.iter().any(|&d| d < 2) {
            return Err("The mesh volume has no usable extent.".into());
        }
        // 20 bytes a sample. Refuse rather than exhaust memory.
        if total > 400_000_000 {
            return Err(format!(
                "A voxel size of {voxel:.4} needs {total} samples. Use a coarser mesh resolution."
            ));
        }
        let mut field = Field::new(dims, origin, voxel);
        field.colors = vec![[0.0; 3]; total];
        Ok(Tsdf {
            field,
            weight: vec![0.0; total],
            truncation,
            colour_sum: vec![[0.0; 3]; total],
        })
    }

    /// Number of samples the surface could pass through.
    pub fn observed(&self) -> usize {
        self.weight.iter().filter(|w| **w > 0.0).count()
    }

    /// Fold one rendered view into the volume.
    ///
    /// `colour` is the source image, already resized to the depth map, and is
    /// what gives the mesh its per-vertex colour. Passing `None` fuses
    /// geometry only.
    pub fn integrate(&mut self, depth: &DepthMap, image: &Image, colour: Option<&ColourImage>) {
        let r = image.rotation();
        let t = image.tvec;
        let rm: [[f32; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| r[i][j] as f32));
        let tv = [t[0] as f32, t[1] as f32, t[2] as f32];

        let dims = self.field.dims;
        let origin = self.field.origin;
        let voxel = self.field.voxel;
        let trunc = self.truncation;

        // One z slice per task. Each slice owns a disjoint span of every
        // buffer, so the update needs no synchronization.
        let slice = dims[0] * dims[1];
        let values = &mut self.field.values;
        let valid = &mut self.field.valid;
        let weight = &mut self.weight;
        let colour_sum = &mut self.colour_sum;

        values
            .par_chunks_mut(slice)
            .zip(valid.par_chunks_mut(slice))
            .zip(weight.par_chunks_mut(slice))
            .zip(colour_sum.par_chunks_mut(slice))
            .enumerate()
            .for_each(|(z, (((vals, vlds), wts), cols))| {
                let wz = origin[2] + z as f32 * voxel;
                for y in 0..dims[1] {
                    let wy = origin[1] + y as f32 * voxel;
                    for x in 0..dims[0] {
                        let wx = origin[0] + x as f32 * voxel;

                        let cam = [
                            rm[0][0] * wx + rm[0][1] * wy + rm[0][2] * wz + tv[0],
                            rm[1][0] * wx + rm[1][1] * wy + rm[1][2] * wz + tv[1],
                            rm[2][0] * wx + rm[2][1] * wy + rm[2][2] * wz + tv[2],
                        ];
                        if cam[2] <= 1e-4 {
                            continue;
                        }
                        let px = depth.fx * cam[0] / cam[2] + depth.cx;
                        let py = depth.fy * cam[1] / cam[2] + depth.cy;
                        if px < 0.0 || py < 0.0 {
                            continue;
                        }
                        let (u, v) = (px as usize, py as usize);
                        if u >= depth.width || v >= depth.height {
                            continue;
                        }
                        let pixel = v * depth.width + u;
                        let measured = depth.depth[pixel];
                        if measured <= 0.0 {
                            continue;
                        }

                        // How squarely this camera sees the surface it hit.
                        // The rendered normal is in world space, so rotate it
                        // into the camera before comparing with the view ray.
                        let nw = depth.normal[pixel];
                        let nc = [
                            rm[0][0] * nw[0] + rm[0][1] * nw[1] + rm[0][2] * nw[2],
                            rm[1][0] * nw[0] + rm[1][1] * nw[1] + rm[1][2] * nw[2],
                            rm[2][0] * nw[0] + rm[2][1] * nw[1] + rm[2][2] * nw[2],
                        ];
                        let ray_len = (cam[0] * cam[0] + cam[1] * cam[1] + cam[2] * cam[2]).sqrt();
                        let cos = if ray_len > 1e-9 {
                            -(nc[0] * cam[0] + nc[1] * cam[1] + nc[2] * cam[2]) / ray_len
                        } else {
                            0.0
                        };
                        if cos < MIN_INCIDENCE_COS {
                            continue;
                        }

                        // Positive in front of the surface, along the view ray.
                        let sdf = measured - cam[2];
                        if sdf < -trunc {
                            continue; // occluded: this sample is behind a surface
                        }
                        let sdf = sdf.min(trunc);

                        let i = y * dims[0] + x;
                        let w = wts[i];
                        let new_w = w + cos;
                        vals[i] = (vals[i] * w + cos * sdf) / new_w;
                        wts[i] = new_w;
                        vlds[i] = true;

                        if let Some(img) = colour {
                            // Colour only from samples close to the surface;
                            // far ones project onto whatever is in front.
                            if sdf.abs() < trunc {
                                let c = img.at(u, v);
                                for k in 0..3 {
                                    cols[i][k] = (cols[i][k] * w + cos * c[k]) / new_w;
                                }
                            }
                        }
                    }
                }
            });
    }

    /// Hand over the field, with the fused colours attached.
    pub fn into_field(mut self) -> Field {
        self.field.colors = self.colour_sum;
        // A sample nothing ever saw must stay unobserved, or marching cubes
        // would read its zero value as a surface crossing.
        for (i, w) in self.weight.iter().enumerate() {
            if *w <= 0.0 {
                self.field.valid[i] = false;
            }
        }
        self.field
    }
}

/// An RGB image at the resolution of a depth map.
pub struct ColourImage {
    pub width: usize,
    pub height: usize,
    /// `width * height * 3`, each channel in `0..1`.
    pub rgb: Vec<f32>,
}

impl ColourImage {
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> [f32; 3] {
        let x = x.min(self.width.saturating_sub(1));
        let y = y.min(self.height.saturating_sub(1));
        let i = (y * self.width + x) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    /// Load `path` and resample it to exactly `width` by `height`.
    pub fn load(path: &std::path::Path, width: u32, height: u32) -> Result<ColourImage, String> {
        let img = image::open(path)
            .map_err(|e| format!("Cannot read {}: {e}", path.display()))?
            .resize_exact(width, height, image::imageops::FilterType::Triangle)
            .to_rgb8();
        Ok(ColourImage {
            width: width as usize,
            height: height as usize,
            rgb: img.into_raw().iter().map(|v| *v as f32 / 255.0).collect(),
        })
    }
}

/// Where a face's active edges connect, as slot indices into `FACE_EDGES`.
///
/// Slot `i` spans face corners `i` and `i + 1`, and is active when their signs
/// differ. A face has zero, two or four active slots.
fn face_segments(vals: [f32; 4], inside: [bool; 4]) -> Vec<(usize, usize)> {
    let active: Vec<usize> = (0..4).filter(|&i| inside[i] != inside[(i + 1) % 4]).collect();
    match active.len() {
        2 => vec![(active[0], active[1])],
        4 => {
            // The asymptotic decider: the bilinear surface over this face
            // saddles at this value. Whichever side of the iso-level it falls
            // on is the pair of corners that stay connected across the face.
            let den = vals[0] + vals[2] - vals[1] - vals[3];
            let centre = if den.abs() < 1e-20 {
                // Perfectly balanced. Any consistent tie-break works, as long
                // as it reads only these four values, so both cubes agree.
                0.0
            } else {
                (vals[0] * vals[2] - vals[1] * vals[3]) / den
            };
            // Slots are (0,1), (1,2), (2,3), (3,0). Corners 0 and 2 stay
            // joined when the saddle sits on their side of the level set.
            if (centre < 0.0) == inside[0] {
                // 0 and 2 connected: the segments cut off corners 1 and 3.
                vec![(0, 1), (2, 3)]
            } else {
                vec![(1, 2), (3, 0)]
            }
        }
        _ => Vec::new(),
    }
}

/// Linear crossing point between two samples, as a fraction from `a` to `b`.
fn crossing(va: f32, vb: f32, iso: f32) -> f32 {
    let d = vb - va;
    if d.abs() < 1e-20 {
        0.5
    } else {
        ((iso - va) / d).clamp(0.0, 1.0)
    }
}

/// Extract the `iso` level set of `field` as an indexed triangle mesh.
///
/// Triangles are wound so their normal points toward increasing field value.
/// For a signed distance that is positive outside, that is the outward normal.
pub fn marching_cubes(field: &Field, iso: f32) -> Mesh {
    let [nx, ny, nz] = field.dims;
    let mut mesh = Mesh::default();
    if nx < 2 || ny < 2 || nz < 2 {
        return mesh;
    }
    let has_colour = field.colors.len() == field.values.len();

    // Vertices live on global grid edges, keyed by (lower sample, axis), so
    // the cubes on either side of an edge share one vertex exactly.
    let mut vertex_of: HashMap<(usize, u8), u32> = HashMap::new();

    for z in 0..nz - 1 {
        for y in 0..ny - 1 {
            for x in 0..nx - 1 {
                let corner_idx: [usize; 8] = std::array::from_fn(|c| {
                    field.index(x + CORNER[c][0], y + CORNER[c][1], z + CORNER[c][2])
                });
                if !corner_idx.iter().all(|&i| field.valid[i]) {
                    continue;
                }
                let vals: [f32; 8] = std::array::from_fn(|c| field.values[corner_idx[c]]);
                let inside: [bool; 8] = std::array::from_fn(|c| vals[c] < iso);
                if inside.iter().all(|&b| b) || inside.iter().all(|&b| !b) {
                    continue;
                }

                // Pair the active edges on every face.
                let mut neighbours: [[i32; 2]; 12] = [[-1; 2]; 12];
                let push = |e: usize, other: usize, n: &mut [[i32; 2]; 12]| {
                    if n[e][0] < 0 {
                        n[e][0] = other as i32;
                    } else {
                        n[e][1] = other as i32;
                    }
                };
                for f in 0..6 {
                    let fc = FACE_CORNERS[f];
                    let fv: [f32; 4] = std::array::from_fn(|i| vals[fc[i]]);
                    let fi: [bool; 4] = std::array::from_fn(|i| inside[fc[i]]);
                    for (a, b) in face_segments(fv, fi) {
                        let ea = FACE_EDGES[f][a];
                        let eb = FACE_EDGES[f][b];
                        push(ea, eb, &mut neighbours);
                        push(eb, ea, &mut neighbours);
                    }
                }

                // Emit (and cache) the vertex sitting on one cube edge.
                let vertex_for = |e: usize, mesh: &mut Mesh, map: &mut HashMap<(usize, u8), u32>| -> u32 {
                    let (ca, cb) = EDGE[e];
                    let (oa, ob) = (CORNER[ca], CORNER[cb]);
                    // The axis this edge runs along, and its lower endpoint.
                    let axis = (0..3).find(|&k| oa[k] != ob[k]).unwrap();
                    let (lo, hi) = if oa[axis] < ob[axis] { (ca, cb) } else { (cb, ca) };
                    let key = (corner_idx[lo], axis as u8);
                    if let Some(&v) = map.get(&key) {
                        return v;
                    }
                    // Always interpolate from the lower endpoint, so both
                    // cubes on this edge compute bit-identical coordinates.
                    let (va, vb) = (vals[lo], vals[hi]);
                    let t = crossing(va, vb, iso);
                    let pa = field.position(x + CORNER[lo][0], y + CORNER[lo][1], z + CORNER[lo][2]);
                    let pb = field.position(x + CORNER[hi][0], y + CORNER[hi][1], z + CORNER[hi][2]);
                    let p = [
                        pa[0] + t * (pb[0] - pa[0]),
                        pa[1] + t * (pb[1] - pa[1]),
                        pa[2] + t * (pb[2] - pa[2]),
                    ];
                    let id = mesh.positions.len() as u32;
                    mesh.positions.push(p);
                    if has_colour {
                        let (ca, cb) = (field.colors[corner_idx[lo]], field.colors[corner_idx[hi]]);
                        let mix: [f32; 3] = std::array::from_fn(|k| ca[k] + t * (cb[k] - ca[k]));
                        mesh.colors.push([
                            (mix[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                            (mix[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                            (mix[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                        ]);
                    }
                    map.insert(key, id);
                    id
                };

                // Gradient of the trilinear field, for winding. Corners with a
                // 1 along an axis, minus those with a 0.
                let g = [
                    (vals[1] + vals[2] + vals[5] + vals[6]) - (vals[0] + vals[3] + vals[4] + vals[7]),
                    (vals[2] + vals[3] + vals[6] + vals[7]) - (vals[0] + vals[1] + vals[4] + vals[5]),
                    (vals[4] + vals[5] + vals[6] + vals[7]) - (vals[0] + vals[1] + vals[2] + vals[3]),
                ];

                // Walk the segments into closed loops and fan each one.
                let mut visited = [false; 12];
                for start in 0..12 {
                    if visited[start] || neighbours[start][0] < 0 {
                        continue;
                    }
                    let mut loop_edges = Vec::new();
                    let mut cur = start;
                    let mut prev: i32 = -1;
                    loop {
                        visited[cur] = true;
                        loop_edges.push(cur);
                        let n = neighbours[cur];
                        if n[1] < 0 {
                            break; // malformed, should not happen
                        }
                        let next = if n[0] != prev { n[0] } else { n[1] } as usize;
                        prev = cur as i32;
                        cur = next;
                        if cur == start || visited[cur] {
                            break;
                        }
                    }
                    if loop_edges.len() < 3 {
                        continue;
                    }
                    let ring: Vec<u32> = loop_edges
                        .iter()
                        .map(|&e| vertex_for(e, &mut mesh, &mut vertex_of))
                        .collect();
                    for k in 1..ring.len() - 1 {
                        let tri = [ring[0], ring[k], ring[k + 1]];
                        let (a, b, c) = (
                            mesh.positions[tri[0] as usize],
                            mesh.positions[tri[1] as usize],
                            mesh.positions[tri[2] as usize],
                        );
                        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                        let n = [
                            ab[1] * ac[2] - ab[2] * ac[1],
                            ab[2] * ac[0] - ab[0] * ac[2],
                            ab[0] * ac[1] - ab[1] * ac[0],
                        ];
                        let facing = n[0] * g[0] + n[1] * g[1] + n[2] * g[2];
                        if facing < 0.0 {
                            mesh.indices.extend_from_slice(&[tri[0], tri[2], tri[1]]);
                        } else {
                            mesh.indices.extend_from_slice(&[tri[0], tri[1], tri[2]]);
                        }
                    }
                }
            }
        }
    }

    mesh.recompute_normals();
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn sphere(dims: usize, radius: f32) -> Field {
        let voxel = 4.0 / (dims - 1) as f32;
        Field::from_fn([dims; 3], [-2.0; 3], voxel, |p| {
            (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - radius
        })
    }

    /// Signed volume of a closed, outward-wound mesh, by the divergence theorem.
    fn volume(m: &Mesh) -> f64 {
        let mut v = 0.0f64;
        for t in m.indices.chunks_exact(3) {
            let a = m.positions[t[0] as usize].map(|x| x as f64);
            let b = m.positions[t[1] as usize].map(|x| x as f64);
            let c = m.positions[t[2] as usize].map(|x| x as f64);
            let cross = [
                b[1] * c[2] - b[2] * c[1],
                b[2] * c[0] - b[0] * c[2],
                b[0] * c[1] - b[1] * c[0],
            ];
            v += a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2];
        }
        v / 6.0
    }

    #[test]
    fn a_field_with_no_sign_change_has_no_surface() {
        let f = Field::from_fn([8; 3], [0.0; 3], 1.0, |_| 1.0);
        let m = marching_cubes(&f, 0.0);
        assert!(m.indices.is_empty() && m.positions.is_empty());
    }

    #[test]
    fn every_edge_is_shared_by_exactly_two_triangles() {
        let m = marching_cubes(&sphere(24, 1.3), 0.0);
        assert!(!m.indices.is_empty());

        // Count each undirected edge. A watertight, manifold surface uses each
        // exactly twice; a crack shows up as an edge used once.
        let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
        for t in m.indices.chunks_exact(3) {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let boundary: Vec<_> = counts.iter().filter(|(_, &c)| c != 2).collect();
        assert!(
            boundary.is_empty(),
            "{} non-manifold edges, e.g. {:?}",
            boundary.len(),
            &boundary[..boundary.len().min(4)]
        );
    }

    #[test]
    fn every_directed_edge_appears_once_so_the_winding_is_consistent() {
        let m = marching_cubes(&sphere(20, 1.2), 0.0);
        let mut seen: HashSet<(u32, u32)> = HashSet::new();
        for t in m.indices.chunks_exact(3) {
            for k in 0..3 {
                let e = (t[k], t[(k + 1) % 3]);
                assert!(seen.insert(e), "directed edge {e:?} used twice");
            }
        }
        // And the opposite direction is always present, which together with
        // the above means the surface is closed and consistently oriented.
        for &(a, b) in &seen {
            assert!(seen.contains(&(b, a)), "edge ({a},{b}) has no partner");
        }
    }

    #[test]
    fn the_sphere_has_the_right_volume_and_outward_normals() {
        let r = 1.3f32;
        let m = marching_cubes(&sphere(40, r), 0.0);
        let want = 4.0 / 3.0 * std::f64::consts::PI * (r as f64).powi(3);
        let got = volume(&m);
        // Positive means outward-wound. Marching cubes chords the sphere, so
        // it slightly under-fills; a couple of percent is the discretization.
        assert!(got > 0.0, "inward-wound mesh, volume {got}");
        assert!(
            (got - want).abs() / want < 0.02,
            "volume {got} against {want}"
        );

        // Every vertex sits on the sphere, and its normal points outward.
        for (p, n) in m.positions.iter().zip(&m.normals) {
            let d = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((d - r).abs() < 0.06, "vertex at radius {d}");
            let outward = p[0] * n[0] + p[1] * n[1] + p[2] * n[2];
            assert!(outward > 0.0, "normal points inward at {p:?}");
        }
    }

    #[test]
    fn a_plane_comes_out_flat_and_at_the_right_height() {
        // z - 0.37, so the surface is the plane z = 0.37.
        let f = Field::from_fn([10; 3], [-1.0; 3], 0.25, |p| p[2] - 0.37);
        let m = marching_cubes(&f, 0.0);
        assert!(!m.positions.is_empty());
        for p in &m.positions {
            assert!((p[2] - 0.37).abs() < 1e-5, "vertex off the plane: {p:?}");
        }
        for n in &m.normals {
            assert!(n[2] > 0.999, "normal not along +z: {n:?}");
        }
    }

    #[test]
    fn an_ambiguous_face_is_resolved_the_same_way_from_both_sides() {
        // Two cubes sharing the z = 1 plane. The shared face has alternating
        // signs, which is the case a fixed table gets wrong. Whatever the
        // decider picks, both cubes must cut the face identically, so the
        // combined surface has no boundary edge on that face.
        let mut f = Field::new([2, 2, 3], [0.0; 3], 1.0);
        f.valid.iter_mut().for_each(|v| *v = true);
        let set = |f: &mut Field, x, y, z, v| {
            let i = f.index(x, y, z);
            f.values[i] = v;
        };
        // Shared face (z = 1): alternating signs around the square.
        set(&mut f, 0, 0, 1, -1.0);
        set(&mut f, 1, 0, 1, 1.0);
        set(&mut f, 1, 1, 1, -1.0);
        set(&mut f, 0, 1, 1, 1.0);
        // Outer faces, chosen so both cubes actually produce surface.
        for (x, y) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
            set(&mut f, x, y, 0, 1.0);
            set(&mut f, x, y, 2, 1.0);
        }

        let m = marching_cubes(&f, 0.0);
        assert!(!m.indices.is_empty());
        let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
        for t in m.indices.chunks_exact(3) {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        // Edges on the outer boundary of the two-cube block are used once.
        // Edges lying in the shared plane must be used twice, or the two cubes
        // disagreed about it.
        let plane_z = 1.0f32;
        for (&(a, b), &c) in &counts {
            let (pa, pb) = (m.positions[a as usize], m.positions[b as usize]);
            if (pa[2] - plane_z).abs() < 1e-6 && (pb[2] - plane_z).abs() < 1e-6 {
                assert_eq!(c, 2, "the shared face was cut differently by each cube");
            }
        }
    }

    #[test]
    fn cubes_touching_an_unobserved_sample_are_skipped() {
        let mut f = sphere(16, 1.0);
        // Blind one sample. Every cube using it must vanish from the output.
        let blind = f.index(8, 8, 8);
        f.valid[blind] = false;
        let m = marching_cubes(&f, 0.0);
        let p = f.position(8, 8, 8);
        for v in &m.positions {
            let far = (0..3).any(|k| (v[k] - p[k]).abs() > f.voxel * 1.001);
            assert!(far, "vertex {v:?} came from a cube touching the blind sample");
        }
    }

    #[test]
    fn colours_are_carried_onto_the_surface() {
        let mut f = sphere(12, 1.0);
        f.colors = f
            .values
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let _ = i;
                [1.0, 0.0, 0.0]
            })
            .collect();
        let m = marching_cubes(&f, 0.0);
        assert_eq!(m.colors.len(), m.positions.len());
        assert!(m.colors.iter().all(|c| *c == [255, 0, 0]));
    }

    /// A camera at the origin looking down +z, seeing a wall square on.
    fn front_camera(w: usize, h: usize) -> (DepthMap, Image) {
        let map = DepthMap {
            width: w,
            height: h,
            depth: vec![0.0; w * h],
            // Facing the camera, which sits at the origin looking down +z.
            normal: vec![[0.0, 0.0, -1.0]; w * h],
            fx: 100.0,
            fy: 100.0,
            cx: w as f32 / 2.0,
            cy: h as f32 / 2.0,
        };
        let img = Image {
            id: 1,
            qvec: [1.0, 0.0, 0.0, 0.0],
            tvec: [0.0, 0.0, 0.0],
            camera_id: 1,
            name: "a.png".into(),
            points2d: Vec::new(),
        };
        (map, img)
    }

    #[test]
    fn integrating_a_flat_wall_puts_the_zero_crossing_at_the_wall() {
        // Every pixel reports a wall at z = 1.0.
        let (mut map, img) = front_camera(64, 64);
        map.depth.iter_mut().for_each(|d| *d = 1.0);

        // A volume straddling the wall, centred on the optical axis.
        let mut v = Tsdf::new([-0.2, -0.2, 0.6], [0.2, 0.2, 1.4], 0.05).unwrap();
        v.integrate(&map, &img, None);
        assert!(v.observed() > 0);
        let f = v.into_field();

        // On the axis, samples in front of the wall are positive, behind it
        // negative, and the sign changes exactly once.
        let (mid_x, mid_y) = (f.dims[0] / 2, f.dims[1] / 2);
        let mut crossings = 0;
        let mut prev: Option<f32> = None;
        for z in 0..f.dims[2] {
            let i = f.index(mid_x, mid_y, z);
            if !f.valid[i] {
                continue;
            }
            let world_z = f.position(mid_x, mid_y, z)[2];
            let val = f.values[i];
            if world_z < 0.95 {
                assert!(val > 0.0, "in front of the wall but negative at z={world_z}");
            } else if world_z > 1.05 && val != 0.0 {
                assert!(val < 0.0, "behind the wall but positive at z={world_z}");
            }
            if let Some(p) = prev {
                if (p < 0.0) != (val < 0.0) {
                    crossings += 1;
                }
            }
            prev = Some(val);
        }
        assert_eq!(crossings, 1, "the wall should be crossed exactly once");
    }

    #[test]
    fn a_sample_deep_behind_the_surface_is_left_unobserved() {
        let (mut map, img) = front_camera(32, 32);
        map.depth.iter_mut().for_each(|d| *d = 0.5);
        // The volume sits far behind the wall, past the truncation band.
        let mut v = Tsdf::new([-0.05, -0.05, 2.0], [0.05, 0.05, 2.2], 0.05).unwrap();
        v.integrate(&map, &img, None);
        assert_eq!(v.observed(), 0, "occluded space must stay unknown");
    }

    #[test]
    fn a_surface_seen_edge_on_is_ignored() {
        let (mut map, img) = front_camera(64, 64);
        map.depth.iter_mut().for_each(|d| *d = 1.0);
        // The same wall, but its normal is perpendicular to every view ray.
        map.normal.iter_mut().for_each(|n| *n = [1.0, 0.0, 0.0]);

        // A slim volume hugging the optical axis, so no voxel is far enough
        // off it to pick up an incidence cosine of its own. The voxel is small
        // because `Tsdf::new` pads by the truncation band on every side.
        let mut v = Tsdf::new([-0.005, -0.005, 0.98], [0.005, 0.005, 1.02], 0.005).unwrap();
        v.integrate(&map, &img, None);
        assert_eq!(v.observed(), 0, "a grazing view must not define a surface");
    }

    #[test]
    fn a_square_on_view_outweighs_a_slanted_one() {
        // Two views of the same wall disagreeing about its depth. The one that
        // sees it square on should dominate the fused value.
        let (mut square, img) = front_camera(64, 64);
        square.depth.iter_mut().for_each(|d| *d = 1.0);

        let mut slanted = square.clone();
        slanted.depth.iter_mut().for_each(|d| *d = 1.1);
        // About 53 degrees off the view ray, so cos is near 0.6.
        slanted.normal.iter_mut().for_each(|n| *n = [0.8, 0.0, -0.6]);

        let mut v = Tsdf::new([-0.02, -0.02, 0.98], [0.02, 0.02, 1.02], 0.02).unwrap();
        v.integrate(&square, &img, None);
        v.integrate(&slanted, &img, None);
        let f = v.into_field();
        let i = f.index(f.dims[0] / 2, f.dims[1] / 2, f.dims[2] / 2);
        let fused = f.values[i];

        // An unweighted mean of the two would sit halfway. Weighted, the
        // square-on view pulls it most of the way to its own reading.
        let mut only_square = Tsdf::new([-0.02, -0.02, 0.98], [0.02, 0.02, 1.02], 0.02).unwrap();
        only_square.integrate(&square, &img, None);
        let alone = only_square.into_field().values[i];

        let mut only_slanted = Tsdf::new([-0.02, -0.02, 0.98], [0.02, 0.02, 1.02], 0.02).unwrap();
        only_slanted.integrate(&slanted, &img, None);
        let slant_alone = only_slanted.into_field().values[i];

        let midpoint = 0.5 * (alone + slant_alone);
        assert!(
            (fused - alone).abs() < (fused - slant_alone).abs(),
            "the square-on view should dominate: fused {fused}, square {alone}, slanted {slant_alone}"
        );
        assert!(
            (fused - alone).abs() < (midpoint - alone).abs(),
            "the fusion is not weighted at all"
        );
    }

    #[test]
    fn a_pixel_with_no_depth_contributes_nothing() {
        let (map, img) = front_camera(32, 32); // all depths are zero
        let mut v = Tsdf::new([-0.2, -0.2, 0.6], [0.2, 0.2, 1.4], 0.1).unwrap();
        v.integrate(&map, &img, None);
        assert_eq!(v.observed(), 0);
    }

    #[test]
    fn two_views_average_rather_than_overwrite() {
        let (mut a, img) = front_camera(64, 64);
        a.depth.iter_mut().for_each(|d| *d = 1.0);
        let mut b = a.clone();
        b.depth.iter_mut().for_each(|d| *d = 1.1);

        let sample = |maps: &[&DepthMap]| -> f32 {
            let mut v = Tsdf::new([-0.05, -0.05, 0.95], [0.05, 0.05, 1.05], 0.05).unwrap();
            for m in maps {
                v.integrate(m, &img, None);
            }
            let f = v.into_field();
            let i = f.index(f.dims[0] / 2, f.dims[1] / 2, 0);
            f.values[i]
        };

        let only_a = sample(&[&a]);
        let only_b = sample(&[&b]);
        let both = sample(&[&a, &b]);
        assert!(
            (both - 0.5 * (only_a + only_b)).abs() < 1e-5,
            "{both} is not the mean of {only_a} and {only_b}"
        );
    }

    #[test]
    fn a_volume_that_would_not_fit_in_memory_is_refused() {
        let err = Tsdf::new([0.0; 3], [100.0; 3], 0.0001).unwrap_err();
        assert!(err.contains("coarser"), "{err}");
        assert!(Tsdf::new([0.0; 3], [1.0; 3], 0.0).is_err());
        assert!(Tsdf::new([0.0; 3], [f32::NAN; 3], 0.1).is_err());
    }

    #[test]
    fn face_segments_pairs_two_active_edges_and_splits_four() {
        // One corner inside: exactly two active slots, joined.
        let s = face_segments([-1.0, 1.0, 1.0, 1.0], [true, false, false, false]);
        assert_eq!(s, vec![(0, 3)]);

        // Alternating: four active slots, two segments, and each slot is used
        // exactly once whichever way the decider goes.
        let s = face_segments([-1.0, 1.0, -1.0, 1.0], [true, false, true, false]);
        assert_eq!(s.len(), 2);
        let mut slots: Vec<usize> = s.iter().flat_map(|&(a, b)| [a, b]).collect();
        slots.sort();
        assert_eq!(slots, vec![0, 1, 2, 3]);

        // No sign change: nothing.
        assert!(face_segments([1.0; 4], [false; 4]).is_empty());
    }
}
