//! Drone / survey georegistration: telemetry ingest, ENU/ECEF frames,
//! COLMAP pose-prior hooks, GCP Sim(3), and adaptive extent planning.

use crate::geospatial::transforms::{
    ecef_to_geodetic, geodetic_to_ecef, Ecef, Enu, EnuFrame, Geodetic,
};
use crate::math::{self, M3, V3};
use crate::project::{ensure_geo_workspace, GeoReference, Project};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod ingest;

pub use ingest::{
    ingest_path, list_telemetry_files, parse_gcp_csv, TelemetryPoint,
};
#[allow(unused_imports)]
pub use ingest::TelemetryFormat;

/// Soft outlier gate: residual > `median ×` this factor.
pub const GCP_OUTLIER_MEDIAN_FACTOR: f64 = 3.0;
/// Soft outlier floor (metres) so tiny medians do not flag noise.
pub const GCP_OUTLIER_FLOOR_M: f64 = 0.15;
/// Well-conditioned survey GCP mean residual target for release gates (metres).
pub const GCP_SURVEY_PASS_MEAN_M: f64 = 0.05;
/// Near-identity / synthetic GCP residual target (metres).
pub const GCP_IDENTITY_PASS_MEAN_M: f64 = 1e-4;

/// Result of a registration / ingest attempt.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RegistrationResult {
    pub geo_reference: GeoReference,
    pub camera_count: usize,
    pub telemetry_count: usize,
    pub matched_frames: usize,
    pub warnings: Vec<String>,
    pub pose_priors_path: Option<String>,
}

/// One surveyed / picked ground control point.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GcpPoint {
    pub id: String,
    /// Survey coordinates: lon/lat/height when `survey_crs` is geographic,
    /// otherwise cartesian metres in that CRS.
    pub survey_xyz: [f64; 3],
    /// e.g. "EPSG:4326", "EPSG:4979", "local-ENU-m"
    pub survey_crs: String,
    /// Optional scene / reconstruction pick (ENU or COLMAP metres).
    pub local_xyz: Option<[f64; 3]>,
    /// Optional image pick (image name, u, v pixels).
    pub image_name: Option<String>,
    pub pixel_uv: Option<[f64; 2]>,
    /// Diagonal covariance (metres) when known.
    pub covariance_m: Option<[f64; 3]>,
    pub outlier: bool,
}

/// Residual report after robust Sim(3) alignment.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GcpResidualReport {
    pub scale: f64,
    pub rotation: [[f64; 3]; 3],
    pub translation: [f64; 3],
    pub mean_residual_m: f64,
    pub max_residual_m: f64,
    pub rms_residual_m: f64,
    pub inlier_ids: Vec<String>,
    pub outlier_ids: Vec<String>,
    pub per_point_m: Vec<(String, f64)>,
}

/// Frame ↔ GPS match retaining covariance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedPose {
    pub image_name: String,
    pub unix_s: Option<f64>,
    pub lon_deg: f64,
    pub lat_deg: f64,
    pub height_m: f64,
    pub enu: [f64; 3],
    pub ecef: [f64; 3],
    /// Diagonal std metres [east/north-ish, …, up] when known.
    pub covariance_m: [f64; 3],
    pub heading_deg: Option<f64>,
    pub source: String,
}

/// Adaptive site→city planner outputs (no flood solvers).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtentPlan {
    pub working_crs: String,
    pub enu_origin: [f64; 3],
    /// [min_e, min_n, max_e, max_n] metres.
    pub bounds_enu: [f64; 4],
    pub extent_diag_m: f64,
    pub dem_resolution_m: f64,
    /// Suggested preview raster cell size (m).
    pub preview_cell_m: f64,
    /// Target max triangle area for scientific mesh (m²).
    pub scientific_mesh_max_area_m2: f64,
    /// Coarser mesh outside the splat footprint (m²).
    pub regional_mesh_max_area_m2: f64,
    /// Suggested terrain / basemap tile zoom hierarchy.
    pub terrain_tile_levels: Vec<u32>,
    pub scale_status: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtentPlanInput {
    /// Camera / telemetry positions in ENU metres.
    pub camera_enu: Vec<[f64; 3]>,
    /// Optional splat AABB [min_e, min_n, min_u, max_e, max_n, max_u].
    pub splat_bounds_enu: Option<[f64; 6]>,
    /// Optional DEM coverage [min_e, min_n, max_e, max_n].
    pub dem_bounds_enu: Option<[f64; 4]>,
    /// Source DEM GSD / accuracy floor (metres). Never refine below this.
    pub dem_accuracy_m: Option<f64>,
    /// Rough GPU/CPU preview budget (cells across longest axis). Default 1024.
    pub preview_budget_cells: Option<u32>,
    pub enu_origin: Option<[f64; 3]>,
    pub geo_reference: Option<GeoReference>,
}

/// Scan imagery / telemetry under `sources_dir` and produce a GeoReference.
pub fn register_from_sources(sources_dir: &Path) -> Result<RegistrationResult, String> {
    if !sources_dir.exists() {
        return Err(format!(
            "Sources directory does not exist: {}",
            sources_dir.display()
        ));
    }
    let mut points = Vec::new();
    let mut warnings = Vec::new();
    let mut formats = Vec::new();

    for path in list_telemetry_files(sources_dir) {
        match ingest_path(&path) {
            Ok((fmt, pts)) => {
                if !pts.is_empty() {
                    formats.push(fmt.id().to_string());
                    points.extend(pts);
                }
            }
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }

    // EXIF from images in the same tree.
    let images = collect_images(sources_dir);
    let mut exif_count = 0usize;
    for img in &images {
        if let Ok(Some(p)) = ingest::read_exif_gps(img) {
            points.push(p);
            exif_count += 1;
        }
    }
    if exif_count > 0 {
        formats.push("exif".into());
    }

    if points.is_empty() {
        return Ok(RegistrationResult {
            geo_reference: unscaled_reference(
                "No GPS/telemetry found — project remains unscaled",
            ),
            camera_count: images.len(),
            telemetry_count: 0,
            matched_frames: 0,
            warnings: {
                warnings.push(
                    "No coordinate source — visual preview OK; scientific flood needs scale."
                        .into(),
                );
                warnings
            },
            pose_priors_path: None,
        });
    }

    let geo = geo_reference_from_points(&points, &formats);
    let matched = match_frames_to_telemetry(&images, &points, 0.5);
    Ok(RegistrationResult {
        camera_count: images.len(),
        telemetry_count: points.len(),
        matched_frames: matched.len(),
        geo_reference: geo,
        warnings,
        pose_priors_path: None,
    })
}

/// Import telemetry into a project workspace and persist GeoReference + pose priors.
pub fn import_telemetry_into_project(
    workspace: &Path,
    paths: &[PathBuf],
) -> Result<RegistrationResult, String> {
    ensure_geo_workspace(workspace)?;
    let dest = workspace.join("geo").join("sources");
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    let mut all_points = Vec::new();
    let mut warnings = Vec::new();
    let mut formats = Vec::new();
    let mut copied = Vec::new();

    for src in paths {
        if !src.exists() {
            warnings.push(format!("Missing: {}", src.display()));
            continue;
        }
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "telemetry.bin".into());
        let dst = unique_path(&dest, &name);
        fs::copy(src, &dst).map_err(|e| format!("Copy {}: {e}", src.display()))?;
        copied.push(dst.clone());
        match ingest_path(&dst) {
            Ok((fmt, pts)) => {
                formats.push(fmt.id().to_string());
                all_points.extend(pts);
            }
            Err(e) => warnings.push(format!("{}: {e}", dst.display())),
        }
    }

    // Also scan images already in the workspace.
    let image_roots = [
        workspace.join("images"),
        workspace.join("geo").join("sources"),
        workspace.to_path_buf(),
    ];
    let mut images = Vec::new();
    for root in &image_roots {
        if root.exists() {
            images.extend(collect_images(root));
        }
    }
    images.sort();
    images.dedup();

    for img in &images {
        if let Ok(Some(p)) = ingest::read_exif_gps(img) {
            all_points.push(p);
            if !formats.iter().any(|f| f == "exif") {
                formats.push("exif".into());
            }
        }
    }

    let result = if all_points.is_empty() {
        RegistrationResult {
            geo_reference: unscaled_reference("Imported paths had no parseable coordinates"),
            camera_count: images.len(),
            telemetry_count: 0,
            matched_frames: 0,
            warnings,
            pose_priors_path: None,
        }
    } else {
        let geo = geo_reference_from_points(&all_points, &formats);
        let matched = match_frames_to_telemetry(&images, &all_points, 0.5);
        let frame = EnuFrame::from_geodetic(Geodetic {
            lon_deg: geo.local_origin.map(|o| o[0]).unwrap_or(0.0),
            lat_deg: geo.local_origin.map(|o| o[1]).unwrap_or(0.0),
            height_m: geo.local_origin.map(|o| o[2]).unwrap_or(0.0),
        });
        let poses: Vec<MatchedPose> = if matched.is_empty() {
            // No frame timestamps — emit one prior per telemetry sample as orphan GPS.
            all_points
                .iter()
                .enumerate()
                .map(|(i, p)| telemetry_as_pose(p, &frame, i))
                .collect()
        } else {
            matched
        };
        let prior_path = write_pose_priors(workspace, &poses)?;
        mark_gps_present(workspace);
        RegistrationResult {
            geo_reference: geo,
            camera_count: images.len(),
            telemetry_count: all_points.len(),
            matched_frames: poses.len(),
            warnings,
            pose_priors_path: Some(prior_path.to_string_lossy().into_owned()),
        }
    };

    persist_geo_reference(workspace, &result.geo_reference)?;
    let _ = copied;
    Ok(result)
}

/// Replace / merge GCP list on a project and optionally refine Sim(3).
pub fn set_project_gcps(
    workspace: &Path,
    gcps: Vec<GcpPoint>,
    refine: bool,
) -> Result<(GeoReference, Option<GcpResidualReport>), String> {
    ensure_geo_workspace(workspace)?;
    let gcp_path = workspace.join("geo").join("sources").join("gcps.json");
    let json = serde_json::to_string_pretty(&gcps).map_err(|e| e.to_string())?;
    fs::write(&gcp_path, json).map_err(|e| e.to_string())?;

    let proj = Project::load(workspace)?;
    let mut geo = proj
        .geo_reference
        .clone()
        .unwrap_or_else(|| unscaled_reference("GCP edit"));

    let report = if refine {
        let picked: Vec<_> = gcps
            .iter()
            .filter(|g| g.local_xyz.is_some() && !g.outlier)
            .cloned()
            .collect();
        if picked.len() >= 3 {
            let report = solve_gcp_sim3(&picked, &geo)?;
            geo.gcp_residual_m = Some(report.mean_residual_m);
            geo.gcp_residual_max_m = Some(report.max_residual_m);
            geo.scale_status = Some("metric".into());
            geo.units = Some("m".into());
            geo.provenance = Some(format!(
                "{}; GCP Sim(3) n={} mean={:.3}m max={:.3}m",
                geo.provenance.unwrap_or_default(),
                report.inlier_ids.len(),
                report.mean_residual_m,
                report.max_residual_m
            ));
            Some(report)
        } else {
            None
        }
    } else {
        None
    };

    persist_geo_reference(workspace, &geo)?;
    Ok((geo, report))
}

/// Apply GCPs from a CSV file to refine a GeoReference.
pub fn refine_with_gcps(
    base: &GeoReference,
    gcp_csv: &Path,
) -> Result<RegistrationResult, String> {
    let gcps = parse_gcp_csv(gcp_csv)?;
    if gcps.is_empty() {
        return Ok(RegistrationResult {
            geo_reference: base.clone(),
            warnings: vec!["GCP CSV contained no points.".into()],
            ..Default::default()
        });
    }
    // CSV-only GCPs lack local picks — store survey coords and wait for viewport picks.
    let mut geo = base.clone();
    geo.provenance = Some(format!(
        "{}; GCP CSV {} ({} pts, awaiting scene picks)",
        base.provenance.clone().unwrap_or_default(),
        gcp_csv
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        gcps.len()
    ));
    if geo.scale_status.as_deref() != Some("metric") {
        geo.scale_status = Some(
            if geo.local_origin.is_some() {
                "approx"
            } else {
                "unscaled"
            }
            .into(),
        );
    }
    Ok(RegistrationResult {
        geo_reference: geo,
        telemetry_count: gcps.len(),
        warnings: vec![
            "GCP survey points loaded; set local picks and call set_geo_gcps(refine=true)."
                .into(),
        ],
        ..Default::default()
    })
}

/// Recompute GeoReference from workspace telemetry + optional ENU origin override.
pub fn compute_geo_reference(
    workspace: &Path,
    origin_lon_lat_h: Option<[f64; 3]>,
) -> Result<RegistrationResult, String> {
    let sources = workspace.join("geo").join("sources");
    let mut result = if sources.exists() {
        register_from_sources(&sources)?
    } else {
        register_from_sources(workspace)?
    };

    if let Some(o) = origin_lon_lat_h {
        let frame = EnuFrame::from_geodetic(Geodetic {
            lon_deg: o[0],
            lat_deg: o[1],
            height_m: o[2],
        });
        result.geo_reference.local_origin = Some(o);
        result.geo_reference.local_origin_ecef = Some(frame.origin_ecef.as_array());
        result.geo_reference.ecef_to_enu = Some(frame.ecef_to_enu_matrix());
        result.geo_reference.enu_to_ecef = Some(frame.enu_to_ecef_matrix());
        result.geo_reference.working_crs = Some("local-ENU-m".into());
        result.geo_reference.source_crs = Some("EPSG:4979".into());
        result.geo_reference.units = Some("m".into());
        if result.geo_reference.scale_status.as_deref() == Some("unscaled") {
            result.geo_reference.scale_status = Some("approx".into());
        }
    }

    // Rewrite pose priors if we have telemetry.
    let images = {
        let mut v = collect_images(&workspace.join("images"));
        v.extend(collect_images(&sources));
        v.sort();
        v.dedup();
        v
    };
    let mut pts = Vec::new();
    if sources.exists() {
        for path in list_telemetry_files(&sources) {
            if let Ok((_, p)) = ingest_path(&path) {
                pts.extend(p);
            }
        }
    }
    for img in &images {
        if let Ok(Some(p)) = ingest::read_exif_gps(img) {
            pts.push(p);
        }
    }
    if !pts.is_empty() {
        if let Some(origin) = result.geo_reference.local_origin {
            let frame = EnuFrame::from_geodetic(Geodetic {
                lon_deg: origin[0],
                lat_deg: origin[1],
                height_m: origin[2],
            });
            let matched = match_frames_to_telemetry(&images, &pts, 0.5);
            let poses = if matched.is_empty() {
                pts.iter()
                    .enumerate()
                    .map(|(i, p)| telemetry_as_pose(p, &frame, i))
                    .collect()
            } else {
                matched
            };
            let prior = write_pose_priors(workspace, &poses)?;
            result.pose_priors_path = Some(prior.to_string_lossy().into_owned());
            result.matched_frames = poses.len();
            result.telemetry_count = pts.len();
            mark_gps_present(workspace);
        }
    }

    persist_geo_reference(workspace, &result.geo_reference)?;
    Ok(result)
}

/// Plan working CRS, tile hierarchy, and mesh/preview resolutions.
pub fn plan_extent(input: &ExtentPlanInput) -> ExtentPlan {
    let mut notes = Vec::new();
    let geo = input.geo_reference.clone().unwrap_or_default();
    let scale_status = geo
        .scale_status
        .clone()
        .unwrap_or_else(|| {
            if geo.local_origin.is_some() {
                "approx".into()
            } else {
                "unscaled".into()
            }
        });

    let origin = input
        .enu_origin
        .or(geo.local_origin)
        .unwrap_or([0.0, 0.0, 0.0]);

    let mut min_e = f64::INFINITY;
    let mut min_n = f64::INFINITY;
    let mut max_e = f64::NEG_INFINITY;
    let mut max_n = f64::NEG_INFINITY;

    let mut expand = |e: f64, n: f64| {
        min_e = min_e.min(e);
        min_n = min_n.min(n);
        max_e = max_e.max(e);
        max_n = max_n.max(n);
    };

    for p in &input.camera_enu {
        expand(p[0], p[1]);
    }
    if let Some(b) = input.splat_bounds_enu {
        expand(b[0], b[1]);
        expand(b[3], b[4]);
    }
    if let Some(b) = input.dem_bounds_enu {
        expand(b[0], b[1]);
        expand(b[2], b[3]);
    }

    if !min_e.is_finite() {
        // Default small site box so planner still returns something.
        min_e = -50.0;
        min_n = -50.0;
        max_e = 50.0;
        max_n = 50.0;
        notes.push("No camera/splat/DEM bounds — using 100 m default box.".into());
    } else {
        // 10% padding.
        let pad_e = (max_e - min_e).max(10.0) * 0.1;
        let pad_n = (max_n - min_n).max(10.0) * 0.1;
        min_e -= pad_e;
        max_e += pad_e;
        min_n -= pad_n;
        max_n += pad_n;
    }

    let width = (max_e - min_e).max(1.0);
    let height = (max_n - min_n).max(1.0);
    let diag = (width * width + height * height).sqrt();

    let dem_acc = input.dem_accuracy_m.unwrap_or(1.0).max(0.05);
    notes.push(format!(
        "DEM accuracy floor {dem_acc:.2} m — will not invent finer detail."
    ));

    let budget = input.preview_budget_cells.unwrap_or(1024).max(64) as f64;
    let preview_from_extent = diag / budget;
    let preview_cell = preview_from_extent.max(dem_acc);

    // Scientific mesh: ~2–4× DEM GSD near detail, coarser regionally.
    let site_edge = dem_acc.max(0.5);
    let scientific_area = (site_edge * site_edge) * 2.0;
    let regional_edge = (diag / 200.0).max(dem_acc * 4.0);
    let regional_area = regional_edge * regional_edge;

    // Tile zooms: ~web-mercator metres/pixel hierarchy around the footprint.
    let mut levels = Vec::new();
    let mut z = 12u32;
    while z <= 18 {
        let m_per_px = 156_543.03 * origin[1].to_radians().cos() / 2f64.powi(z as i32);
        if m_per_px >= dem_acc * 0.5 || z == 12 {
            levels.push(z);
        }
        if m_per_px < dem_acc {
            break;
        }
        z += 1;
    }
    if levels.is_empty() {
        levels = vec![14, 15, 16];
    }

    if scale_status == "unscaled" {
        notes.push(
            "Unscaled project — planner metres are relative; flood science locked.".into(),
        );
    }

    let working_crs = geo
        .working_crs
        .clone()
        .unwrap_or_else(|| "local-ENU-m".into());

    ExtentPlan {
        working_crs,
        enu_origin: origin,
        bounds_enu: [min_e, min_n, max_e, max_n],
        extent_diag_m: diag,
        dem_resolution_m: dem_acc,
        preview_cell_m: preview_cell,
        scientific_mesh_max_area_m2: scientific_area,
        regional_mesh_max_area_m2: regional_area,
        terrain_tile_levels: levels,
        scale_status,
        notes,
    }
}

/// Write COLMAP-oriented pose prior text (GPS XYZ + covariance).
///
/// Layout (commented header + rows):
/// `# image_name ecef_x ecef_y ecef_z std_x std_y std_z`
///
/// Reconstruction looks for `workspace/geo/pose_priors.txt` and
/// `workspace/pose_priors.txt` (see `pipeline::solver::has_pose_priors`).
pub fn write_pose_priors(workspace: &Path, poses: &[MatchedPose]) -> Result<PathBuf, String> {
    ensure_geo_workspace(workspace)?;
    let mut body = String::from(
        "# InstaSplatter pose priors (ECEF metres + diagonal std)\n\
         # image_name ecef_x ecef_y ecef_z std_x std_y std_z\n",
    );
    for p in poses {
        body.push_str(&format!(
            "{} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4}\n",
            p.image_name, p.ecef[0], p.ecef[1], p.ecef[2], p.covariance_m[0], p.covariance_m[1],
            p.covariance_m[2]
        ));
    }
    let geo_path = workspace.join("geo").join("pose_priors.txt");
    fs::write(&geo_path, &body).map_err(|e| e.to_string())?;
    let root_path = workspace.join("pose_priors.txt");
    fs::write(&root_path, &body).map_err(|e| e.to_string())?;

    // Gravity / heading priors when available (hook for COLMAP rotation priors).
    let mut grav = String::from("# image_name heading_deg\n");
    let mut any_heading = false;
    for p in poses {
        if let Some(h) = p.heading_deg {
            grav.push_str(&format!("{} {:.3}\n", p.image_name, h));
            any_heading = true;
        }
    }
    if any_heading {
        let _ = fs::write(workspace.join("gravity_priors.txt"), &grav);
        let _ = fs::write(workspace.join("geo").join("gravity_priors.txt"), &grav);
    }
    Ok(geo_path)
}

fn mark_gps_present(workspace: &Path) {
    let images = workspace.join("images");
    if images.is_dir() {
        let _ = fs::write(images.join(".gps_present"), b"1");
    }
    let hints = workspace.join("capture_hints");
    let _ = fs::create_dir_all(&hints);
    let _ = fs::write(hints.join("gps"), b"1");
}

fn persist_geo_reference(workspace: &Path, geo: &GeoReference) -> Result<(), String> {
    let mut proj = match Project::load(workspace) {
        Ok(p) => p,
        Err(_) => {
            // Allow geo ops before a full project exists (tests / early import).
            return Ok(());
        }
    };
    proj.geo_reference = Some(geo.clone());
    proj.touch();
    proj.save()
}

fn unscaled_reference(reason: &str) -> GeoReference {
    GeoReference {
        units: Some("m".into()),
        working_crs: Some("local-unscaled".into()),
        scale_status: Some("unscaled".into()),
        provenance: Some(reason.into()),
        ..Default::default()
    }
}

fn geo_reference_from_points(points: &[TelemetryPoint], formats: &[String]) -> GeoReference {
    let mut lon = 0.0;
    let mut lat = 0.0;
    let mut h = 0.0;
    let mut w_sum = 0.0;
    let mut unc_acc = 0.0;
    for p in points {
        let w = weight_from_cov(p.covariance_m);
        lon += p.lon_deg * w;
        lat += p.lat_deg * w;
        h += p.height_m * w;
        w_sum += w;
        unc_acc += p.covariance_m[0].max(p.covariance_m[1]);
    }
    if w_sum <= 0.0 {
        return unscaled_reference("Empty telemetry");
    }
    lon /= w_sum;
    lat /= w_sum;
    h /= w_sum;
    let unc = unc_acc / points.len() as f64;

    let frame = EnuFrame::from_geodetic(Geodetic {
        lon_deg: lon,
        lat_deg: lat,
        height_m: h,
    });
    let fmt_list = if formats.is_empty() {
        "telemetry".into()
    } else {
        formats.join("+")
    };
    GeoReference {
        source_crs: Some("EPSG:4979".into()),
        vertical_datum: Some("ellipsoidal".into()),
        units: Some("m".into()),
        working_crs: Some("local-ENU-m".into()),
        ecef_to_enu: Some(frame.ecef_to_enu_matrix()),
        enu_to_ecef: Some(frame.enu_to_ecef_matrix()),
        local_origin: Some([lon, lat, h]),
        local_origin_ecef: Some(frame.origin_ecef.as_array()),
        uncertainty_m: Some(unc),
        gcp_residual_m: None,
        gcp_residual_max_m: None,
        provenance: Some(format!(
            "{fmt_list}; n={}; origin=[{lon:.8},{lat:.8},{h:.2}]",
            points.len()
        )),
        scale_status: Some("metric".into()),
    }
}

fn weight_from_cov(cov: [f64; 3]) -> f64 {
    let s = cov[0].max(0.05).max(cov[1].max(0.05));
    1.0 / (s * s)
}

/// Match image files to nearest telemetry by unix timestamp (seconds).
pub fn match_frames_to_telemetry(
    images: &[PathBuf],
    points: &[TelemetryPoint],
    max_dt_s: f64,
) -> Vec<MatchedPose> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut timed: Vec<&TelemetryPoint> = points.iter().filter(|p| p.unix_s.is_some()).collect();
    if timed.is_empty() {
        return Vec::new();
    }
    timed.sort_by(|a, b| {
        a.unix_s
            .partial_cmp(&b.unix_s)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Origin from all points.
    let geo = geo_reference_from_points(points, &[]);
    let origin = geo.local_origin.unwrap_or([0.0, 0.0, 0.0]);
    let frame = EnuFrame::from_geodetic(Geodetic {
        lon_deg: origin[0],
        lat_deg: origin[1],
        height_m: origin[2],
    });

    let mut out = Vec::new();
    for img in images {
        let name = img
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let t = frame_unix_hint(img).or_else(|| ingest::read_exif_unix(img).ok().flatten());
        let Some(t) = t else {
            continue;
        };
        let Some(best) = nearest_timed(&timed, t, max_dt_s) else {
            continue;
        };
        let ecef = geodetic_to_ecef(Geodetic {
            lon_deg: best.lon_deg,
            lat_deg: best.lat_deg,
            height_m: best.height_m,
        });
        let enu = frame.ecef_to_enu(ecef);
        out.push(MatchedPose {
            image_name: name,
            unix_s: Some(t),
            lon_deg: best.lon_deg,
            lat_deg: best.lat_deg,
            height_m: best.height_m,
            enu: enu.as_array(),
            ecef: ecef.as_array(),
            covariance_m: best.covariance_m,
            heading_deg: best.heading_deg,
            source: best.source.clone(),
        });
    }
    out
}

fn nearest_timed<'a>(
    timed: &[&'a TelemetryPoint],
    t: f64,
    max_dt_s: f64,
) -> Option<&'a TelemetryPoint> {
    let mut best: Option<&TelemetryPoint> = None;
    let mut best_dt = f64::INFINITY;
    for p in timed {
        let dt = (p.unix_s.unwrap() - t).abs();
        if dt < best_dt {
            best_dt = dt;
            best = Some(*p);
        }
    }
    if best_dt <= max_dt_s {
        best
    } else {
        None
    }
}

fn telemetry_as_pose(p: &TelemetryPoint, frame: &EnuFrame, idx: usize) -> MatchedPose {
    let ecef = geodetic_to_ecef(Geodetic {
        lon_deg: p.lon_deg,
        lat_deg: p.lat_deg,
        height_m: p.height_m,
    });
    let enu = frame.ecef_to_enu(ecef);
    MatchedPose {
        image_name: p
            .image_name
            .clone()
            .unwrap_or_else(|| format!("telemetry_{idx:05}")),
        unix_s: p.unix_s,
        lon_deg: p.lon_deg,
        lat_deg: p.lat_deg,
        height_m: p.height_m,
        enu: enu.as_array(),
        ecef: ecef.as_array(),
        covariance_m: p.covariance_m,
        heading_deg: p.heading_deg,
        source: p.source.clone(),
    }
}

fn frame_unix_hint(img: &Path) -> Option<f64> {
    // Prefer sidecar `.time` (seconds) written by ingest tools.
    let side = img.with_extension("time");
    if let Ok(t) = fs::read_to_string(&side) {
        if let Ok(v) = t.trim().parse::<f64>() {
            return Some(v);
        }
    }
    // Filename patterns: *_20230101_120000*, DJI_YYYYMMDDHHMMSS
    let stem = img.file_stem()?.to_string_lossy();
    if let Some(v) = ingest::parse_time_from_name(&stem) {
        return Some(v);
    }
    let meta = fs::metadata(img).ok()?;
    let modified = meta.modified().ok()?;
    Some(
        modified
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs_f64(),
    )
}

fn collect_images(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        if p.is_dir() {
            // Avoid deep descent into geo/tiles etc.
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "images" | "frames" | "sources") {
                out.extend(collect_images(&p));
            }
            continue;
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            ext.as_str(),
            "jpg" | "jpeg" | "png" | "tif" | "tiff" | "webp"
        ) {
            out.push(p);
        }
    }
    out.sort();
    out
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let ext = Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for i in 1..1000 {
        let p = dir.join(format!("{stem}_{i}{ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!(
        "{stem}_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ))
}

/// Robust Umeyama Sim(3): local picks → survey ENU. Marks outliers by residual.
pub fn solve_gcp_sim3(
    gcps: &[GcpPoint],
    geo: &GeoReference,
) -> Result<GcpResidualReport, String> {
    let frame = match geo.local_origin {
        Some(o) => EnuFrame::from_geodetic(Geodetic {
            lon_deg: o[0],
            lat_deg: o[1],
            height_m: o[2],
        }),
        None => {
            // Derive origin from survey lon/lat GCPs.
            let geographic: Vec<_> = gcps
                .iter()
                .filter(|g| {
                    g.survey_crs.contains("4326")
                        || g.survey_crs.contains("4979")
                        || g.survey_crs.eq_ignore_ascii_case("wgs84")
                })
                .collect();
            if geographic.is_empty() {
                return Err("Need GeoReference origin or geographic GCPs for Sim(3).".into());
            }
            let n = geographic.len() as f64;
            let lon = geographic.iter().map(|g| g.survey_xyz[0]).sum::<f64>() / n;
            let lat = geographic.iter().map(|g| g.survey_xyz[1]).sum::<f64>() / n;
            let h = geographic.iter().map(|g| g.survey_xyz[2]).sum::<f64>() / n;
            EnuFrame::from_geodetic(Geodetic {
                lon_deg: lon,
                lat_deg: lat,
                height_m: h,
            })
        }
    };

    let mut src = Vec::new();
    let mut tgt = Vec::new();
    let mut ids = Vec::new();
    for g in gcps {
        let Some(local) = g.local_xyz else { continue };
        let survey_enu = survey_to_enu(g, &frame)?;
        src.push([local[0] as f32, local[1] as f32, local[2] as f32]);
        tgt.push([
            survey_enu[0] as f32,
            survey_enu[1] as f32,
            survey_enu[2] as f32,
        ]);
        ids.push(g.id.clone());
    }
    if src.len() < 3 {
        return Err("Need at least 3 GCPs with local picks.".into());
    }

    let sim = estimate_sim3_gcp(&src, &tgt).ok_or("Sim(3) solve failed")?;
    let mut per = Vec::new();
    let mut residuals = Vec::new();
    for i in 0..src.len() {
        let pred = apply_sim3(&sim, src[i]);
        let d = [
            pred[0] as f64 - tgt[i][0] as f64,
            pred[1] as f64 - tgt[i][1] as f64,
            pred[2] as f64 - tgt[i][2] as f64,
        ];
        let r = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        residuals.push(r);
        per.push((ids[i].clone(), r));
    }

    let mean = residuals.iter().sum::<f64>() / residuals.len() as f64;
    let max = residuals.iter().cloned().fold(0.0_f64, f64::max);
    let rms = (residuals.iter().map(|r| r * r).sum::<f64>() / residuals.len() as f64).sqrt();
    // Soft outlier: > k× median, floored, or elevated vs mean.
    let mut sorted = residuals.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2];
    let thresh = (median * GCP_OUTLIER_MEDIAN_FACTOR)
        .max(GCP_OUTLIER_FLOOR_M)
        .max(mean * 2.5);

    let mut inliers = Vec::new();
    let mut outliers = Vec::new();
    for (i, r) in residuals.iter().enumerate() {
        if *r > thresh {
            outliers.push(ids[i].clone());
        } else {
            inliers.push(ids[i].clone());
        }
    }

    Ok(GcpResidualReport {
        scale: sim.scale,
        rotation: sim.rotation,
        translation: sim.translation,
        mean_residual_m: mean,
        max_residual_m: max,
        rms_residual_m: rms,
        inlier_ids: inliers,
        outlier_ids: outliers,
        per_point_m: per,
    })
}

fn survey_to_enu(g: &GcpPoint, frame: &EnuFrame) -> Result<[f64; 3], String> {
    let crs = g.survey_crs.to_ascii_lowercase();
    if crs.contains("4326") || crs.contains("4979") || crs == "wgs84" {
        let ecef = geodetic_to_ecef(Geodetic {
            lon_deg: g.survey_xyz[0],
            lat_deg: g.survey_xyz[1],
            height_m: g.survey_xyz[2],
        });
        return Ok(frame.ecef_to_enu(ecef).as_array());
    }
    if crs.contains("enu") || crs.contains("local") {
        return Ok(g.survey_xyz);
    }
    // Assume already metres in the working ENU frame.
    Ok(g.survey_xyz)
}

struct Sim3Solve {
    scale: f64,
    rotation: M3,
    translation: V3,
}

fn apply_sim3(s: &Sim3Solve, p: [f32; 3]) -> [f32; 3] {
    let v = math::m3_mul_v(s.rotation, [p[0] as f64, p[1] as f64, p[2] as f64]);
    [
        (s.scale * v[0] + s.translation[0]) as f32,
        (s.scale * v[1] + s.translation[1]) as f32,
        (s.scale * v[2] + s.translation[2]) as f32,
    ]
}

fn estimate_sim3_gcp(src: &[[f32; 3]], tgt: &[[f32; 3]]) -> Option<Sim3Solve> {
    let n = src.len().min(tgt.len());
    if n < 3 {
        return None;
    }
    let mut ca = [0.0f64; 3];
    let mut cb = [0.0f64; 3];
    for i in 0..n {
        ca[0] += src[i][0] as f64;
        ca[1] += src[i][1] as f64;
        ca[2] += src[i][2] as f64;
        cb[0] += tgt[i][0] as f64;
        cb[1] += tgt[i][1] as f64;
        cb[2] += tgt[i][2] as f64;
    }
    let nf = n as f64;
    ca = [ca[0] / nf, ca[1] / nf, ca[2] / nf];
    cb = [cb[0] / nf, cb[1] / nf, cb[2] / nf];

    let mut ra = 0.0;
    let mut rb = 0.0;
    for i in 0..n {
        let da = [
            src[i][0] as f64 - ca[0],
            src[i][1] as f64 - ca[1],
            src[i][2] as f64 - ca[2],
        ];
        let db = [
            tgt[i][0] as f64 - cb[0],
            tgt[i][1] as f64 - cb[1],
            tgt[i][2] as f64 - cb[2],
        ];
        ra += da[0] * da[0] + da[1] * da[1] + da[2] * da[2];
        rb += db[0] * db[0] + db[1] * db[1] + db[2] * db[2];
    }
    ra = (ra / nf).sqrt().max(1e-9);
    rb = (rb / nf).sqrt().max(1e-9);
    let scale = rb / ra;

    let mut h = [[0.0f64; 3]; 3];
    for i in 0..n {
        let pa = [
            (src[i][0] as f64 - ca[0]) / ra,
            (src[i][1] as f64 - ca[1]) / ra,
            (src[i][2] as f64 - ca[2]) / ra,
        ];
        let pb = [
            (tgt[i][0] as f64 - cb[0]) / rb,
            (tgt[i][1] as f64 - cb[1]) / rb,
            (tgt[i][2] as f64 - cb[2]) / rb,
        ];
        for r in 0..3 {
            for c in 0..3 {
                h[r][c] += pa[r] * pb[c];
            }
        }
    }
    let (u, _s, v) = math::svd3(h);
    // Kabsch / Umeyama: H = Σ src tgtᵀ = U S Vᵀ ⇒ R = V Uᵀ
    let mut r = math::m3_mul(v, math::m3_transpose(u));
    if math::m3_det(r) < 0.0 {
        let mut v_fix = v;
        v_fix[0][2] *= -1.0;
        v_fix[1][2] *= -1.0;
        v_fix[2][2] *= -1.0;
        r = math::m3_mul(v_fix, math::m3_transpose(u));
    }
    r = math::orthonormalize(r);
    let t = [
        cb[0] - scale * (r[0][0] * ca[0] + r[0][1] * ca[1] + r[0][2] * ca[2]),
        cb[1] - scale * (r[1][0] * ca[0] + r[1][1] * ca[1] + r[1][2] * ca[2]),
        cb[2] - scale * (r[2][0] * ca[0] + r[2][1] * ca[1] + r[2][2] * ca[2]),
    ];
    Some(Sim3Solve {
        scale,
        rotation: r,
        translation: t,
    })
}

#[allow(dead_code)]
fn _keep_ecef_helpers_linked() {
    let _ = ecef_to_geodetic(Ecef::default());
    let _ = Enu::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn unscaled_when_empty_sources() {
        let dir = std::env::temp_dir().join(format!(
            "instasplatter_reg_empty_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let r = register_from_sources(&dir).unwrap();
        assert_eq!(r.geo_reference.scale_status.as_deref(), Some("unscaled"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn planner_respects_dem_floor() {
        let plan = plan_extent(&ExtentPlanInput {
            camera_enu: vec![[0.0, 0.0, 0.0], [200.0, 100.0, 0.0]],
            dem_accuracy_m: Some(2.0),
            preview_budget_cells: Some(10_000),
            ..Default::default()
        });
        assert!(plan.preview_cell_m >= 2.0 - 1e-9);
        assert!(plan.scientific_mesh_max_area_m2 >= 4.0);
        assert!(!plan.terrain_tile_levels.is_empty());
        assert_eq!(plan.working_crs, "local-ENU-m");
    }

    #[test]
    fn csv_ingest_builds_metric_frame() {
        let dir = std::env::temp_dir().join(format!(
            "instasplatter_reg_csv_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let csv = dir.join("track.csv");
        let mut f = fs::File::create(&csv).unwrap();
        writeln!(
            f,
            "time,lat,lon,alt,std_h,std_v\n\
             1700000000.0,-36.8485,174.7633,80.0,0.5,1.0\n\
             1700000001.0,-36.8486,174.7634,81.0,0.5,1.0"
        )
        .unwrap();
        let r = register_from_sources(&dir).unwrap();
        assert_eq!(r.geo_reference.scale_status.as_deref(), Some("metric"));
        assert!(r.geo_reference.ecef_to_enu.is_some());
        assert!(r.geo_reference.enu_to_ecef.is_some());
        assert!(r.telemetry_count >= 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn gcp_sim3_near_identity() {
        let geo = GeoReference {
            local_origin: Some([174.76, -36.85, 50.0]),
            scale_status: Some("metric".into()),
            ..Default::default()
        };
        let locals = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [5.0, 5.0, 1.0],
        ];
        let mut gcps = Vec::new();
        for (i, local) in locals.iter().enumerate() {
            gcps.push(GcpPoint {
                id: format!("g{i}"),
                survey_xyz: *local,
                survey_crs: "local-ENU-m".into(),
                local_xyz: Some(*local),
                ..Default::default()
            });
        }
        let report = solve_gcp_sim3(&gcps, &geo).unwrap();
        assert!((report.scale - 1.0).abs() < 1e-5);
        assert!(report.mean_residual_m < GCP_IDENTITY_PASS_MEAN_M);
        assert!(report.outlier_ids.is_empty());
    }

    #[test]
    fn gcp_sim3_geographic_survey() {
        let geo = GeoReference {
            local_origin: Some([174.76, -36.85, 50.0]),
            scale_status: Some("metric".into()),
            ..Default::default()
        };
        let frame = EnuFrame::from_geodetic(Geodetic {
            lon_deg: 174.76,
            lat_deg: -36.85,
            height_m: 50.0,
        });
        let local = [10.0, -5.0, 2.0];
        let ecef = frame.enu_to_ecef(Enu::from_array(local));
        let g = ecef_to_geodetic(ecef);
        let gcps = vec![
            GcpPoint {
                id: "a".into(),
                survey_xyz: [174.76, -36.85, 50.0],
                survey_crs: "EPSG:4326".into(),
                local_xyz: Some([0.0, 0.0, 0.0]),
                ..Default::default()
            },
            GcpPoint {
                id: "b".into(),
                survey_xyz: [g.lon_deg, g.lat_deg, g.height_m],
                survey_crs: "EPSG:4326".into(),
                local_xyz: Some(local),
                ..Default::default()
            },
            GcpPoint {
                id: "c".into(),
                survey_xyz: {
                    let e = frame.enu_to_ecef(Enu {
                        east: 0.0,
                        north: 8.0,
                        up: 0.0,
                    });
                    let gg = ecef_to_geodetic(e);
                    [gg.lon_deg, gg.lat_deg, gg.height_m]
                },
                survey_crs: "EPSG:4326".into(),
                local_xyz: Some([0.0, 8.0, 0.0]),
                ..Default::default()
            },
        ];
        let report = solve_gcp_sim3(&gcps, &geo).unwrap();
        assert!((report.scale - 1.0).abs() < 1e-3);
        assert!(report.mean_residual_m < GCP_SURVEY_PASS_MEAN_M);
    }

    #[test]
    fn residual_thresholds_are_documented_for_release_gates() {
        assert!(GCP_OUTLIER_MEDIAN_FACTOR >= 2.0);
        assert!(GCP_OUTLIER_FLOOR_M > 0.0);
        assert!(GCP_SURVEY_PASS_MEAN_M <= GCP_OUTLIER_FLOOR_M);
        assert!(GCP_IDENTITY_PASS_MEAN_M < GCP_SURVEY_PASS_MEAN_M);
    }
}
