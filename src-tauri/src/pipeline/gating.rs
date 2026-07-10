//! Stage 1 - frame quality gating (ROADMAP §3 stage 1).
//! Sharpness scoring via variance-of-Laplacian on a downscaled grayscale
//! image; rejects the blurriest fraction, then subsamples evenly.

use image::imageops::FilterType;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// What happened to the candidate frames, so the caller can tell the user
/// exactly why a frame did not make it in rather than just a final count.
#[derive(Debug, Clone, Copy, Default)]
pub struct GatingReport {
    pub total: usize,
    /// Could not be decoded at all: truncated, wrong extension, zero pixels.
    pub unreadable: usize,
    /// Decodable but among the blurriest `reject_fraction`.
    pub blur_rejected: usize,
    pub kept: usize,
}

/// Variance of the 3x3 Laplacian response, the classic sharpness proxy.
fn sharpness_score(path: &Path) -> f64 {
    let img = match image::open(path) {
        Ok(i) => i,
        Err(_) => return -1.0, // unreadable, reject
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
///
/// Returns the survivors alongside a report of why anything else was
/// dropped, so a run with mostly corrupt input and a run with mostly blurry
/// input produce different, specific messages rather than the same bare
/// count.
pub fn select_frames(
    candidates: &[PathBuf],
    max_frames: usize,
    reject_fraction: f32,
) -> (Vec<PathBuf>, GatingReport) {
    let scored: Vec<(usize, f64)> = candidates
        .par_iter()
        .enumerate()
        .map(|(i, p)| (i, sharpness_score(p)))
        .collect();

    let unreadable = scored.iter().filter(|(_, s)| *s < 0.0).count();

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
    let out: Vec<PathBuf> = if valid.len() <= keep {
        valid.iter().map(|(i, _)| candidates[*i].clone()).collect()
    } else {
        let mut out = Vec::with_capacity(keep);
        for k in 0..keep {
            let idx = (k as f64 * (valid.len() - 1) as f64 / (keep - 1) as f64).round() as usize;
            out.push(candidates[valid[idx].0].clone());
        }
        out.dedup();
        out
    };

    let report = GatingReport {
        total: candidates.len(),
        unreadable,
        blur_rejected: reject_n,
        kept: out.len(),
    };
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_png(path: &Path, w: u32, h: u32, fill: u8) {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([fill, fill, fill]));
        img.save(path).unwrap();
    }

    fn checker_png(path: &Path, w: u32, h: u32) {
        let mut img = image::RgbImage::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let on = ((x / 8) + (y / 8)) % 2 == 0;
            *px = image::Rgb(if on { [230, 230, 230] } else { [20, 20, 20] });
        }
        img.save(path).unwrap();
    }

    fn tempdir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("instasplatter_gating_{name}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn unreadable_files_are_counted_separately_from_blur_rejects() {
        let dir = tempdir("unreadable");
        let mut paths = Vec::new();
        for i in 0..6 {
            let p = dir.join(format!("sharp_{i}.png"));
            checker_png(&p, 64, 64);
            paths.push(p);
        }
        for i in 0..2 {
            let p = dir.join(format!("bad_{i}.png"));
            fs::write(&p, b"not an image").unwrap();
            paths.push(p);
        }

        let (kept, report) = select_frames(&paths, 100, 0.0);
        assert_eq!(report.total, 8);
        assert_eq!(report.unreadable, 2);
        assert_eq!(report.kept, kept.len());
        assert_eq!(kept.len(), 6);
        for p in &kept {
            assert!(p.to_string_lossy().contains("sharp"));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_blurriest_fraction_is_rejected() {
        let dir = tempdir("blur");
        let mut paths = Vec::new();
        for i in 0..4 {
            let p = dir.join(format!("sharp_{i}.png"));
            checker_png(&p, 64, 64);
            paths.push(p);
        }
        for i in 0..4 {
            let p = dir.join(format!("flat_{i}.png"));
            write_png(&p, 64, 64, 128);
            paths.push(p);
        }

        let (kept, report) = select_frames(&paths, 100, 0.5);
        assert_eq!(report.unreadable, 0);
        assert!(report.blur_rejected >= 3, "{report:?}");
        // The flat (zero-variance) images are the ones rejected.
        assert!(kept.iter().all(|p| p.to_string_lossy().contains("sharp")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn even_subsampling_covers_the_whole_span() {
        let dir = tempdir("subsample");
        let mut paths = Vec::new();
        for i in 0..20 {
            let p = dir.join(format!("f_{i:03}.png"));
            checker_png(&p, 48, 48);
            paths.push(p);
        }
        let (kept, report) = select_frames(&paths, 5, 0.0);
        assert_eq!(report.kept, kept.len());
        assert!(kept.len() <= 5 && kept.len() >= 4, "{}", kept.len());
        // First and last frame of the span should both survive.
        assert!(kept.first().unwrap().to_string_lossy().contains("f_000"));
        assert!(kept.last().unwrap().to_string_lossy().contains("f_019"));
        let _ = fs::remove_dir_all(&dir);
    }
}
