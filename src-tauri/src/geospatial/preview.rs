//! Live flood preview protocol shared with scientific (ANUGA) runs.
//!
//! Frontend WebGPU/CPU preview owns the graphics path; this module defines
//! checkpoint envelopes so scientific streams can be compared without fighting
//! `hydro.rs` for solver ownership.

use serde::{Deserialize, Serialize};

/// Engine that produced a result sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreviewResultSource {
    LivePreview,
    Anuga,
    Swmm,
    Ensemble,
}

/// Decimated depth / mass sample used for live-vs-scientific comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCheckpoint {
    pub run_id: String,
    pub source: PreviewResultSource,
    /// Scenario time from start (seconds).
    pub time_s: f64,
    pub max_depth_m: f32,
    pub wet_fraction: f32,
    pub mass_m3: f32,
    /// Optional downsampled depth field (row-major). Empty when streaming stats only.
    #[serde(default)]
    pub depth_sample: Vec<f32>,
    pub sample_cols: Option<u32>,
    pub sample_rows: Option<u32>,
}

/// Frontend-facing compare report (mirrors `preview/compare.ts`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCompareReport {
    pub depth_rmse_m: Option<f32>,
    pub extent_iou: Option<f32>,
    pub mass_rel_error: Option<f32>,
    pub wet_fraction_delta: f32,
    pub max_depth_delta_m: f32,
    pub within_tolerance: bool,
    pub checkpoint_time_s: f64,
}

/// Default tolerances before promoting a preview beyond the “Live preview” badge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCompareTolerance {
    pub mass_rel: f32,
    pub max_depth_m: f32,
    pub wet_fraction: f32,
    pub depth_rmse_m: f32,
    pub extent_iou: f32,
}

impl Default for PreviewCompareTolerance {
    fn default() -> Self {
        Self {
            mass_rel: 0.15,
            max_depth_m: 0.35,
            wet_fraction: 0.12,
            depth_rmse_m: 0.4,
            extent_iou: 0.7,
        }
    }
}

/// Compare preview stats against a scientific checkpoint.
pub fn compare_checkpoint(
    preview_max_depth_m: f32,
    preview_wet_fraction: f32,
    preview_mass_m3: f32,
    checkpoint: &PreviewCheckpoint,
    tol: &PreviewCompareTolerance,
) -> PreviewCompareReport {
    let mass_rel_error = if checkpoint.mass_m3 > 1e-3 {
        ((preview_mass_m3 - checkpoint.mass_m3) / checkpoint.mass_m3).abs()
    } else if preview_mass_m3 < 1e-3 {
        0.0
    } else {
        1.0
    };
    let max_depth_delta_m = (preview_max_depth_m - checkpoint.max_depth_m).abs();
    let wet_fraction_delta = (preview_wet_fraction - checkpoint.wet_fraction).abs();

    let within = mass_rel_error <= tol.mass_rel
        && max_depth_delta_m <= tol.max_depth_m
        && wet_fraction_delta <= tol.wet_fraction;

    PreviewCompareReport {
        depth_rmse_m: None,
        extent_iou: None,
        mass_rel_error: Some(mass_rel_error),
        wet_fraction_delta,
        max_depth_delta_m,
        within_tolerance: within,
        checkpoint_time_s: checkpoint.time_s,
    }
}

/// Event payload for streaming scientific checkpoints to the live preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCheckpointEvent {
    pub workspace: String,
    pub checkpoint: PreviewCheckpoint,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_tolerance_when_close() {
        let cp = PreviewCheckpoint {
            run_id: "t".into(),
            source: PreviewResultSource::Anuga,
            time_s: 3600.0,
            max_depth_m: 1.0,
            wet_fraction: 0.4,
            mass_m3: 1_000.0,
            depth_sample: vec![],
            sample_cols: None,
            sample_rows: None,
        };
        let report = compare_checkpoint(1.05, 0.42, 1_050.0, &cp, &PreviewCompareTolerance::default());
        assert!(report.within_tolerance);
    }

    #[test]
    fn diverges_on_mass() {
        let cp = PreviewCheckpoint {
            run_id: "t".into(),
            source: PreviewResultSource::Anuga,
            time_s: 3600.0,
            max_depth_m: 1.0,
            wet_fraction: 0.4,
            mass_m3: 1_000.0,
            depth_sample: vec![],
            sample_cols: None,
            sample_rows: None,
        };
        let report = compare_checkpoint(1.0, 0.4, 2_000.0, &cp, &PreviewCompareTolerance::default());
        assert!(!report.within_tolerance);
    }
}
