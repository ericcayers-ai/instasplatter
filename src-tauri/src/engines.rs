//! Engine management: locate, download and verify the COLMAP and Brush
//! binaries (ROADMAP-V2 1.7).
//!
//! Downloads land in a `.part` file, are checked against a pinned SHA-256,
//! extracted into a staging directory and only then moved into place. A run
//! that is interrupted at any point leaves either the previous good install
//! or nothing at all, never a half-extracted engine that fails later with a
//! confusing error.

use crate::profiler::hidden_command;
use crate::settings::app_data_dir;
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::Emitter;

/// A downloadable engine archive with the digest published on its release.
struct Archive {
    url: &'static str,
    sha256: &'static str,
    /// Path inside the engine directory that must exist after extraction.
    sentinel: &'static [&'static str],
}

const BRUSH: Archive = Archive {
    url: "https://github.com/ArthurBrussee/brush/releases/download/v0.3.0/brush-app-x86_64-pc-windows-msvc.zip",
    sha256: "b68e3e9cf052d51bf3ee30776fa5a364de7f2ba13b58443128ff797bb7bcfcd6",
    sentinel: &["brush_app.exe"],
};

const COLMAP_CUDA: Archive = Archive {
    url: "https://github.com/colmap/colmap/releases/download/4.1.0/colmap-x64-windows-cuda.zip",
    sha256: "ccd2f8c5b44f3e0ce645170d6abad30ff763ede97eeb0e6e23af1993e624e64b",
    sentinel: &["bin", "colmap.exe"],
};

const COLMAP_NOCUDA: Archive = Archive {
    url: "https://github.com/colmap/colmap/releases/download/4.1.0/colmap-x64-windows-nocuda.zip",
    sha256: "dc8179bb4f3f48edec683bcec7627176b66e53a33ef0e2aa98d487f45873af5f",
    sentinel: &["bin", "colmap.exe"],
};

pub fn engines_dir() -> PathBuf {
    app_data_dir().join("engines")
}

pub fn brush_exe() -> PathBuf {
    engines_dir().join("brush").join("brush_app.exe")
}

pub fn colmap_exe() -> PathBuf {
    engines_dir().join("colmap").join("bin").join("colmap.exe")
}

pub fn ffmpeg_exe() -> String {
    let bundled = engines_dir().join("ffmpeg").join("ffmpeg.exe");
    if bundled.exists() {
        bundled.to_string_lossy().into_owned()
    } else {
        "ffmpeg".to_string()
    }
}

pub fn ffprobe_exe() -> String {
    let bundled = engines_dir().join("ffmpeg").join("ffprobe.exe");
    if bundled.exists() {
        bundled.to_string_lossy().into_owned()
    } else {
        "ffprobe".to_string()
    }
}

fn ffmpeg_available() -> bool {
    hidden_command(&ffmpeg_exe())
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub colmap: bool,
    pub brush: bool,
    pub ffmpeg: bool,
}

pub fn status() -> EngineStatus {
    EngineStatus {
        colmap: colmap_exe().exists(),
        brush: brush_exe().exists(),
        ffmpeg: ffmpeg_available(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    engine: String,
    downloaded: u64,
    total: u64,
    phase: String,
}

fn emit(app: &tauri::AppHandle, engine: &str, downloaded: u64, total: u64, phase: &str) {
    let _ = app.emit(
        "engine://download",
        DownloadProgress {
            engine: engine.into(),
            downloaded,
            total,
            phase: phase.into(),
        },
    );
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Remove a path, tolerating the "already gone" case.
fn remove_any(p: &Path) {
    if p.is_dir() {
        let _ = fs::remove_dir_all(p);
    } else {
        let _ = fs::remove_file(p);
    }
}

/// Download `archive` to a temporary file, hashing as we stream, and return
/// its path. The partial file is removed if anything goes wrong.
async fn download_verified(
    app: &tauri::AppHandle,
    engine: &str,
    archive: &Archive,
) -> Result<PathBuf, String> {
    fs::create_dir_all(engines_dir()).map_err(|e| format!("Cannot create the engines folder: {e}"))?;
    let tmp_zip = engines_dir().join(format!("{engine}.zip.part"));
    // A leftover partial from a previous interrupted run is never reused: we
    // cannot know how much of it is valid.
    remove_any(&tmp_zip);

    let resp = reqwest::get(archive.url)
        .await
        .map_err(|e| format!("Could not reach the download server: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Download of {engine} failed with HTTP {}.",
            resp.status()
        ));
    }
    let total = resp.content_length().unwrap_or(0);

    let mut file = fs::File::create(&tmp_zip).map_err(|e| format!("Cannot write {engine}: {e}"))?;
    let mut hasher = Sha256::new();
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = 0u64;

    let result = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("The download was interrupted: {e}"))?;
            file.write_all(&chunk)
                .map_err(|e| format!("Cannot write {engine}: {e}"))?;
            hasher.update(&chunk);
            downloaded += chunk.len() as u64;
            if downloaded - last_emit > 2_000_000 {
                last_emit = downloaded;
                emit(app, engine, downloaded, total, "downloading");
            }
        }
        file.flush().map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    }
    .await;

    drop(file);
    if let Err(e) = result {
        remove_any(&tmp_zip);
        return Err(e);
    }

    if total > 0 && downloaded != total {
        remove_any(&tmp_zip);
        return Err(format!(
            "The {engine} download stopped early ({downloaded} of {total} bytes). Check your connection and try again."
        ));
    }

    emit(app, engine, downloaded, total, "verifying");
    let digest = hex(&hasher.finalize());
    if digest != archive.sha256 {
        remove_any(&tmp_zip);
        return Err(format!(
            "The downloaded {engine} archive did not match its published checksum. \
             Expected {}, got {digest}. The file may be corrupt or tampered with.",
            archive.sha256
        ));
    }

    Ok(tmp_zip)
}

/// Extract `zip` into `staging`. Rejects entries that escape the destination.
fn extract_zip(zip_path: &Path, staging: &Path) -> Result<(), String> {
    let f = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| format!("The archive is not readable: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        // `enclosed_name` returns None for absolute paths and `..` traversal.
        let rel = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out_path = staging.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Some archives wrap everything in a single top-level folder. If the sentinel
/// is not at the root, look one level down and lift that folder up.
fn normalize_root(staging: &Path, sentinel: &[&str]) -> Result<(), String> {
    let mut direct = staging.to_path_buf();
    for part in sentinel {
        direct = direct.join(part);
    }
    if direct.exists() {
        return Ok(());
    }

    let entries: Vec<PathBuf> = fs::read_dir(staging)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    for nested in entries {
        let mut candidate = nested.clone();
        for part in sentinel {
            candidate = candidate.join(part);
        }
        if candidate.exists() {
            let lifted = staging.with_extension("lifted");
            remove_any(&lifted);
            fs::rename(&nested, &lifted).map_err(|e| e.to_string())?;
            remove_any(staging);
            fs::rename(&lifted, staging).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err(format!(
        "The archive did not contain {}.",
        sentinel.join("/")
    ))
}

/// Download, verify, extract and atomically install one engine.
async fn install(app: &tauri::AppHandle, engine: &str, archive: &Archive) -> Result<(), String> {
    let dest = engines_dir().join(engine);
    let staging = engines_dir().join(format!("{engine}.staging"));

    let zip_path = download_verified(app, engine, archive).await?;

    emit(app, engine, 0, 0, "extracting");
    remove_any(&staging);
    let staging2 = staging.clone();
    let zip2 = zip_path.clone();
    let sentinel = archive.sentinel;
    let extracted = tokio::task::spawn_blocking(move || -> Result<(), String> {
        fs::create_dir_all(&staging2).map_err(|e| e.to_string())?;
        extract_zip(&zip2, &staging2)?;
        normalize_root(&staging2, sentinel)
    })
    .await
    .map_err(|e| e.to_string())?;

    remove_any(&zip_path);
    if let Err(e) = extracted {
        remove_any(&staging);
        return Err(format!("Could not unpack {engine}: {e}"));
    }

    // Swap the verified staging directory into place last, so a crash before
    // this point never leaves a broken install behind.
    remove_any(&dest);
    fs::rename(&staging, &dest).map_err(|e| {
        remove_any(&staging);
        format!("Could not install {engine}: {e}")
    })?;

    emit(app, engine, 0, 0, "done");
    Ok(())
}

/// Ensure COLMAP and Brush exist locally, downloading them on first use.
/// An engine folder that exists but is missing its executable is treated as
/// absent and reinstalled.
pub async fn ensure_engines(app: tauri::AppHandle, has_cuda: bool) -> Result<EngineStatus, String> {
    if !brush_exe().exists() {
        install(&app, "brush", &BRUSH).await?;
    }
    if !colmap_exe().exists() {
        let archive = if has_cuda { &COLMAP_CUDA } else { &COLMAP_NOCUDA };
        install(&app, "colmap", archive).await?;
    }

    let st = status();
    if !st.brush || !st.colmap {
        return Err(
            "The reconstruction engines are still missing after installation. \
             Check that antivirus software is not removing them."
                .into(),
        );
    }
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encodes_lowercase_fixed_width() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff, 0xa5]), "000fffa5");
    }

    #[test]
    fn pinned_digests_are_lowercase_sha256() {
        for a in [&BRUSH, &COLMAP_CUDA, &COLMAP_NOCUDA] {
            assert_eq!(a.sha256.len(), 64, "{}", a.url);
            assert!(
                a.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{}",
                a.url
            );
            assert!(!a.sentinel.is_empty());
        }
    }

    #[test]
    fn normalize_root_lifts_a_nested_folder() {
        let base = std::env::temp_dir().join("instasplatter_normalize_test");
        let _ = fs::remove_dir_all(&base);
        let staging = base.join("colmap.staging");
        fs::create_dir_all(staging.join("colmap-x64").join("bin")).unwrap();
        fs::write(staging.join("colmap-x64").join("bin").join("colmap.exe"), b"x").unwrap();

        normalize_root(&staging, &["bin", "colmap.exe"]).unwrap();
        assert!(staging.join("bin").join("colmap.exe").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn normalize_root_accepts_a_flat_archive() {
        let base = std::env::temp_dir().join("instasplatter_normalize_flat_test");
        let _ = fs::remove_dir_all(&base);
        let staging = base.join("brush.staging");
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("brush_app.exe"), b"x").unwrap();
        normalize_root(&staging, &["brush_app.exe"]).unwrap();
        assert!(staging.join("brush_app.exe").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn normalize_root_reports_a_missing_sentinel() {
        let base = std::env::temp_dir().join("instasplatter_normalize_missing_test");
        let _ = fs::remove_dir_all(&base);
        let staging = base.join("brush.staging");
        fs::create_dir_all(staging.join("junk")).unwrap();
        let err = normalize_root(&staging, &["brush_app.exe"]).unwrap_err();
        assert!(err.contains("brush_app.exe"), "{err}");
        let _ = fs::remove_dir_all(&base);
    }
}
