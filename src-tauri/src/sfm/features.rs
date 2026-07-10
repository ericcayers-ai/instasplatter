//! Feature detection and description for the incremental engine.
//!
//! The reference for Phase 2's behaviour, Inria's on-the-fly-nvs, initializes
//! poses from learned features. Learned descriptors mean a neural runtime, and
//! the roadmap's leading constraint is that the base install stays lean: one
//! cross-vendor binary, no CUDA and no PyTorch. So the shipped backend is
//! classical, an oriented-FAST detector with a rotated BRIEF descriptor, and
//! the seam for a learned backend is the `Frame` produced here. A sidecar that
//! fills the same struct from a learned model can be dropped in without any
//! change to matching, pose solving or bundle adjustment.

use image::imageops::FilterType;
use std::path::Path;

/// Descriptor length in bytes (256 bits).
pub const DESC_BYTES: usize = 32;

/// Pixels kept clear of the border so descriptor patches stay in bounds.
const BORDER: i32 = 20;

/// Radius of the intensity-centroid patch used for orientation.
const ORIENT_RADIUS: i32 = 15;

#[derive(Debug, Clone)]
pub struct GrayImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl GrayImage {
    pub fn new(width: usize, height: usize) -> GrayImage {
        GrayImage {
            width,
            height,
            data: vec![0; width * height],
        }
    }

    #[inline]
    pub fn at(&self, x: i32, y: i32) -> u8 {
        // Callers stay inside the border, so clamping is a safety net only.
        let x = x.clamp(0, self.width as i32 - 1) as usize;
        let y = y.clamp(0, self.height as i32 - 1) as usize;
        self.data[y * self.width + x]
    }

    /// Separable 5-tap box blur, run twice, which approximates a Gaussian
    /// closely enough for BRIEF and costs almost nothing.
    pub fn blurred(&self) -> GrayImage {
        let pass = |src: &GrayImage| -> GrayImage {
            let mut mid = GrayImage::new(src.width, src.height);
            for y in 0..src.height {
                for x in 0..src.width {
                    let mut sum = 0u32;
                    for k in -2..=2i32 {
                        sum += src.at(x as i32 + k, y as i32) as u32;
                    }
                    mid.data[y * src.width + x] = (sum / 5) as u8;
                }
            }
            let mut out = GrayImage::new(src.width, src.height);
            for y in 0..src.height {
                for x in 0..src.width {
                    let mut sum = 0u32;
                    for k in -2..=2i32 {
                        sum += mid.at(x as i32, y as i32 + k) as u32;
                    }
                    out.data[y * src.width + x] = (sum / 5) as u8;
                }
            }
            out
        };
        pass(&pass(self))
    }

    /// Nearest-neighbour-free bilinear downscale by `factor` (> 1).
    pub fn downscale(&self, factor: f32) -> GrayImage {
        let w = ((self.width as f32) / factor).floor().max(1.0) as usize;
        let h = ((self.height as f32) / factor).floor().max(1.0) as usize;
        let mut out = GrayImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let sx = (x as f32 + 0.5) * factor - 0.5;
                let sy = (y as f32 + 0.5) * factor - 0.5;
                let x0 = sx.floor() as i32;
                let y0 = sy.floor() as i32;
                let fx = sx - x0 as f32;
                let fy = sy - y0 as f32;
                let p = |dx: i32, dy: i32| self.at(x0 + dx, y0 + dy) as f32;
                let v = p(0, 0) * (1.0 - fx) * (1.0 - fy)
                    + p(1, 0) * fx * (1.0 - fy)
                    + p(0, 1) * (1.0 - fx) * fy
                    + p(1, 1) * fx * fy;
                out.data[y * w + x] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
        out
    }
}

/// A detected feature, in full-resolution image coordinates.
#[derive(Debug, Clone, Copy)]
pub struct KeyPoint {
    pub x: f32,
    pub y: f32,
    pub score: f32,
    pub angle: f32,
}

/// One image's features, the unit every later stage consumes.
#[derive(Debug, Clone)]
pub struct Frame {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub keypoints: Vec<KeyPoint>,
    /// `keypoints.len() * DESC_BYTES` bytes, one descriptor per keypoint.
    pub descriptors: Vec<u8>,
}

impl Frame {
    pub fn descriptor(&self, i: usize) -> &[u8] {
        &self.descriptors[i * DESC_BYTES..(i + 1) * DESC_BYTES]
    }

    pub fn len(&self) -> usize {
        self.keypoints.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keypoints.is_empty()
    }
}

/// The 16 pixels of the Bresenham circle of radius 3, clockwise from north.
const CIRCLE: [(i32, i32); 16] = [
    (0, -3), (1, -3), (2, -2), (3, -1),
    (3, 0), (3, 1), (2, 2), (1, 3),
    (0, 3), (-1, 3), (-2, 2), (-3, 1),
    (-3, 0), (-3, -1), (-2, -2), (-1, -3),
];

/// FAST-9: a corner has 9 contiguous circle pixels all brighter, or all
/// darker, than the centre by more than `threshold`.
fn is_corner(img: &GrayImage, x: i32, y: i32, threshold: i32) -> Option<f32> {
    let p = img.at(x, y) as i32;
    let hi = p + threshold;
    let lo = p - threshold;

    // Cheap rejection on the four compass points, which sit 4 apart on the
    // circle. Any run of 9 contiguous pixels covers at least two of them, and
    // they all lie on the same side of the centre, so a corner must show at
    // least two consistently bright or two consistently dark. Asking for three
    // would be the FAST-12 test, and it throws away real FAST-9 corners.
    let mut brighter = 0;
    let mut darker = 0;
    for i in [0usize, 4, 8, 12] {
        let v = img.at(x + CIRCLE[i].0, y + CIRCLE[i].1) as i32;
        if v > hi {
            brighter += 1;
        } else if v < lo {
            darker += 1;
        }
    }
    if brighter < 2 && darker < 2 {
        return None;
    }

    let vals: [i32; 16] = std::array::from_fn(|i| img.at(x + CIRCLE[i].0, y + CIRCLE[i].1) as i32);

    let contiguous = |pred: &dyn Fn(i32) -> bool| -> bool {
        let mut run = 0;
        // Walk 16 + 8 so a run wrapping past index 15 is still seen.
        for i in 0..24 {
            if pred(vals[i % 16]) {
                run += 1;
                if run >= 9 {
                    return true;
                }
            } else {
                run = 0;
            }
        }
        false
    };

    if !contiguous(&|v| v > hi) && !contiguous(&|v| v < lo) {
        return None;
    }
    let score: i32 = vals.iter().map(|v| (v - p).abs()).sum();
    Some(score as f32)
}

/// Intensity-centroid orientation over a disc of radius `ORIENT_RADIUS`.
fn orientation(img: &GrayImage, x: i32, y: i32) -> f32 {
    let mut m01 = 0i64;
    let mut m10 = 0i64;
    let r = ORIENT_RADIUS;
    for dy in -r..=r {
        let span = ((r * r - dy * dy) as f64).sqrt() as i32;
        for dx in -span..=span {
            let v = img.at(x + dx, y + dy) as i64;
            m10 += (dx as i64) * v;
            m01 += (dy as i64) * v;
        }
    }
    (m01 as f32).atan2(m10 as f32)
}

/// The BRIEF sampling pattern: 256 point pairs inside a 31x31 patch.
/// Generated once from a fixed seed, so descriptors are comparable across
/// runs and across machines.
fn brief_pattern() -> Vec<(i8, i8, i8, i8)> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || -> f64 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };
    // Box-Muller, clamped into the patch. A Gaussian spread concentrates the
    // comparisons near the centre, which is what makes BRIEF discriminative.
    let mut gauss = || -> i8 {
        let u1 = next().max(1e-12);
        let u2 = next();
        let g = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        (g * 6.25).round().clamp(-15.0, 15.0) as i8
    };
    (0..256).map(|_| (gauss(), gauss(), gauss(), gauss())).collect()
}

/// Sample the pattern rotated by the keypoint angle, which is what turns
/// BRIEF into rotation-aware rBRIEF.
fn describe(img: &GrayImage, kp: &KeyPoint, pattern: &[(i8, i8, i8, i8)], out: &mut [u8]) {
    let (s, c) = kp.angle.sin_cos();
    let x = kp.x.round() as i32;
    let y = kp.y.round() as i32;
    for (k, (ax, ay, bx, by)) in pattern.iter().enumerate() {
        let rot = |px: i8, py: i8| -> (i32, i32) {
            let px = px as f32;
            let py = py as f32;
            ((c * px - s * py).round() as i32, (s * px + c * py).round() as i32)
        };
        let (adx, ady) = rot(*ax, *ay);
        let (bdx, bdy) = rot(*bx, *by);
        if img.at(x + adx, y + ady) < img.at(x + bdx, y + bdy) {
            out[k / 8] |= 1 << (k % 8);
        }
    }
}

/// Keep the strongest corner in each cell of a grid, so features spread over
/// the image instead of piling onto one high-contrast object.
fn spread_and_cap(mut kps: Vec<KeyPoint>, width: usize, height: usize, target: usize) -> Vec<KeyPoint> {
    if kps.len() <= target {
        return kps;
    }
    // Roughly `target` cells, so about one keypoint each.
    let cells = target.max(1);
    let aspect = width as f32 / height.max(1) as f32;
    let rows = ((cells as f32 / aspect).sqrt()).ceil().max(1.0) as usize;
    let cols = ((cells as f32 / rows as f32).ceil()).max(1.0) as usize;
    let cw = (width as f32 / cols as f32).max(1.0);
    let ch = (height as f32 / rows as f32).max(1.0);

    let mut best: Vec<Option<KeyPoint>> = vec![None; rows * cols];
    let mut leftovers = Vec::new();
    for kp in kps.drain(..) {
        let c = ((kp.x / cw) as usize).min(cols - 1);
        let r = ((kp.y / ch) as usize).min(rows - 1);
        let slot = &mut best[r * cols + c];
        match slot {
            Some(prev) if prev.score >= kp.score => leftovers.push(kp),
            Some(prev) => {
                leftovers.push(*prev);
                *slot = Some(kp);
            }
            None => *slot = Some(kp),
        }
    }

    let mut out: Vec<KeyPoint> = best.into_iter().flatten().collect();
    if out.len() < target {
        // Backfill with the strongest of the rest.
        leftovers.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        out.extend(leftovers.into_iter().take(target - out.len()));
    }
    out
}

/// Detect corners at one pyramid level, with 3x3 non-maximum suppression.
fn detect_level(img: &GrayImage, threshold: i32, scale: f32) -> Vec<KeyPoint> {
    let w = img.width as i32;
    let h = img.height as i32;
    if w <= 2 * BORDER || h <= 2 * BORDER {
        return Vec::new();
    }

    let mut scores = vec![0.0f32; (w * h) as usize];
    let mut candidates = Vec::new();
    for y in BORDER..(h - BORDER) {
        for x in BORDER..(w - BORDER) {
            if let Some(s) = is_corner(img, x, y, threshold) {
                scores[(y * w + x) as usize] = s;
                candidates.push((x, y, s));
            }
        }
    }

    candidates
        .into_iter()
        .filter(|&(x, y, s)| {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx, dy) != (0, 0) && scores[((y + dy) * w + x + dx) as usize] > s {
                        return false;
                    }
                }
            }
            true
        })
        .map(|(x, y, s)| KeyPoint {
            x: x as f32 * scale,
            y: y as f32 * scale,
            score: s,
            angle: 0.0,
        })
        .collect()
}

/// Options controlling detection density.
#[derive(Debug, Clone, Copy)]
pub struct DetectOptions {
    pub threshold: i32,
    pub max_features: usize,
    pub pyramid_levels: usize,
    pub pyramid_factor: f32,
}

impl Default for DetectOptions {
    fn default() -> DetectOptions {
        DetectOptions {
            threshold: 20,
            max_features: 2500,
            pyramid_levels: 3,
            pyramid_factor: 1.3,
        }
    }
}

/// Detect and describe features across a small scale pyramid. Keypoint
/// coordinates are always reported in level-0 pixels.
pub fn detect_and_describe(name: &str, img: &GrayImage, opts: DetectOptions) -> Frame {
    let pattern = brief_pattern();
    let mut all: Vec<(KeyPoint, usize)> = Vec::new();
    let mut levels: Vec<GrayImage> = Vec::new();

    let mut current = img.clone();
    for level in 0..opts.pyramid_levels.max(1) {
        let scale = opts.pyramid_factor.powi(level as i32);
        // A weaker threshold at coarse levels, where contrast is averaged down.
        let thresh = (opts.threshold as f32 / (1.0 + 0.15 * level as f32)).round() as i32;
        let blurred = current.blurred();
        for kp in detect_level(&current, thresh.max(5), scale) {
            all.push((kp, level));
        }
        levels.push(blurred);
        if level + 1 < opts.pyramid_levels {
            current = current.downscale(opts.pyramid_factor);
            if current.width <= 2 * BORDER as usize || current.height <= 2 * BORDER as usize {
                break;
            }
        }
    }

    // Cap globally, keeping coverage even.
    let kps: Vec<KeyPoint> = spread_and_cap(
        all.iter().map(|(k, _)| *k).collect(),
        img.width,
        img.height,
        opts.max_features,
    );
    // Recover each survivor's level by matching position back.
    let level_of: std::collections::HashMap<(u32, u32), usize> = all
        .iter()
        .map(|(k, l)| ((k.x.to_bits(), k.y.to_bits()), *l))
        .collect();

    let mut keypoints = Vec::with_capacity(kps.len());
    let mut descriptors = vec![0u8; kps.len() * DESC_BYTES];
    let mut written = 0usize;

    for kp in kps {
        let level = *level_of
            .get(&(kp.x.to_bits(), kp.y.to_bits()))
            .unwrap_or(&0);
        let lvl = &levels[level.min(levels.len() - 1)];
        let scale = opts.pyramid_factor.powi(level as i32);
        let lx = (kp.x / scale).round() as i32;
        let ly = (kp.y / scale).round() as i32;
        if lx < BORDER
            || ly < BORDER
            || lx >= lvl.width as i32 - BORDER
            || ly >= lvl.height as i32 - BORDER
        {
            continue;
        }

        let angle = orientation(lvl, lx, ly);
        let local = KeyPoint {
            x: lx as f32,
            y: ly as f32,
            score: kp.score,
            angle,
        };
        describe(
            lvl,
            &local,
            &pattern,
            &mut descriptors[written * DESC_BYTES..(written + 1) * DESC_BYTES],
        );
        keypoints.push(KeyPoint { angle, ..kp });
        written += 1;
    }
    descriptors.truncate(written * DESC_BYTES);

    Frame {
        name: name.to_string(),
        width: img.width,
        height: img.height,
        keypoints,
        descriptors,
    }
}

/// A small colour copy of a frame, kept so triangulated points can be given
/// the colour of the pixel they came from without decoding the image twice.
#[derive(Debug, Clone)]
pub struct Thumb {
    pub width: usize,
    pub height: usize,
    /// `width * height * 3` bytes.
    pub rgb: Vec<u8>,
}

impl Thumb {
    /// Colour at a point expressed in the coordinates of a `gw` by `gh` image.
    pub fn sample(&self, x: f32, y: f32, gw: usize, gh: usize) -> [u8; 3] {
        if self.width == 0 || self.height == 0 || gw == 0 || gh == 0 {
            return [128, 128, 128];
        }
        let tx = ((x / gw as f32) * self.width as f32) as i64;
        let ty = ((y / gh as f32) * self.height as f32) as i64;
        let tx = tx.clamp(0, self.width as i64 - 1) as usize;
        let ty = ty.clamp(0, self.height as i64 - 1) as usize;
        let i = (ty * self.width + tx) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

/// One decoded frame: the working grayscale image, the factor that maps its
/// coordinates back to the original pixels, and a colour thumbnail.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub gray: GrayImage,
    pub scale: f32,
    pub thumb: Thumb,
}

/// Decode an image once and produce everything the engine needs from it.
///
/// The grayscale copy is downscaled so its longest side is at most `max_dim`,
/// which is what bounds detection and matching cost. The thumbnail is taken
/// from that copy rather than from the original, so a large photo is only
/// resampled once.
pub fn load_frame(path: &Path, max_dim: u32, thumb_dim: u32) -> Result<Loaded, String> {
    let img = image::open(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let (w0, h0) = (img.width(), img.height());
    let work = if max_dim > 0 && w0.max(h0) > max_dim {
        img.resize(max_dim, max_dim, FilterType::Triangle)
    } else {
        img
    };
    let (w, h) = (work.width(), work.height());
    if w == 0 || h == 0 {
        return Err(format!("{} has no pixels.", path.display()));
    }

    let thumb_img = if thumb_dim > 0 && w.max(h) > thumb_dim {
        work.resize(thumb_dim, thumb_dim, FilterType::Triangle)
    } else {
        work.clone()
    };
    let rgb = thumb_img.to_rgb8();
    let (tw, th) = rgb.dimensions();
    let thumb = Thumb {
        width: tw as usize,
        height: th as usize,
        rgb: rgb.into_raw(),
    };

    let luma = work.into_luma8();
    Ok(Loaded {
        gray: GrayImage {
            width: w as usize,
            height: h as usize,
            data: luma.into_raw(),
        },
        scale: w0 as f32 / w as f32,
        thumb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A checkerboard. Its junctions are saddle points, not blob corners.
    fn checker(w: usize, h: usize) -> GrayImage {
        let mut img = GrayImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let on = ((x / 20) + (y / 20)) % 2 == 0;
                img.data[y * w + x] = if on { 220 } else { 30 };
            }
        }
        img
    }

    /// Isolated 20 pixel bright squares on a 40 pixel pitch, over a dark
    /// field. Each square contributes four true corners.
    fn squares(w: usize, h: usize) -> GrayImage {
        let mut img = GrayImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let on = x % 40 < 20 && y % 40 < 20;
                img.data[y * w + x] = if on { 220 } else { 30 };
            }
        }
        img
    }

    /// Distance from `v` to the nearest square edge along one axis.
    fn edge_distance(v: f32) -> f32 {
        let m = v % 40.0;
        (m - 0.0).abs().min((m - 19.0).abs()).min((m - 40.0).abs())
    }

    #[test]
    fn fast_finds_corners_on_squares_and_none_on_a_flat_field() {
        let img = squares(200, 200);
        let kps = detect_level(&img, 20, 1.0);
        assert!(kps.len() > 20, "found {}", kps.len());

        let flat = GrayImage::new(200, 200);
        assert!(detect_level(&flat, 20, 1.0).is_empty());
    }

    #[test]
    fn fast_does_not_fire_on_checkerboard_saddle_points() {
        // A checker junction has two bright and two dark quadrants, so the
        // longest run of same-sign pixels around the circle is 5, well short
        // of the 9 FAST-9 requires. This is a property of the detector, not a
        // defect: FAST responds to corners of regions, not to saddles.
        let img = checker(200, 200);
        assert!(detect_level(&img, 20, 1.0).is_empty());
    }

    #[test]
    fn detected_corners_sit_near_square_corners() {
        let img = squares(200, 200);
        let kps = detect_level(&img, 20, 1.0);
        assert!(!kps.is_empty());
        for kp in kps {
            // A square corner is where both axes are near an edge.
            assert!(
                edge_distance(kp.x) < 4.0 && edge_distance(kp.y) < 4.0,
                "stray corner at {},{}",
                kp.x,
                kp.y
            );
        }
    }

    #[test]
    fn a_run_of_nine_that_wraps_the_circle_still_counts() {
        // Bright arc centred on index 0, i.e. spanning indices 12..=4.
        let mut img = GrayImage::new(40, 40);
        for p in img.data.iter_mut() {
            *p = 100;
        }
        let (cx, cy) = (20i32, 20i32);
        for i in 0..24 {
            let k = (i + 12) % 16;
            if i < 9 {
                let (dx, dy) = CIRCLE[k];
                img.data[((cy + dy) * 40 + cx + dx) as usize] = 250;
            }
        }
        assert!(is_corner(&img, cx, cy, 20).is_some(), "wrapped run missed");
    }

    #[test]
    fn descriptors_are_stable_and_discriminative() {
        let img = checker(240, 240).blurred();
        let pattern = brief_pattern();
        let kp = KeyPoint { x: 100.0, y: 100.0, score: 1.0, angle: 0.3 };

        let mut a = [0u8; DESC_BYTES];
        let mut b = [0u8; DESC_BYTES];
        describe(&img, &kp, &pattern, &mut a);
        describe(&img, &kp, &pattern, &mut b);
        assert_eq!(a, b, "the same patch must describe identically");

        // A different location gives a materially different descriptor.
        let other = KeyPoint { x: 110.0, y: 103.0, ..kp };
        let mut c = [0u8; DESC_BYTES];
        describe(&img, &other, &pattern, &mut c);
        let dist: u32 = a.iter().zip(&c).map(|(x, y)| (x ^ y).count_ones()).sum();
        assert!(dist > 10, "descriptors too similar: {dist}");
    }

    #[test]
    fn the_brief_pattern_is_deterministic_and_in_bounds() {
        let a = brief_pattern();
        let b = brief_pattern();
        assert_eq!(a.len(), 256);
        assert_eq!(a, b);
        for (ax, ay, bx, by) in a {
            for v in [ax, ay, bx, by] {
                assert!((-15..=15).contains(&v));
            }
        }
    }

    #[test]
    fn orientation_points_toward_the_bright_side() {
        let mut img = GrayImage::new(80, 80);
        // Bright to the right of x = 40, dark to the left.
        for y in 0..80 {
            for x in 0..80 {
                img.data[y * 80 + x] = if x > 40 { 255 } else { 0 };
            }
        }
        let a = orientation(&img, 40, 40);
        assert!(a.abs() < 0.2, "expected ~0 rad, got {a}");

        // Bright below: the centroid is at +y, so the angle is ~ +pi/2.
        let mut img2 = GrayImage::new(80, 80);
        for y in 0..80 {
            for x in 0..80 {
                img2.data[y * 80 + x] = if y > 40 { 255 } else { 0 };
            }
        }
        let b = orientation(&img2, 40, 40);
        assert!((b - std::f32::consts::FRAC_PI_2).abs() < 0.2, "got {b}");
    }

    #[test]
    fn spread_and_cap_keeps_coverage_rather_than_only_the_strongest() {
        let mut kps = Vec::new();
        // 100 weak points spread out, 50 very strong ones bunched in a corner.
        for i in 0..100 {
            kps.push(KeyPoint { x: (i % 10) as f32 * 20.0 + 5.0, y: (i / 10) as f32 * 20.0 + 5.0, score: 1.0, angle: 0.0 });
        }
        for i in 0..50 {
            kps.push(KeyPoint { x: 1.0 + (i % 5) as f32, y: 1.0 + (i / 5) as f32, score: 1000.0, angle: 0.0 });
        }
        let out = spread_and_cap(kps, 200, 200, 30);
        assert!(out.len() <= 30);
        // Not everything ends up in the top-left corner.
        let far = out.iter().filter(|k| k.x > 50.0 || k.y > 50.0).count();
        assert!(far >= 10, "coverage collapsed: {far} of {}", out.len());
    }

    #[test]
    fn detect_and_describe_produces_matching_counts() {
        let img = squares(240, 240);
        let f = detect_and_describe("a.png", &img, DetectOptions::default());
        assert!(f.len() > 10, "{}", f.len());
        assert_eq!(f.descriptors.len(), f.len() * DESC_BYTES);
        assert_eq!(f.descriptor(0).len(), DESC_BYTES);
        // Coordinates are inside the image.
        for kp in &f.keypoints {
            assert!(kp.x >= 0.0 && kp.x < 240.0 && kp.y >= 0.0 && kp.y < 240.0);
        }
    }

    #[test]
    fn downscale_halves_dimensions_and_preserves_a_constant_field() {
        let mut img = GrayImage::new(64, 32);
        img.data.iter_mut().for_each(|p| *p = 77);
        let d = img.downscale(2.0);
        assert_eq!((d.width, d.height), (32, 16));
        assert!(d.data.iter().all(|p| *p == 77));
    }

    #[test]
    fn blur_leaves_a_constant_field_unchanged() {
        let mut img = GrayImage::new(40, 40);
        img.data.iter_mut().for_each(|p| *p = 128);
        assert!(img.blurred().data.iter().all(|p| *p == 128));
    }
}
