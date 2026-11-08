//! Stages an explicit multi-file image selection as one reconstruction input.
//! The pipeline accepts a video or directory, so the picker copies only the
//! chosen images into a short-lived directory instead of silently ingesting
//! every neighbouring file.

use crate::settings::app_data_dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const IMAGE_EXTS: [&str; 8] = ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "heic"];

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn selections_dir() -> PathBuf {
    app_data_dir().join("capture-selections")
}

fn prune_old_selections(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let max_age = std::time::Duration::from_secs(24 * 60 * 60);
    for entry in entries.flatten() {
        let path = entry.path();
        let old = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|age| age > max_age)
            .unwrap_or(false);
        if old {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

pub fn stage(paths: &[String]) -> Result<PathBuf, String> {
    if paths.is_empty() {
        return Err("Choose at least one image.".into());
    }

    let files: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    for path in &files {
        if !path.is_file() {
            return Err(format!("Image does not exist: {}", path.display()));
        }
        if !is_image(path) {
            return Err(format!("Unsupported image type: {}", path.display()));
        }
    }

    let root = selections_dir();
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("Cannot create image staging folder: {e}"))?;
    prune_old_selections(&root);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let staged = root.join(format!("selection-{stamp}"));
    std::fs::create_dir_all(&staged).map_err(|e| format!("Cannot stage selected images: {e}"))?;

    for (index, source) in files.iter().enumerate() {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("Image has no usable filename: {}", source.display()))?;
        let destination = staged.join(format!("{index:05}-{name}"));
        if let Err(error) = std::fs::copy(source, &destination) {
            let _ = std::fs::remove_dir_all(&staged);
            return Err(format!("Could not stage {}: {error}", source.display()));
        }
    }

    Ok(staged)
}

/// Convert an explicit set of local equirectangular panoramas into the
/// perspective image folder accepted by the reconstruction pipeline.
pub fn stage_panoramas(paths: &[String]) -> Result<PathBuf, String> {
    stage_panoramas_in_root(paths, &selections_dir())
}

fn stage_panoramas_in_root(paths: &[String], root: &Path) -> Result<PathBuf, String> {
    crate::panorama::validate_selection_count(paths.len())?;

    let files: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    for path in &files {
        if !path.is_file() {
            return Err(format!("Panorama does not exist: {}", path.display()));
        }
        if !is_image(path) {
            return Err(format!("Unsupported panorama type: {}", path.display()));
        }
    }
    let mut unique_sources = HashSet::with_capacity(files.len());
    for path in &files {
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("Cannot resolve panorama {}: {e}", path.display()))?;
        if !unique_sources.insert(canonical) {
            return Err(
                "The same panorama file was selected more than once; choose panoramas from distinct positions."
                    .into(),
            );
        }
    }

    std::fs::create_dir_all(root)
        .map_err(|e| format!("Cannot create panorama staging folder: {e}"))?;
    prune_old_selections(root);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let staged = root.join(format!("panorama-{stamp}"));
    std::fs::create_dir_all(&staged)
        .map_err(|e| format!("Cannot create panorama staging folder: {e}"))?;

    let mut sources = Vec::with_capacity(files.len());
    for (index, source) in files.iter().enumerate() {
        match crate::panorama::project_file(source, &staged, index) {
            Ok(manifest_source) => sources.push(manifest_source),
            Err(error) => {
                let _ = std::fs::remove_dir_all(&staged);
                return Err(error);
            }
        }
    }
    if let Err(error) = crate::panorama::write_manifest(&staged, sources) {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(error);
    }

    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use serde_json::{json, Value};

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "instasplatter-{name}-{}-{stamp}",
            std::process::id()
        ))
    }

    fn write_panorama(path: &Path, width: u32, height: u32) {
        RgbImage::from_pixel(width, height, Rgb([20, 40, 60]))
            .save(path)
            .unwrap();
    }

    fn stage_and_read_manifest(paths: &[String], staging_root: &Path) -> Option<Vec<u8>> {
        let staged = stage_panoramas_in_root(paths, staging_root).ok()?;
        let manifest = std::fs::read(staged.join("panorama-manifest.json")).ok();
        std::fs::remove_dir_all(staged).unwrap();
        manifest
    }

    #[test]
    fn image_extension_check_is_case_insensitive() {
        assert!(is_image(Path::new("frame.JPG")));
        assert!(is_image(Path::new("frame.tiff")));
        assert!(!is_image(Path::new("clip.mp4")));
    }

    #[test]
    fn staged_manifest_is_deterministic_and_maps_each_source_to_four_faces() {
        let root = temp_dir("panorama-manifest");
        std::fs::create_dir_all(&root).unwrap();
        let north = root.join("north.png");
        let south = root.join("south.png");
        write_panorama(&north, 64, 32);
        write_panorama(&south, 64, 32);
        let paths = vec![
            north.to_string_lossy().into_owned(),
            south.to_string_lossy().into_owned(),
        ];

        let staging_root = root.join("staging");
        let first = stage_and_read_manifest(&paths, &staging_root);
        let second = stage_and_read_manifest(&paths, &staging_root);
        let expected = json!({
            "format": "instasplatter-panorama-projection-v1",
            "projection": "equirectangular-to-perspective",
            "horizontalFovDegrees": 90,
            "yawConvention": "0=front; positive=right",
            "sourceCount": 2,
            "sources": [
                {
                    "sourceIndex": 0,
                    "sourceFileName": "north.png",
                    "views": [
                        {"face": "front", "yawDegrees": 0, "fileName": "pano-000-front.jpg"},
                        {"face": "right", "yawDegrees": 90, "fileName": "pano-000-right.jpg"},
                        {"face": "back", "yawDegrees": 180, "fileName": "pano-000-back.jpg"},
                        {"face": "left", "yawDegrees": 270, "fileName": "pano-000-left.jpg"}
                    ]
                },
                {
                    "sourceIndex": 1,
                    "sourceFileName": "south.png",
                    "views": [
                        {"face": "front", "yawDegrees": 0, "fileName": "pano-001-front.jpg"},
                        {"face": "right", "yawDegrees": 90, "fileName": "pano-001-right.jpg"},
                        {"face": "back", "yawDegrees": 180, "fileName": "pano-001-back.jpg"},
                        {"face": "left", "yawDegrees": 270, "fileName": "pano-001-left.jpg"}
                    ]
                }
            ]
        });

        let first = first.expect("staged panoramas should include their manifest");
        assert_eq!(
            first,
            second.expect("manifest should be written on every stage")
        );
        assert_eq!(serde_json::from_slice::<Value>(&first).unwrap(), expected);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_same_local_panorama_cannot_satisfy_the_two_position_requirement() {
        let root = temp_dir("panorama-duplicate");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("single.png");
        write_panorama(&source, 64, 32);
        let original = std::fs::read(&source).unwrap();
        let path = source.to_string_lossy().into_owned();

        let result = stage_panoramas_in_root(&[path.clone(), path], &root.join("staging"));
        if let Ok(staged) = &result {
            std::fs::remove_dir_all(staged).unwrap();
        }
        assert!(
            result
                .as_ref()
                .is_err_and(|message| message.contains("same panorama file")),
            "repeated source path should be rejected, got {result:?}"
        );
        assert_eq!(std::fs::read(&source).unwrap(), original);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_panorama_dimensions_are_rejected_without_changing_sources() {
        let root = temp_dir("panorama-invalid");
        std::fs::create_dir_all(&root).unwrap();
        let valid = root.join("valid.png");
        let invalid = root.join("not-equirectangular.png");
        write_panorama(&valid, 64, 32);
        write_panorama(&invalid, 64, 40);
        let valid_before = std::fs::read(&valid).unwrap();
        let invalid_before = std::fs::read(&invalid).unwrap();

        let result = stage_panoramas_in_root(
            &[
                valid.to_string_lossy().into_owned(),
                invalid.to_string_lossy().into_owned(),
            ],
            &root.join("staging"),
        );
        if let Ok(staged) = &result {
            std::fs::remove_dir_all(staged).unwrap();
        }
        assert!(
            result
                .as_ref()
                .is_err_and(|message| message.contains("Expected a 2:1 equirectangular panorama")),
            "invalid panorama dimensions should be rejected, got {result:?}"
        );
        assert_eq!(std::fs::read(&valid).unwrap(), valid_before);
        assert_eq!(std::fs::read(&invalid).unwrap(), invalid_before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn panorama_staging_uses_only_the_injected_root() {
        let root = temp_dir("panorama-staging-root");
        let inputs = root.join("inputs");
        let staging_root = root.join("isolated-staging");
        std::fs::create_dir_all(&inputs).unwrap();
        let north = inputs.join("north.png");
        let south = inputs.join("south.png");
        write_panorama(&north, 64, 32);
        write_panorama(&south, 64, 32);
        let paths = [
            north.to_string_lossy().into_owned(),
            south.to_string_lossy().into_owned(),
        ];

        let staged = stage_panoramas_in_root(&paths, &staging_root).unwrap();
        assert!(staged.starts_with(&staging_root));
        assert!(staged.join(crate::panorama::MANIFEST_FILE_NAME).is_file());
        assert_eq!(
            std::fs::read_dir(&staging_root).unwrap().count(),
            1,
            "only the generated staging folder should be created under the injected root"
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
