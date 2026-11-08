//! COLMAP sparse-model reading and writing.
//!
//! The batch path reads back what `colmap mapper` produced (binary), the
//! incremental engine in Phase 2 writes its own poses (text, which Brush also
//! reads), and Phase 4 reads poses to render depth from the trained splat.
//!
//! COLMAP stores world-to-camera: `x_cam = R * x_world + t`, with `R` from
//! the quaternion `(w, x, y, z)` and `t` the translation. The camera centre in
//! world space is therefore `C = -Rᵀ t`.

use crate::math::{m3_mul_v, m3_transpose, scale, M3, V3};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraModel {
    SimplePinhole,
    Pinhole,
    SimpleRadial,
    Radial,
    Opencv,
}

impl CameraModel {
    pub fn from_id(id: i32) -> Option<CameraModel> {
        Some(match id {
            0 => CameraModel::SimplePinhole,
            1 => CameraModel::Pinhole,
            2 => CameraModel::SimpleRadial,
            3 => CameraModel::Radial,
            4 => CameraModel::Opencv,
            _ => return None,
        })
    }

    pub fn from_name(name: &str) -> Option<CameraModel> {
        Some(match name {
            "SIMPLE_PINHOLE" => CameraModel::SimplePinhole,
            "PINHOLE" => CameraModel::Pinhole,
            "SIMPLE_RADIAL" => CameraModel::SimpleRadial,
            "RADIAL" => CameraModel::Radial,
            "OPENCV" => CameraModel::Opencv,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            CameraModel::SimplePinhole => "SIMPLE_PINHOLE",
            CameraModel::Pinhole => "PINHOLE",
            CameraModel::SimpleRadial => "SIMPLE_RADIAL",
            CameraModel::Radial => "RADIAL",
            CameraModel::Opencv => "OPENCV",
        }
    }

    pub fn num_params(self) -> usize {
        match self {
            CameraModel::SimplePinhole => 3,
            CameraModel::Pinhole => 4,
            CameraModel::SimpleRadial => 4,
            CameraModel::Radial => 5,
            CameraModel::Opencv => 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Camera {
    pub id: u32,
    pub model: CameraModel,
    pub width: u64,
    pub height: u64,
    pub params: Vec<f64>,
}

impl Camera {
    /// `(fx, fy)` regardless of model.
    pub fn focal(&self) -> (f64, f64) {
        match self.model {
            CameraModel::SimplePinhole | CameraModel::SimpleRadial | CameraModel::Radial => {
                (self.params[0], self.params[0])
            }
            CameraModel::Pinhole | CameraModel::Opencv => (self.params[0], self.params[1]),
        }
    }

    /// `(cx, cy)` regardless of model.
    pub fn principal_point(&self) -> (f64, f64) {
        match self.model {
            CameraModel::SimplePinhole | CameraModel::SimpleRadial | CameraModel::Radial => {
                (self.params[1], self.params[2])
            }
            CameraModel::Pinhole | CameraModel::Opencv => (self.params[2], self.params[3]),
        }
    }

    /// Project a camera-space point to pixels. Ignores distortion, which is
    /// small for the models we emit and irrelevant for depth rendering.
    pub fn project(&self, cam: V3) -> Option<(f64, f64)> {
        if cam[2] <= 1e-6 {
            return None;
        }
        let (fx, fy) = self.focal();
        let (cx, cy) = self.principal_point();
        Some((fx * cam[0] / cam[2] + cx, fy * cam[1] / cam[2] + cy))
    }
}

#[derive(Debug, Clone)]
pub struct Image {
    pub id: u32,
    /// World-to-camera quaternion `(w, x, y, z)`.
    pub qvec: [f64; 4],
    /// World-to-camera translation.
    pub tvec: V3,
    pub camera_id: u32,
    pub name: String,
    /// `(x, y, point3D_id)`; `point3D_id` is `u64::MAX` when unobserved.
    pub points2d: Vec<(f64, f64, u64)>,
}

impl Image {
    pub fn rotation(&self) -> M3 {
        quat_to_m3(self.qvec)
    }

    /// Camera centre in world space.
    pub fn center(&self) -> V3 {
        let r = self.rotation();
        scale(m3_mul_v(m3_transpose(r), self.tvec), -1.0)
    }

    /// World point into camera space.
    pub fn world_to_cam(&self, p: V3) -> V3 {
        crate::math::add(m3_mul_v(self.rotation(), p), self.tvec)
    }
}

#[derive(Debug, Clone)]
pub struct Point3D {
    pub id: u64,
    pub xyz: V3,
    pub rgb: [u8; 3],
    pub error: f64,
    /// `(image_id, point2D_idx)`
    pub track: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub cameras: HashMap<u32, Camera>,
    pub images: Vec<Image>,
    pub points: Vec<Point3D>,
}

/// Rotation matrix from a COLMAP `(w, x, y, z)` quaternion.
pub fn quat_to_m3(q: [f64; 4]) -> M3 {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let (w, x, y, z) = if n < 1e-12 {
        (1.0, 0.0, 0.0, 0.0)
    } else {
        (q[0] / n, q[1] / n, q[2] / n, q[3] / n)
    };
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

/// COLMAP `(w, x, y, z)` quaternion from a rotation matrix.
pub fn m3_to_quat(m: M3) -> [f64; 4] {
    let t = m[0][0] + m[1][1] + m[2][2];
    if t > 0.0 {
        let s = (t + 1.0).sqrt() * 2.0;
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

// ---- Binary reading --------------------------------------------------------

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.at + n > self.b.len() {
            return Err("COLMAP file ended unexpectedly.".into());
        }
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn cstr(&mut self) -> Result<String, String> {
        let start = self.at;
        while self.at < self.b.len() && self.b[self.at] != 0 {
            self.at += 1;
        }
        if self.at >= self.b.len() {
            return Err("Unterminated string in COLMAP file.".into());
        }
        let s = String::from_utf8_lossy(&self.b[start..self.at]).into_owned();
        self.at += 1;
        Ok(s)
    }
}

fn read_cameras_bin(path: &Path) -> Result<HashMap<u32, Camera>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut c = Cursor { b: &bytes, at: 0 };
    let n = c.u64()?;
    let mut out = HashMap::new();
    for _ in 0..n {
        let id = c.u32()?;
        let model_id = c.i32()?;
        let model = CameraModel::from_id(model_id)
            .ok_or_else(|| format!("Unsupported COLMAP camera model id {model_id}."))?;
        let width = c.u64()?;
        let height = c.u64()?;
        let mut params = Vec::with_capacity(model.num_params());
        for _ in 0..model.num_params() {
            params.push(c.f64()?);
        }
        out.insert(
            id,
            Camera {
                id,
                model,
                width,
                height,
                params,
            },
        );
    }
    Ok(out)
}

fn read_images_bin(path: &Path) -> Result<Vec<Image>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut c = Cursor { b: &bytes, at: 0 };
    let n = c.u64()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let id = c.u32()?;
        let qvec = [c.f64()?, c.f64()?, c.f64()?, c.f64()?];
        let tvec = [c.f64()?, c.f64()?, c.f64()?];
        let camera_id = c.u32()?;
        let name = c.cstr()?;
        let np = c.u64()?;
        let mut points2d = Vec::with_capacity(np as usize);
        for _ in 0..np {
            points2d.push((c.f64()?, c.f64()?, c.u64()?));
        }
        out.push(Image {
            id,
            qvec,
            tvec,
            camera_id,
            name,
            points2d,
        });
    }
    Ok(out)
}

fn read_points3d_bin(path: &Path) -> Result<Vec<Point3D>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut c = Cursor { b: &bytes, at: 0 };
    let n = c.u64()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let id = c.u64()?;
        let xyz = [c.f64()?, c.f64()?, c.f64()?];
        let rgb = [c.u8()?, c.u8()?, c.u8()?];
        let error = c.f64()?;
        let tl = c.u64()?;
        let mut track = Vec::with_capacity(tl as usize);
        for _ in 0..tl {
            track.push((c.u32()?, c.u32()?));
        }
        out.push(Point3D {
            id,
            xyz,
            rgb,
            error,
            track,
        });
    }
    Ok(out)
}

// ---- Text reading ----------------------------------------------------------

fn read_cameras_txt(path: &Path) -> Result<HashMap<u32, Camera>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 5 {
            return Err(format!("Malformed cameras.txt line: {line}"));
        }
        let id: u32 = f[0].parse().map_err(|_| "Bad camera id.".to_string())?;
        let model = CameraModel::from_name(f[1])
            .ok_or_else(|| format!("Unsupported COLMAP camera model '{}'.", f[1]))?;
        let width = f[2].parse().map_err(|_| "Bad width.".to_string())?;
        let height = f[3].parse().map_err(|_| "Bad height.".to_string())?;
        let params: Result<Vec<f64>, _> = f[4..].iter().map(|v| v.parse::<f64>()).collect();
        let params = params.map_err(|_| "Bad camera params.".to_string())?;
        if params.len() != model.num_params() {
            return Err(format!(
                "Camera {id}: model {} expects {} params, found {}.",
                model.name(),
                model.num_params(),
                params.len()
            ));
        }
        out.insert(
            id,
            Camera {
                id,
                model,
                width,
                height,
                params,
            },
        );
    }
    Ok(out)
}

fn read_images_txt(path: &Path) -> Result<Vec<Image>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.starts_with('#'))
        .collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        if lines[i].is_empty() {
            i += 1;
            continue;
        }
        let f: Vec<&str> = lines[i].split_whitespace().collect();
        if f.len() < 10 {
            return Err(format!("Malformed images.txt header line: {}", lines[i]));
        }
        let p = |s: &str| s.parse::<f64>().map_err(|_| "Bad number.".to_string());
        let img = Image {
            id: f[0].parse().map_err(|_| "Bad image id.".to_string())?,
            qvec: [p(f[1])?, p(f[2])?, p(f[3])?, p(f[4])?],
            tvec: [p(f[5])?, p(f[6])?, p(f[7])?],
            camera_id: f[8].parse().map_err(|_| "Bad camera id.".to_string())?,
            name: f[9..].join(" "),
            points2d: Vec::new(),
        };
        // The second line holds the 2D observations; it may be empty.
        i += 1;
        let mut img = img;
        if i < lines.len() {
            let g: Vec<&str> = lines[i].split_whitespace().collect();
            for chunk in g.chunks(3) {
                if chunk.len() == 3 {
                    img.points2d.push((
                        p(chunk[0])?,
                        p(chunk[1])?,
                        chunk[2]
                            .parse::<i64>()
                            .map(|v| v as u64)
                            .unwrap_or(u64::MAX),
                    ));
                }
            }
            i += 1;
        }
        out.push(img);
    }
    Ok(out)
}

// ---- Public API ------------------------------------------------------------

/// Locate the valid sparse reconstruction with the most registered images.
/// Candidate order is stable, so ties prefer the conventional `sparse/0` path.
pub fn find_model_dir(root: &Path) -> Option<PathBuf> {
    // Accept a workspace root, a `sparse` directory, or a model directory
    // itself. Keep the existing path precedence for equal-sized models, then
    // inspect every numbered reconstruction COLMAP wrote under `sparse/`.
    let sparse = root.join("sparse");
    let mut candidates = Vec::new();
    candidates.push(sparse.join("0"));
    let numbered_models = |parent: &Path| {
        let mut found: Vec<(usize, PathBuf)> = fs::read_dir(parent)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if !path.is_dir() {
                    return None;
                }
                Some((path.file_name()?.to_str()?.parse::<usize>().ok()?, path))
            })
            .collect();
        found.sort_by_key(|(index, _)| *index);
        found.into_iter().map(|(_, path)| path).collect::<Vec<_>>()
    };
    for candidate in numbered_models(&sparse) {
        if candidate != sparse.join("0") {
            candidates.push(candidate);
        }
    }
    candidates.push(sparse);
    for candidate in numbered_models(root) {
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates.push(root.to_path_buf());

    let mut best: Option<(PathBuf, usize)> = None;
    for candidate in candidates {
        let Ok((_, images)) = read_model_views(&candidate) else {
            continue;
        };
        let registered = images.len();
        if best
            .as_ref()
            .is_none_or(|(_, best_registered)| registered > *best_registered)
        {
            best = Some((candidate, registered));
        }
    }
    best.map(|(path, _)| path)
}

/// Read a sparse model, binary preferred, falling back to text.
pub fn read_model(dir: &Path) -> Result<Model, String> {
    let (cameras, images) = read_model_views(dir)?;

    // COLMAP writes points3D with a capital D; Brush writes lowercase. Accept
    // either, and treat the point cloud as optional: poses are what we need.
    let points = ["points3D.bin", "points3d.bin"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.exists())
        .map(|p| read_points3d_bin(&p))
        .transpose()?
        .unwrap_or_default();

    Ok(Model {
        cameras,
        images,
        points,
    })
}

fn read_model_views(dir: &Path) -> Result<(HashMap<u32, Camera>, Vec<Image>), String> {
    let read_binary = || -> Result<_, String> {
        let cameras = read_cameras_bin(&dir.join("cameras.bin"))?;
        let images = read_images_bin(&dir.join("images.bin"))?;
        validate_registered_images(&cameras, &images)?;
        Ok((cameras, images))
    };
    let read_text = || -> Result<_, String> {
        let cameras = read_cameras_txt(&dir.join("cameras.txt"))?;
        let images = read_images_txt(&dir.join("images.txt"))?;
        validate_registered_images(&cameras, &images)?;
        Ok((cameras, images))
    };

    let has_binary_pair = dir.join("cameras.bin").exists() && dir.join("images.bin").exists();
    let has_text_pair = dir.join("cameras.txt").exists() && dir.join("images.txt").exists();
    let (cameras, images) = if has_binary_pair {
        match read_binary() {
            Ok(model) => model,
            Err(binary_error) if has_text_pair => read_text().map_err(|text_error| {
                format!("Binary model invalid ({binary_error}); text model invalid ({text_error}).")
            })?,
            Err(error) => return Err(error),
        }
    } else if has_text_pair {
        read_text()?
    } else {
        return Err(format!(
            "No complete COLMAP model found in {}.",
            dir.display()
        ));
    };
    Ok((cameras, images))
}

fn validate_registered_images(
    cameras: &HashMap<u32, Camera>,
    images: &[Image],
) -> Result<(), String> {
    if images.is_empty() {
        return Err("The COLMAP model contains no registered images.".into());
    }
    if cameras.is_empty() {
        return Err("The COLMAP model contains no cameras.".into());
    }
    if let Some(image) = images
        .iter()
        .find(|image| !cameras.contains_key(&image.camera_id))
    {
        return Err(format!(
            "COLMAP image {} references missing camera {}.",
            image.name, image.camera_id
        ));
    }
    Ok(())
}

/// Write a sparse model as text. Brush reads this layout directly, and it is
/// what the incremental engine emits so the batch and live paths agree.
pub fn write_model_txt(dir: &Path, model: &Model) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let mut c = fs::File::create(dir.join("cameras.txt")).map_err(|e| e.to_string())?;
    writeln!(c, "# Camera list with one line of data per camera:").map_err(|e| e.to_string())?;
    writeln!(c, "#   CAMERA_ID, MODEL, WIDTH, HEIGHT, PARAMS[]").map_err(|e| e.to_string())?;
    let mut ids: Vec<&u32> = model.cameras.keys().collect();
    ids.sort();
    for id in ids {
        let cam = &model.cameras[id];
        let params: Vec<String> = cam.params.iter().map(|v| format!("{v}")).collect();
        writeln!(
            c,
            "{} {} {} {} {}",
            cam.id,
            cam.model.name(),
            cam.width,
            cam.height,
            params.join(" ")
        )
        .map_err(|e| e.to_string())?;
    }

    let mut f = fs::File::create(dir.join("images.txt")).map_err(|e| e.to_string())?;
    writeln!(f, "# Image list with two lines of data per image:").map_err(|e| e.to_string())?;
    writeln!(
        f,
        "#   IMAGE_ID, QW, QX, QY, QZ, TX, TY, TZ, CAMERA_ID, NAME"
    )
    .map_err(|e| e.to_string())?;
    writeln!(f, "#   POINTS2D[] as (X, Y, POINT3D_ID)").map_err(|e| e.to_string())?;
    for img in &model.images {
        writeln!(
            f,
            "{} {} {} {} {} {} {} {} {} {}",
            img.id,
            img.qvec[0],
            img.qvec[1],
            img.qvec[2],
            img.qvec[3],
            img.tvec[0],
            img.tvec[1],
            img.tvec[2],
            img.camera_id,
            img.name
        )
        .map_err(|e| e.to_string())?;
        let obs: Vec<String> = img
            .points2d
            .iter()
            .map(|(x, y, id)| {
                let id_s = if *id == u64::MAX {
                    "-1".to_string()
                } else {
                    id.to_string()
                };
                format!("{x} {y} {id_s}")
            })
            .collect();
        writeln!(f, "{}", obs.join(" ")).map_err(|e| e.to_string())?;
    }

    let mut p = fs::File::create(dir.join("points3D.txt")).map_err(|e| e.to_string())?;
    writeln!(p, "# 3D point list with one line of data per point:").map_err(|e| e.to_string())?;
    writeln!(
        p,
        "#   POINT3D_ID, X, Y, Z, R, G, B, ERROR, TRACK[] as (IMAGE_ID, POINT2D_IDX)"
    )
    .map_err(|e| e.to_string())?;
    for pt in &model.points {
        let track: Vec<String> = pt.track.iter().map(|(i, k)| format!("{i} {k}")).collect();
        writeln!(
            p,
            "{} {} {} {} {} {} {} {} {}",
            pt.id,
            pt.xyz[0],
            pt.xyz[1],
            pt.xyz[2],
            pt.rgb[0],
            pt.rgb[1],
            pt.rgb[2],
            pt.error,
            track.join(" ")
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{m3_mul_v, rodrigues};

    #[test]
    fn quaternion_matrix_roundtrip() {
        for w in [[0.0, 0.0, 0.0], [0.3, -0.5, 0.9], [2.0, 0.1, -0.4]] {
            let m = rodrigues(w);
            let q = m3_to_quat(m);
            let m2 = quat_to_m3(q);
            for r in 0..3 {
                for c in 0..3 {
                    assert!((m[r][c] - m2[r][c]).abs() < 1e-9, "{m:?} {m2:?}");
                }
            }
        }
    }

    #[test]
    fn camera_centre_inverts_the_world_to_camera_transform() {
        let r = rodrigues([0.2, -0.6, 0.3]);
        let center = [1.5, -2.0, 4.0];
        // t = -R * C
        let t = crate::math::scale(m3_mul_v(r, center), -1.0);
        let img = Image {
            id: 1,
            qvec: m3_to_quat(r),
            tvec: t,
            camera_id: 1,
            name: "a.jpg".into(),
            points2d: vec![],
        };
        let c = img.center();
        for k in 0..3 {
            assert!((c[k] - center[k]).abs() < 1e-9, "{c:?}");
        }
        // The centre maps to the camera origin.
        let o = img.world_to_cam(center);
        assert!(o.iter().all(|v| v.abs() < 1e-9), "{o:?}");
    }

    #[test]
    fn opencv_camera_reports_focal_and_principal_point() {
        let cam = Camera {
            id: 1,
            model: CameraModel::Opencv,
            width: 1920,
            height: 1080,
            params: vec![1400.0, 1410.0, 960.0, 540.0, 0.0, 0.0, 0.0, 0.0],
        };
        assert_eq!(cam.focal(), (1400.0, 1410.0));
        assert_eq!(cam.principal_point(), (960.0, 540.0));
        let (u, v) = cam.project([0.0, 0.0, 5.0]).unwrap();
        assert!((u - 960.0).abs() < 1e-9 && (v - 540.0).abs() < 1e-9);
        assert!(cam.project([0.0, 0.0, -1.0]).is_none());
    }

    #[test]
    fn text_model_roundtrip() {
        let dir = std::env::temp_dir().join("instasplatter_colmap_txt_test");
        let _ = fs::remove_dir_all(&dir);

        let mut cameras = HashMap::new();
        cameras.insert(
            1,
            Camera {
                id: 1,
                model: CameraModel::Pinhole,
                width: 640,
                height: 480,
                params: vec![500.0, 501.0, 320.0, 240.0],
            },
        );
        let model = Model {
            cameras,
            images: vec![
                Image {
                    id: 1,
                    qvec: [1.0, 0.0, 0.0, 0.0],
                    tvec: [0.0, 0.0, 0.0],
                    camera_id: 1,
                    name: "img_00000.jpg".into(),
                    points2d: vec![(10.0, 20.0, 7), (30.0, 40.0, u64::MAX)],
                },
                Image {
                    id: 2,
                    qvec: [0.9238795, 0.0, 0.3826834, 0.0],
                    tvec: [1.0, -2.0, 3.0],
                    camera_id: 1,
                    name: "img_00001.jpg".into(),
                    points2d: vec![],
                },
            ],
            points: vec![Point3D {
                id: 7,
                xyz: [1.0, 2.0, 3.0],
                rgb: [10, 20, 30],
                error: 0.5,
                track: vec![(1, 0)],
            }],
        };

        write_model_txt(&dir, &model).unwrap();
        let back = read_model(&dir).unwrap();

        assert_eq!(back.images.len(), 2);
        assert_eq!(back.images[0].name, "img_00000.jpg");
        assert_eq!(back.images[0].points2d.len(), 2);
        assert_eq!(back.images[0].points2d[0].2, 7);
        assert_eq!(back.images[0].points2d[1].2, u64::MAX);
        assert_eq!(back.images[1].points2d.len(), 0);
        let cam = &back.cameras[&1];
        assert_eq!(cam.model, CameraModel::Pinhole);
        assert_eq!(cam.width, 640);
        assert!((cam.params[1] - 501.0).abs() < 1e-9);
        // points3D.txt is written but read_model only loads binary points;
        // poses are the contract, so images must survive intact.
        for k in 0..3 {
            assert!((back.images[1].tvec[k] - model.images[1].tvec[k]).abs() < 1e-9);
        }

        let _ = fs::remove_dir_all(&dir);
    }

    fn write_text_test_model(dir: &Path, registered_images: usize) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("cameras.txt"),
            "1 PINHOLE 640 480 500 500 320 240\n",
        )
        .unwrap();
        let mut images = String::new();
        for id in 1..=registered_images {
            images.push_str(&format!("{id} 1 0 0 0 0 0 0 1 img_{id:05}.jpg\n\n"));
        }
        fs::write(dir.join("images.txt"), images).unwrap();
    }

    #[test]
    fn find_model_dir_selects_largest_valid_registered_model() {
        let root = std::env::temp_dir().join(format!(
            "instasplatter_find_model_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let s0 = root.join("sparse").join("0");
        let s1 = root.join("sparse").join("1");
        let incomplete = root.join("sparse").join("2");
        let corrupt = root.join("sparse").join("3");

        write_text_test_model(&s0, 4);
        write_text_test_model(&s1, 42);
        fs::create_dir_all(&incomplete).unwrap();
        fs::write(
            incomplete.join("cameras.txt"),
            "1 PINHOLE 640 480 500 500 320 240\n",
        )
        .unwrap();
        fs::create_dir_all(&corrupt).unwrap();
        fs::write(
            corrupt.join("cameras.txt"),
            "1 PINHOLE 640 480 500 500 320 240\n",
        )
        .unwrap();
        fs::write(corrupt.join("images.txt"), "not a COLMAP image record\n").unwrap();

        assert_eq!(read_model(&s0).unwrap().images.len(), 4);
        assert_eq!(read_model(&s1).unwrap().images.len(), 42);
        assert_eq!(find_model_dir(&root).unwrap(), s1);
        assert_eq!(find_model_dir(&root.join("sparse")).unwrap(), s1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_model_dir_keeps_candidate_order_for_equal_sized_models() {
        let root = std::env::temp_dir().join(format!(
            "instasplatter_find_model_tie_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let s0 = root.join("sparse").join("0");
        let s1 = root.join("sparse").join("1");
        write_text_test_model(&s0, 4);
        write_text_test_model(&s1, 4);

        assert_eq!(find_model_dir(&root).unwrap(), s0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn read_model_falls_back_to_complete_text_when_binary_pair_is_unusable() {
        let root = std::env::temp_dir().join(format!(
            "instasplatter_model_fallback_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        write_text_test_model(&root, 1);

        // An incomplete binary pair and then a corrupt one must not mask the
        // complete text model in the same directory.
        fs::write(root.join("cameras.bin"), []).unwrap();
        assert_eq!(read_model(&root).unwrap().images.len(), 1);
        fs::write(root.join("images.bin"), []).unwrap();
        assert_eq!(read_model(&root).unwrap().images.len(), 1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_model_is_reported() {
        let root = std::env::temp_dir().join("instasplatter_missing_model_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert!(find_model_dir(&root).is_none());
        assert!(read_model(&root).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
