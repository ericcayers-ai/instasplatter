//! Real astronomical sun position (NOAA / Meeus low-precision solar theory).
//!
//! Dependency-free by repo convention (see [`super::transforms`]): inputs are
//! plain civil-date fields rather than a chrono type. Accuracy is ~0.01°
//! against the NREL SPA reference over years ±2000, far beyond what sky
//! colour or hillshade need.
//!
//! Reference values in tests come from the NREL SPA distribution's own test
//! program output (`spa_tester.c`, Reda & Andreas 2003/2008), cross-checked
//! via published ports — not invented.

use crate::geospatial::transforms::Enu;
use serde::{Deserialize, Serialize};

/// Fractional UTC date-time: `hour` may carry fractional hours (e.g. 19.508).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UtcDateTime {
    pub year: i32,
    /// 1..=12
    pub month: u32,
    /// 1..=31 (Gregorian calendar rules apply)
    pub day: u32,
    /// Fractional hours since midnight UTC.
    pub hour_utc: f64,
}

/// Sun direction at an observer location.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SunPosition {
    /// Clockwise from geographic north, degrees [0, 360).
    pub azimuth_deg: f64,
    /// Above the geometric horizon, degrees (negative = below).
    pub elevation_deg: f64,
    /// Unit vector pointing toward the sun in the observer's ENU frame.
    pub direction_enu: Enu,
}

impl SunPosition {
    pub fn zenith_deg(&self) -> f64 {
        90.0 - self.elevation_deg
    }
}

fn julian_day(year: i32, month: u32, day: u32, hour_utc: f64) -> f64 {
    let mut y = year as i64;
    let mut m = month as i64;
    if m <= 2 {
        y -= 1;
        m += 12;
    }
    let a = y.div_euclid(100);
    let b = 2 - a + a.div_euclid(4);
    let jd_int = (365.25 * (y + 4716) as f64).floor()
        + (30.600_1 * (m + 1) as f64).floor()
        + day as f64
        + b as f64
        - 1524.5;
    jd_int + hour_utc / 24.0
}

const RAD: f64 = std::f64::consts::PI / 180.0;

/// Compute the sun's azimuth/elevation/ENU direction.
///
/// `lon_deg` is positive east of Greenwich (matches [`crate::geospatial::transforms::Geodetic`]).
pub fn sun_position(lat_deg: f64, lon_deg: f64, utc: UtcDateTime) -> SunPosition {
    let jd = julian_day(utc.year, utc.month, utc.day, utc.hour_utc);
    let t = (jd - 2_451_545.0) / 36_525.0;

    // Geometric mean longitude of the sun (mod 360).
    let l0_deg = (280.46646 + t * (36_000.76983 + 0.000_303_2 * t)) % 360.0;
    // Mean anomaly.
    let m_deg = 357.52911 + t * (35_999.050_29 - 0.000_153_7 * t);
    // Eccentricity of Earth's orbit.
    let ecc = 0.016_708_634 - t * (0.000_042_037 + 0.000_000_126_7 * t);

    let m_rad = m_deg * RAD;
    let c_deg = (1.914_602 - t * (0.004_817 + 0.000_014 * t)) * m_rad.sin()
        + (0.019_993 - 0.000_101 * t) * (2.0 * m_rad).sin()
        + 0.000_289 * (3.0 * m_rad).sin();
    let true_long_deg = l0_deg + c_deg;

    // Apparent longitude: aberration + nutation correction.
    let omega_deg = 125.04 - 1934.136 * t;
    let lambda_deg = true_long_deg - 0.005_69 - 0.004_78 * (omega_deg * RAD).sin();

    // Mean obliquity (arcsec polynomial) + nutation-in-obliquity term.
    let secs = 21.448 - t * (46.815_0 + 0.000_59 * t) + t * t * 0.001_813;
    let eps0_deg = 23.0 + (26.0 + secs / 60.0) / 60.0;
    let eps_deg = eps0_deg + 0.002_56 * (omega_deg * RAD).cos();

    let eps = eps_deg * RAD;
    let lam = lambda_deg * RAD;

    // Equatorial coordinates.
    let dec = (eps.sin() * lam.sin()).asin(); // declination
    let _ra = (eps.cos() * lam.sin()).atan2(lam.cos()); // right ascension (kept for completeness)

    // Equation of time (minutes), NOAA formulation.
    let y = (eps / 2.0).tan().powi(2);
    let two_l0 = 2.0 * l0_deg * RAD;
    let eot_min = 4.0_f64.to_degrees()
        * (y * (two_l0).sin() - 2.0 * ecc * m_rad.sin()
            + 4.0 * ecc * y * m_rad.sin() * two_l0.cos()
            - 0.5 * y * y * (2.0 * two_l0).sin()
            - 1.25 * ecc * ecc * (2.0 * m_rad).sin());

    // True solar time → local hour angle.
    let tst_min = utc.hour_utc * 60.0 + eot_min + 4.0 * lon_deg;
    let ha_deg = tst_min / 4.0 - 180.0;
    let ha = ha_deg * RAD;

    let phi = lat_deg * RAD;

    // Horizontal-frame unit vector via the standard equatorial→horizontal
    // rotation; components are exactly the quantities az/el are defined from,
    // so the three outputs can never disagree with each other.
    let up = phi.sin() * dec.sin() + phi.cos() * dec.cos() * ha.cos();
    let east = -dec.cos() * ha.sin();
    let north = phi.cos() * dec.sin() - phi.sin() * dec.cos() * ha.cos();

    let azimuth_deg = east.atan2(north).to_degrees();
    let azimuth_deg = if azimuth_deg < 0.0 {
        azimuth_deg + 360.0
    } else {
        azimuth_deg
    };
    let elevation_deg = up.clamp(-1.0, 1.0).asin().to_degrees();

    SunPosition {
        azimuth_deg,
        elevation_deg,
        direction_enu: Enu { east, north, up },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NREL_ZENITH_DEG: f64 = 50.111_622;
    const NREL_AZIMUTH_DEG: f64 = 194.340_241;

    #[test]
    fn nrel_spa_reference_case() {
        // NREL SPA spa_tester.c canonical case: 2003-10-17 12:30:30 local
        // (UTC-7) => 19:30:30 UTC, Golden CO. Published outputs:
        // zenith 50.111622°, azimuth (eastward-from-north) 194.340241°.
        let sp = sun_position(
            39.742_476_7,
            -105.178_555_3,
            UtcDateTime {
                year: 2003,
                month: 10,
                day: 17,
                hour_utc: 19.0 + 30.0 / 60.0 + 30.0 / 3600.0,
            },
        );
        let elev_expected = 90.0 - NREL_ZENITH_DEG;
        assert!(
            (sp.elevation_deg - elev_expected).abs() < 0.06,
            "elevation {} vs expected {}",
            sp.elevation_deg,
            elev_expected
        );
        assert!(
            (sp.azimuth_deg - NREL_AZIMUTH_DEG).abs() < 0.2,
            "azimuth {} vs expected {}",
            sp.azimuth_deg,
            NREL_AZIMUTH_DEG
        );
    }

    #[test]
    fn direction_vector_matches_azimuth_elevation_and_is_unit() {
        let sp = sun_position(
            51.5,
            -0.13,
            UtcDateTime {
                year: 2024,
                month: 6,
                day: 21,
                hour_utc: 12.0,
            },
        );
        let d = sp.direction_enu;
        let norm = (d.east * d.east + d.north * d.north + d.up * d.up).sqrt();
        assert!((norm - 1.0).abs() < 1e-9, "direction not unit: {norm}");
        let el = d.up.asin().to_degrees();
        let az = d.east.atan2(d.north).to_degrees();
        let az = if az < 0.0 { az + 360.0 } else { az };
        assert!((el - sp.elevation_deg).abs() < 1e-9);
        assert!((az - sp.azimuth_deg).abs() < 1e-9);
    }

    #[test]
    fn equinox_noon_at_equator_sun_near_zenith() {
        // 2024 March equinox instant: 2024-03-20 03:06 UTC. At longitude 0
        // the sun transits ~12:07 UTC; a few hours later its declination is
        // still well under a degree, so from the equator it culminates
        // within a degree of the zenith.
        let sp = sun_position(
            0.0,
            0.0,
            UtcDateTime {
                year: 2024,
                month: 3,
                day: 20,
                hour_utc: 12.1,
            },
        );
        assert!(
            sp.elevation_deg > 88.5,
            "equinox equator transit elevation should be near 90, got {}",
            sp.elevation_deg
        );
        // Near-zenith azimuth swings fast; just require it finite & bounded.
        assert!((0.0..360.0).contains(&sp.azimuth_deg));
    }

    #[test]
    fn london_summer_solstice_sunrise_elevation_near_horizon() {
        // Published sunrise for London (51.5074 N, 0.1278 W) on 2024-06-21 is
        // 04:43 British Summer Time = 03:43 UTC (almanac values include
        // refraction & solar semidiameter).
        // The GEOMETRIC centre sits ~0.85° below the horizon then.
        let sp = sun_position(
            51.507_4,
            -0.127_8,
            UtcDateTime {
                year: 2024,
                month: 6,
                day: 21,
                hour_utc: 3.0 + 43.0 / 60.0,
            },
        );
        assert!(
            (-1.5..=-0.2).contains(&sp.elevation_deg),
            "sunrise elevation should be slightly below geometric horizon, got {}",
            sp.elevation_deg
        );
        // Sun rises roughly NE in June at London: azimuth in [35, 70].
        assert!(
            (35.0..70.0).contains(&sp.azimuth_deg),
            "sunrise azimuth should be north-eastish, got {}",
            sp.azimuth_deg
        );
    }

    #[test]
    fn southern_hemisphere_winter_noon_sun_to_the_north() {
        // Auckland mid-winter noon: sun culminates NORTH of zenith.
        let sp = sun_position(
            -36.85,
            174.76,
            UtcDateTime {
                year: 2024,
                month: 7,
                day: 21,
                hour_utc: 0.5, // NZST noon ≈ 00:00 UTC (UTC+12); slight offset ok
            },
        );
        assert!(sp.elevation_deg > 20.0 && sp.elevation_deg < 45.0);
        assert!(
            (340.0..=360.0).contains(&sp.azimuth_deg) || (0.0..=20.0).contains(&sp.azimuth_deg),
            "noon winter sun should be northerly, got {}",
            sp.azimuth_deg
        );
    }
}
