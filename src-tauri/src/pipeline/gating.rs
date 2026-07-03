//! Stage 1 — frame quality gating (ROADMAP §3 stage 1).
//! Sharpness scoring via variance-of-Laplacian on a downscaled grayscale
//! image; rejects the blurriest fraction, then subsamples evenly.

use image::imageops::FilterType;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// Variance of the 3x3 Laplacian response — the classic sharpness proxy.
fn sharpness_score(path: &Path) -> f64 {
    let img = match image::open(path) {
        Ok(i) => i,
        Err(_) => return -1.0, // unreadable → reject
    };
    let gray = img
        .resize(480, 480, FilterType::Triangle)
        .into_luma8();
    let (w, h) = gray.dimensions();
    if w < 3 || h < 3 {
        return -1.0;
    }
    let px = gray.as_raw();
    let w = w as usize;
    let h = h as usize;
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let n = ((w - 2) * (h - 2)) as f64;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = px[y * w + x] as f64;
            let lap = px[(y - 1) * w + x] as f64
                + px[(y + 1) * w + x] as f64
                + px[y * w + x - 1] as f64
                + px[y * w + x + 1] as f64
                - 4.0 * c;
            sum += lap;
            sum_sq += lap * lap;
        }
    }
    let mean = sum / n;
    sum_sq / n - mean * mean
}

/// Score all candidates, reject the blurriest `reject_fraction`, then pick
/// up to `max_frames` evenly across the (temporally sorted) survivors so
/// coverage stays uniform rather than clustering on sharp segments.
pub fn select_frames(
    candidates: &[PathBuf],
    max_frames: usize,
    reject_fraction: f32,
) -> Vec<PathBuf> {
    let scored: Vec<(usize, f64)> = candidates
        .par_iter()
        .enumerate()
        .map(|(i, p)| (i, sharpness_score(p)))
        .collect();

    // Drop unreadable + the blurriest fraction (but never below 3 frames).
    let mut valid: Vec<(usize, f64)> = scored.into_iter().filter(|(_, s)| *s >= 0.0).collect();
    let mut by_score = valid.clone();
    by_score.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let reject_n = ((valid.len() as f32) * reject_fraction) as usize;
    let reject_n = reject_n.min(valid.len().saturating_sub(3));
    let cutoff: std::collections::HashSet<usize> =
        by_score.iter().take(reject_n).map(|(i, _)| *i).collect();
    valid.retain(|(i, _)| !cutoff.contains(i));
    valid.sort_by_key(|(i, _)| *i); // restore temporal order

    // Even subsample down to max_frames.
    let keep = valid.len().min(max_frames.max(3));
    if valid.len() <= keep {
        return valid.iter().map(|(i, _)| candidates[*i].clone()).collect();
    }
    let mut out = Vec::with_capacity(keep);
    for k in 0..keep {
        let idx = (k as f64 * (valid.len() - 1) as f64 / (keep - 1) as f64).round() as usize;
        out.push(candidates[valid[idx].0].clone());
    }
    out.dedup();
    out
}
