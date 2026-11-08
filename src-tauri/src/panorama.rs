use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageReader, Rgb, RgbImage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const MAX_FACE_SIZE: u32 = 2048;
pub(crate) const MANIFEST_FILE_NAME: &str = "panorama-manifest.json";
const MANIFEST_FORMAT: &str = "instasplatter-panorama-projection-v1";
const VIEW_HEADINGS: [(&str, f64); 4] = [
    ("front", 0.0),
    ("right", std::f64::consts::FRAC_PI_2),
    ("back", std::f64::consts::PI),
    ("left", 3.0 * std::f64::consts::FRAC_PI_2),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PanoramaSourceManifest {
    pub source_index: usize,
    pub source_file_name: String,
    pub views: Vec<ProjectedFaceManifest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectedFaceManifest {
    pub face: String,
    pub yaw_degrees: i16,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PanoramaManifest {
    format: String,
    projection: String,
    horizontal_fov_degrees: u8,
    yaw_convention: String,
    source_count: usize,
    sources: Vec<PanoramaSourceManifest>,
}

/// Copy the projection metadata into the reconstruction workspace, retaining
/// only projected faces that survived image selection and renaming them to
/// match the copied dataset files. Returns `false` when the source file is not
/// an InstaSplat panorama manifest.
pub(crate) fn remap_manifest(
    source_path: &Path,
    output_path: &Path,
    selected_names: &HashMap<String, String>,
) -> Result<bool, String> {
    let contents = std::fs::read(source_path).map_err(|e| {
        format!(
            "Cannot read panorama manifest {}: {e}",
            source_path.display()
        )
    })?;
    let value: serde_json::Value = serde_json::from_slice(&contents).map_err(|e| {
        format!(
            "Cannot parse panorama manifest {}: {e}",
            source_path.display()
        )
    })?;
    if value.get("format").and_then(serde_json::Value::as_str) != Some(MANIFEST_FORMAT) {
        return Ok(false);
    }
    let mut manifest: PanoramaManifest = serde_json::from_value(value)
        .map_err(|e| format!("Invalid panorama manifest {}: {e}", source_path.display()))?;
    for source in &mut manifest.sources {
        source.views.retain_mut(|view| {
            if let Some(file_name) = selected_names.get(&view.file_name) {
                view.file_name.clone_from(file_name);
                true
            } else {
                false
            }
        });
    }
    manifest.sources.retain(|source| !source.views.is_empty());
    manifest.source_count = manifest.sources.len();

    let remapped = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| format!("Cannot serialize panorama manifest: {e}"))?;
    std::fs::write(output_path, remapped).map_err(|e| {
        format!(
            "Cannot write panorama manifest {}: {e}",
            output_path.display()
        )
    })?;
    Ok(true)
}

pub(crate) fn write_manifest(
    output_dir: &Path,
    sources: Vec<PanoramaSourceManifest>,
) -> Result<PathBuf, String> {
    let manifest = PanoramaManifest {
        format: MANIFEST_FORMAT.to_string(),
        projection: "equirectangular-to-perspective".to_string(),
        horizontal_fov_degrees: 90,
        yaw_convention: "0=front; positive=right".to_string(),
        source_count: sources.len(),
        sources,
    };
    let path = output_dir.join(MANIFEST_FILE_NAME);
    let contents = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| format!("Cannot serialize panorama manifest: {e}"))?;
    std::fs::write(&path, contents)
        .map_err(|e| format!("Cannot write panorama manifest {}: {e}", path.display()))?;
    Ok(path)
}

pub(crate) fn validate_selection_count(count: usize) -> Result<(), String> {
    if count < 2 {
        return Err(
            "Choose at least two panoramas from different positions; one panorama has no translation baseline for 3D reconstruction."
                .into(),
        );
    }
    Ok(())
}

fn validate_equirectangular_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width < 64 || height < 32 {
        return Err("Panorama must be at least 64 × 32 pixels.".into());
    }
    if width != height.saturating_mul(2) {
        return Err(format!(
            "Expected a 2:1 equirectangular panorama; got {width} × {height}."
        ));
    }
    Ok(())
}

/// Maps one square-view camera ray to normalized equirectangular coordinates.
/// The source seam is centered opposite the forward view, so yaw zero samples
/// the middle of the panorama and positive yaw moves toward increasing x.
fn view_ray_to_equirectangular_uv(yaw: f64, local_x: f64, local_y: f64) -> (f64, f64) {
    let inv_len = (local_x * local_x + local_y * local_y + 1.0).sqrt().recip();
    let cos_yaw = yaw.cos();
    let sin_yaw = yaw.sin();
    let x = (local_x * cos_yaw + sin_yaw) * inv_len;
    let y = local_y * inv_len;
    let z = (-local_x * sin_yaw + cos_yaw) * inv_len;
    let longitude = x.atan2(z);
    let latitude = y.clamp(-1.0, 1.0).asin();
    let u = (longitude / std::f64::consts::TAU + 0.5).rem_euclid(1.0);
    let v = (0.5 - latitude / std::f64::consts::PI).clamp(0.0, 1.0);
    (u, v)
}

fn sample_bilinear(image: &RgbImage, u: f64, v: f64) -> Rgb<u8> {
    let width = image.width() as i64;
    let height = image.height() as i64;
    let x = u * width as f64 - 0.5;
    let y = v * height as f64 - 0.5;
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let tx = x - x.floor();
    let ty = y - y.floor();
    let x0 = x0.rem_euclid(width) as u32;
    let x1 = (x0 as i64 + 1).rem_euclid(width) as u32;
    let y0 = y0.clamp(0, height - 1) as u32;
    let y1 = (y.floor() as i64 + 1).clamp(0, height - 1) as u32;

    let p00 = image.get_pixel(x0, y0).0;
    let p10 = image.get_pixel(x1, y0).0;
    let p01 = image.get_pixel(x0, y1).0;
    let p11 = image.get_pixel(x1, y1).0;
    let mut out = [0; 3];
    for channel in 0..3 {
        let top = p00[channel] as f64 * (1.0 - tx) + p10[channel] as f64 * tx;
        let bottom = p01[channel] as f64 * (1.0 - tx) + p11[channel] as f64 * tx;
        out[channel] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
    }
    Rgb(out)
}

pub(crate) fn project_file(
    source: &Path,
    output_dir: &Path,
    panorama_index: usize,
) -> Result<PanoramaSourceManifest, String> {
    let reader = ImageReader::open(source)
        .map_err(|e| format!("Cannot read panorama {}: {e}", source.display()))?
        .with_guessed_format()
        .map_err(|e| {
            format!(
                "Cannot detect panorama format for {}: {e}",
                source.display()
            )
        })?;
    let source_image = reader
        .decode()
        .map_err(|e| format!("Cannot decode panorama {}: {e}", source.display()))?
        .to_rgb8();
    validate_equirectangular_dimensions(source_image.width(), source_image.height())?;

    // A 90-degree perspective face at this size preserves source sampling and
    // keeps each generated view below a predictable memory ceiling.
    let face_size = (source_image.height() / 2).min(MAX_FACE_SIZE);
    let source_file_name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| format!("Panorama has no usable filename: {}", source.display()))?;
    let mut views = Vec::with_capacity(VIEW_HEADINGS.len());
    for (label, yaw) in VIEW_HEADINGS {
        let mut face = RgbImage::new(face_size, face_size);
        for py in 0..face_size {
            let local_y = 1.0 - 2.0 * (py as f64 + 0.5) / face_size as f64;
            for px in 0..face_size {
                let local_x = 2.0 * (px as f64 + 0.5) / face_size as f64 - 1.0;
                let (u, v) = view_ray_to_equirectangular_uv(yaw, local_x, local_y);
                face.put_pixel(px, py, sample_bilinear(&source_image, u, v));
            }
        }

        let file_name = format!("pano-{panorama_index:03}-{label}.jpg");
        let path = output_dir.join(&file_name);
        let mut file = std::fs::File::create(&path)
            .map_err(|e| format!("Cannot create projected view {}: {e}", path.display()))?;
        JpegEncoder::new_with_quality(&mut file, 92)
            .encode_image(&DynamicImage::ImageRgb8(face))
            .map_err(|e| format!("Cannot encode projected view {}: {e}", path.display()))?;
        views.push(ProjectedFaceManifest {
            face: label.to_string(),
            yaw_degrees: yaw.to_degrees().round() as i16,
            file_name,
        });
    }
    Ok(PanoramaSourceManifest {
        source_index: panorama_index,
        source_file_name,
        views,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    fn test_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "instasplatter-{name}-{}-{stamp}",
            std::process::id()
        ))
    }

    #[test]
    fn accepts_only_equirectangular_dimensions() {
        assert!(validate_equirectangular_dimensions(1280, 640).is_ok());
        assert!(validate_equirectangular_dimensions(4000, 2000).is_ok());
        assert!(validate_equirectangular_dimensions(1280, 720).is_err());
        assert!(validate_equirectangular_dimensions(1, 1).is_err());
    }

    #[test]
    fn reconstruction_requires_panorama_positions_with_a_baseline() {
        assert!(validate_selection_count(1).is_err());
        assert!(validate_selection_count(2).is_ok());
    }

    #[test]
    fn view_centres_sample_the_four_cardinal_panorama_headings() {
        let expected = [0.5, 0.75, 0.0, 0.25];
        for (index, expected_u) in expected.iter().enumerate() {
            let yaw = index as f64 * std::f64::consts::FRAC_PI_2;
            let (u, v) = view_ray_to_equirectangular_uv(yaw, 0.0, 0.0);
            let wrapped_error = ((u - *expected_u + 0.5).rem_euclid(1.0) - 0.5).abs();
            assert!(wrapped_error < 1e-9, "heading {index} mapped to u={u}");
            assert!((v - 0.5).abs() < 1e-9, "horizon mapped to v={v}");
        }
    }

    #[test]
    fn projects_one_panorama_into_four_square_perspective_images() {
        let root = test_dir("panorama-projection");
        let output = root.join("projected");
        std::fs::create_dir_all(&output).unwrap();
        let source = root.join("source.png");
        let mut pano = RgbImage::from_pixel(64, 32, Rgb([20, 20, 20]));
        for x in 0..64 {
            let color = match x / 16 {
                0 => Rgb([255, 0, 0]),
                1 => Rgb([0, 255, 0]),
                2 => Rgb([0, 0, 255]),
                _ => Rgb([255, 255, 0]),
            };
            for y in 0..32 {
                pano.put_pixel(x, y, color);
            }
        }
        pano.save(&source).unwrap();

        let panorama = project_file(&source, &output, 0).unwrap();
        assert_eq!(panorama.views.len(), 4);
        for view in &panorama.views {
            let path = output.join(&view.file_name);
            assert!(path.is_file());
            assert_eq!(image::open(path).unwrap().dimensions(), (16, 16));
        }
        let expected_centres = [
            Rgb([0, 0, 255]),
            Rgb([255, 255, 0]),
            Rgb([255, 0, 0]),
            Rgb([0, 255, 0]),
        ];
        for (view, expected) in panorama.views.iter().zip(expected_centres) {
            let image = image::open(output.join(&view.file_name)).unwrap().to_rgb8();
            let actual = image.get_pixel(8, 8);
            for channel in 0..3 {
                assert!(
                    actual[channel].abs_diff(expected[channel]) < 30,
                    "{} center color was {:?}, expected {:?}",
                    view.file_name,
                    actual,
                    expected
                );
            }
        }

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remapping_manifest_keeps_selected_views_and_drops_empty_sources() {
        let root = test_dir("panorama-manifest-remap");
        let staging = root.join("staging");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let sources = vec![
            PanoramaSourceManifest {
                source_index: 0,
                source_file_name: "north.png".into(),
                views: vec![
                    ProjectedFaceManifest {
                        face: "front".into(),
                        yaw_degrees: 0,
                        file_name: "pano-000-front.jpg".into(),
                    },
                    ProjectedFaceManifest {
                        face: "right".into(),
                        yaw_degrees: 90,
                        file_name: "pano-000-right.jpg".into(),
                    },
                ],
            },
            PanoramaSourceManifest {
                source_index: 1,
                source_file_name: "south.png".into(),
                views: vec![ProjectedFaceManifest {
                    face: "front".into(),
                    yaw_degrees: 0,
                    file_name: "pano-001-front.jpg".into(),
                }],
            },
        ];
        let source_manifest = write_manifest(&staging, sources).unwrap();
        let selected_names = std::collections::HashMap::from([(
            "pano-000-front.jpg".to_string(),
            "img_00000.jpg".to_string(),
        )]);

        assert!(remap_manifest(
            &source_manifest,
            &workspace.join(MANIFEST_FILE_NAME),
            &selected_names,
        )
        .unwrap());

        let remapped: PanoramaManifest =
            serde_json::from_slice(&std::fs::read(workspace.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        assert_eq!(remapped.source_count, 1);
        assert_eq!(remapped.sources.len(), 1);
        assert_eq!(remapped.sources[0].source_index, 0);
        assert_eq!(remapped.sources[0].source_file_name, "north.png");
        assert_eq!(
            remapped.sources[0].views,
            vec![ProjectedFaceManifest {
                face: "front".into(),
                yaw_degrees: 0,
                file_name: "img_00000.jpg".into(),
            }]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
