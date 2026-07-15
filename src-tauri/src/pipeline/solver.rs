//! Capture-aware camera / densify / polish policy (Standard vs Experimental).
//!
//! Standard stays commercially redistributable. Experimental requires an
//! explicit license ack and unlocks NC research sidecars via capture-profile
//! routing (see `experimental.rs`) — never a blind merge of all engines.
//!
//! Pose routing scores hypotheses instead of fail-down-only lists. Accepted
//! results always land in one canonical COLMAP/ENU frame before refinement.

use super::experimental;
use super::sidecars::SidecarStatus;
use crate::colmap::Model;
use crate::settings::ResolvedSettings;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Capture shape used to order pose / densify candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureProfile {
    OrderedVideo,
    /// Long sequential capture — streaming / SLAM research adapters.
    LongVideo,
    UnorderedPhotos,
    GpsRtkDrone,
    FisheyeOrRollingShutter,
    DynamicScene,
    /// Large aerial / urban — partition/LOD adapters + neural pose repairs.
    LargeScene,
}

/// Detect a capture profile from frame count + lightweight workspace hints.
/// GPS telemetry / EXIF priors (when present under workspace) unlock the drone profile.
pub fn detect_capture_profile(
    images_dir: &Path,
    workspace: &Path,
    experimental: bool,
) -> CaptureProfile {
    let n = count_images(images_dir);
    let has_gps = has_pose_priors(workspace) || has_exif_gps_hint(images_dir);
    let sequential_names = looks_sequential(images_dir);
    let large = n >= 400;
    let aerial_hint = workspace.join("capture_hints").join("aerial").exists()
        || workspace.join("capture_hints").join("urban").exists();

    if has_gps {
        return CaptureProfile::GpsRtkDrone;
    }
    if workspace.join("capture_hints").join("fisheye").exists()
        || workspace.join("capture_hints").join("rolling_shutter").exists()
    {
        return CaptureProfile::FisheyeOrRollingShutter;
    }
    if experimental && workspace.join("capture_hints").join("dynamic").exists() {
        return CaptureProfile::DynamicScene;
    }
    // Large aerial/urban before generic "lots of frames".
    if large && (has_gps || aerial_hint || !sequential_names) {
        return CaptureProfile::LargeScene;
    }
    if sequential_names && n >= 120 {
        return CaptureProfile::LongVideo;
    }
    if large {
        return CaptureProfile::LargeScene;
    }
    if sequential_names && n >= 40 {
        return CaptureProfile::OrderedVideo;
    }
    CaptureProfile::UnorderedPhotos
}

fn count_images(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| {
                            matches!(
                                x.to_ascii_lowercase().as_str(),
                                "jpg" | "jpeg" | "png" | "webp" | "tif" | "tiff" | "bmp"
                            )
                        })
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

fn looks_sequential(dir: &Path) -> bool {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    if names.len() < 8 {
        return false;
    }
    // Consecutive numeric stems with small gaps ⇒ ordered video / frame dump.
    let nums: Vec<i64> = names
        .iter()
        .filter_map(|n| {
            let stem = Path::new(n).file_stem()?.to_str()?;
            let digits: String = stem.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() >= 3 {
                digits.parse().ok()
            } else {
                None
            }
        })
        .collect();
    if nums.len() < names.len() * 3 / 4 {
        return false;
    }
    let mut gaps = 0usize;
    for w in nums.windows(2) {
        if w[1] > w[0] && (w[1] - w[0]) <= 5 {
            gaps += 1;
        }
    }
    gaps as f32 / (nums.len().saturating_sub(1).max(1) as f32) > 0.7
}

/// Pose-prior files written by geo-registration / EXIF ingestion.
pub fn has_pose_priors(workspace: &Path) -> bool {
    let sparse = workspace.join("sparse");
    [
        workspace.join("pose_priors.txt"),
        workspace.join("image_priors.txt"),
        sparse.join("pose_priors.txt"),
        sparse.join("0").join("pose_priors.txt"),
        workspace.join("geo").join("pose_priors.txt"),
    ]
    .iter()
    .any(|p| p.exists())
}

fn has_exif_gps_hint(images_dir: &Path) -> bool {
    // Lightweight marker from ingest / geo agents — avoid parsing every EXIF here.
    images_dir.join(".gps_present").exists()
        || images_dir
            .parent()
            .map(|p| p.join("capture_hints").join("gps").exists())
            .unwrap_or(false)
}

/// Human label for status chips / banner.
pub fn camera_chip(solver: &str) -> String {
    match solver {
        "vggt-omega" => "Cameras: VGGT-Ω".into(),
        "mast3r" => "Cameras: MASt3R-SfM".into(),
        "dust3r" => "Cameras: DUSt3R".into(),
        "pi3x" => "Cameras: Pi3X".into(),
        "stream-vggt" => "Cameras: StreamVGGT".into(),
        "vggt-long" => "Cameras: VGGT-Long".into(),
        "mast3r-slam" => "Cameras: MASt3R-SLAM".into(),
        "slam3r" => "Cameras: SLAM3R".into(),
        "monst3r" => "Cameras: MonST3R".into(),
        "easi3r" => "Cameras: Easi3R".into(),
        "vggt-commercial" => "Cameras: VGGT-Commercial".into(),
        "mapanything" => "Cameras: MapAnything".into(),
        "colmap" => "Cameras: COLMAP".into(),
        "colmap-pose-prior" => "Cameras: COLMAP (pose-prior)".into(),
        "colmap-fallback" => "Cameras: COLMAP (fallback)".into(),
        "live-init" => "Cameras: Live tracking".into(),
        other => format!("Cameras: {other}"),
    }
}

/// Hypothesis quality metrics used to pick among pose solvers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HypothesisScore {
    pub solver: String,
    pub registered_ratio: f32,
    pub median_reproj_error: f32,
    pub mean_track_length: f32,
    pub cheirality_ratio: f32,
    pub loop_consistency: f32,
    pub gps_residual: Option<f32>,
    pub composite: f32,
}

impl HypothesisScore {
    /// Higher is better. Soft gates reject obviously broken models.
    pub fn compute(solver: &str, model: &Model, expected_images: usize) -> Self {
        let n_img = model.images.len().max(1);
        let expected = expected_images.max(n_img);
        let registered_ratio = (n_img as f32 / expected as f32).clamp(0.0, 1.0);

        let mut errors: Vec<f64> = model.points.iter().map(|p| p.error).filter(|e| e.is_finite()).collect();
        errors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_reproj_error = if errors.is_empty() {
            99.0
        } else {
            errors[errors.len() / 2] as f32
        };

        let mean_track_length = if model.points.is_empty() {
            0.0
        } else {
            model.points.iter().map(|p| p.track.len() as f32).sum::<f32>()
                / model.points.len() as f32
        };

        // Proxy cheirality: fraction of points with track length ≥ 2 and low error.
        let good = model
            .points
            .iter()
            .filter(|p| p.track.len() >= 2 && p.error <= 4.0)
            .count();
        let cheirality_ratio = if model.points.is_empty() {
            0.0
        } else {
            good as f32 / model.points.len() as f32
        };

        // Loop consistency proxy from coverage × track stability.
        let loop_consistency = (registered_ratio * (mean_track_length / 4.0).clamp(0.0, 1.0))
            .clamp(0.0, 1.0);

        let mut s = Self {
            solver: solver.to_string(),
            registered_ratio,
            median_reproj_error,
            mean_track_length,
            cheirality_ratio,
            loop_consistency,
            gps_residual: None,
            composite: 0.0,
        };
        s.composite = s.score_value();
        s
    }

    pub fn score_value(&self) -> f32 {
        let reproj = (1.0 - (self.median_reproj_error / 8.0).clamp(0.0, 1.0)).max(0.0);
        let track = (self.mean_track_length / 6.0).clamp(0.0, 1.0);
        let mut c = 0.30 * self.registered_ratio
            + 0.22 * reproj
            + 0.18 * track
            + 0.18 * self.cheirality_ratio
            + 0.12 * self.loop_consistency;
        if let Some(gps) = self.gps_residual {
            // Lower residual is better; meters-scale penalty.
            c *= 1.0 - (gps / 5.0).clamp(0.0, 0.5);
        }
        c
    }

    pub fn passes_gates(&self) -> bool {
        self.registered_ratio >= 0.35
            && self.median_reproj_error <= 6.0
            && self.mean_track_length >= 1.5
            && self.cheirality_ratio >= 0.25
            && self.composite >= 0.28
    }
}

/// Ordered pose solvers for a capture profile (Standard, commercial-only).
pub fn standard_pose_candidates(profile: CaptureProfile, st: &SidecarStatus) -> Vec<&'static str> {
    let mut v = Vec::new();
    match profile {
        CaptureProfile::GpsRtkDrone => {
            // COLMAP pose-prior mapper is preferred; neural repairs weak sections.
            v.push("colmap-pose-prior");
            if st.vggt_commercial {
                v.push("vggt-commercial");
            }
            if st.mapanything {
                v.push("mapanything");
            }
        }
        CaptureProfile::OrderedVideo | CaptureProfile::LongVideo => {
            if st.vggt_commercial {
                v.push("vggt-commercial");
            }
            if st.mapanything {
                v.push("mapanything");
            }
        }
        CaptureProfile::FisheyeOrRollingShutter | CaptureProfile::LargeScene => {
            if st.vggt_commercial {
                v.push("vggt-commercial");
            }
            if st.mapanything {
                v.push("mapanything");
            }
        }
        CaptureProfile::DynamicScene | CaptureProfile::UnorderedPhotos => {
            if st.vggt_commercial {
                v.push("vggt-commercial");
            }
            if st.mapanything {
                v.push("mapanything");
            }
        }
    }
    v.dedup();
    v
}

/// Ordered pose solvers for Experimental Mode (capture-aware; not blind merge).
pub fn experimental_pose_candidates(profile: CaptureProfile, st: &SidecarStatus) -> Vec<&'static str> {
    experimental::experimental_pose_for_profile(profile, st)
}

/// Back-compat wrappers used by older call sites / tests.
pub fn standard_pose_chain(st: &SidecarStatus) -> Vec<&'static str> {
    standard_pose_candidates(CaptureProfile::UnorderedPhotos, st)
}

pub fn experimental_pose_chain(st: &SidecarStatus) -> Vec<&'static str> {
    experimental_pose_candidates(CaptureProfile::UnorderedPhotos, st)
}

/// Whether a launcher is non-commercial / research-gated.
pub fn is_research_sidecar(name: &str) -> bool {
    matches!(
        name,
        "vggt-omega"
            | "vggt-research"
            | "mast3r"
            | "dust3r"
            | "difix"
            | "pi3x"
            | "stream-vggt"
            | "vggt-long"
            | "mast3r-slam"
            | "slam3r"
            | "monst3r"
            | "easi3r"
            | "city-gaussian"
            | "urban-gs"
            | "horizon-gs"
            | "gof"
            | "pgsr"
            | "rade-gs"
            | "sugar"
            | "milo"
    )
}

/// Select densify launcher order. Standard prefers commercial/Apache sources.
/// Experimental evaluates research engines for the capture profile — fusion
/// (not concatenation) is downstream; 4D / surface adapters stay separate.
pub fn densify_neural_order(
    settings: &ResolvedSettings,
    st: &SidecarStatus,
    profile: CaptureProfile,
) -> Vec<&'static str> {
    if settings.experimental_mode {
        return experimental::experimental_densify_for_profile(profile, st);
    }
    let mut v = Vec::new();
    // Standard: commercial + Apache densifiers only (RoMa is separate).
    if st.vggt_commercial {
        v.push("vggt-commercial");
    }
    if st.mapanything {
        v.push("mapanything");
    }
    if st.depth_anything_3 {
        v.push("depth-anything-3");
    } else if st.depth_anything_v2 {
        v.push("depth-anything-v2");
    }
    v
}

/// Polish order. Experimental: Difix then Fixer (both). Standard: Fixer only.
pub fn polish_order(settings: &ResolvedSettings, st: &SidecarStatus) -> Vec<&'static str> {
    let mut v = Vec::new();
    if settings.experimental_mode && st.difix {
        v.push("difix");
    }
    if st.fixer {
        v.push("fixer");
    }
    v
}

/// Pick the matcher front-end for COLMAP routing (LightGlue stub when selected/available).
pub fn matcher_front_end(settings: &ResolvedSettings, lightglue_ready: bool) -> &'static str {
    match settings.matcher.as_str() {
        "lightglue" if lightglue_ready => "lightglue",
        "sequential" => "sequential",
        "exhaustive" => "exhaustive",
        "roma" => "roma",
        _ if lightglue_ready && settings.experimental_mode => "lightglue",
        _ => "auto",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::sidecars::SidecarStatus;
    use crate::profiler::{GpuVendor, HardwareProfile, Preset};
    use crate::settings::{resolve, Settings};

    fn profile() -> HardwareProfile {
        HardwareProfile {
            gpu_name: "Test".into(),
            gpu_vendor: GpuVendor::Nvidia,
            vram_mb: 16_000,
            has_cuda: true,
            cpu_name: "CPU".into(),
            cpu_threads: 8,
            ram_mb: 32_000,
            auto_preset: Preset::Balanced,
        }
    }

    fn empty_st() -> SidecarStatus {
        SidecarStatus::default()
    }

    #[test]
    fn experimental_pose_chain_orders_research_first() {
        let mut st = empty_st();
        st.vggt_omega = true;
        st.mast3r = true;
        st.dust3r = true;
        st.pi3x = true;
        st.vggt_commercial = true;
        assert_eq!(
            experimental_pose_chain(&st),
            vec![
                "vggt-omega",
                "mast3r",
                "dust3r",
                "pi3x",
                "vggt-commercial"
            ]
        );
    }

    #[test]
    fn experimental_long_video_profile_routing() {
        let mut st = empty_st();
        st.stream_vggt = true;
        st.vggt_long = true;
        st.mast3r_slam = true;
        st.slam3r = true;
        st.vggt_omega = true;
        assert_eq!(
            experimental_pose_candidates(CaptureProfile::LongVideo, &st),
            vec![
                "stream-vggt",
                "vggt-long",
                "mast3r-slam",
                "slam3r",
                "vggt-omega"
            ]
        );
    }

    #[test]
    fn experimental_dynamic_does_not_pull_static_pi3x() {
        let mut st = empty_st();
        st.vggt_omega = true;
        st.monst3r = true;
        st.easi3r = true;
        st.pi3x = true;
        st.dust3r = true;
        let pose = experimental_pose_candidates(CaptureProfile::DynamicScene, &st);
        assert!(!pose.contains(&"pi3x"));
        assert!(!pose.contains(&"dust3r"));
        assert!(pose.contains(&"monst3r"));
        assert_eq!(
            experimental::experimental_four_d_candidates(&st),
            vec!["monst3r", "easi3r"]
        );
    }

    #[test]
    fn standard_pose_chain_is_commercial_only() {
        let mut st = empty_st();
        st.vggt_omega = true;
        st.vggt_commercial = true;
        st.mapanything = true;
        assert_eq!(
            standard_pose_chain(&st),
            vec!["vggt-commercial", "mapanything"]
        );
    }

    #[test]
    fn gps_profile_prefers_pose_prior_mapper() {
        let mut st = empty_st();
        st.vggt_commercial = true;
        st.mapanything = true;
        assert_eq!(
            standard_pose_candidates(CaptureProfile::GpsRtkDrone, &st),
            vec!["colmap-pose-prior", "vggt-commercial", "mapanything"]
        );
    }

    #[test]
    fn research_gate_idents() {
        assert!(is_research_sidecar("vggt-omega"));
        assert!(is_research_sidecar("mast3r"));
        assert!(is_research_sidecar("pi3x"));
        assert!(is_research_sidecar("stream-vggt"));
        assert!(is_research_sidecar("monst3r"));
        assert!(is_research_sidecar("city-gaussian"));
        assert!(!is_research_sidecar("vggt-commercial"));
        assert!(!is_research_sidecar("roma-v2"));
        assert!(!is_research_sidecar("depth-anything-3"));
        assert!(!is_research_sidecar("mapanything"));
        assert!(!is_research_sidecar("gs-2d")); // Apache surface adapter
    }

    #[test]
    fn hypothesis_score_gates_weak_models() {
        let weak = HypothesisScore {
            solver: "x".into(),
            registered_ratio: 0.1,
            median_reproj_error: 12.0,
            mean_track_length: 1.0,
            cheirality_ratio: 0.1,
            loop_consistency: 0.1,
            gps_residual: None,
            composite: 0.05,
        };
        assert!(!weak.passes_gates());

        let strong = HypothesisScore {
            solver: "y".into(),
            registered_ratio: 0.9,
            median_reproj_error: 1.2,
            mean_track_length: 4.0,
            cheirality_ratio: 0.8,
            loop_consistency: 0.7,
            gps_residual: Some(0.2),
            composite: 0.0,
        };
        let mut strong = strong;
        strong.composite = strong.score_value();
        assert!(strong.passes_gates());
        assert!(strong.composite > 0.5);
    }

    #[test]
    fn experimental_resolve_forces_research_and_max_floors() {
        let s = Settings {
            experimental_mode: Some(true),
            experimental_license_acked: Some(true),
            preset: Some("balanced".into()),
            ..Default::default()
        };
        let r = resolve(&s, &profile());
        assert!(r.experimental_mode);
        assert!(r.allow_research_sidecars);
        assert_eq!(r.preset, Preset::Max);
        assert!(r.max_frames >= Preset::Max.params().max_frames);
        assert!(r.max_splats >= Preset::Max.params().max_splats);
        assert_eq!(r.roma_quality, "precise");
    }

    #[test]
    fn experimental_without_ack_stays_off() {
        let s = Settings {
            experimental_mode: Some(true),
            experimental_license_acked: Some(false),
            ..Default::default()
        };
        let r = resolve(&s, &profile());
        assert!(!r.experimental_mode);
        assert!(!r.allow_research_sidecars);
    }

    #[test]
    fn polish_order_experimental_puts_difix_first() {
        let s = Settings {
            experimental_mode: Some(true),
            experimental_license_acked: Some(true),
            ..Default::default()
        };
        let r = resolve(&s, &profile());
        let mut st = empty_st();
        st.fixer = true;
        st.difix = true;
        assert_eq!(polish_order(&r, &st), vec!["difix", "fixer"]);
    }

    #[test]
    fn standard_densify_excludes_nc_even_when_installed() {
        let s = Settings {
            allow_research_sidecars: Some(true),
            experimental_mode: Some(false),
            ..Default::default()
        };
        let r = resolve(&s, &profile());
        let mut st = empty_st();
        st.vggt_omega = true;
        st.mast3r = true;
        st.dust3r = true;
        st.pi3x = true;
        st.vggt_commercial = true;
        st.depth_anything_3 = true;
        st.mapanything = true;
        assert_eq!(
            densify_neural_order(&r, &st, CaptureProfile::UnorderedPhotos),
            vec!["vggt-commercial", "mapanything", "depth-anything-3"]
        );
        assert!(!r.allow_research_sidecars);
    }

    #[test]
    fn da3_preferred_over_legacy_dav2() {
        let s = Settings::default();
        let r = resolve(&s, &profile());
        let mut st = empty_st();
        st.depth_anything_3 = true;
        st.depth_anything_v2 = true;
        assert_eq!(
            densify_neural_order(&r, &st, CaptureProfile::UnorderedPhotos),
            vec!["depth-anything-3"]
        );
    }

    #[test]
    fn standard_mode_ignores_long_video_research_chain() {
        let mut st = empty_st();
        st.stream_vggt = true;
        st.vggt_commercial = true;
        assert_eq!(
            standard_pose_candidates(CaptureProfile::LongVideo, &st),
            vec!["vggt-commercial"]
        );
    }
}
