//! Settings system (ROADMAP §7): every setting is optional. `None` means
//! **Auto**, resolved from the hardware profile and preset at job start.
//!
//! v0.3 defaults lean toward high quality: progressive resolution, mip filter,
//! dense init, and stronger floater suppression are ON unless the user opts out.

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
    /// 0..1, the fraction of blurriest frames to reject.
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

    /// Coarse-to-fine resolution schedule (DashGaussian). Default ON in v0.3.
    pub progressive_resolution: Option<bool>,
    /// Mip-Splatting 3D smoothing filter. Default ON in v0.3.
    pub mip_filter: Option<bool>,
    /// Use the native incremental solver instead of a blocking COLMAP pass.
    pub live_init: Option<bool>,
    /// Seed Brush from dense MVS / neural pointmaps. Default ON.
    pub dense_init: Option<bool>,
    /// Prefer installed neural densifiers (DAV2 / VGGT commercial) when present.
    pub use_neural_init: Option<bool>,
    /// Allow non-commercial research sidecars (VGGT-NC, etc.). Default OFF.
    pub allow_research_sidecars: Option<bool>,
    /// Run NVIDIA Fixer / Difix polish after training when a launcher is installed.
    /// Default ON (no-op until Fixer is present).
    pub post_polish: Option<bool>,

    // ---- Cleanliness / robustness ----
    /// 0 (Detailed) .. 1 (Clean). Scales floater-suppression losses & noise.
    pub strictness: Option<f32>,

    // ---- Output ----
    /// "ply" | "splat" | "spz". PLY is the default.
    pub export_format: Option<String>,
    pub keep_intermediates: Option<bool>,
}

impl Settings {
    pub fn load() -> Settings {
        fs::read_to_string(settings_path())
            .ok()
            .map(|s| s.trim_start_matches('\u{feff}').to_string())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = app_data_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        // No UTF-8 BOM: serde_json rejects BOM on load, and some editors write one.
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
    pub progressive_resolution: bool,
    pub mip_filter: bool,
    pub live_init: bool,
    pub dense_init: bool,
    pub use_neural_init: bool,
    pub allow_research_sidecars: bool,
    pub post_polish: bool,
    pub strictness: f32,
    pub export_format: String,
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

    // Bias slightly cleaner than v0.2 (0.5 → 0.55) to suppress needle floaters
    // without the AbsGS-scale opac/scale L1 wiping the cloud mid-train.
    let strictness = settings.strictness.unwrap_or(0.55).clamp(0.0, 1.0);
    // Map the Clean↔Detailed slider onto Brush's floater controls.
    // Baselines are raised vs Brush defaults; strictness scales them further.
    let scale = |base: f64| base * (10f64).powf((strictness as f64 - 0.5) * 4.0);

    ResolvedSettings {
        preset,
        max_frames: settings.max_frames.unwrap_or(p.max_frames),
        max_resolution: settings.max_resolution.unwrap_or(p.max_resolution),
        // Milder rejection so coverage is not over-thinned (was 0.15).
        blur_reject_fraction: settings.blur_reject_fraction.unwrap_or(0.08).clamp(0.0, 0.9),
        matcher: settings.matcher.clone().unwrap_or_else(|| "auto".into()),
        sift_gpu: settings.sift_gpu.unwrap_or(profile.has_cuda),
        total_steps: settings.total_steps.unwrap_or(p.total_steps),
        max_splats: settings.max_splats.unwrap_or(p.max_splats),
        sh_degree: settings.sh_degree.unwrap_or(p.sh_degree),
        refine_every: settings.refine_every.unwrap_or(p.refine_every),
        ssim_weight: settings.ssim_weight.unwrap_or(0.25),
        export_every: settings.export_every.unwrap_or(p.export_every),
        progressive_resolution: settings.progressive_resolution.unwrap_or(true),
        mip_filter: settings.mip_filter.unwrap_or(true),
        live_init: settings.live_init.unwrap_or(false),
        dense_init: settings.dense_init.unwrap_or(true),
        use_neural_init: settings.use_neural_init.unwrap_or(true),
        allow_research_sidecars: settings.allow_research_sidecars.unwrap_or(false),
        post_polish: settings.post_polish.unwrap_or(true),
        strictness,
        export_format: settings
            .export_format
            .clone()
            .filter(|f| crate::splat::export::Format::parse(f).is_some())
            .unwrap_or_else(|| "ply".into()),
        keep_intermediates: settings.keep_intermediates.unwrap_or(false),
        // AbsGS-style opacity/scale L1 — stronger than stock Brush, softer than
        // early v0.3 (5e-8/5e-7 at strictness 0.62 collapsed some scenes).
        opac_loss_weight: scale(2e-8),
        scale_loss_weight: scale(2e-7),
        mean_noise_weight: 45.0 * (0.5 + strictness as f64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::GpuVendor;

    fn profile() -> HardwareProfile {
        HardwareProfile {
            gpu_name: "Test GPU".into(),
            gpu_vendor: GpuVendor::Nvidia,
            vram_mb: 8192,
            has_cuda: true,
            cpu_name: "Test CPU".into(),
            cpu_threads: 8,
            ram_mb: 32000,
            auto_preset: Preset::High,
        }
    }

    #[test]
    fn unset_settings_resolve_to_the_hardware_preset() {
        let r = resolve(&Settings::default(), &profile());
        assert_eq!(r.preset, Preset::High);
        assert_eq!(r.total_steps, Preset::High.params().total_steps);
        assert_eq!(r.export_format, "ply");
        // v0.3: quality features default ON.
        assert!(r.progressive_resolution);
        assert!(r.mip_filter);
        assert!(r.dense_init);
        assert!(r.use_neural_init);
        assert!(!r.allow_research_sidecars);
        assert!(r.post_polish);
        assert!(!r.live_init);
        assert!((r.blur_reject_fraction - 0.08).abs() < 1e-6);
    }

    #[test]
    fn an_explicit_preset_overrides_the_profile() {
        let s = Settings {
            preset: Some("draft".into()),
            ..Default::default()
        };
        assert_eq!(resolve(&s, &profile()).preset, Preset::Draft);
    }

    #[test]
    fn an_unknown_export_format_falls_back_to_ply() {
        let s = Settings {
            export_format: Some("obj".into()),
            ..Default::default()
        };
        assert_eq!(resolve(&s, &profile()).export_format, "ply");
        let s = Settings {
            export_format: Some("spz".into()),
            ..Default::default()
        };
        assert_eq!(resolve(&s, &profile()).export_format, "spz");
    }

    #[test]
    fn strictness_is_clamped_and_scales_the_floater_losses() {
        let low = resolve(
            &Settings {
                strictness: Some(-5.0),
                ..Default::default()
            },
            &profile(),
        );
        let high = resolve(
            &Settings {
                strictness: Some(5.0),
                ..Default::default()
            },
            &profile(),
        );
        assert_eq!(low.strictness, 0.0);
        assert_eq!(high.strictness, 1.0);
        assert!(high.opac_loss_weight > low.opac_loss_weight);
        assert!(high.mean_noise_weight > low.mean_noise_weight);
    }

    #[test]
    fn load_tolerates_a_utf8_bom() {
        let dir = app_data_dir();
        let _ = fs::create_dir_all(&dir);
        let path = settings_path();
        let backup = fs::read(&path).ok();
        let body = "\u{feff}{\"preset\":\"draft\",\"maxFrames\":42}";
        fs::write(&path, body).unwrap();
        let s = Settings::load();
        match backup {
            Some(b) => {
                let _ = fs::write(&path, b);
            }
            None => {
                let _ = fs::remove_file(&path);
            }
        }
        assert_eq!(s.preset.as_deref(), Some("draft"));
        assert_eq!(s.max_frames, Some(42));
    }
}
