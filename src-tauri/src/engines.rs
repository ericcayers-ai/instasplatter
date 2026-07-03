//! Engine management: locate / download COLMAP and Brush binaries
//! (ROADMAP §2 engine layer, §12 first-run download decision).

use crate::profiler::hidden_command;
use crate::settings::app_data_dir;
use futures_util::StreamExt;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tauri::Emitter;

const BRUSH_URL: &str = "https://github.com/ArthurBrussee/brush/releases/download/v0.3.0/brush-app-x86_64-pc-windows-msvc.zip";
const COLMAP_CUDA_URL: &str =
    "https://github.com/colmap/colmap/releases/download/4.1.0/colmap-x64-windows-cuda.zip";
const COLMAP_NOCUDA_URL: &str =
    "https://github.com/colmap/colmap/releases/download/4.1.0/colmap-x64-windows-nocuda.zip";

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
    // Prefer a bundled copy if present, else rely on PATH.
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

async fn download_and_extract(
    app: &tauri::AppHandle,
    engine: &str,
    url: &str,
    dest: &PathBuf,
    strip_root: bool,
) -> Result<(), String> {
    let tmp_zip = engines_dir().join(format!("{engine}.zip.part"));
    fs::create_dir_all(engines_dir()).map_err(|e| e.to_string())?;

    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("download failed: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = fs::File::create(&tmp_zip).map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit > 2_000_000 {
            last_emit = downloaded;
            let _ = app.emit(
                "engine://download",
                DownloadProgress {
                    engine: engine.into(),
                    downloaded,
                    total,
                    phase: "downloading".into(),
                },
            );
        }
    }
    drop(file);

    let _ = app.emit(
        "engine://download",
        DownloadProgress {
            engine: engine.into(),
            downloaded,
            total,
            phase: "extracting".into(),
        },
    );

    // Extract (blocking task — zip crate is sync).
    let tmp_zip2 = tmp_zip.clone();
    let dest2 = dest.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let f = fs::File::open(&tmp_zip2).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let raw = match entry.enclosed_name() {
                Some(p) => p.to_path_buf(),
                None => continue,
            };
            // Optionally strip the top-level folder from archive paths.
            let rel: PathBuf = if strip_root {
                let mut comps = raw.components();
                comps.next();
                comps.as_path().to_path_buf()
            } else {
                raw
            };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let out_path = dest2.join(rel);
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
    })
    .await
    .map_err(|e| e.to_string())??;

    let _ = fs::remove_file(&tmp_zip);
    let _ = app.emit(
        "engine://download",
        DownloadProgress {
            engine: engine.into(),
            downloaded,
            total,
            phase: "done".into(),
        },
    );
    Ok(())
}

/// Ensure COLMAP + Brush exist locally, downloading them on first use.
pub async fn ensure_engines(app: tauri::AppHandle, has_cuda: bool) -> Result<EngineStatus, String> {
    if !brush_exe().exists() {
        download_and_extract(&app, "brush", BRUSH_URL, &engines_dir().join("brush"), false)
            .await?;
    }
    if !colmap_exe().exists() {
        let url = if has_cuda {
            COLMAP_CUDA_URL
        } else {
            COLMAP_NOCUDA_URL
        };
        download_and_extract(&app, "colmap", url, &engines_dir().join("colmap"), false).await?;
        // Some archives nest a root dir; normalize if bin/colmap.exe is missing.
        if !colmap_exe().exists() {
            let root = engines_dir().join("colmap");
            if let Ok(entries) = fs::read_dir(&root) {
                for e in entries.flatten() {
                    let candidate = e.path().join("bin").join("colmap.exe");
                    if candidate.exists() {
                        // Move nested contents up one level.
                        let nested = e.path();
                        let tmp = engines_dir().join("colmap_nested_tmp");
                        let _ = fs::rename(&nested, &tmp);
                        let _ = fs::remove_dir_all(&root);
                        let _ = fs::rename(&tmp, &root);
                        break;
                    }
                }
            }
        }
    }
    Ok(status())
}
