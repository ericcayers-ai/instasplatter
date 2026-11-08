//! Rasterize a registered reconstruction splat into a proxy DEM GeoTIFF.
//!
//! Opacity-weighted mean ellipsoidal height on an AOI grid → `geo/derived/splat_dem.tif`,
//! then staged through [`super::dem::condition_dem_with_opts`]. The bed is labelled
//! **proxy** / non-authoritative relative to USGS / OpenTopography.

use super::dem::{condition_dem_with_opts, DemProduct};
use super::exports::{write_geotiff, RasterGrid};
use super::transforms::{ecef_to_geodetic, Enu, EnuFrame, Geodetic};
use crate::project::{GeoReference, ModelTransform, Project};
use crate::splat::ply::read_ply;
use crate::splat::SplatCloud;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const NODATA: f32 = -9999.0;
const MAX_DIM: u32 = 1024;
const MIN_OPACITY: f32 = 0.05;

/// Options for splat → DEM generation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SplatDemOpts {
    /// WGS84 AOI `[west, south, east, north]`. Falls back to scenario AOI when omitted.
    pub aoi_wgs84: Option<[f64; 4]>,
    /// Target cell size in metres (default 2).
    pub cell_size_m: Option<f64>,
}

/// Summary of a successful splat rasterization (before flood conditioning).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplatDemRasterStats {
    pub path: String,
    pub cols: u32,
    pub rows: u32,
    pub cell_size_m: f64,
    pub points_used: u64,
    pub cells_filled: u64,
    pub aoi_wgs84: [f64; 4],
}

/// Generate a proxy DEM from the workspace splat + metric GeoReference.
///
/// Writes `geo/derived/splat_dem.tif`, stages it as `dtm_flood.tif`, and returns
/// a [`DemProduct`] with `bed_source = "proxy"`.
pub fn generate_splat_dem(workspace: &Path, opts: &SplatDemOpts) -> Result<DemProduct, String> {
    let project = Project::load(workspace)?;
    let geo = project.geo_reference.as_ref().ok_or_else(|| {
        "Geo registration missing — import telemetry or set GCPs (metric GeoReference) before generating a DEM from splat.".to_string()
    })?;
    require_metric_registration(geo)?;

    let splat_path = project.latest_splat.as_ref().ok_or_else(|| {
        "No reconstruction splat found — run Reconstruction first, then Generate DEM from splat."
            .to_string()
    })?;
    let splat_path = Path::new(splat_path);
    if !splat_path.exists() {
        return Err(format!(
            "Reconstruction splat missing on disk: {}",
            splat_path.display()
        ));
    }

    let aoi = opts
        .aoi_wgs84
        .or_else(|| first_scenario_aoi(&project))
        .ok_or_else(|| {
            "Draw an AOI before generating a DEM from splat (Flood lab needs WGS84 bounds)."
                .to_string()
        })?;
    validate_aoi(aoi)?;

    let cell_size_m = opts.cell_size_m.unwrap_or(2.0).clamp(0.25, 100.0);
    let cloud = read_ply(splat_path)?;
    if cloud.is_empty() {
        return Err("Splat PLY has no Gaussians — cannot rasterize DEM.".into());
    }

    crate::geospatial::prepare_workspace(workspace)?;
    let derived = workspace.join("geo").join("derived");
    fs::create_dir_all(&derived).map_err(|e| e.to_string())?;
    let splat_tif = derived.join("splat_dem.tif");

    let stats = rasterize_splat_to_geotiff(
        &cloud,
        geo,
        project.model_transform.as_ref(),
        aoi,
        cell_size_m,
        &splat_tif,
    )?;

    let mut product = condition_dem_with_opts(
        &splat_tif,
        &derived,
        Some(aoi),
        Some(cell_size_m),
        Some("EPSG:4326"),
        Some(NODATA as f64),
    )?;

    // Splat heights are a reconstruction proxy — never claim USGS/OpenTopo authority.
    product.synthetic = false;
    product.bed_source = Some("proxy".into());
    product.notes.insert(
        0,
        format!(
            "Splat→DEM proxy bed (non-authoritative): {}×{} @ {:.2} m, {} Gaussians → {} cells filled.",
            stats.cols, stats.rows, stats.cell_size_m, stats.points_used, stats.cells_filled
        ),
    );
    product.notes.push(
        "Prefer USGS 3DEP / OpenTopography / user GeoTIFF for authoritative flood beds.".into(),
    );
    rewrite_preview_bed_source(&derived, "proxy")?;

    // Sidecar so catalog / later agents can identify provenance.
    let side = derived.join("splat_dem.meta.json");
    let body = serde_json::json!({
        "kind": "splatDem",
        "authority": "proxy",
        "nonAuthoritative": true,
        "sourceSplat": splat_path.to_string_lossy(),
        "raster": stats,
        "geoScaleStatus": geo.scale_status,
        "localOrigin": geo.local_origin,
    });
    fs::write(
        &side,
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(product)
}

/// Opacity-weighted mean height rasterization onto a WGS84 AOI grid.
pub fn rasterize_splat_to_geotiff(
    cloud: &SplatCloud,
    geo: &GeoReference,
    model_tf: Option<&ModelTransform>,
    aoi: [f64; 4],
    cell_size_m: f64,
    dest: &Path,
) -> Result<SplatDemRasterStats, String> {
    require_metric_registration(geo)?;
    validate_aoi(aoi)?;

    let origin = geo.local_origin.ok_or_else(|| {
        "GeoReference has no local origin — recompute geo registration before splat DEM."
            .to_string()
    })?;
    let frame = EnuFrame::from_geodetic(Geodetic {
        lon_deg: origin[0],
        lat_deg: origin[1],
        height_m: origin[2],
    });

    let [west, south, east, north] = aoi;
    let lat0 = 0.5 * (south + north);
    let m_per_deg_lat = 111_320.0;
    let m_per_deg_lon = (111_320.0 * (lat0 * std::f64::consts::PI / 180.0).cos()).max(1e-6);
    let cell = cell_size_m.max(0.25);
    let dx_deg = cell / m_per_deg_lon;
    let dy_deg = cell / m_per_deg_lat;

    let mut cols = ((east - west) / dx_deg).ceil() as u32;
    let mut rows = ((north - south) / dy_deg).ceil() as u32;
    cols = cols.clamp(2, MAX_DIM);
    rows = rows.clamp(2, MAX_DIM);
    let dx = (east - west) / cols as f64;
    let dy = (north - south) / rows as f64;

    let n = (cols as usize) * (rows as usize);
    let mut sum_w = vec![0.0f64; n];
    let mut sum_wz = vec![0.0f64; n];
    let mut points_used = 0u64;

    let identity = ModelTransform::identity();
    let tf = model_tf.unwrap_or(&identity);

    for i in 0..cloud.len() {
        let opacity = cloud.opacity(i);
        if opacity < MIN_OPACITY {
            continue;
        }
        let enu = apply_model_transform(cloud.positions[i], tf);
        let ecef = frame.enu_to_ecef(Enu {
            east: enu[0],
            north: enu[1],
            up: enu[2],
        });
        let g = ecef_to_geodetic(ecef);
        if g.lon_deg < west || g.lon_deg >= east || g.lat_deg < south || g.lat_deg >= north {
            continue;
        }
        let col = ((g.lon_deg - west) / dx).floor() as i64;
        let row_from_south = ((g.lat_deg - south) / dy).floor() as i64;
        if col < 0 || row_from_south < 0 {
            continue;
        }
        let col = col as u32;
        let row_from_south = row_from_south as u32;
        if col >= cols || row_from_south >= rows {
            continue;
        }
        // Row 0 = north (GIS top-left).
        let row = rows - 1 - row_from_south;
        let idx = (row as usize) * (cols as usize) + (col as usize);
        let w = opacity as f64;
        sum_w[idx] += w;
        sum_wz[idx] += w * g.height_m;
        points_used += 1;
    }

    if points_used == 0 {
        return Err(
            "No splat Gaussians fall inside the AOI after geo registration — expand the AOI or check ENU alignment."
                .into(),
        );
    }

    let mut values = vec![NODATA; n];
    let mut cells_filled = 0u64;
    for i in 0..n {
        if sum_w[i] > 0.0 {
            values[i] = (sum_wz[i] / sum_w[i]) as f32;
            cells_filled += 1;
        }
    }

    let grid = RasterGrid {
        cols,
        rows,
        origin: [west, north],
        pixel_size: [dx, dy],
        crs: "EPSG:4326".into(),
        nodata: NODATA,
        values,
        quantity: "elevation".into(),
        units: "m".into(),
    };
    write_geotiff(dest, &grid)?;

    Ok(SplatDemRasterStats {
        path: dest.to_string_lossy().into_owned(),
        cols,
        rows,
        cell_size_m: cell,
        points_used,
        cells_filled,
        aoi_wgs84: aoi,
    })
}

/// Clear blocking error when registration is missing / unscaled.
pub fn require_metric_registration(geo: &GeoReference) -> Result<(), String> {
    match geo.scale_status.as_deref() {
        Some("metric") | Some("approx") => {}
        Some("unscaled") => {
            return Err(
                "Geo registration is unscaled — import telemetry or set GCPs for a metric GeoReference before generating a DEM from splat."
                    .into(),
            );
        }
        None => {
            return Err(
                "Geo registration missing — import telemetry or set GCPs (metric GeoReference) before generating a DEM from splat."
                    .into(),
            );
        }
        Some(other) => {
            return Err(format!(
                "Geo registration scale status '{other}' is not metric — cannot generate splat DEM."
            ));
        }
    }
    if geo.local_origin.is_none() {
        return Err(
            "GeoReference has no local origin — recompute geo registration before splat DEM."
                .into(),
        );
    }
    Ok(())
}

fn apply_model_transform(p: [f32; 3], tf: &ModelTransform) -> [f64; 3] {
    let sx = p[0] as f64 * tf.scale[0] as f64;
    let sy = p[1] as f64 * tf.scale[1] as f64;
    let sz = p[2] as f64 * tf.scale[2] as f64;
    let r = &tf.rotation;
    // Row-major 3×3: out = R * scaled + t
    [
        r[0] as f64 * sx + r[1] as f64 * sy + r[2] as f64 * sz + tf.translation[0] as f64,
        r[3] as f64 * sx + r[4] as f64 * sy + r[5] as f64 * sz + tf.translation[1] as f64,
        r[6] as f64 * sx + r[7] as f64 * sy + r[8] as f64 * sz + tf.translation[2] as f64,
    ]
}

fn validate_aoi(aoi: [f64; 4]) -> Result<(), String> {
    let [west, south, east, north] = aoi;
    if !(west < east && south < north) {
        return Err("AOI must have west < east and south < north".into());
    }
    if (east - west) < 1e-5 || (north - south) < 1e-5 {
        return Err("AOI is too small for splat DEM rasterization".into());
    }
    Ok(())
}

fn first_scenario_aoi(project: &Project) -> Option<[f64; 4]> {
    project.flood_scenarios.iter().find_map(|s| s.aoi_wgs84)
}

fn rewrite_preview_bed_source(derived: &Path, bed_source: &str) -> Result<(), String> {
    let path = derived.join("dtm_flood_preview.json");
    if !path.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("preview sidecar parse: {e}"))?;
    if let Some(obj) = v.as_object_mut() {
        obj.insert(
            "bedSource".into(),
            serde_json::Value::String(bed_source.into()),
        );
        obj.insert("synthetic".into(), serde_json::Value::Bool(false));
        obj.insert(
            "authority".into(),
            serde_json::Value::String("proxy".into()),
        );
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::Preset;
    use crate::settings::ResolvedSettings;
    use crate::splat::ply::write_ply;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_ws(tag: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("instasplatter_splat_dem_{tag}_{id}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn test_settings() -> ResolvedSettings {
        ResolvedSettings {
            preset: Preset::Balanced,
            max_frames: 100,
            max_resolution: 1280,
            blur_reject_fraction: 0.15,
            matcher: "auto".into(),
            sift_gpu: true,
            total_steps: 1000,
            max_splats: 1_000_000,
            sh_degree: 1,
            refine_every: 200,
            ssim_weight: 0.2,
            export_every: 500,
            progressive_resolution: false,
            mip_filter: false,
            live_init: false,
            dense_init: true,
            use_neural_init: false,
            allow_research_sidecars: false,
            experimental_mode: false,
            experimental_license_acked: false,
            post_polish: false,
            trainer: "brush".into(),
            gsplat_strategy: "mcmc".into(),
            gsplat_absgrad: true,
            gsplat_antialiased: true,
            gsplat_appearance: true,
            gsplat_bilateral_grid: true,
            roma_quality: "base".into(),
            strictness: 0.5,
            export_format: "ply".into(),
            keep_intermediates: false,
            opac_loss_weight: 1e-9,
            scale_loss_weight: 1e-8,
            mean_noise_weight: 40.0,
        }
    }

    fn metric_geo(lon: f64, lat: f64, h: f64) -> GeoReference {
        let frame = EnuFrame::from_geodetic(Geodetic {
            lon_deg: lon,
            lat_deg: lat,
            height_m: h,
        });
        GeoReference {
            source_crs: Some("EPSG:4979".into()),
            units: Some("m".into()),
            working_crs: Some("local-ENU-m".into()),
            ecef_to_enu: Some(frame.ecef_to_enu_matrix()),
            enu_to_ecef: Some(frame.enu_to_ecef_matrix()),
            local_origin: Some([lon, lat, h]),
            local_origin_ecef: Some(frame.origin_ecef.as_array()),
            scale_status: Some("metric".into()),
            provenance: Some("test".into()),
            ..Default::default()
        }
    }

    fn sample_cloud_flat(n_side: usize, z: f32) -> SplatCloud {
        let mut c = SplatCloud::default();
        for ix in 0..n_side {
            for iy in 0..n_side {
                let e = (ix as f32 - n_side as f32 * 0.5) * 2.0;
                let n = (iy as f32 - n_side as f32 * 0.5) * 2.0;
                c.positions.push([e, n, z]);
                c.scales_log.push([-2.0; 3]);
                c.rot_wxyz.push([1.0, 0.0, 0.0, 0.0]);
                // logit(~0.73)
                c.opacity_logit.push(1.0);
                c.sh_dc.push([0.0, 0.0, 0.0]);
            }
        }
        c.rest_per_channel = 0;
        c
    }

    #[test]
    fn rasterize_happy_path_writes_geotiff() {
        let ws = temp_ws("happy");
        let derived = ws.join("geo").join("derived");
        fs::create_dir_all(&derived).unwrap();
        let dest = derived.join("splat_dem.tif");

        let lon = 174.7633;
        let lat = -36.8485;
        let h0 = 80.0;
        let geo = metric_geo(lon, lat, h0);
        let cloud = sample_cloud_flat(8, 5.0);
        // ~40 m footprint around origin → small AOI in degrees
        let aoi = [lon - 0.0004, lat - 0.0004, lon + 0.0004, lat + 0.0004];

        let stats = rasterize_splat_to_geotiff(&cloud, &geo, None, aoi, 2.0, &dest).unwrap();
        assert!(dest.exists());
        assert!(stats.points_used > 0);
        assert!(stats.cells_filled > 0);
        assert!(stats.cols >= 2 && stats.rows >= 2);

        // Round-trip via condition path.
        let prod = condition_dem_with_opts(
            &dest,
            &derived,
            Some(aoi),
            Some(2.0),
            Some("EPSG:4326"),
            Some(NODATA as f64),
        )
        .unwrap();
        assert!(!prod.synthetic);
        assert!(Path::new(prod.dtm_path.as_ref().unwrap()).exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn generate_rejects_missing_registration() {
        let ws = temp_ws("noreg");
        let mut proj = Project::new("job_splat_dem", Path::new("in.mp4"), &ws, &test_settings());
        proj.geo_reference = None;
        let ply = ws.join("result.ply");
        write_ply(&ply, &sample_cloud_flat(4, 1.0)).unwrap();
        proj.latest_splat = Some(ply.to_string_lossy().into_owned());
        proj.save().unwrap();

        let err = generate_splat_dem(
            &ws,
            &SplatDemOpts {
                aoi_wgs84: Some([174.76, -36.85, 174.77, -36.84]),
                cell_size_m: Some(2.0),
            },
        )
        .unwrap_err();
        assert!(
            err.to_lowercase().contains("registration")
                || err.to_lowercase().contains("georeference")
                || err.to_lowercase().contains("geo registration"),
            "unexpected error: {err}"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn generate_rejects_unscaled_registration() {
        let ws = temp_ws("unscaled");
        let mut proj = Project::new("job_unscaled", Path::new("in.mp4"), &ws, &test_settings());
        proj.geo_reference = Some(GeoReference {
            scale_status: Some("unscaled".into()),
            local_origin: Some([174.76, -36.85, 50.0]),
            provenance: Some("no telemetry".into()),
            ..Default::default()
        });
        let ply = ws.join("result.ply");
        write_ply(&ply, &sample_cloud_flat(4, 1.0)).unwrap();
        proj.latest_splat = Some(ply.to_string_lossy().into_owned());
        proj.save().unwrap();

        let err = generate_splat_dem(
            &ws,
            &SplatDemOpts {
                aoi_wgs84: Some([174.76, -36.85, 174.77, -36.84]),
                cell_size_m: Some(2.0),
            },
        )
        .unwrap_err();
        assert!(
            err.to_lowercase().contains("unscaled") || err.to_lowercase().contains("metric"),
            "unexpected error: {err}"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn generate_happy_path_stages_proxy_dem() {
        let ws = temp_ws("gen");
        let lon = 174.7633;
        let lat = -36.8485;
        let h0 = 80.0;
        let mut proj = Project::new("job_gen", Path::new("in.mp4"), &ws, &test_settings());
        proj.geo_reference = Some(metric_geo(lon, lat, h0));
        let ply = ws.join("result.ply");
        write_ply(&ply, &sample_cloud_flat(10, 3.0)).unwrap();
        proj.latest_splat = Some(ply.to_string_lossy().into_owned());
        proj.save().unwrap();

        let aoi = [lon - 0.0005, lat - 0.0005, lon + 0.0005, lat + 0.0005];
        let prod = generate_splat_dem(
            &ws,
            &SplatDemOpts {
                aoi_wgs84: Some(aoi),
                cell_size_m: Some(2.0),
            },
        )
        .unwrap();

        assert_eq!(prod.bed_source.as_deref(), Some("proxy"));
        assert!(!prod.synthetic);
        assert!(prod
            .notes
            .iter()
            .any(|n| n.contains("non-authoritative") || n.contains("proxy")));
        let splat_tif = ws.join("geo").join("derived").join("splat_dem.tif");
        assert!(splat_tif.exists());
        assert!(Path::new(prod.dtm_path.as_ref().unwrap()).exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn require_metric_blocks_none_status() {
        let geo = GeoReference {
            local_origin: Some([0.0, 0.0, 0.0]),
            scale_status: None,
            ..Default::default()
        };
        let err = require_metric_registration(&geo).unwrap_err();
        assert!(err.contains("registration"));
    }
}
