//! Settings system (ROADMAP §7): every setting is optional — `None` means
//! **Auto**, resolved from the hardware profile + preset at job start.

use crate::profiler::{HardwareProfile, Preset};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("InstaSplatter")
}

pub fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// "auto" | "draft" | "eco" | "balanced" | "high" | "max"
    pub preset: Option<String>,

    // ---- Input ----
    pub max_frames: Option<u32>,
    pub max_resolution: Option<u32>,
    /// 0..1 — fraction of blurriest frames to reject.
    pub blur_reject_fraction: Option<f32>,

    // ---- SfM ----
    /// "auto" | "sequential" | "exhaustive"
    pub matcher: Option<String>,
    pub sift_gpu: Option<bool>,

    // ---- Training ----
    pub total_steps: Option<u32>,
    pub max_splats: Option<u32>,
    pub sh_degree: Option<u32>,
    pub refine_every: Option<u32>,
    pub ssim_weight: Option<f32>,
    pub export_every: Option<u32>,

    // ---- Cleanliness / robustness ----
    /// 0 (Detailed) .. 1 (Clean). Scales floater-suppression losses & noise.
    pub strictness: Option<f32>,

    // ---- Output ----
    pub keep_intermediates: Option<bool>,
}

impl Settings {
    pub fn load() -> Settings {
        fs::read_to_string(settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = app_data_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(settings_path(), json).map_err(|e| e.to_string())
    }
}

/// Fully-resolved parameters actually used by a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSettings {
    pub preset: Preset,
    pub max_frames: u32,
    pub max_resolution: u32,
    pub blur_reject_fraction: f32,
    pub matcher: String,
    pub sift_gpu: bool,
    pub total_steps: u32,
    pub max_splats: u32,
    pub sh_degree: u32,
    pub refine_every: u32,
    pub ssim_weight: f32,
    pub export_every: u32,
    pub strictness: f32,
    pub keep_intermediates: bool,
    // Derived floater-suppression knobs (Brush):
    pub opac_loss_weight: f64,
    pub scale_loss_weight: f64,
    pub mean_noise_weight: f64,
}

pub fn resolve(settings: &Settings, profile: &HardwareProfile) -> ResolvedSettings {
    let preset = settings
        .preset
        .as_deref()
        .and_then(Preset::from_str_loose)
        .unwrap_or(profile.auto_preset);
    let p = preset.params();

    let strictness = settings.strictness.unwrap_or(0.5).clamp(0.0, 1.0);
    // Map the Clean↔Detailed slider onto Brush's floater controls.
    // Baselines are Brush defaults; strictness scales them up to ~100x.
    let scale = |base: f64| base * (10f64).powf((strictness as f64 - 0.5) * 4.0);

    ResolvedSettings {
        preset,
        max_frames: settings.max_frames.unwrap_or(p.max_frames),
        max_resolution: settings.max_resolution.unwrap_or(p.max_resolution),
        blur_reject_fraction: settings.blur_reject_fraction.unwrap_or(0.15).clamp(0.0, 0.9),
        matcher: settings.matcher.clone().unwrap_or_else(|| "auto".into()),
        sift_gpu: settings.sift_gpu.unwrap_or(profile.has_cuda),
        total_steps: settings.total_steps.unwrap_or(p.total_steps),
        max_splats: settings.max_splats.unwrap_or(p.max_splats),
        sh_degree: settings.sh_degree.unwrap_or(p.sh_degree),
        refine_every: settings.refine_every.unwrap_or(p.refine_every),
        ssim_weight: settings.ssim_weight.unwrap_or(0.2),
        export_every: settings.export_every.unwrap_or(p.export_every),
        strictness,
        keep_intermediates: settings.keep_intermediates.unwrap_or(false),
        opac_loss_weight: scale(1e-9),
        scale_loss_weight: scale(1e-8),
        mean_noise_weight: 40.0 * (0.5 + strictness as f64),
    }
}
