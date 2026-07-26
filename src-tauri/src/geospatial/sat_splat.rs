//! "Satellite Splat" — build a Gaussian-splat point cloud for any WGS84 AOI
//! directly from public elevation + imagery catalogs, no on-site photo/video
//! capture or prior Reconstruction required.
//!
//! Complements [`super::splat_dem`], which runs the opposite direction
//! (reconstruction splat → proxy DEM). This module goes catalog → splat, so
//! it works "anywhere on the map" — an AOI is the only input needed.

use super::catalog::{fetch_asset_with_opts, CatalogFetchOpts};
use super::dem::{condition_dem_with_opts, sample_dem_grid, DemSampleGrid};
use super::transforms::{geodetic_to_ecef, EnuFrame, Geodetic};
use crate::splat::ply::write_ply;
use crate::splat::{SplatCloud, SH_C0};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Elevation connectors tried in order. USGS 3DEP is CONUS-only but higher
/// fidelity; Copernicus GLO-30 is global, so the pair covers "anywhere".
const DEM_CONNECTORS: [&str; 2] = ["usgs-3dep", "copernicus-glo30"];
const MAX_GRID_DIM: u32 = 256;

/// Options for satellite-splat generation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SatSplatOpts {
    /// WGS84 AOI `[west, south, east, north]`.
    pub aoi_wgs84: [f64; 4],
    /// Target elevation-sample spacing in metres (default 10).
    pub cell_size_m: Option<f64>,
    pub grid_cols: Option<u32>,
    pub grid_rows: Option<u32>,
}

/// Summary of a successful satellite-splat generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SatSplatProduct {
    pub path: String,
    pub points: u64,
    pub cols: u32,
    pub rows: u32,
    pub aoi_wgs84: [f64; 4],
    pub dem_source: String,
    pub imagery_source: Option<String>,
    pub notes: Vec<String>,
}

fn validate_aoi(aoi: [f64; 4]) -> Result<[f64; 4], String> {
    let [west, south, east, north] = aoi;
    if !west.is_finite() || !south.is_finite() || !east.is_finite() || !north.is_finite() {
        return Err("AOI bounds must be finite numbers".into());
    }
    if !(west < east && south < north) {
        return Err("AOI must have west < east and south < north".into());
    }
    if !(-180.0..=180.0).contains(&west) || !(-180.0..=180.0).contains(&east) {
        return Err("AOI longitude must be within [-180, 180]".into());
    }
    if !(-90.0..=90.0).contains(&south) || !(-90.0..=90.0).contains(&north) {
        return Err("AOI latitude must be within [-90, 90]".into());
    }
    if (east - west) < 1e-5 || (north - south) < 1e-5 {
        return Err("AOI is too small for a satellite splat".into());
    }
    // Catalog connectors already cap raw fetch size; this is a fast local
    // reject so a huge accidental AOI fails immediately with a clear message
    // instead of stalling on a slow/oversized fetch first.
    if (east - west) > 2.0 || (north - south) > 2.0 {
        return Err(
            "AOI is too large for a satellite splat (max ~2° per side) — draw a tighter box."
                .into(),
        );
    }
    Ok(aoi)
}

/// Generate a satellite-imagery-draped Gaussian splat for an arbitrary AOI.
/// Writes `geo/derived/sat_splat.ply`.
pub fn generate_sat_splat(
    workspace: &Path,
    opts: &SatSplatOpts,
) -> Result<SatSplatProduct, String> {
    let aoi = validate_aoi(opts.aoi_wgs84)?;
    let cell_size_m = opts.cell_size_m.unwrap_or(10.0).clamp(1.0, 500.0);
    let cols = opts.grid_cols.unwrap_or(96).clamp(4, MAX_GRID_DIM);
    let rows = opts.grid_rows.unwrap_or(96).clamp(4, MAX_GRID_DIM);

    crate::geospatial::prepare_workspace(workspace)?;
    let catalog_dir = workspace.join("geo").join("catalog");
    let derived = workspace.join("geo").join("derived");
    fs::create_dir_all(&catalog_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&derived).map_err(|e| e.to_string())?;

    let mut notes = Vec::new();
    let mut dem_source = String::new();
    let mut fetched_dem: Option<PathBuf> = None;
    for connector in DEM_CONNECTORS {
        let fetch_opts = CatalogFetchOpts {
            aoi_wgs84: Some(aoi),
            cell_size_m: Some(cell_size_m),
            ..Default::default()
        };
        match fetch_asset_with_opts(connector, &catalog_dir.to_string_lossy(), &fetch_opts) {
            Ok(entry) => {
                if let Some(local) = entry.local_path.as_deref().map(PathBuf::from) {
                    if local.exists() {
                        notes.push(format!(
                            "Elevation: {connector}{}",
                            entry
                                .attribution
                                .as_deref()
                                .map(|a| format!(" ({a})"))
                                .unwrap_or_default()
                        ));
                        fetched_dem = Some(local);
                        dem_source = connector.to_string();
                        break;
                    }
                }
            }
            Err(e) => notes.push(format!("{connector} unavailable: {e}")),
        }
    }

    let dem_path = fetched_dem.ok_or_else(|| {
        format!(
            "No elevation source reachable for this AOI (tried {}). Check network connectivity and try again.",
            DEM_CONNECTORS.join(", ")
        )
    })?;

    condition_dem_with_opts(
        &dem_path,
        &derived,
        Some(aoi),
        Some(cell_size_m),
        Some("EPSG:4326"),
        Some(-9999.0),
    )?;
    let grid = sample_dem_grid(workspace, cols, rows, Some(aoi))?;
    if grid.z.is_empty() {
        return Err("Elevation grid came back empty — cannot build a satellite splat.".into());
    }

    // Imagery is best-effort: a missing/blocked STAC thumbnail should not
    // fail the whole feature — fall back to an elevation-shaded colormap so
    // "works anywhere" also covers offline / restricted-network use.
    let imagery = fetch_thumbnail_rgb(&catalog_dir, aoi);
    let imagery_source = imagery
        .as_ref()
        .map(|_| "Earth Search STAC (Sentinel-2 L2A thumbnail)".to_string());
    if imagery.is_none() {
        notes
            .push("No satellite thumbnail available — used elevation-shaded color instead.".into());
    }

    let cloud = build_splat_cloud(&grid, aoi, cell_size_m, imagery.as_ref());
    if cloud.is_empty() {
        return Err("Generated splat cloud is empty — AOI may be entirely nodata.".into());
    }

    let out_path = derived.join("sat_splat.ply");
    write_ply(&out_path, &cloud)?;

    let side = derived.join("sat_splat.meta.json");
    let body = serde_json::json!({
        "kind": "satSplat",
        "aoiWgs84": aoi,
        "demSource": dem_source,
        "imagerySource": imagery_source,
        "points": cloud.len(),
        "cellSizeM": cell_size_m,
    });
    fs::write(
        &side,
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(SatSplatProduct {
        path: out_path.to_string_lossy().into_owned(),
        points: cloud.len() as u64,
        cols,
        rows,
        aoi_wgs84: aoi,
        dem_source,
        imagery_source,
        notes,
    })
}

/// Fetch an Earth Search STAC item for `aoi` and download its thumbnail
/// asset, decoding it via the `image` crate. Returns `None` on any failure
/// (network, missing asset, unsupported encoding) rather than propagating an
/// error — imagery is a color enhancement, not a hard requirement.
fn fetch_thumbnail_rgb(catalog_dir: &Path, aoi: [f64; 4]) -> Option<image::RgbImage> {
    let fetch_opts = CatalogFetchOpts {
        aoi_wgs84: Some(aoi),
        ..Default::default()
    };
    let entry = fetch_asset_with_opts(
        "earth-search-stac",
        &catalog_dir.to_string_lossy(),
        &fetch_opts,
    )
    .ok()?;
    let meta_path = PathBuf::from(entry.local_path?);
    let text = fs::read_to_string(&meta_path).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let features = doc.get("features")?.as_array()?;
    let href = features.iter().find_map(|f| {
        f.get("assets")?
            .get("thumbnail")?
            .get("href")?
            .as_str()
            .map(str::to_string)
    })?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("InstaSplatter/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()?;
    let bytes = client.get(&href).send().ok()?.bytes().ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    Some(img.to_rgb8())
}

/// Convert a sampled elevation grid (+ optional imagery) into a Gaussian
/// splat cloud, one splat per grid cell, positioned in a local ENU frame
/// anchored at the AOI centre.
fn build_splat_cloud(
    grid: &DemSampleGrid,
    aoi: [f64; 4],
    _cell_size_m: f64,
    imagery: Option<&image::RgbImage>,
) -> SplatCloud {
    let [west, south, east, north] = aoi;
    let origin = Geodetic {
        lon_deg: (west + east) / 2.0,
        lat_deg: (south + north) / 2.0,
        height_m: 0.0,
    };
    let frame = EnuFrame::from_geodetic(origin);

    let cols = grid.cols.max(1);
    let rows = grid.rows.max(1);
    let mut z_min = f32::INFINITY;
    let mut z_max = f32::NEG_INFINITY;
    for &z in &grid.z {
        if z.is_finite() {
            z_min = z_min.min(z);
            z_max = z_max.max(z);
        }
    }
    if !z_min.is_finite() || !z_max.is_finite() {
        z_min = 0.0;
        z_max = 0.0;
    }
    let z_range = (z_max - z_min).max(1.0);

    // log(scale) so each splat footprint roughly covers its grid cell —
    // slightly overlapping neighbours avoids visible gaps in the cloud.
    let cell_span_deg_x = (east - west) / cols as f64;
    let cell_span_deg_y = (north - south) / rows as f64;
    let approx_cell_m_x =
        cell_span_deg_x * 111_320.0 * origin.lat_deg.to_radians().cos().abs().max(0.05);
    let approx_cell_m_y = cell_span_deg_y * 111_320.0;
    let scale_x = (approx_cell_m_x.max(0.1) * 0.6) as f32;
    let scale_y = (approx_cell_m_y.max(0.1) * 0.6) as f32;
    let scale_log = [
        scale_x.ln(),
        scale_y.ln(),
        (scale_x.min(scale_y) * 0.25).max(0.01).ln(),
    ];

    let mut cloud = SplatCloud::default();
    for r in 0..rows {
        for c in 0..cols {
            let idx = (r * cols + c) as usize;
            let z = grid.z.get(idx).copied().unwrap_or(f32::NAN);
            if !z.is_finite() {
                continue; // nodata cell — skip rather than fabricate a point.
            }
            let lon = west + (c as f64 + 0.5) * cell_span_deg_x;
            let lat = south + (r as f64 + 0.5) * cell_span_deg_y;
            let geo = Geodetic {
                lon_deg: lon,
                lat_deg: lat,
                height_m: z as f64,
            };
            let enu = frame.ecef_to_enu(geodetic_to_ecef(geo));

            let rgb = imagery
                .map(|img| {
                    sample_rgb_bilinear(
                        img,
                        c as f64 / cols.max(1) as f64,
                        1.0 - r as f64 / rows.max(1) as f64,
                    )
                })
                .unwrap_or_else(|| elevation_colormap(z, z_min, z_range));

            cloud
                .positions
                .push([enu.east as f32, enu.up as f32, -enu.north as f32]);
            cloud.scales_log.push(scale_log);
            cloud.rot_wxyz.push([1.0, 0.0, 0.0, 0.0]);
            cloud.opacity_logit.push(8.0); // ~0.9997 opacity — solid terrain, not translucent
            cloud.sh_dc.push([
                (rgb[0] - 0.5) / SH_C0,
                (rgb[1] - 0.5) / SH_C0,
                (rgb[2] - 0.5) / SH_C0,
            ]);
        }
    }
    cloud
}

fn sample_rgb_bilinear(img: &image::RgbImage, u: f64, v: f64) -> [f32; 3] {
    let (w, h) = img.dimensions();
    let x = (u.clamp(0.0, 1.0) * (w.saturating_sub(1)) as f64).round() as u32;
    let y = (v.clamp(0.0, 1.0) * (h.saturating_sub(1)) as f64).round() as u32;
    let p = img.get_pixel(x.min(w.saturating_sub(1)), y.min(h.saturating_sub(1)));
    [
        p[0] as f32 / 255.0,
        p[1] as f32 / 255.0,
        p[2] as f32 / 255.0,
    ]
}

/// Green→brown→white elevation ramp used when no imagery is available.
fn elevation_colormap(z: f32, z_min: f32, z_range: f32) -> [f32; 3] {
    let t = ((z - z_min) / z_range).clamp(0.0, 1.0);
    if t < 0.5 {
        let k = t * 2.0;
        [0.25 + 0.35 * k, 0.45 + 0.15 * k, 0.2]
    } else {
        let k = (t - 0.5) * 2.0;
        [0.6 + 0.35 * k, 0.6 + 0.3 * k, 0.2 + 0.7 * k]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_aoi_rejects_inverted_bounds() {
        assert!(validate_aoi([-1.0, 40.0, -2.0, 41.0]).is_err());
    }

    #[test]
    fn validate_aoi_rejects_huge_bounds() {
        assert!(validate_aoi([-100.0, 20.0, 0.0, 60.0]).is_err());
    }

    #[test]
    fn validate_aoi_rejects_out_of_range_latitude() {
        assert!(validate_aoi([-1.0, 95.0, 0.0, 96.0]).is_err());
    }

    #[test]
    fn validate_aoi_accepts_small_valid_box() {
        let aoi = validate_aoi([-122.42, 37.77, -122.40, 37.79]).unwrap();
        assert_eq!(aoi, [-122.42, 37.77, -122.40, 37.79]);
    }

    #[test]
    fn build_splat_cloud_skips_nodata_and_colors_by_elevation() {
        let grid = DemSampleGrid {
            cols: 2,
            rows: 2,
            bounds: [-122.42, 37.77, -122.40, 37.79],
            z: vec![10.0, f32::NAN, 20.0, 30.0],
            synthetic: false,
            bed_source: "real".into(),
            notes: vec![],
            source_path: None,
        };
        let cloud = build_splat_cloud(&grid, grid.bounds, 10.0, None);
        assert_eq!(
            cloud.len(),
            3,
            "nodata cell must be skipped, not fabricated"
        );
        assert!(cloud
            .positions
            .iter()
            .all(|p| p.iter().all(|c| c.is_finite())));
    }

    #[test]
    fn generate_sat_splat_rejects_invalid_aoi_before_any_network_call() {
        let ws = std::env::temp_dir().join(format!(
            "sat_splat_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let opts = SatSplatOpts {
            aoi_wgs84: [10.0, 10.0, 5.0, 5.0], // inverted — must fail fast, no fetch attempted
            ..Default::default()
        };
        let err = generate_sat_splat(&ws, &opts).unwrap_err();
        assert!(err.contains("west < east"));
        let _ = fs::remove_dir_all(&ws);
    }
}
