//! Phase 2: the native incremental live-init engine.
//!
//! COLMAP solves every camera before the first splat exists, so the user
//! stares at a progress bar for the whole of it. This engine instead registers
//! one frame at a time and hands each pose to the viewport as it lands, which
//! is what makes the scene paint itself in.
//!
//! The loop is the classical one:
//!
//!   1. seed a reconstruction from the first image pair with real parallax,
//!   2. register each further frame by PnP against the points it already sees,
//!   3. triangulate the correspondences that are still unexplained,
//!   4. refine a sliding window of recent keyframes with local bundle
//!      adjustment, so drift is corrected before it compounds.
//!
//! Everything downstream of this module consumes a COLMAP sparse model, so the
//! result is written in exactly the layout the batch path produces. If the
//! engine loses confidence it returns `Err(reason)` and the pipeline says so
//! plainly before falling back to COLMAP; a half-finished reconstruction is
//! never handed to the trainer.
//!
//! An opt-in sidecar could replace steps 1 to 3 with a feed-forward pose
//! predictor such as VGGT. That would need a neural runtime, which the base
//! install deliberately does not carry, so the seam is left at `Frame`: any
//! backend that fills one can drive the rest of this file unchanged.

pub mod ba;
pub mod features;
pub mod geometry;
pub mod matcher;

use crate::colmap::{m3_to_quat, Camera, CameraModel, Image, Model, Point3D};
use crate::math::{m3_mul_v, m3_transpose, sub, V3};
use crate::pipeline::{JobCtx, JobEvent};
use ba::{bundle_adjust, BaOptions, Observation};
use features::{DetectOptions, Frame, Thumb};
use geometry::{
    parallax, ransac_essential, ransac_pnp, reprojection_error, triangulate, Intrinsics, Pose, Rng,
};
use matcher::{match_frames, median_displacement, MatchOptions};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Longest side of the image the engine actually works on. Detection and
/// matching cost grows with area, and pose accuracy does not, past this.
const WORK_MAX_DIM: u32 = 1600;

/// Longest side of the colour thumbnail used to tint triangulated points.
const THUMB_MAX_DIM: u32 = 256;

/// Inlier threshold for RANSAC and for pruning, in working-image pixels.
const RANSAC_PX: f64 = 2.5;

/// Frames considered as the second half of the seed pair.
const SEED_SPAN: usize = 30;

/// A seed pair needs this many surviving correspondences,
const MIN_SEED_INLIERS: usize = 60;

/// this much median motion between the two images,
const MIN_SEED_DISPLACEMENT_PX: f32 = 4.0;

/// and this much median parallax, or its depths would be guesswork.
const MIN_SEED_PARALLAX: f64 = 1.5 * std::f64::consts::PI / 180.0;

/// A new point needs this much parallax before it is worth keeping.
const MIN_TRACK_PARALLAX: f64 = 1.0 * std::f64::consts::PI / 180.0;

/// Already-registered neighbours matched against each new frame.
const REF_FRAMES: usize = 3;

/// Below this many 3D-2D correspondences a frame cannot be registered.
const MIN_PNP_MATCHES: usize = 16;

/// Fraction of correspondences that must survive pose estimation.
const MIN_CONFIDENCE: f32 = 0.35;

/// Consecutive frames that may fail before the engine gives up.
const MAX_CONSECUTIVE_FAILURES: usize = 4;

/// Keyframes in the local bundle adjustment window.
const BA_WINDOW: usize = 8;

/// Points in any one bundle adjustment. The reduced camera system is dense, so
/// the pose count is what really bounds cost, but a runaway point count would
/// still make each iteration crawl.
const MAX_BA_POINTS: usize = 6000;

/// Fraction of the frames that must register for the result to be usable.
const MIN_REGISTERED_FRACTION: f32 = 0.6;

/// A 3D point and every keypoint that sees it.
#[derive(Debug, Clone)]
pub struct Track {
    pub xyz: V3,
    pub rgb: [u8; 3],
    /// `(frame index, keypoint index)`, in registration order.
    pub obs: Vec<(usize, usize)>,
}

/// The engine's output: a pose per registered frame, and the points.
#[derive(Debug, Clone, Default)]
pub struct Reconstruction {
    pub poses: Vec<Option<Pose>>,
    pub tracks: Vec<Track>,
}

impl Reconstruction {
    pub fn registered(&self) -> Vec<usize> {
        (0..self.poses.len())
            .filter(|&i| self.poses[i].is_some())
            .collect()
    }
}

/// Handed to the caller the moment a camera is solved.
pub struct RegisterEvent<'a> {
    pub frame: &'a Frame,
    pub pose: Pose,
    pub registered: usize,
    pub total: usize,
    /// Share of the candidate correspondences that survived pose estimation.
    pub confidence: f32,
    /// Median depth of the points this camera sees, in scene units.
    pub median_depth: f64,
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

/// Keypoint `i` of `frame` in calibrated coordinates.
fn bearing(frame: &Frame, i: usize, intr: Intrinsics) -> [f64; 2] {
    let kp = frame.keypoints[i];
    intr.normalize(kp.x as f64, kp.y as f64)
}

/// The colour of keypoint `i`, taken from the frame's thumbnail.
fn keypoint_colour(frame: &Frame, thumb: &Thumb, i: usize) -> [u8; 3] {
    let kp = frame.keypoints[i];
    thumb.sample(kp.x, kp.y, frame.width, frame.height)
}

/// Accept a triangulated point only if both cameras see it in front of them,
/// the rays meet at a real angle, and it reprojects where it was observed.
fn point_is_good(
    a: &Pose,
    b: &Pose,
    xa: [f64; 2],
    xb: [f64; 2],
    p: V3,
    max_err: f64,
) -> bool {
    if a.apply(p)[2] <= 1e-6 || b.apply(p)[2] <= 1e-6 {
        return false;
    }
    if parallax(a, b, p) < MIN_TRACK_PARALLAX {
        return false;
    }
    let ea = reprojection_error(a, p, xa);
    let eb = reprojection_error(b, p, xb);
    match (ea, eb) {
        (Some(ea), Some(eb)) => ea < max_err * max_err && eb < max_err * max_err,
        _ => false,
    }
}

/// Find the first image pair with enough parallax to define a scene.
///
/// The reference is allowed to advance a little: a blurred or featureless
/// opening frame should not doom the whole reconstruction.
fn find_seed(
    frames: &[Frame],
    intr: Intrinsics,
    rng: &mut Rng,
) -> Option<(usize, usize, Pose, Vec<(usize, usize)>, Vec<usize>)> {
    let n = frames.len();
    for i in 0..n.min(3) {
        for j in (i + 1)..n.min(i + 1 + SEED_SPAN) {
            let m = match_frames(&frames[i], &frames[j], MatchOptions::default());
            if m.len() < MIN_SEED_INLIERS {
                continue;
            }
            // A pair the camera barely moved between cannot have parallax, and
            // RANSAC on it is pure cost.
            if median_displacement(&frames[i], &frames[j], &m) < MIN_SEED_DISPLACEMENT_PX {
                continue;
            }
            let x1: Vec<[f64; 2]> = m.iter().map(|&(a, _)| bearing(&frames[i], a, intr)).collect();
            let x2: Vec<[f64; 2]> = m.iter().map(|&(_, b)| bearing(&frames[j], b, intr)).collect();

            let tv = match ransac_essential(&x1, &x2, intr.mean_focal(), RANSAC_PX, 400, rng) {
                Some(tv) => tv,
                None => continue,
            };
            if tv.inliers.len() < MIN_SEED_INLIERS {
                continue;
            }

            let first = Pose::identity();
            let angles: Vec<f64> = tv
                .inliers
                .iter()
                .filter_map(|&k| triangulate(&first, &tv.pose, x1[k], x2[k]))
                .filter(|p| first.apply(*p)[2] > 1e-6 && tv.pose.apply(*p)[2] > 1e-6)
                .map(|p| parallax(&first, &tv.pose, p))
                .collect();
            if angles.len() < MIN_SEED_INLIERS || median(angles) < MIN_SEED_PARALLAX {
                continue;
            }
            return Some((i, j, tv.pose, m, tv.inliers));
        }
    }
    None
}

/// Reproject every observation and drop the ones that no longer agree with the
/// reconstruction. Tracks left with fewer than two views are removed entirely.
fn prune(
    frames: &[Frame],
    poses: &[Option<Pose>],
    tracks: &mut Vec<Track>,
    intr: Intrinsics,
    max_err_px: f64,
) -> usize {
    let thresh = (max_err_px / intr.mean_focal()).powi(2);
    let before = tracks.len();
    for t in tracks.iter_mut() {
        let xyz = t.xyz;
        t.obs.retain(|&(f, k)| match &poses[f] {
            Some(p) => reprojection_error(p, xyz, bearing(&frames[f], k, intr))
                .map(|e| e < thresh)
                .unwrap_or(false),
            None => false,
        });
    }
    tracks.retain(|t| t.obs.len() >= 2 && t.xyz.iter().all(|v| v.is_finite()));
    before - tracks.len()
}

/// Root-mean-square reprojection residual, in working-image pixels.
fn rms_px(frames: &[Frame], poses: &[Option<Pose>], tracks: &[Track], intr: Intrinsics) -> f64 {
    let mut sq = 0.0;
    let mut n = 0usize;
    for t in tracks {
        for &(f, k) in &t.obs {
            if let Some(p) = &poses[f] {
                if let Some(e) = reprojection_error(p, t.xyz, bearing(&frames[f], k, intr)) {
                    sq += e;
                    n += 1;
                }
            }
        }
    }
    if n == 0 {
        return 0.0;
    }
    (sq / n as f64).sqrt() * intr.mean_focal()
}

/// Reconstruct poses and points from an ordered set of frames.
///
/// `on_register` is called the moment each camera is solved, and may return an
/// error to abort the run (which is how cancellation reaches the loop).
pub fn reconstruct<F>(
    frames: &[Frame],
    thumbs: &[Thumb],
    intr: Intrinsics,
    mut on_register: F,
) -> Result<Reconstruction, String>
where
    F: FnMut(RegisterEvent) -> Result<(), String>,
{
    let n = frames.len();
    if n < 3 {
        return Err("fewer than three usable frames".into());
    }
    let max_err = RANSAC_PX / intr.mean_focal();
    // Fixed seed: the same frames must reconstruct identically every run.
    let mut rng = Rng::new(0x15A5_EED0);
    let mut recon = Reconstruction {
        poses: vec![None; n],
        tracks: Vec::new(),
    };
    let mut kp_track: Vec<HashMap<usize, usize>> = vec![HashMap::new(); n];

    // ---- 1. Seed ----------------------------------------------------------
    let (i0, j0, pose_j, seed_matches, seed_inliers) = find_seed(frames, intr, &mut rng)
        .ok_or("no image pair had enough parallax to start from")?;

    recon.poses[i0] = Some(Pose::identity());
    recon.poses[j0] = Some(pose_j);
    let first = Pose::identity();
    for &k in &seed_inliers {
        let (a, b) = seed_matches[k];
        let xa = bearing(&frames[i0], a, intr);
        let xb = bearing(&frames[j0], b, intr);
        let p = match triangulate(&first, &pose_j, xa, xb) {
            Some(p) => p,
            None => continue,
        };
        if !point_is_good(&first, &pose_j, xa, xb, p, max_err) {
            continue;
        }
        let t = recon.tracks.len();
        recon.tracks.push(Track {
            xyz: p,
            rgb: keypoint_colour(&frames[i0], &thumbs[i0], a),
            obs: vec![(i0, a), (j0, b)],
        });
        kp_track[i0].insert(a, t);
        kp_track[j0].insert(b, t);
    }
    if recon.tracks.len() < MIN_SEED_INLIERS / 2 {
        return Err("the seed pair produced too few stable points".into());
    }

    let mut order = vec![i0, j0];
    for &f in &order {
        let depth = median(
            recon
                .tracks
                .iter()
                .filter(|t| t.obs.iter().any(|(g, _)| *g == f))
                .map(|t| recon.poses[f].unwrap().apply(t.xyz)[2])
                .collect(),
        );
        on_register(RegisterEvent {
            frame: &frames[f],
            pose: recon.poses[f].unwrap(),
            registered: order.iter().take_while(|&&g| g != f).count() + 1,
            total: n,
            confidence: 1.0,
            median_depth: depth,
        })?;
    }

    // ---- 2. Register the rest ---------------------------------------------
    let mut consecutive_failures = 0usize;
    for k in 0..n {
        if recon.poses[k].is_some() {
            continue;
        }

        // Nearest already-registered frames, which share the most view.
        let mut refs: Vec<usize> = order.clone();
        refs.sort_by_key(|&r| (r as i64 - k as i64).abs());
        refs.truncate(REF_FRAMES);
        refs.sort_unstable();

        let pairs: Vec<(usize, Vec<(usize, usize)>)> = refs
            .iter()
            .map(|&r| (r, match_frames(&frames[k], &frames[r], MatchOptions::default())))
            .collect();

        // 3D-2D correspondences: a keypoint in `k` matched to a keypoint in a
        // reference frame that already belongs to a track.
        let mut used_kp: HashSet<usize> = HashSet::new();
        let mut used_track: HashSet<usize> = HashSet::new();
        let mut cand: Vec<(usize, usize)> = Vec::new(); // (track, kp in k)
        for (r, m) in &pairs {
            for &(a, b) in m {
                let t = match kp_track[*r].get(&b) {
                    Some(t) => *t,
                    None => continue,
                };
                if used_kp.contains(&a) || used_track.contains(&t) {
                    continue;
                }
                used_kp.insert(a);
                used_track.insert(t);
                cand.push((t, a));
            }
        }

        let failure = |why: &str| format!("frame {} {}", frames[k].name, why);
        if cand.len() < MIN_PNP_MATCHES {
            consecutive_failures += 1;
            if consecutive_failures > MAX_CONSECUTIVE_FAILURES {
                return Err(failure("and the frames before it saw too few known points"));
            }
            continue;
        }

        let pts: Vec<V3> = cand.iter().map(|&(t, _)| recon.tracks[t].xyz).collect();
        let obs: Vec<[f64; 2]> = cand
            .iter()
            .map(|&(_, a)| bearing(&frames[k], a, intr))
            .collect();

        let (pose, inliers) =
            match ransac_pnp(&pts, &obs, intr.mean_focal(), RANSAC_PX, 400, &mut rng) {
                Some(v) => v,
                None => {
                    consecutive_failures += 1;
                    if consecutive_failures > MAX_CONSECUTIVE_FAILURES {
                        return Err(failure("and the frames before it could not be located"));
                    }
                    continue;
                }
            };

        let confidence = inliers.len() as f32 / cand.len() as f32;
        if confidence < MIN_CONFIDENCE {
            consecutive_failures += 1;
            if consecutive_failures > MAX_CONSECUTIVE_FAILURES {
                return Err(failure("and the frames before it matched too poorly"));
            }
            continue;
        }
        consecutive_failures = 0;

        recon.poses[k] = Some(pose);
        order.push(k);
        for &c in &inliers {
            let (t, a) = cand[c];
            recon.tracks[t].obs.push((k, a));
            kp_track[k].insert(a, t);
        }

        // ---- 3. Triangulate what is still unexplained ----------------------
        for (r, m) in &pairs {
            let rp = recon.poses[*r].unwrap();
            for &(a, b) in m {
                if kp_track[k].contains_key(&a) || kp_track[*r].contains_key(&b) {
                    continue;
                }
                let xk = bearing(&frames[k], a, intr);
                let xr = bearing(&frames[*r], b, intr);
                let p = match triangulate(&rp, &pose, xr, xk) {
                    Some(p) => p,
                    None => continue,
                };
                if !point_is_good(&rp, &pose, xr, xk, p, max_err) {
                    continue;
                }
                let t = recon.tracks.len();
                recon.tracks.push(Track {
                    xyz: p,
                    rgb: keypoint_colour(&frames[k], &thumbs[k], a),
                    obs: vec![(*r, b), (k, a)],
                });
                kp_track[*r].insert(b, t);
                kp_track[k].insert(a, t);
            }
        }

        // ---- 4. Local bundle adjustment ------------------------------------
        let window: Vec<usize> = order.iter().rev().take(BA_WINDOW).copied().collect();
        if window.len() > 2 {
            let free: Vec<usize> = window[..window.len() - 2].to_vec();
            adjust_with_frames(frames, &mut recon, &free, intr, 5);
        }

        let depth = median(
            recon
                .tracks
                .iter()
                .filter(|t| t.obs.iter().any(|(g, _)| *g == k))
                .map(|t| recon.poses[k].unwrap().apply(t.xyz)[2])
                .collect(),
        );
        on_register(RegisterEvent {
            frame: &frames[k],
            pose: recon.poses[k].unwrap(),
            registered: order.len(),
            total: n,
            confidence,
            median_depth: depth,
        })?;
    }

    let registered = order.len();
    if (registered as f32) < MIN_REGISTERED_FRACTION * n as f32 {
        return Err(format!("only {registered} of {n} frames could be located"));
    }
    Ok(recon)
}

/// Refine every pose in `free`, and the points they see, against the cameras
/// that hold them in place.
///
/// Any camera that observes a selected point but is not listed in `free` is
/// held fixed. Those cameras stop a window of poses from drifting away from
/// the part of the reconstruction that has already settled, and they are also
/// what removes the gauge freedom. When the window happens to cover every
/// observer, the two oldest cameras are pinned instead, which anchors both the
/// coordinate frame and the scale.
fn adjust_with_frames(
    frames: &[Frame],
    recon: &mut Reconstruction,
    free: &[usize],
    intr: Intrinsics,
    iterations: usize,
) {
    if free.is_empty() || recon.tracks.is_empty() {
        return;
    }
    let free_set: HashSet<usize> = free.iter().copied().collect();

    let mut sel: Vec<usize> = (0..recon.tracks.len())
        .filter(|&t| {
            recon.tracks[t].obs.len() >= 2
                && recon.tracks[t].obs.iter().any(|(f, _)| free_set.contains(f))
        })
        .collect();
    if sel.is_empty() {
        return;
    }
    if sel.len() > MAX_BA_POINTS {
        sel.sort_by_key(|&t| (std::cmp::Reverse(recon.tracks[t].obs.len()), t));
        sel.truncate(MAX_BA_POINTS);
        sel.sort_unstable();
    }

    let mut cams: Vec<usize> = sel
        .iter()
        .flat_map(|&t| recon.tracks[t].obs.iter().map(|(f, _)| *f))
        .filter(|f| recon.poses[*f].is_some())
        .collect::<HashSet<usize>>()
        .into_iter()
        .collect();
    cams.sort_unstable();

    let (mut fixed, mut movable): (Vec<usize>, Vec<usize>) =
        cams.iter().partition(|c| !free_set.contains(c));
    while fixed.len() < 2 && !movable.is_empty() {
        fixed.push(movable.remove(0));
    }
    if movable.is_empty() {
        return;
    }
    fixed.sort_unstable();

    let mut order = fixed.clone();
    order.extend(movable.iter().copied());
    let local_cam: HashMap<usize, usize> = order.iter().enumerate().map(|(i, &c)| (c, i)).collect();
    let local_pt: HashMap<usize, usize> = sel.iter().enumerate().map(|(i, &t)| (t, i)).collect();

    let mut ba_poses: Vec<Pose> = order.iter().map(|&c| recon.poses[c].unwrap()).collect();
    let mut ba_points: Vec<V3> = sel.iter().map(|&t| recon.tracks[t].xyz).collect();

    let mut obs = Vec::new();
    for &t in &sel {
        for &(f, k) in &recon.tracks[t].obs {
            if let Some(&cam) = local_cam.get(&f) {
                obs.push(Observation {
                    cam,
                    point: local_pt[&t],
                    obs: bearing(&frames[f], k, intr),
                });
            }
        }
    }

    bundle_adjust(
        &mut ba_poses,
        &mut ba_points,
        &obs,
        BaOptions {
            iterations,
            huber: RANSAC_PX / intr.mean_focal(),
            fixed_cams: fixed.len(),
        },
    );

    for (i, &c) in order.iter().enumerate().skip(fixed.len()) {
        recon.poses[c] = Some(ba_poses[i]);
    }
    for (i, &t) in sel.iter().enumerate() {
        recon.tracks[t].xyz = ba_points[i];
    }
}

/// Sweep the whole sequence with overlapping local adjustments.
///
/// A full bundle adjustment over hundreds of cameras would mean a dense
/// reduced system of thousands of unknowns, which is far too slow to run while
/// the user waits. Overlapping windows propagate the same corrections at a
/// cost that stays linear in the number of frames.
pub fn refine_sweep(
    frames: &[Frame],
    recon: &mut Reconstruction,
    intr: Intrinsics,
    passes: usize,
    mut on_progress: impl FnMut(f32),
) {
    let order = recon.registered();
    if order.len() < 4 {
        return;
    }
    let window = BA_WINDOW + 2;
    let stride = (window / 2).max(1);
    let starts: Vec<usize> = (0..order.len()).step_by(stride).collect();
    let total = (starts.len() * passes).max(1);
    let mut done = 0usize;

    for _ in 0..passes {
        for &start in &starts {
            let end = (start + window).min(order.len());
            if end - start >= 4 {
                let free: Vec<usize> = order[start + 2..end].to_vec();
                adjust_with_frames(frames, recon, &free, intr, 4);
            }
            done += 1;
            on_progress(done as f32 / total as f32);
        }
    }
}

// ---- Model output ----------------------------------------------------------

const IMAGE_EXTS: [&str; 6] = ["jpg", "jpeg", "png", "bmp", "tif", "tiff"];

fn list_images(dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("Cannot read {}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    Ok(paths)
}

/// Turn the reconstruction into the COLMAP sparse model the trainer reads.
///
/// The engine works on downscaled images, so every pixel quantity is scaled
/// back to the original resolution on the way out. Frames that never
/// registered are simply absent, exactly as COLMAP's own mapper leaves them.
fn build_model(
    frames: &[Frame],
    recon: &Reconstruction,
    intr: Intrinsics,
    scale_up: f64,
    orig: (u64, u64),
) -> Model {
    let mut model = Model::default();
    model.cameras.insert(
        1,
        Camera {
            id: 1,
            model: CameraModel::Pinhole,
            width: orig.0,
            height: orig.1,
            params: vec![
                intr.fx * scale_up,
                intr.fy * scale_up,
                intr.cx * scale_up,
                intr.cy * scale_up,
            ],
        },
    );

    // Image ids are one-based and dense over the registered frames.
    let registered = recon.registered();
    let image_id: HashMap<usize, u32> = registered
        .iter()
        .enumerate()
        .map(|(i, &f)| (f, i as u32 + 1))
        .collect();

    // Where each keypoint lands in its image's points2d list, and which point
    // it observes. Tracks are one-based too.
    let mut observed: Vec<HashMap<usize, u64>> = vec![HashMap::new(); frames.len()];
    for (t, track) in recon.tracks.iter().enumerate() {
        for &(f, k) in &track.obs {
            if image_id.contains_key(&f) {
                observed[f].insert(k, t as u64 + 1);
            }
        }
    }

    for &f in &registered {
        let pose = recon.poses[f].unwrap();
        let points2d = frames[f]
            .keypoints
            .iter()
            .enumerate()
            .map(|(k, kp)| {
                let id = observed[f].get(&k).copied().unwrap_or(u64::MAX);
                (kp.x as f64 * scale_up, kp.y as f64 * scale_up, id)
            })
            .collect();
        model.images.push(Image {
            id: image_id[&f],
            qvec: m3_to_quat(pose.r),
            tvec: pose.t,
            camera_id: 1,
            name: frames[f].name.clone(),
            points2d,
        });
    }

    for (t, track) in recon.tracks.iter().enumerate() {
        let mut err = 0.0;
        let mut n = 0usize;
        let mut list = Vec::new();
        for &(f, k) in &track.obs {
            let id = match image_id.get(&f) {
                Some(id) => *id,
                None => continue,
            };
            if let Some(p) = &recon.poses[f] {
                if let Some(e) = reprojection_error(p, track.xyz, bearing(&frames[f], k, intr)) {
                    err += e.sqrt();
                    n += 1;
                }
            }
            list.push((id, k as u32));
        }
        if list.len() < 2 {
            continue;
        }
        model.points.push(Point3D {
            id: t as u64 + 1,
            xyz: track.xyz,
            rgb: track.rgb,
            // COLMAP reports this in pixels of the original image.
            error: if n > 0 {
                err / n as f64 * intr.mean_focal() * scale_up
            } else {
                0.0
            },
            track: list,
        });
    }

    model
}

/// World-space frustum for the viewport: the camera centre, and the four
/// image corners pushed out to `depth`.
fn frustum(pose: &Pose, intr: Intrinsics, w: usize, h: usize, depth: f64) -> ([f32; 3], [[f32; 3]; 4]) {
    let apex = pose.center();
    let rt = m3_transpose(pose.r);
    let corner = |u: f64, v: f64| -> [f32; 3] {
        let cam = [
            (u - intr.cx) / intr.fx * depth,
            (v - intr.cy) / intr.fy * depth,
            depth,
        ];
        let world = m3_mul_v(rt, sub(cam, pose.t));
        [world[0] as f32, world[1] as f32, world[2] as f32]
    };
    let (w, h) = (w as f64, h as f64);
    (
        [apex[0] as f32, apex[1] as f32, apex[2] as f32],
        [
            corner(0.0, 0.0),
            corner(w, 0.0),
            corner(w, h),
            corner(0.0, h),
        ],
    )
}

/// Run the incremental engine over `images_dir` and leave a COLMAP sparse
/// model in `<workspace>/sparse/0`.
///
/// Every error string returned here is shown to the user verbatim, prefixed by
/// the pipeline with a plain statement that it is switching to the batch
/// solver. `__cancelled__` is the one value that must propagate untouched.
pub async fn run_incremental(ctx: &JobCtx, images_dir: &Path) -> Result<(), String> {
    tokio::task::block_in_place(|| run_blocking(ctx, images_dir))
}

fn run_blocking(ctx: &JobCtx, images_dir: &Path) -> Result<(), String> {
    let paths = list_images(images_dir)?;
    if paths.len() < 3 {
        return Err("fewer than three images to work from".into());
    }
    ctx.check_cancel()?;

    // ---- Detect features on every frame ------------------------------------
    ctx.stage_progress("sfm", 0.0, "Finding features…");
    let done = AtomicUsize::new(0);
    let total = paths.len();
    let loaded: Vec<Result<(Frame, Thumb, f32), String>> = paths
        .par_iter()
        .map(|p| {
            if ctx.cancelled() {
                return Err("__cancelled__".to_string());
            }
            let l = features::load_frame(p, WORK_MAX_DIM, THUMB_MAX_DIM)?;
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let frame = features::detect_and_describe(&name, &l.gray, DetectOptions::default());
            let seen = done.fetch_add(1, Ordering::Relaxed) + 1;
            if seen % 4 == 0 || seen == total {
                ctx.stage_progress(
                    "sfm",
                    0.30 * seen as f32 / total as f32,
                    &format!("Finding features in frame {seen} of {total}"),
                );
            }
            Ok((frame, l.thumb, l.scale))
        })
        .collect();

    let mut frames: Vec<Frame> = Vec::with_capacity(total);
    let mut thumbs: Vec<Thumb> = Vec::with_capacity(total);
    let mut scale_up = 0.0f32;
    for entry in loaded {
        let (frame, thumb, scale) = entry?;
        if frames.is_empty() {
            scale_up = scale;
        } else if frame.width != frames[0].width || frame.height != frames[0].height {
            return Err("the frames are not all the same size".into());
        }
        frames.push(frame);
        thumbs.push(thumb);
    }
    ctx.check_cancel()?;

    let weak = frames.iter().filter(|f| f.len() < 200).count();
    if weak * 3 > frames.len() {
        return Err("most frames are too smooth or too blurred to track".into());
    }

    // ---- Intrinsics ---------------------------------------------------------
    // Without EXIF this is the same prior COLMAP starts from: a field of view
    // near 45 degrees. Bundle adjustment does not refine it, so the poses carry
    // whatever error the guess introduces; the batch path remains the accurate
    // one, which is why live init is opt-in.
    let (w, h) = (frames[0].width, frames[0].height);
    let f = 1.2 * w.max(h) as f64;
    let intr = Intrinsics {
        fx: f,
        fy: f,
        cx: w as f64 / 2.0,
        cy: h as f64 / 2.0,
    };

    // ---- Reconstruct --------------------------------------------------------
    ctx.stage_progress("sfm", 0.30, "Locating the first cameras…");
    let mut recon = reconstruct(&frames, &thumbs, intr, |ev| {
        if ctx.cancelled() {
            return Err("__cancelled__".to_string());
        }
        let depth = if ev.median_depth.is_finite() && ev.median_depth > 1e-6 {
            ev.median_depth * 0.12
        } else {
            0.1
        };
        let (apex, corners) = frustum(&ev.pose, intr, ev.frame.width, ev.frame.height, depth);
        ctx.emit(JobEvent::CameraRegistered {
            job_id: ctx.job_id.clone(),
            name: ev.frame.name.clone(),
            registered: ev.registered as u32,
            total: ev.total as u32,
            confidence: ev.confidence,
            apex,
            corners,
        });
        ctx.stage_progress(
            "sfm",
            0.30 + 0.55 * ev.registered as f32 / ev.total as f32,
            &format!("Located {} of {} cameras", ev.registered, ev.total),
        );
        Ok(())
    })?;
    ctx.check_cancel()?;

    // ---- Refine and clean ---------------------------------------------------
    ctx.stage_progress("sfm", 0.85, "Refining camera poses…");
    refine_sweep(&frames, &mut recon, intr, 2, |frac| {
        ctx.stage_progress("sfm", 0.85 + 0.10 * frac, "Refining camera poses…");
    });
    ctx.check_cancel()?;

    let dropped = prune(&frames, &recon.poses, &mut recon.tracks, intr, RANSAC_PX * 2.0);
    let registered = recon.registered().len();
    if recon.tracks.len() < 200 {
        return Err("too few stable points survived refinement".into());
    }
    ctx.log(format!(
        "Live camera tracking: {registered} of {total} frames, {} points, {dropped} discarded, {:.2} px rms",
        recon.tracks.len(),
        rms_px(&frames, &recon.poses, &recon.tracks, intr)
    ));
    if registered < total {
        ctx.notice(format!(
            "{} of {total} frames could not be located and will not be used for training.",
            total - registered
        ));
    }

    // ---- Write the sparse model --------------------------------------------
    ctx.stage_progress("sfm", 0.97, "Writing the camera model…");
    let orig = (
        (w as f32 * scale_up).round() as u64,
        (h as f32 * scale_up).round() as u64,
    );
    let model = build_model(&frames, &recon, intr, scale_up as f64, orig);
    let dir = ctx.workspace.join("sparse").join("0");
    crate::colmap::write_model_txt(&dir, &model)?;

    ctx.stage_progress("sfm", 1.0, "Cameras solved");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{norm, rodrigues, scale};
    use features::KeyPoint;

    fn intr() -> Intrinsics {
        Intrinsics {
            fx: 900.0,
            fy: 900.0,
            cx: 480.0,
            cy: 360.0,
        }
    }

    /// A descriptor unique to each 3D point, so matching is exact and the test
    /// exercises geometry rather than the descriptor.
    fn desc_for(point: usize) -> [u8; features::DESC_BYTES] {
        let mut state = (point as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut d = [0u8; features::DESC_BYTES];
        for v in d.iter_mut() {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *v = (state >> 33) as u8;
        }
        d
    }

    fn cloud(n: usize) -> Vec<V3> {
        let mut rng = Rng::new(99);
        (0..n)
            .map(|_| {
                let f = |lo: f64, hi: f64, r: &mut Rng| {
                    lo + (r.next_u64() % 100_000) as f64 / 100_000.0 * (hi - lo)
                };
                [
                    f(-3.0, 3.0, &mut rng),
                    f(-2.0, 2.0, &mut rng),
                    f(6.0, 14.0, &mut rng),
                ]
            })
            .collect()
    }

    /// Cameras sliding sideways while turning slightly inward.
    fn truth_poses(n: usize) -> Vec<Pose> {
        (0..n)
            .map(|c| {
                let a = c as f64 * 0.05;
                Pose {
                    r: rodrigues([0.005 * c as f64, a, -0.004 * c as f64]),
                    t: [-0.5 * c as f64, 0.03 * c as f64, 0.02 * c as f64],
                }
            })
            .collect()
    }

    /// Project a cloud into each pose and package the result as `Frame`s.
    fn synthetic(points: &[V3], poses: &[Pose]) -> (Vec<Frame>, Vec<Thumb>) {
        let k = intr();
        let (w, h) = (960usize, 720usize);
        let mut frames = Vec::new();
        for (ci, pose) in poses.iter().enumerate() {
            let mut keypoints = Vec::new();
            let mut descriptors = Vec::new();
            for (pi, p) in points.iter().enumerate() {
                let c = pose.apply(*p);
                let px = match k.project(c) {
                    Some(px) => px,
                    None => continue,
                };
                if px[0] < 0.0 || px[1] < 0.0 || px[0] >= w as f64 || px[1] >= h as f64 {
                    continue;
                }
                keypoints.push(KeyPoint {
                    x: px[0] as f32,
                    y: px[1] as f32,
                    score: 1.0,
                    angle: 0.0,
                });
                descriptors.extend_from_slice(&desc_for(pi));
            }
            frames.push(Frame {
                name: format!("frame_{ci:03}.png"),
                width: w,
                height: h,
                keypoints,
                descriptors,
            });
        }
        let thumbs: Vec<Thumb> = (0..poses.len())
            .map(|_| Thumb {
                width: 2,
                height: 2,
                rgb: vec![200, 100, 50, 200, 100, 50, 200, 100, 50, 200, 100, 50],
            })
            .collect();
        (frames, thumbs)
    }

    /// Poses are only defined up to a similarity. Express the estimate and the
    /// truth in the first camera's frame and recover the single scale factor.
    fn scale_between(est: &[Option<Pose>], truth: &[Pose], a: usize, b: usize) -> f64 {
        let ec = sub(est[b].unwrap().center(), est[a].unwrap().center());
        let tc = sub(
            relative(&truth[b], &truth[a]).center(),
            relative(&truth[a], &truth[a]).center(),
        );
        norm(ec) / norm(tc).max(1e-12)
    }

    /// `p` expressed relative to `base`: the pose it would have if `base` were
    /// the world origin.
    fn relative(p: &Pose, base: &Pose) -> Pose {
        // x_p = R_p x_w + t_p and x_b = R_b x_w + t_b, so
        // x_p = R_p R_bᵀ (x_b - t_b) + t_p.
        let r = crate::math::m3_mul(p.r, m3_transpose(base.r));
        Pose {
            r,
            t: sub(p.t, m3_mul_v(r, base.t)),
        }
    }

    #[test]
    fn the_seed_pair_has_real_parallax() {
        let points = cloud(300);
        let poses = truth_poses(6);
        let (frames, _) = synthetic(&points, &poses);
        let mut rng = Rng::new(1);
        let (i, j, _, _, inliers) = find_seed(&frames, intr(), &mut rng).expect("no seed found");
        assert_eq!(i, 0);
        assert!(j >= 1 && j < 6, "seed second frame {j}");
        assert!(inliers.len() >= MIN_SEED_INLIERS);
    }

    #[test]
    fn a_static_camera_never_seeds() {
        let points = cloud(300);
        // Every camera at exactly the same place: no baseline, no parallax.
        let poses = vec![Pose::identity(); 5];
        let (frames, _) = synthetic(&points, &poses);
        let mut rng = Rng::new(1);
        assert!(find_seed(&frames, intr(), &mut rng).is_none());
    }

    #[test]
    fn reconstruct_recovers_every_pose_up_to_scale() {
        let points = cloud(400);
        let truth = truth_poses(7);
        let (frames, thumbs) = synthetic(&points, &truth);

        let mut seen = Vec::new();
        let recon = reconstruct(&frames, &thumbs, intr(), |ev| {
            seen.push(ev.frame.name.clone());
            assert!(ev.confidence > 0.0 && ev.confidence <= 1.0);
            Ok(())
        })
        .expect("reconstruction failed");

        assert_eq!(recon.registered().len(), 7, "not every frame registered");
        assert_eq!(seen.len(), 7, "not every camera was announced");
        assert!(recon.tracks.len() > 200, "{} points", recon.tracks.len());

        // The engine puts its first camera at the origin, so compare truth in
        // that camera's frame.
        let anchor = 0usize;
        let s = scale_between(&recon.poses, &truth, anchor, 1);
        assert!(s.is_finite() && s > 0.0, "degenerate scale {s}");

        for c in 0..7 {
            let est = recon.poses[c].expect("missing pose");
            let want = relative(&truth[c], &truth[anchor]);
            assert!(
                geometry::rotation_angle_between(est.r, want.r) < 0.02,
                "camera {c} rotation off by {}",
                geometry::rotation_angle_between(est.r, want.r)
            );
            // Centres agree once the arbitrary global scale is removed.
            let ec = est.center();
            let wc = scale(want.center(), s);
            let err = norm(sub(ec, wc));
            let extent = norm(wc).max(1.0);
            assert!(err / extent < 0.05, "camera {c} centre off by {err}");
        }
    }

    #[test]
    fn the_reconstruction_reprojects_where_the_points_were_seen() {
        let points = cloud(400);
        let truth = truth_poses(6);
        let (frames, thumbs) = synthetic(&points, &truth);
        let recon = reconstruct(&frames, &thumbs, intr(), |_| Ok(())).unwrap();
        let rms = rms_px(&frames, &recon.poses, &recon.tracks, intr());
        assert!(rms < 1.0, "rms {rms} px");
    }

    #[test]
    fn refinement_does_not_make_the_reconstruction_worse() {
        let points = cloud(400);
        let truth = truth_poses(8);
        let (frames, thumbs) = synthetic(&points, &truth);
        let mut recon = reconstruct(&frames, &thumbs, intr(), |_| Ok(())).unwrap();
        let before = rms_px(&frames, &recon.poses, &recon.tracks, intr());
        refine_sweep(&frames, &mut recon, intr(), 2, |_| {});
        let after = rms_px(&frames, &recon.poses, &recon.tracks, intr());
        assert!(after <= before + 1e-6, "rms grew from {before} to {after}");
    }

    #[test]
    fn a_cancelling_callback_aborts_the_run() {
        let points = cloud(300);
        let truth = truth_poses(6);
        let (frames, thumbs) = synthetic(&points, &truth);
        let err = reconstruct(&frames, &thumbs, intr(), |_| Err("__cancelled__".into()))
            .expect_err("should have aborted");
        assert_eq!(err, "__cancelled__");
    }

    #[test]
    fn too_few_frames_is_an_error_not_a_panic() {
        let points = cloud(100);
        let truth = truth_poses(2);
        let (frames, thumbs) = synthetic(&points, &truth);
        assert!(reconstruct(&frames, &thumbs, intr(), |_| Ok(())).is_err());
    }

    #[test]
    fn pruning_removes_an_observation_that_no_longer_fits() {
        let points = cloud(300);
        let truth = truth_poses(5);
        let (frames, thumbs) = synthetic(&points, &truth);
        let mut recon = reconstruct(&frames, &thumbs, intr(), |_| Ok(())).unwrap();

        // A clean reconstruction loses nothing.
        assert_eq!(prune(&frames, &recon.poses, &mut recon.tracks, intr(), 4.0), 0);

        // Move one point far away; every one of its observations is now wrong.
        let victim = 0usize;
        let obs_before = recon.tracks[victim].obs.len();
        assert!(obs_before >= 2);
        recon.tracks[victim].xyz = [50.0, 50.0, 50.0];
        let dropped = prune(&frames, &recon.poses, &mut recon.tracks, intr(), 4.0);
        assert_eq!(dropped, 1, "the displaced point should have been removed");
    }

    #[test]
    fn the_model_carries_poses_points_and_original_resolution() {
        let points = cloud(400);
        let truth = truth_poses(6);
        let (frames, thumbs) = synthetic(&points, &truth);
        let recon = reconstruct(&frames, &thumbs, intr(), |_| Ok(())).unwrap();

        // Pretend the working images were half the size of the originals.
        let model = build_model(&frames, &recon, intr(), 2.0, (1920, 1440));
        assert_eq!(model.images.len(), 6);
        assert!(model.points.len() > 200);

        let cam = &model.cameras[&1];
        assert_eq!(cam.model, CameraModel::Pinhole);
        assert_eq!((cam.width, cam.height), (1920, 1440));
        assert_eq!(cam.focal(), (1800.0, 1800.0));
        assert_eq!(cam.principal_point(), (960.0, 720.0));

        // Image ids are one-based and dense; names survive.
        let ids: Vec<u32> = model.images.iter().map(|i| i.id).collect();
        assert_eq!(ids, (1..=6).collect::<Vec<_>>());
        assert_eq!(model.images[0].name, "frame_000.png");

        // Every track entry points at a real image and a real keypoint.
        for p in &model.points {
            assert!(p.track.len() >= 2);
            for &(img, kp) in &p.track {
                let img = model.images.iter().find(|i| i.id == img).expect("bad image id");
                assert!((kp as usize) < img.points2d.len());
                assert_eq!(img.points2d[kp as usize].2, p.id, "observation disagrees");
            }
        }

        // A pose round-trips through the quaternion.
        let est = recon.poses[3].unwrap();
        let back = crate::colmap::quat_to_m3(model.images[3].qvec);
        assert!(geometry::rotation_angle_between(est.r, back) < 1e-9);
    }

    #[test]
    fn a_frustum_opens_away_from_the_camera_centre() {
        let pose = Pose {
            r: rodrigues([0.1, -0.2, 0.05]),
            t: [0.3, -0.1, 0.4],
        };
        let k = intr();
        let (apex, corners) = frustum(&pose, k, 960, 720, 2.0);
        let apex64 = [apex[0] as f64, apex[1] as f64, apex[2] as f64];

        // The apex is the camera centre.
        let c = pose.center();
        for i in 0..3 {
            assert!((apex64[i] - c[i]).abs() < 1e-6);
        }
        // Each corner is in front of the camera, at the requested depth.
        for corner in corners {
            let w = [corner[0] as f64, corner[1] as f64, corner[2] as f64];
            let cam = pose.apply(w);
            assert!((cam[2] - 2.0).abs() < 1e-6, "depth {}", cam[2]);
        }
        // The four corners are distinct and spread around the axis.
        for i in 0..4 {
            for j in (i + 1)..4 {
                let d = norm(sub(
                    [corners[i][0] as f64, corners[i][1] as f64, corners[i][2] as f64],
                    [corners[j][0] as f64, corners[j][1] as f64, corners[j][2] as f64],
                ));
                assert!(d > 0.1, "corners {i} and {j} coincide");
            }
        }
    }
}
