//! Mip-Splatting's 3D smoothing filter (ROADMAP-V2 1.5).
//!
//! A Gaussian smaller than the pixel footprint of the closest camera that
//! sees it is not constrained by any observation, which is where spiky
//! artefacts and needle-shaped floaters come from. Mip-Splatting bounds each
//! Gaussian below by the scene's maximal sampling rate:
//!
//! ```text
//!   nu_k   = max over views that see k of (focal_n / depth_nk)
//!   Sigma' = Sigma + (s / nu_k)^2 * I          with s = 0.2
//!   alpha' = alpha * sqrt(|Sigma| / |Sigma'|)
//! ```
//!
//! The convolution is isotropic, so in the splat's own eigenframe it reduces
//! to `scale_i' = sqrt(scale_i^2 + sigma^2)`, and no rotation is touched.
//!
//! The paper applies this inside the training loop. We drive Brush through
//! its CLI, so instead we apply the filter at each progressive-resolution
//! stage boundary and bake it once more into the final export. Applying it
//! between stages means the optimiser still gets to compensate for the
//! widened Gaussians on the next stage, which is the property that matters.

use super::SplatCloud;
use crate::colmap::Model;
use rayon::prelude::*;

/// Filter strength from the Mip-Splatting paper.
pub const DEFAULT_FILTER_SIZE: f32 = 0.2;

/// Per-splat maximal sampling rate `nu_k` in pixels per world unit, or `None`
/// where no training view observes the splat.
///
/// `focal_scale` accounts for training on downscaled images: the sampling rate
/// that actually constrains a Gaussian is the one at the resolution the
/// optimiser sees, not the resolution the photographs were taken at.
fn sampling_rates(cloud: &SplatCloud, model: &Model, focal_scale: f32) -> Vec<Option<f32>> {
    // Cache each view's focal and bounds once.
    let views: Vec<(&crate::colmap::Image, f64, f64, f64)> = model
        .images
        .iter()
        .filter_map(|img| {
            let cam = model.cameras.get(&img.camera_id)?;
            let (fx, fy) = cam.focal();
            Some((img, (fx + fy) * 0.5, cam.width as f64, cam.height as f64))
        })
        .collect();

    if views.is_empty() {
        return vec![None; cloud.len()];
    }

    cloud
        .positions
        .par_iter()
        .map(|p| {
            let pw = [p[0] as f64, p[1] as f64, p[2] as f64];
            let mut best: Option<f32> = None;
            for (img, focal, w, h) in &views {
                let cam = img.world_to_cam(pw);
                let depth = cam[2];
                if depth <= 1e-4 {
                    continue;
                }
                // A 10% border margin keeps splats just outside the frame from
                // being dropped by a hard frustum test.
                let camera = &model.cameras[&img.camera_id];
                if let Some((u, v)) = camera.project(cam) {
                    let (mx, my) = (w * 0.1, h * 0.1);
                    if u < -mx || u > w + mx || v < -my || v > h + my {
                        continue;
                    }
                } else {
                    continue;
                }
                let nu = (focal / depth) as f32 * focal_scale;
                best = Some(match best {
                    Some(b) if b >= nu => b,
                    _ => nu,
                });
            }
            best
        })
        .collect()
}

/// Result of one filter pass, for logging.
#[derive(Debug, Clone, Copy, Default)]
pub struct FilterStats {
    pub filtered: usize,
    pub skipped_unobserved: usize,
    pub mean_sigma: f32,
}

/// Scale from full-resolution focal lengths to the focal lengths actually
/// used when training is capped at `max_resolution` on the longest side.
pub fn focal_scale_for(model: &Model, max_resolution: u32) -> f32 {
    let longest = model
        .cameras
        .values()
        .map(|c| c.width.max(c.height))
        .max()
        .unwrap_or(0);
    if longest == 0 || max_resolution == 0 || max_resolution as u64 >= longest {
        1.0
    } else {
        max_resolution as f32 / longest as f32
    }
}

/// Apply the 3D smoothing filter in place. `filter_size` is `s` above, and
/// `focal_scale` maps full-resolution focals onto the training resolution.
pub fn apply_3d_filter(
    cloud: &mut SplatCloud,
    model: &Model,
    filter_size: f32,
    focal_scale: f32,
) -> FilterStats {
    let rates = sampling_rates(cloud, model, focal_scale.max(1e-6));

    let mut filtered = 0usize;
    let mut skipped = 0usize;
    let mut sigma_sum = 0.0f64;

    for i in 0..cloud.len() {
        let nu = match rates[i] {
            Some(v) if v > 1e-6 => v,
            // Never observed: no sampling rate is defined, so leave it alone
            // rather than inventing a bound from unrelated cameras.
            _ => {
                skipped += 1;
                continue;
            }
        };
        let sigma = filter_size / nu;
        let sigma2 = sigma * sigma;

        let s = cloud.scale(i);
        let s2 = [s[0] * s[0], s[1] * s[1], s[2] * s[2]];
        let n2 = [s2[0] + sigma2, s2[1] + sigma2, s2[2] + sigma2];

        // Preserve the integrated opacity: alpha * sqrt(|Sigma| / |Sigma'|).
        let det_ratio = (s2[0] / n2[0]) * (s2[1] / n2[1]) * (s2[2] / n2[2]);
        let alpha = cloud.opacity(i) * det_ratio.sqrt();
        let alpha = alpha.clamp(1e-6, 1.0 - 1e-6);

        cloud.scales_log[i] = [
            0.5 * n2[0].ln(),
            0.5 * n2[1].ln(),
            0.5 * n2[2].ln(),
        ];
        cloud.opacity_logit[i] = (alpha / (1.0 - alpha)).ln();

        sigma_sum += sigma as f64;
        filtered += 1;
    }

    FilterStats {
        filtered,
        skipped_unobserved: skipped,
        mean_sigma: if filtered > 0 {
            (sigma_sum / filtered as f64) as f32
        } else {
            0.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colmap::{Camera, CameraModel, Image};
    use std::collections::HashMap;

    /// One camera at the origin looking down +z with focal 1000 px.
    fn single_view_model() -> Model {
        let mut cameras = HashMap::new();
        cameras.insert(
            1,
            Camera {
                id: 1,
                model: CameraModel::Pinhole,
                width: 1000,
                height: 1000,
                params: vec![1000.0, 1000.0, 500.0, 500.0],
            },
        );
        Model {
            cameras,
            images: vec![Image {
                id: 1,
                qvec: [1.0, 0.0, 0.0, 0.0],
                tvec: [0.0, 0.0, 0.0],
                camera_id: 1,
                name: "a.jpg".into(),
                points2d: vec![],
            }],
            points: vec![],
        }
    }

    fn one_splat(pos: [f32; 3], scale: f32, opacity: f32) -> SplatCloud {
        let logit = (opacity / (1.0 - opacity)).ln();
        SplatCloud {
            positions: vec![pos],
            scales_log: vec![[scale.ln(); 3]],
            rot_wxyz: vec![[1.0, 0.0, 0.0, 0.0]],
            opacity_logit: vec![logit],
            sh_dc: vec![[0.0; 3]],
            sh_rest: vec![],
            rest_per_channel: 0,
        }
    }

    #[test]
    fn filter_widens_a_splat_to_the_sampling_bound() {
        let model = single_view_model();
        // Depth 10, focal 1000 => nu = 100 px/unit => sigma = 0.2/100 = 0.002.
        let mut cloud = one_splat([0.0, 0.0, 10.0], 0.001, 0.5);
        let stats = apply_3d_filter(&mut cloud, &model, DEFAULT_FILTER_SIZE, 1.0);

        assert_eq!(stats.filtered, 1);
        assert_eq!(stats.skipped_unobserved, 0);
        assert!((stats.mean_sigma - 0.002).abs() < 1e-6, "{stats:?}");

        let s = cloud.scale(0)[0];
        let expect = (0.001f32 * 0.001 + 0.002 * 0.002).sqrt();
        assert!((s - expect).abs() < 1e-7, "{s} vs {expect}");
    }

    #[test]
    fn a_splat_much_larger_than_the_filter_is_essentially_unchanged() {
        let model = single_view_model();
        let mut cloud = one_splat([0.0, 0.0, 10.0], 0.5, 0.6);
        let before_scale = cloud.scale(0)[0];
        let before_alpha = cloud.opacity(0);
        apply_3d_filter(&mut cloud, &model, DEFAULT_FILTER_SIZE, 1.0);
        assert!((cloud.scale(0)[0] - before_scale).abs() < 1e-4);
        assert!((cloud.opacity(0) - before_alpha).abs() < 1e-3);
    }

    #[test]
    fn opacity_falls_so_the_integrated_mass_is_preserved() {
        let model = single_view_model();
        let mut cloud = one_splat([0.0, 0.0, 10.0], 0.0005, 0.9);
        let s0 = cloud.scale(0);
        let a0 = cloud.opacity(0);
        apply_3d_filter(&mut cloud, &model, DEFAULT_FILTER_SIZE, 1.0);
        let s1 = cloud.scale(0);
        let a1 = cloud.opacity(0);

        assert!(a1 < a0, "opacity must drop: {a0} -> {a1}");
        // alpha * sqrt(det Sigma) is the conserved quantity.
        let m0 = a0 * (s0[0] * s0[1] * s0[2]);
        let m1 = a1 * (s1[0] * s1[1] * s1[2]);
        assert!((m0 - m1).abs() / m0 < 1e-3, "{m0} vs {m1}");
    }

    #[test]
    fn splats_behind_or_outside_every_camera_are_left_alone() {
        let model = single_view_model();
        // Behind the camera.
        let mut behind = one_splat([0.0, 0.0, -5.0], 0.001, 0.5);
        let before = behind.scales_log[0];
        let stats = apply_3d_filter(&mut behind, &model, DEFAULT_FILTER_SIZE, 1.0);
        assert_eq!(stats.filtered, 0);
        assert_eq!(stats.skipped_unobserved, 1);
        assert_eq!(behind.scales_log[0], before);

        // Far off to the side: projects well outside the image.
        let mut aside = one_splat([100.0, 0.0, 1.0], 0.001, 0.5);
        let stats = apply_3d_filter(&mut aside, &model, DEFAULT_FILTER_SIZE, 1.0);
        assert_eq!(stats.filtered, 0);
    }

    #[test]
    fn the_nearest_camera_determines_the_bound() {
        // Two views, one at depth 10 and one at depth 2. The closer view has
        // the higher sampling rate, so it should set the (smaller) sigma.
        let mut model = single_view_model();
        let second = Image {
            id: 2,
            qvec: [1.0, 0.0, 0.0, 0.0],
            // Camera centre at z = 8, still looking down +z.
            tvec: [0.0, 0.0, -8.0],
            camera_id: 1,
            name: "b.jpg".into(),
            points2d: vec![],
        };
        model.images.push(second);

        let mut cloud = one_splat([0.0, 0.0, 10.0], 1e-6, 0.5);
        let stats = apply_3d_filter(&mut cloud, &model, DEFAULT_FILTER_SIZE, 1.0);
        // nu = 1000 / 2 = 500 => sigma = 0.0004, not 0.002 from the far view.
        assert!((stats.mean_sigma - 0.0004).abs() < 1e-7, "{stats:?}");
    }

    #[test]
    fn training_on_downscaled_images_widens_the_filter() {
        let model = single_view_model();
        // Half resolution halves the effective focal, halving nu and doubling
        // sigma: 0.2 / (1000/10 * 0.5) = 0.004.
        let mut cloud = one_splat([0.0, 0.0, 10.0], 1e-6, 0.5);
        let stats = apply_3d_filter(&mut cloud, &model, DEFAULT_FILTER_SIZE, 0.5);
        assert!((stats.mean_sigma - 0.004).abs() < 1e-6, "{stats:?}");
    }

    #[test]
    fn focal_scale_is_the_ratio_of_training_to_native_resolution() {
        let model = single_view_model(); // 1000x1000
        assert_eq!(focal_scale_for(&model, 500), 0.5);
        // Never upscale: asking for more pixels than the source has changes nothing.
        assert_eq!(focal_scale_for(&model, 4000), 1.0);
        assert_eq!(focal_scale_for(&model, 1000), 1.0);
        assert_eq!(focal_scale_for(&Model::default(), 500), 1.0);
    }

    #[test]
    fn a_model_with_no_views_is_a_no_op() {
        let model = Model::default();
        let mut cloud = one_splat([0.0, 0.0, 10.0], 0.001, 0.5);
        let before = cloud.scales_log[0];
        let stats = apply_3d_filter(&mut cloud, &model, DEFAULT_FILTER_SIZE, 1.0);
        assert_eq!(stats.filtered, 0);
        assert_eq!(cloud.scales_log[0], before);
    }
}
