//! WGS84 geodetic ↔ ECEF ↔ local ENU transforms.
//!
//! Kept dependency-free (no PROJ) so registration stays redistributable.
//! Horizontal CRS labels are carried on [`crate::project::GeoReference`];
//! full projected-CRS warps land later with GDAL/PROJ workers.

use serde::{Deserialize, Serialize};

/// WGS84 semi-major axis (m) and flattening.
const A: f64 = 6_378_137.0;
const F: f64 = 1.0 / 298.257_223_563;
const E2: f64 = F * (2.0 - F);

/// Geodetic position: longitude/latitude degrees, ellipsoidal height metres.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Geodetic {
    pub lon_deg: f64,
    pub lat_deg: f64,
    pub height_m: f64,
}

/// ECEF cartesian metres.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Ecef {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Ecef {
    pub fn as_array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }

    pub fn from_array(a: [f64; 3]) -> Self {
        Self {
            x: a[0],
            y: a[1],
            z: a[2],
        }
    }
}

/// Local East-North-Up metres relative to an origin.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Enu {
    pub east: f64,
    pub north: f64,
    pub up: f64,
}

impl Enu {
    pub fn as_array(self) -> [f64; 3] {
        [self.east, self.north, self.up]
    }

    pub fn from_array(a: [f64; 3]) -> Self {
        Self {
            east: a[0],
            north: a[1],
            up: a[2],
        }
    }
}

/// Fixed ENU frame anchored at a geodetic origin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnuFrame {
    pub origin: Geodetic,
    pub origin_ecef: Ecef,
    /// Rows: east, north, up basis vectors in ECEF.
    pub rotation_ecef_from_enu: [[f64; 3]; 3],
}

impl EnuFrame {
    pub fn from_geodetic(origin: Geodetic) -> Self {
        let origin_ecef = geodetic_to_ecef(origin);
        let (sin_lon, cos_lon) = origin.lon_deg.to_radians().sin_cos();
        let (sin_lat, cos_lat) = origin.lat_deg.to_radians().sin_cos();
        // Columns of R are ENU axes in ECEF; store as rows for clarity.
        let east = [-sin_lon, cos_lon, 0.0];
        let north = [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat];
        let up = [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat];
        Self {
            origin,
            origin_ecef,
            rotation_ecef_from_enu: [east, north, up],
        }
    }

    pub fn ecef_to_enu(&self, p: Ecef) -> Enu {
        let d = [
            p.x - self.origin_ecef.x,
            p.y - self.origin_ecef.y,
            p.z - self.origin_ecef.z,
        ];
        let r = &self.rotation_ecef_from_enu;
        Enu {
            east: r[0][0] * d[0] + r[0][1] * d[1] + r[0][2] * d[2],
            north: r[1][0] * d[0] + r[1][1] * d[1] + r[1][2] * d[2],
            up: r[2][0] * d[0] + r[2][1] * d[1] + r[2][2] * d[2],
        }
    }

    pub fn enu_to_ecef(&self, p: Enu) -> Ecef {
        let r = &self.rotation_ecef_from_enu;
        // ECEF = origin + R^T * enu  (since rows are axes)
        Ecef {
            x: self.origin_ecef.x + r[0][0] * p.east + r[1][0] * p.north + r[2][0] * p.up,
            y: self.origin_ecef.y + r[0][1] * p.east + r[1][1] * p.north + r[2][1] * p.up,
            z: self.origin_ecef.z + r[0][2] * p.east + r[1][2] * p.north + r[2][2] * p.up,
        }
    }

    /// Homogeneous 4×4 row-major: ECEF → ENU.
    pub fn ecef_to_enu_matrix(&self) -> [f64; 16] {
        let r = &self.rotation_ecef_from_enu;
        let o = &self.origin_ecef;
        let t_e = -(r[0][0] * o.x + r[0][1] * o.y + r[0][2] * o.z);
        let t_n = -(r[1][0] * o.x + r[1][1] * o.y + r[1][2] * o.z);
        let t_u = -(r[2][0] * o.x + r[2][1] * o.y + r[2][2] * o.z);
        [
            r[0][0], r[0][1], r[0][2], t_e, //
            r[1][0], r[1][1], r[1][2], t_n, //
            r[2][0], r[2][1], r[2][2], t_u, //
            0.0, 0.0, 0.0, 1.0,
        ]
    }

    /// Homogeneous 4×4 row-major: ENU → ECEF.
    pub fn enu_to_ecef_matrix(&self) -> [f64; 16] {
        let r = &self.rotation_ecef_from_enu;
        let o = &self.origin_ecef;
        [
            r[0][0], r[1][0], r[2][0], o.x, //
            r[0][1], r[1][1], r[2][1], o.y, //
            r[0][2], r[1][2], r[2][2], o.z, //
            0.0, 0.0, 0.0, 1.0,
        ]
    }
}

pub fn geodetic_to_ecef(g: Geodetic) -> Ecef {
    let lon = g.lon_deg.to_radians();
    let lat = g.lat_deg.to_radians();
    let (sin_lon, cos_lon) = lon.sin_cos();
    let (sin_lat, cos_lat) = lat.sin_cos();
    let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt();
    Ecef {
        x: (n + g.height_m) * cos_lat * cos_lon,
        y: (n + g.height_m) * cos_lat * sin_lon,
        z: (n * (1.0 - E2) + g.height_m) * sin_lat,
    }
}

/// Approximate inverse (Bowring-style iteration, centimetre-class for survey work).
pub fn ecef_to_geodetic(p: Ecef) -> Geodetic {
    let lon = p.y.atan2(p.x);
    let p_xy = (p.x * p.x + p.y * p.y).sqrt();
    let mut lat = (p.z / p_xy).atan();
    for _ in 0..6 {
        let sin_lat = lat.sin();
        let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt();
        lat = (p.z + E2 * n * sin_lat).atan2(p_xy);
    }
    let sin_lat = lat.sin();
    let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt();
    let height = if cos_near_poles(lat) {
        p_xy / lat.cos() - n
    } else {
        p.z / sin_lat - n * (1.0 - E2)
    };
    Geodetic {
        lon_deg: lon.to_degrees(),
        lat_deg: lat.to_degrees(),
        height_m: height,
    }
}

fn cos_near_poles(lat: f64) -> bool {
    lat.cos().abs() > 1e-10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auckland_roundtrip_geodetic_ecef() {
        let g = Geodetic {
            lon_deg: 174.7633,
            lat_deg: -36.8485,
            height_m: 80.0,
        };
        let e = geodetic_to_ecef(g);
        let back = ecef_to_geodetic(e);
        assert!((back.lon_deg - g.lon_deg).abs() < 1e-8);
        assert!((back.lat_deg - g.lat_deg).abs() < 1e-8);
        assert!((back.height_m - g.height_m).abs() < 1e-3);
    }

    #[test]
    fn enu_axes_orthogonal_and_roundtrip() {
        let frame = EnuFrame::from_geodetic(Geodetic {
            lon_deg: 174.76,
            lat_deg: -36.85,
            height_m: 50.0,
        });
        let local = Enu {
            east: 120.0,
            north: -45.0,
            up: 12.5,
        };
        let ecef = frame.enu_to_ecef(local);
        let back = frame.ecef_to_enu(ecef);
        assert!((back.east - local.east).abs() < 1e-6);
        assert!((back.north - local.north).abs() < 1e-6);
        assert!((back.up - local.up).abs() < 1e-6);

        // East ≈ +X at lon≈90°… better: moving east increases longitude.
        let east_pt = frame.enu_to_ecef(Enu {
            east: 100.0,
            north: 0.0,
            up: 0.0,
        });
        let geo = ecef_to_geodetic(east_pt);
        assert!(geo.lon_deg > frame.origin.lon_deg);
    }

    #[test]
    fn matrices_match_point_api() {
        let frame = EnuFrame::from_geodetic(Geodetic {
            lon_deg: 10.0,
            lat_deg: 50.0,
            height_m: 0.0,
        });
        let p = Ecef {
            x: frame.origin_ecef.x + 30.0,
            y: frame.origin_ecef.y - 10.0,
            z: frame.origin_ecef.z + 5.0,
        };
        let m = frame.ecef_to_enu_matrix();
        let enu = frame.ecef_to_enu(p);
        let mx = m[0] * p.x + m[1] * p.y + m[2] * p.z + m[3];
        let my = m[4] * p.x + m[5] * p.y + m[6] * p.z + m[7];
        let mz = m[8] * p.x + m[9] * p.y + m[10] * p.z + m[11];
        assert!((mx - enu.east).abs() < 1e-6);
        assert!((my - enu.north).abs() < 1e-6);
        assert!((mz - enu.up).abs() < 1e-6);
    }
}
