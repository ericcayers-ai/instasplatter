//! Analytic real-time sky colour (Preetham et al. 1999 single-scattering).
//!
//! Port of the standard Preetham formulation as shipped in three.js' Sky
//! shader (the reference open-source implementation of "A Practical Analytic
//! Model for Daylight"), re-derived as pure Rust over the shared [`Enu`]
//! type. Closed form, no iteration, cheap enough for per-pixel use.
//!
//! Wavelengths are the Preetham primaries (680/550/450 nm); Rayleigh uses the
//! 3/(16π)(1+cos²θ) phase and King-corrected total scattering precomputed in
//! `TOTAL_RAYLEIGH`; Mie is Junge/turbidity-driven (`TOTAL_MIE`) with an
//! Henyey-Greenstein lobe — the same real-time substitution Digital-Earth's
//! offline path makes when it swaps Klein-Nishina for HG, documented here per
//! the roadmap rule on explicit deviations.

use crate::geospatial::transforms::Enu;
use serde::{Deserialize, Serialize};

const PI: f64 = std::f64::consts::PI;

/// Preetham primary wavelengths (m).
pub const LAMBDA: [f64; 3] = [680e-9, 550e-9, 450e-9];

/// Total Rayleigh scattering coefficient at the primaries (King-corrected).
pub const TOTAL_RAYLEIGH: [f64; 3] = [
    5.804_542_996_261_093e-6,
    1.356_291_141_984_563_5e-5,
    3.026_590_246_882_487_6e-5,
];

/// π · (2π/λ)^(v−2) · K with v=4, K=[0.686,0.678,0.666].
const MIE_CONST: [f64; 3] = [
    1.839_991_851_443_397_8e14,
    2.779_802_391_966_052_8e14,
    4.079_047_954_386_109_4e14,
];

/// Earth-shadow softening of sun intensity near/below the horizon.
const CUTOFF_ANGLE: f64 = 1.611_073_155_687_073_4;
const STEEPNESS: f64 = 1.5;
const SUN_EE: f64 = 1000.0;

/// Zenith optical lengths (m).
const RAYLEIGH_ZENITH_LENGTH: f64 = 8.4e3;
const MIE_ZENITH_LENGTH: f64 = 1.25e3;

/// Linear RGB (sRGB primaries, no transfer function applied).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SkyRgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl SkyRgb {
    fn splat(v: f64) -> Self {
        Self { r: v, g: v, b: v }
    }
    pub fn luminance(&self) -> f64 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }
}

fn elementwise_mul(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

fn elementwise_div(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] / b[0], a[1] / b[1], a[2] / b[2]]
}

fn pow3(v: [f64; 3], p: f64) -> [f64; 3] {
    [v[0].powf(p), v[1].powf(p), v[2].powf(p)]
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a * (1.0 - t) + b * t
}

fn sun_intensity(sun_up_cos: f64) -> f64 {
    let c = sun_up_cos.clamp(-1.0, 1.0);
    SUN_EE * (1.0 - (-((CUTOFF_ANGLE - c.acos()) / STEEPNESS)).exp()).max(0.0)
}

fn total_mie(turbidity: f64) -> [f64; 3] {
    let c = (0.2 * turbidity) * 10e-18;
    elementwise_mul([c; 3], MIE_CONST).map(|m| 0.434 * m)
}

fn rayleigh_phase(cos_theta: f64) -> f64 {
    // 3/(16π) (1 + cos²θ)
    3.0 / (16.0 * PI) * (1.0 + cos_theta * cos_theta)
}

fn hg_phase(cos_theta: f64, g: f64) -> f64 {
    let g2 = g * g;
    (1.0 - g2) / (4.0 * PI * (1.0 - 2.0 * g * cos_theta + g2).powf(1.5))
}

/// Analytic single-scattering sky radiance toward `view_dir` given a unit
/// `sun_dir` (both in the observer's ENU frame) and atmospheric turbidity.
///
/// `rayleigh_coefficient` scales molecular scattering (1.0 = clear day);
/// `mie_g` is the Henyey-Greenstein asymmetry (0.8 typical).
pub fn sky_color(
    sun_dir: Enu,
    view_dir: Enu,
    turbidity: f64,
    rayleigh_coefficient: f64,
    mie_g: f64,
) -> SkyRgb {
    let n = |v: Enu| -> [f64; 3] {
        let l = (v.east * v.east + v.north * v.north + v.up * v.up).sqrt();
        if l < 1e-12 {
            [0.0, 0.0, 1.0]
        } else {
            [v.east / l, v.north / l, v.up / l]
        }
    };
    let sun = n(sun_dir);
    let dir = n(view_dir);

    let sun_e = sun_intensity(sun[2]);
    if sun_e <= 0.0 {
        // Night: keep a faint airglow-like floor so downstream tonemap has
        // something to work with (same constant floor as the reference).
        return SkyRgb {
            r: 0.0,
            g: 0.0003,
            b: 0.00075,
        };
    }

    let beta_r = TOTAL_RAYLEIGH.map(|r| r * rayleigh_coefficient);
    let beta_m = total_mie(turbidity);

    // Optical path along the view ray (Kasten-style relative air mass).
    let zen_cos = dir[2];
    let zenith_angle = zen_cos.clamp(0.0, 1.0).acos();
    let inv = 1.0 / (zenith_angle.cos() + 0.15 * (93.885 - zenith_angle.to_degrees()).powf(-1.253));
    let s_r = RAYLEIGH_ZENITH_LENGTH * inv;
    let s_m = MIE_ZENITH_LENGTH * inv;

    let fex: [f64; 3] = std::array::from_fn(|i| (-(beta_r[i] * s_r + beta_m[i] * s_m)).exp());

    let cos_theta = dot(dir, sun);
    let r_phase = rayleigh_phase(cos_theta);
    let beta_r_theta: [f64; 3] = elementwise_mul(beta_r, [r_phase; 3]);
    let m_phase = hg_phase(cos_theta, mie_g);
    let beta_m_theta: [f64; 3] = elementwise_mul(beta_m, [m_phase; 3]);

    let ratio = elementwise_div(add3(beta_r_theta, beta_m_theta), add3(beta_r, beta_m));
    let mut lin = pow3(
        elementwise_mul(elementwise_mul([sun_e; 3], ratio), sub1(fex)),
        1.5,
    );
    // Sun-transmittance term that kicks in as the sun approaches the horizon.
    let horizon_mix = ((1.0 - sun[2]).powi(5)).clamp(0.0, 1.0);
    let trans = pow3(
        elementwise_mul(elementwise_mul([sun_e; 3], ratio), fex),
        0.5,
    );
    for i in 0..3 {
        lin[i] *= mix(1.0, trans[i], horizon_mix);
    }

    // Isotropic background (reference uses 0.1 * Fex) + night-sky floor.
    let rgb: [f64; 3] =
        std::array::from_fn(|i| (lin[i] + 0.1 * fex[i]) * 0.04 + [0.0, 0.0003, 0.00075][i]);

    SkyRgb {
        r: rgb[0],
        g: rgb[1],
        b: rgb[2],
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub1(v: [f64; 3]) -> [f64; 3] {
    v.map(|x| 1.0 - x)
}

/// Convenience wrapper with the reference defaults (clear day, HG 0.8).
pub fn sky_color_default(sun_dir: Enu, view_dir: Enu, turbidity: f64) -> SkyRgb {
    sky_color(sun_dir, view_dir, turbidity, 1.0, 0.8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enu(east: f64, north: f64, up: f64) -> Enu {
        let l = (east * east + north * north + up * up).sqrt();
        Enu {
            east: east / l,
            north: north / l,
            up: up / l,
        }
    }

    fn sun_at_elevation_azimuth(elev_deg: f64, az_deg: f64) -> Enu {
        let e = elev_deg.to_radians();
        let a = az_deg.to_radians();
        enu(a.sin() * e.cos(), a.cos() * e.cos(), e.sin())
    }

    #[test]
    fn rayleigh_scattering_is_symmetric_around_the_sun() {
        let sun = enu(3.0_f64.sqrt() / 2.0, 0.0, 0.5);
        let view_a = enu(1.0 / 3.0_f64.sqrt(), (2.0_f64 / 3.0).sqrt(), 0.0);
        let view_b = enu(-1.0 / 3.0_f64.sqrt(), (2.0_f64 / 3.0).sqrt(), 0.0);
        let a = sky_color(sun, view_a, 2.0, 1.0, 0.0);
        let b = sky_color(sun, view_b, 2.0, 1.0, 0.0);

        assert!((a.r - b.r).abs() < 1e-12, "red differs: {a:?} vs {b:?}");
        assert!((a.g - b.g).abs() < 1e-12, "green differs: {a:?} vs {b:?}");
        assert!((a.b - b.b).abs() < 1e-12, "blue differs: {a:?} vs {b:?}");
    }

    #[test]
    fn noon_zenith_view_is_blue_and_horizon_warmer() {
        let sun = sun_at_elevation_azimuth(70.0, 180.0);
        let up_view = enu(0.0, 0.0, 1.0);
        let horiz_view = enu(0.0, -1.0, 0.02);

        let zenith_c = sky_color_default(sun, up_view, 2.0);
        let horizon_c = sky_color_default(sun, horiz_view, 2.0);

        assert!(
            zenith_c.b > zenith_c.r,
            "noon zenith should be blue-dominant: {:?}",
            zenith_c
        );
        let rb_zenith = zenith_c.r / zenith_c.b.max(1e-9);
        let rb_horizon = horizon_c.r / horizon_c.b.max(1e-9);
        assert!(
            rb_horizon > rb_zenith,
            "horizon should be warmer than zenith ({} vs {})",
            rb_horizon,
            rb_zenith
        );
        assert!(horizon_c.luminance() > 0.0);
    }

    #[test]
    fn sunset_toward_sun_is_red_dominant_near_horizon() {
        let sun = sun_at_elevation_azimuth(1.5, 270.0); // low western sun
        let toward = enu(-1.0, 0.05, 0.03);
        let c = sky_color_default(sun, toward, 4.0);
        assert!(
            c.r > c.b,
            "sunset sky near the sun must be red-dominant: {:?}",
            c
        );
    }

    #[test]
    fn sun_below_horizon_gives_night_floor() {
        let sun = sun_at_elevation_azimuth(-12.0, 90.0);
        let view = enu(0.3, 0.3, 0.9);
        let c = sky_color_default(sun, view, 2.0);
        assert!(
            c.luminance() < 0.01,
            "night sky should be nearly black: {:?}",
            c
        );
    }

    #[test]
    fn higher_turbidity_brightens_haze() {
        let sun = sun_at_elevation_azimuth(25.0, 200.0);
        let view = enu(-0.5, -0.85, 0.06);
        let clean = sky_color_default(sun, view, 1.5);
        let hazy = sky_color_default(sun, view, 8.0);
        assert!(
            hazy.luminance() >= clean.luminance(),
            "haze should not darken the horizon sky: {:?} vs {:?}",
            hazy,
            clean
        );
    }

    #[test]
    fn output_stays_finite_across_sweep() {
        for &elev in &[89.0, 45.0, 10.0, 3.0, 0.5, -1.0, -20.0] {
            for &az in &[0.0_f64, 90.0, 180.0, 270.0] {
                let sun = sun_at_elevation_azimuth(elev, az);
                for &ve in &[-0.2_f64, 0.01, 0.3, 1.0] {
                    let view = enu(0.6, 0.5, ve.max(0.001));
                    let c = sky_color_default(sun, view, 3.0);
                    assert!(c.r.is_finite() && c.g.is_finite() && c.b.is_finite());
                    assert!(c.r >= 0.0 && c.g >= 0.0 && c.b >= 0.0);
                }
            }
        }
    }
}
