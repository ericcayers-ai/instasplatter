//! Stages an explicit multi-file image selection as one reconstruction input.
//! The pipeline accepts a video or directory, so the picker copies only the
//! chosen images into a short-lived directory instead of silently ingesting
//! every neighbouring file.

use crate::settings::app_data_dir;
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
    std::fs::create_dir_all(&root).map_err(|e| format!("Cannot create image staging folder: {e}"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_extension_check_is_case_insensitive() {
        assert!(is_image(Path::new("frame.JPG")));
        assert!(is_image(Path::new("frame.tiff")));
        assert!(!is_image(Path::new("clip.mp4")));
    }
}
