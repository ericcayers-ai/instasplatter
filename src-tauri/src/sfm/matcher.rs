//! Descriptor matching: brute-force Hamming with Lowe's ratio test and a
//! mutual-consistency check. Both filters are cheap and together they remove
//! nearly all of the wrong matches that would otherwise poison RANSAC.

use super::features::{Frame, DESC_BYTES};
use rayon::prelude::*;

#[inline]
fn hamming(a: &[u8], b: &[u8]) -> u32 {
    debug_assert_eq!(a.len(), DESC_BYTES);
    let mut d = 0u32;
    for k in 0..DESC_BYTES {
        d += (a[k] ^ b[k]).count_ones();
    }
    d
}

/// Index of the best and second-best match in `to` for descriptor `desc`.
fn best_two(desc: &[u8], to: &Frame) -> Option<(usize, u32, u32)> {
    let mut best = (usize::MAX, u32::MAX);
    let mut second = u32::MAX;
    for j in 0..to.len() {
        let d = hamming(desc, to.descriptor(j));
        if d < best.1 {
            second = best.1;
            best = (j, d);
        } else if d < second {
            second = d;
        }
    }
    if best.0 == usize::MAX {
        None
    } else {
        Some((best.0, best.1, second))
    }
}

/// Tuning for `match_frames`.
#[derive(Debug, Clone, Copy)]
pub struct MatchOptions {
    /// Lowe's ratio: keep a match only if it is clearly better than the runner-up.
    pub ratio: f32,
    /// Absolute Hamming ceiling on 256-bit descriptors.
    pub max_distance: u32,
    /// Require each match to be the other frame's best choice too.
    pub cross_check: bool,
}

impl Default for MatchOptions {
    fn default() -> MatchOptions {
        MatchOptions {
            ratio: 0.8,
            max_distance: 64,
            cross_check: true,
        }
    }
}

/// Matched keypoint indices `(index in a, index in b)`.
pub type Matches = Vec<(usize, usize)>;

pub fn match_frames(a: &Frame, b: &Frame, opts: MatchOptions) -> Matches {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }

    let forward: Vec<Option<usize>> = (0..a.len())
        .into_par_iter()
        .map(|i| {
            let (j, d1, d2) = best_two(a.descriptor(i), b)?;
            if d1 > opts.max_distance {
                return None;
            }
            // With only one candidate there is no ratio to test.
            if d2 != u32::MAX && (d1 as f32) > opts.ratio * d2 as f32 {
                return None;
            }
            Some(j)
        })
        .collect();

    if !opts.cross_check {
        return forward
            .into_iter()
            .enumerate()
            .filter_map(|(i, j)| j.map(|j| (i, j)))
            .collect();
    }

    let backward: Vec<Option<usize>> = (0..b.len())
        .into_par_iter()
        .map(|j| best_two(b.descriptor(j), a).map(|(i, _, _)| i))
        .collect();

    forward
        .into_iter()
        .enumerate()
        .filter_map(|(i, j)| {
            let j = j?;
            (backward[j] == Some(i)).then_some((i, j))
        })
        .collect()
}

/// Median pixel displacement of a match set, a cheap proxy for how far the
/// camera moved. Used to decide when there is enough parallax to triangulate.
pub fn median_displacement(a: &Frame, b: &Frame, matches: &Matches) -> f32 {
    if matches.is_empty() {
        return 0.0;
    }
    let mut d: Vec<f32> = matches
        .iter()
        .map(|&(i, j)| {
            let p = a.keypoints[i];
            let q = b.keypoints[j];
            ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt()
        })
        .collect();
    d.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    d[d.len() / 2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfm::features::KeyPoint;

    fn frame(descs: &[[u8; DESC_BYTES]]) -> Frame {
        Frame {
            name: "f".into(),
            width: 100,
            height: 100,
            keypoints: descs
                .iter()
                .enumerate()
                .map(|(i, _)| KeyPoint {
                    x: i as f32 * 10.0,
                    y: 0.0,
                    score: 1.0,
                    angle: 0.0,
                })
                .collect(),
            descriptors: descs.iter().flatten().copied().collect(),
        }
    }

    fn desc(seed: u8) -> [u8; DESC_BYTES] {
        let mut d = [0u8; DESC_BYTES];
        for (k, v) in d.iter_mut().enumerate() {
            *v = seed.wrapping_mul(31).wrapping_add(k as u8 * 7);
        }
        d
    }

    fn flip_bits(mut d: [u8; DESC_BYTES], n: usize) -> [u8; DESC_BYTES] {
        for k in 0..n {
            d[k / 8] ^= 1 << (k % 8);
        }
        d
    }

    #[test]
    fn hamming_counts_differing_bits() {
        assert_eq!(hamming(&[0u8; DESC_BYTES], &[0u8; DESC_BYTES]), 0);
        let mut b = [0u8; DESC_BYTES];
        b[0] = 0b1011;
        assert_eq!(hamming(&[0u8; DESC_BYTES], &b), 3);
    }

    #[test]
    fn identical_descriptors_match_one_to_one() {
        let descs = [desc(1), desc(2), desc(3), desc(4)];
        let a = frame(&descs);
        let b = frame(&descs);
        let m = match_frames(&a, &b, MatchOptions::default());
        assert_eq!(m.len(), 4);
        for (i, j) in m {
            assert_eq!(i, j);
        }
    }

    #[test]
    fn slightly_perturbed_descriptors_still_match() {
        let a = frame(&[desc(1), desc(2), desc(3)]);
        let b = frame(&[
            flip_bits(desc(1), 5),
            flip_bits(desc(2), 8),
            flip_bits(desc(3), 3),
        ]);
        let m = match_frames(&a, &b, MatchOptions::default());
        assert_eq!(m.len(), 3, "{m:?}");
        for (i, j) in m {
            assert_eq!(i, j);
        }
    }

    #[test]
    fn the_distance_ceiling_rejects_unrelated_descriptors() {
        let a = frame(&[desc(1)]);
        let mut far = desc(1);
        // Invert every bit: distance 256, far beyond the ceiling.
        for v in far.iter_mut() {
            *v = !*v;
        }
        let b = frame(&[far]);
        assert!(match_frames(&a, &b, MatchOptions::default()).is_empty());
    }

    #[test]
    fn the_ratio_test_rejects_ambiguous_matches() {
        // Two near-identical candidates in b at distances 5 and 6. The default
        // ratio of 0.8 needs the best to beat the runner-up by more than that
        // (5 > 0.8 * 6), so the match is not trustworthy and is dropped.
        let a = frame(&[desc(1)]);
        let b = frame(&[flip_bits(desc(1), 5), flip_bits(desc(1), 6)]);
        assert!(match_frames(&a, &b, MatchOptions::default()).is_empty());

        // Relaxing the ratio lets it through, confirming that is why it failed.
        let loose = MatchOptions { ratio: 0.99, ..Default::default() };
        assert_eq!(match_frames(&a, &b, loose).len(), 1);
    }

    #[test]
    fn cross_check_removes_a_match_the_other_side_disagrees_with() {
        // b0 is a's best choice, but b0's own best choice is a1, not a0.
        let a = frame(&[flip_bits(desc(9), 20), desc(9)]);
        let b = frame(&[desc(9)]);
        let strict = MatchOptions { ratio: 0.99, ..Default::default() };
        let m = match_frames(&a, &b, strict);
        assert_eq!(m, vec![(1, 0)]);

        let loose = MatchOptions { cross_check: false, ratio: 0.99, ..Default::default() };
        assert_eq!(match_frames(&a, &b, loose).len(), 2);
    }

    #[test]
    fn empty_frames_match_to_nothing() {
        let a = frame(&[]);
        let b = frame(&[desc(1)]);
        assert!(match_frames(&a, &b, MatchOptions::default()).is_empty());
        assert!(match_frames(&b, &a, MatchOptions::default()).is_empty());
    }

    #[test]
    fn median_displacement_is_the_middle_shift() {
        let a = frame(&[desc(1), desc(2), desc(3)]);
        let mut b = frame(&[desc(1), desc(2), desc(3)]);
        b.keypoints[0].x = 0.0; // shift 0
        b.keypoints[1].x = 13.0; // shift 3
        b.keypoints[2].x = 120.0; // shift 100
        let m = vec![(0, 0), (1, 1), (2, 2)];
        assert!((median_displacement(&a, &b, &m) - 3.0).abs() < 1e-5);
        assert_eq!(median_displacement(&a, &b, &Matches::new()), 0.0);
    }
}
