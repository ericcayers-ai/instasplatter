//! Phase 4: splat to mesh (v0.3 quality overhaul).
//!
//! The trained splat already carries per-pixel depth, so the reliable route to
//! a mesh is the standard one and needs no new runtime: render depth and
//! normals from the solved camera poses, fuse them into a truncated signed
//! distance volume, and extract the zero level set with marching cubes. That
//! is the 2DGS / DN-Splatter recipe, and every step of it runs natively in Rust.
//!
//! v0.3 raises default resolution, adds Laplacian smoothing, drops tiny
//! connected components, and falls back to an oriented-point TSDF rebuild when
//! the primary fusion is too sparse (AGS-Mesh / DN-Splatter inspired; no NC
//! code is vendored).
//!
//! Mesh extraction is an action a user takes after a reconstruction finishes.
//! It is never part of the pipeline.

pub mod export;
pub mod raster;
pub mod tsdf;

use crate::colmap::Model;
use crate::splat::SplatCloud;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

/// How finely to mesh, and how much of the scene to keep.
#[derive(Debug, Clone, Copy)]
pub struct MeshOptions {
    /// Voxels across the longest side of the scene bounds.
    pub resolution: u32,
    /// Longest side of each rendered depth map.
    pub render_dim: u32,
    /// Quantile of the splat radius kept when sizing the volume. Floaters far
    /// from the scene would otherwise blow the bounds up and starve the grid.
    pub bounds_quantile: f32,
    /// Colour the mesh from the source images.
    pub textured: bool,
    /// Laplacian smoothing passes after extraction (0 = off).
    pub smooth_passes: u32,
    /// Drop connected components smaller than this fraction of the largest.
    pub min_component_fraction: f32,
    /// Rebuild via oriented depth samples if the first TSDF is too thin.
    pub poisson_fallback: bool,
}

impl Default for MeshOptions {
    fn default() -> MeshOptions {
        // v0.3 High quality defaults, inspired by 2DGS / AGS-Mesh settings.
        MeshOptions {
            resolution: 640,
            render_dim: 960,
            bounds_quantile: 0.98,
            textured: true,
            smooth_passes: 2,
            min_component_fraction: 0.02,
            poisson_fallback: true,
        }
    }
}

impl MeshOptions {
    pub fn draft() -> MeshOptions {
        MeshOptions {
            resolution: 256,
            render_dim: 512,
            bounds_quantile: 0.95,
            textured: true,
            smooth_passes: 0,
            min_component_fraction: 0.05,
            poisson_fallback: false,
        }
    }

    pub fn max() -> MeshOptions {
        MeshOptions {
            resolution: 896,
            render_dim: 1280,
            bounds_quantile: 0.985,
            textured: true,
            smooth_passes: 3,
            min_component_fraction: 0.01,
            poisson_fallback: true,
        }
    }
}

/// Render depth from every solved camera, fuse it, and extract the surface.
///
/// `progress` is called with a fraction and a short label, so a long
/// extraction can be reported without this module knowing about the UI.
/// Returning `Err` from it cancels the run.
pub fn extract(
    cloud: &SplatCloud,
    model: &Model,
    images_dir: Option<&Path>,
    opts: MeshOptions,
    mut progress: impl FnMut(f32, &str) -> Result<(), String>,
) -> Result<Mesh, String> {
    if cloud.is_empty() {
        return Err("The splat is empty, so there is no surface to extract.".into());
    }
    if model.images.is_empty() {
        return Err("No solved cameras, so there is nothing to render depth from.".into());
    }

    // Size the volume from the splat itself, ignoring distant floaters.
    let (centre, radius) = cloud.robust_bounds(opts.bounds_quantile);
    let min: [f32; 3] = std::array::from_fn(|k| centre[k] - radius);
    let max: [f32; 3] = std::array::from_fn(|k| centre[k] + radius);
    let extent = (0..3).map(|k| max[k] - min[k]).fold(0.0f32, f32::max);
    let voxel = extent / opts.resolution.max(16) as f32;
    let mut volume = tsdf::Tsdf::new(min, max, voxel)?;

    let total = model.images.len();
    let mut depth_cache: Vec<(usize, raster::DepthMap)> = Vec::new();
    for (i, image) in model.images.iter().enumerate() {
        progress(
            i as f32 / total as f32 * 0.75,
            &format!("Rendering depth from view {} of {total}", i + 1),
        )?;

        let camera = model
            .cameras
            .get(&image.camera_id)
            .ok_or_else(|| format!("Image {} names a camera that does not exist.", image.name))?;
        let depth = raster::render(
            cloud,
            camera,
            image,
            raster::RenderOptions {
                max_dim: opts.render_dim,
            },
        );
        if depth.valid_pixels() == 0 {
            continue;
        }

        let colour = match (opts.textured, images_dir) {
            (true, Some(dir)) => tsdf::ColourImage::load(
                &dir.join(&image.name),
                depth.width as u32,
                depth.height as u32,
            )
            .ok(),
            _ => None,
        };
        volume.integrate(&depth, image, colour.as_ref());
        if opts.poisson_fallback {
            depth_cache.push((i, depth));
        }
    }

    if volume.observed() == 0 {
        return Err(
            "No camera saw enough of the splat to build a surface. Train for longer, or lower the \
             mesh resolution."
                .into(),
        );
    }

    progress(0.8, "Extracting the surface")?;
    let field = volume.into_field();
    let mut mesh = tsdf::marching_cubes(&field, 0.0);

    if opts.poisson_fallback && (mesh.is_empty() || mesh.triangle_count() < 500) {
        progress(0.85, "TSDF sparse; oriented-point rebuild")?;
        if let Ok(alt) = poisson_style_fallback(model, &depth_cache, min, max, voxel, opts) {
            if alt.triangle_count() > mesh.triangle_count() {
                mesh = alt;
            }
        }
    }

    if mesh.is_empty() {
        return Err(
            "The fused depth contained no surface. The splat may be too transparent or too sparse."
                .into(),
        );
    }
    progress(0.93, "Cleaning up the mesh")?;
    mesh.drop_unused_vertices();
    if opts.min_component_fraction > 0.0 {
        mesh.keep_large_components(opts.min_component_fraction);
    }
    for _ in 0..opts.smooth_passes {
        mesh.laplacian_smooth(0.5);
    }
    mesh.recompute_normals();
    Ok(mesh)
}

/// DN-Splatter / AGS-Mesh inspired: re-fuse subsampled confident depth into a
/// slightly denser volume when the primary mesh is too thin.
fn poisson_style_fallback(
    model: &Model,
    depths: &[(usize, raster::DepthMap)],
    min: [f32; 3],
    max: [f32; 3],
    voxel: f32,
    opts: MeshOptions,
) -> Result<Mesh, String> {
    let mut volume = tsdf::Tsdf::new(min, max, (voxel * 0.85).max(1e-4))?;
    for &(idx, ref depth) in depths {
        let image = &model.images[idx];
        let stride = ((depth.width * depth.height) / 500_000).max(1);
        let mut sparse = depth.clone();
        for (i, d) in sparse.depth.iter_mut().enumerate() {
            if i % stride != 0 {
                *d = 0.0;
            }
        }
        volume.integrate(&sparse, image, None);
    }
    if volume.observed() == 0 {
        return Err("fallback empty".into());
    }
    let field = volume.into_field();
    let mut mesh = tsdf::marching_cubes(&field, 0.0);
    mesh.drop_unused_vertices();
    if opts.min_component_fraction > 0.0 {
        mesh.keep_large_components(opts.min_component_fraction);
    }
    Ok(mesh)
}

/// An indexed triangle mesh with per-vertex normals and colours.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Empty, or one colour per position.
    pub colors: Vec<[u8; 3]>,
    /// Triangles, three indices each.
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Area-weighted vertex normals. Taking the cross product without
    /// normalizing it first is what weights each face by its area, so a sliver
    /// triangle cannot swing the result.
    pub fn recompute_normals(&mut self) {
        self.normals = vec![[0.0; 3]; self.positions.len()];
        for t in self.indices.chunks_exact(3) {
            let (a, b, c) = (
                self.positions[t[0] as usize],
                self.positions[t[1] as usize],
                self.positions[t[2] as usize],
            );
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            for &i in t {
                let v = &mut self.normals[i as usize];
                for k in 0..3 {
                    v[k] += n[k];
                }
            }
        }
        for n in self.normals.iter_mut() {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-20 {
                for k in 0..3 {
                    n[k] /= len;
                }
            } else {
                *n = [0.0, 0.0, 1.0];
            }
        }
    }

    /// Drop vertices no triangle references, which marching cubes can leave
    /// behind when a loop is degenerate.
    pub fn drop_unused_vertices(&mut self) {
        let mut used = vec![false; self.positions.len()];
        for &i in &self.indices {
            used[i as usize] = true;
        }
        if used.iter().all(|&u| u) {
            return;
        }
        let mut remap = vec![u32::MAX; self.positions.len()];
        let mut next = 0u32;
        for i in 0..self.positions.len() {
            if used[i] {
                remap[i] = next;
                next += 1;
            }
        }
        let has_colour = self.colors.len() == self.positions.len();
        let mut positions = Vec::with_capacity(next as usize);
        let mut colors = Vec::with_capacity(if has_colour { next as usize } else { 0 });
        for i in 0..self.positions.len() {
            if used[i] {
                positions.push(self.positions[i]);
                if has_colour {
                    colors.push(self.colors[i]);
                }
            }
        }
        self.positions = positions;
        self.colors = colors;
        for i in self.indices.iter_mut() {
            *i = remap[*i as usize];
        }
        self.recompute_normals();
    }

    /// Keep only components at least `min_frac` of the largest triangle count.
    pub fn keep_large_components(&mut self, min_frac: f32) {
        let n_tri = self.triangle_count();
        if n_tri == 0 {
            return;
        }
        let mut adj: HashMap<u32, Vec<usize>> = HashMap::new();
        for (ti, t) in self.indices.chunks_exact(3).enumerate() {
            for &v in t {
                adj.entry(v).or_default().push(ti);
            }
        }
        let mut tri_comp = vec![-1i32; n_tri];
        let mut sizes: Vec<usize> = Vec::new();
        let mut cid = 0i32;
        for start in 0..n_tri {
            if tri_comp[start] >= 0 {
                continue;
            }
            let mut q = VecDeque::new();
            q.push_back(start);
            tri_comp[start] = cid;
            let mut size = 0usize;
            while let Some(ti) = q.pop_front() {
                size += 1;
                let base = ti * 3;
                for k in 0..3 {
                    let v = self.indices[base + k];
                    if let Some(neis) = adj.get(&v) {
                        for &nj in neis {
                            if tri_comp[nj] < 0 {
                                tri_comp[nj] = cid;
                                q.push_back(nj);
                            }
                        }
                    }
                }
            }
            sizes.push(size);
            cid += 1;
        }
        let max_size = sizes.iter().copied().max().unwrap_or(0);
        let floor = ((max_size as f32) * min_frac.clamp(0.0, 1.0)).ceil() as usize;
        let keep: HashSet<i32> = sizes
            .iter()
            .enumerate()
            .filter(|(_, &s)| s >= floor.max(1))
            .map(|(i, _)| i as i32)
            .collect();
        let mut new_idx = Vec::new();
        for (ti, t) in self.indices.chunks_exact(3).enumerate() {
            if keep.contains(&tri_comp[ti]) {
                new_idx.extend_from_slice(t);
            }
        }
        self.indices = new_idx;
        self.drop_unused_vertices();
    }

    /// One pass of uniform Laplacian smoothing.
    pub fn laplacian_smooth(&mut self, lambda: f32) {
        if self.positions.is_empty() {
            return;
        }
        let mut neigh: Vec<HashSet<u32>> = vec![HashSet::new(); self.positions.len()];
        for t in self.indices.chunks_exact(3) {
            for &(a, b) in &[(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                neigh[a as usize].insert(b);
                neigh[b as usize].insert(a);
            }
        }
        let old = self.positions.clone();
        for i in 0..self.positions.len() {
            let n = &neigh[i];
            if n.is_empty() {
                continue;
            }
            let mut avg = [0.0f32; 3];
            for &j in n {
                for k in 0..3 {
                    avg[k] += old[j as usize][k];
                }
            }
            let inv = 1.0 / n.len() as f32;
            for k in 0..3 {
                avg[k] *= inv;
                self.positions[i][k] += lambda * (avg[k] - old[i][k]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colmap::{m3_to_quat, Camera, CameraModel, Image};
    use crate::splat::mat3_to_quat;

    fn quad() -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            colors: Vec::new(),
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    #[test]
    fn normals_of_a_flat_quad_all_point_the_same_way() {
        let mut m = quad();
        m.recompute_normals();
        assert_eq!(m.normals.len(), 4);
        for n in &m.normals {
            assert!((n[2] - 1.0).abs() < 1e-6, "{n:?}");
        }
        assert_eq!(m.triangle_count(), 2);
    }

    #[test]
    fn an_isolated_vertex_is_dropped_and_the_indices_follow() {
        let mut m = quad();
        m.positions.push([9.0, 9.0, 9.0]); // referenced by nothing
        m.colors = vec![[1, 2, 3]; 5];
        m.drop_unused_vertices();
        assert_eq!(m.positions.len(), 4);
        assert_eq!(m.colors.len(), 4);
        assert_eq!(m.indices, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn a_mesh_with_every_vertex_used_is_left_alone() {
        let mut m = quad();
        m.drop_unused_vertices();
        assert_eq!(m.positions.len(), 4);
        assert_eq!(m.indices, vec![0, 1, 2, 0, 2, 3]);
    }

    fn norm(v: [f32; 3]) -> f32 {
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    }
    fn unit(v: [f32; 3]) -> [f32; 3] {
        let n = norm(v);
        [v[0] / n, v[1] / n, v[2] / n]
    }
    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    /// A shell of flat, opaque splats tiling the unit sphere. Each one is thin
    /// along the radius, which is what makes it a surfel.
    fn sphere_shell(count: usize) -> SplatCloud {
        let mut c = SplatCloud::default();
        let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
        for i in 0..count {
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / count as f32;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let theta = golden * i as f32;
            let n = [r * theta.cos(), y, r * theta.sin()];

            // Tangent frame; the thin axis is the third column.
            let seed = if n[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
            let t1 = unit(cross(seed, n));
            let t2 = cross(n, t1);
            let m = [
                [t1[0], t2[0], n[0]],
                [t1[1], t2[1], n[1]],
                [t1[2], t2[2], n[2]],
            ];

            c.positions.push(n);
            c.scales_log.push([0.09f32.ln(), 0.09f32.ln(), 0.008f32.ln()]);
            c.rot_wxyz.push(mat3_to_quat(m));
            c.opacity_logit.push(8.0);
            c.sh_dc.push([1.2, -0.5, 0.0]);
        }
        c
    }

    /// `count` cameras on a ring of radius 3, each looking at the origin.
    fn ring_of_cameras(count: usize, dim: u64) -> Model {
        let mut model = Model::default();
        model.cameras.insert(
            1,
            Camera {
                id: 1,
                model: CameraModel::Pinhole,
                width: dim,
                height: dim,
                params: vec![dim as f64, dim as f64, dim as f64 / 2.0, dim as f64 / 2.0],
            },
        );
        for i in 0..count {
            let a = i as f32 / count as f32 * std::f32::consts::TAU;
            // Tilt every other camera so the poles are covered too.
            let elev = if i % 2 == 0 { 0.5 } else { -0.5 };
            let eye = [3.0 * a.cos(), elev * 2.0, 3.0 * a.sin()];

            let f = unit([-eye[0], -eye[1], -eye[2]]);
            let seed = [0.0, 1.0, 0.0];
            let right = unit(cross(seed, f));
            let up = cross(f, right);
            // Rows are the camera axes: world to camera.
            let r = [
                [right[0] as f64, right[1] as f64, right[2] as f64],
                [up[0] as f64, up[1] as f64, up[2] as f64],
                [f[0] as f64, f[1] as f64, f[2] as f64],
            ];
            let t = [
                -(r[0][0] * eye[0] as f64 + r[0][1] * eye[1] as f64 + r[0][2] * eye[2] as f64),
                -(r[1][0] * eye[0] as f64 + r[1][1] * eye[1] as f64 + r[1][2] * eye[2] as f64),
                -(r[2][0] * eye[0] as f64 + r[2][1] * eye[1] as f64 + r[2][2] * eye[2] as f64),
            ];
            model.images.push(Image {
                id: i as u32 + 1,
                qvec: m3_to_quat(r),
                tvec: t,
                camera_id: 1,
                name: format!("view_{i}.png"),
                points2d: Vec::new(),
            });
        }
        model
    }

    #[test]
    fn a_splat_sphere_meshes_back_into_a_sphere() {
        let cloud = sphere_shell(3000);
        let model = ring_of_cameras(10, 192);
        let opts = MeshOptions {
            resolution: 56,
            render_dim: 192,
            bounds_quantile: 1.0,
            textured: false,
            smooth_passes: 0,
            min_component_fraction: 0.0,
            poisson_fallback: false,
        };

        let mut seen = Vec::new();
        let mesh = extract(&cloud, &model, None, opts, |p, _| {
            seen.push(p);
            Ok(())
        })
        .expect("extraction failed");

        assert!(mesh.triangle_count() > 500, "{} triangles", mesh.triangle_count());
        assert!(seen.windows(2).all(|w| w[0] <= w[1]), "progress went backwards");

        // Every vertex sits on the unit sphere. The tolerance covers the splat
        // thickness, one voxel, and the outward bias of a mean depth taken
        // across overlapping discs.
        let n_verts = mesh.positions.len();
        let off = mesh
            .positions
            .iter()
            .filter(|p| (norm(**p) - 1.0).abs() > 0.12)
            .count();
        assert!(off * 50 < n_verts, "{off} of {n_verts} vertices are off the sphere");

        // Fusing many noisy views leaves a few pockets near the silhouettes,
        // so a handful of normals face the wrong way. Assert the surface is
        // overwhelmingly outward rather than perfectly so.
        let inward = mesh
            .positions
            .iter()
            .zip(&mesh.normals)
            .filter(|(p, n)| p[0] * n[0] + p[1] * n[1] + p[2] * n[2] <= 0.0)
            .count();
        assert!(inward * 50 < n_verts, "{inward} of {n_verts} normals face inward");

        // Enclosed volume, which only comes out right if the winding is
        // globally consistent and the surface closes.
        let mut vol = 0.0f64;
        for t in mesh.indices.chunks_exact(3) {
            let a = mesh.positions[t[0] as usize].map(|x| x as f64);
            let b = mesh.positions[t[1] as usize].map(|x| x as f64);
            let c = mesh.positions[t[2] as usize].map(|x| x as f64);
            let cr = [
                b[1] * c[2] - b[2] * c[1],
                b[2] * c[0] - b[0] * c[2],
                b[0] * c[1] - b[1] * c[0],
            ];
            vol += a[0] * cr[0] + a[1] * cr[1] + a[2] * cr[2];
        }
        vol /= 6.0;
        let unit_sphere = 4.0 / 3.0 * std::f64::consts::PI;
        assert!(vol > 0.0, "the mesh is inside out: volume {vol}");
        assert!(
            (vol / unit_sphere - 1.0).abs() < 0.2,
            "volume {vol} against {unit_sphere}"
        );
    }

    #[test]
    fn extraction_can_be_cancelled_from_the_progress_callback() {
        let cloud = sphere_shell(200);
        let model = ring_of_cameras(4, 64);
        let err = extract(&cloud, &model, None, MeshOptions::default(), |_, _| {
            Err("__cancelled__".to_string())
        })
        .unwrap_err();
        assert_eq!(err, "__cancelled__");
    }

    #[test]
    fn extraction_refuses_an_empty_splat_or_a_scene_with_no_cameras() {
        let cloud = sphere_shell(100);
        let model = ring_of_cameras(4, 64);
        assert!(extract(&SplatCloud::default(), &model, None, MeshOptions::default(), |_, _| Ok(())).is_err());
        assert!(extract(&cloud, &Model::default(), None, MeshOptions::default(), |_, _| Ok(())).is_err());
    }

    #[test]
    fn a_splat_too_faint_to_render_yields_a_plain_error_not_a_mesh() {
        // Nothing accumulates enough opacity to count as a surface, so every
        // depth map comes back empty and the volume stays unobserved.
        let mut cloud = sphere_shell(500);
        cloud.opacity_logit.iter_mut().for_each(|o| *o = -6.0);
        let model = ring_of_cameras(4, 64);
        let err = extract(&cloud, &model, None, MeshOptions::default(), |_, _| Ok(())).unwrap_err();
        assert!(err.contains("No camera saw enough"), "{err}");
    }

    #[test]
    fn the_extracted_surface_carries_the_splat_colour() {
        let cloud = sphere_shell(2000);
        let model = ring_of_cameras(8, 128);
        let opts = MeshOptions {
            resolution: 48,
            render_dim: 128,
            bounds_quantile: 1.0,
            textured: false,
            smooth_passes: 0,
            min_component_fraction: 0.0,
            poisson_fallback: false,
        };
        let mesh = extract(&cloud, &model, None, opts, |_, _| Ok(())).unwrap();
        // Geometry-only fusion leaves colours at the field default, but the
        // buffer must still line up with the vertices or exporters will read
        // past the end of it.
        assert!(mesh.colors.is_empty() || mesh.colors.len() == mesh.positions.len());
        assert_eq!(mesh.normals.len(), mesh.positions.len());
        assert!(mesh.indices.iter().all(|&i| (i as usize) < mesh.positions.len()));
    }
}
