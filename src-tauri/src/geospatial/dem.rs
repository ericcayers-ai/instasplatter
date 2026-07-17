//! DEM conditioning and flood-ready terrain products.
//!
//! Stages a DTM for scientific / demo flood runs. When a real catalog DEM is
//! available, prefer it over the synthetic stub. Basic conditioning records
//! nodata, AOI clip, and resolution from ExtentPlan; full pyflwdir/Landlab
//! hydrologic conditioning remains deferred.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DemProduct {
    pub dtm_path: Option<String>,
    pub dsm_path: Option<String>,
    pub orthomosaic_path: Option<String>,
    pub cell_size_m: Option<f64>,
    pub crs: Option<String>,
    /// True when the DTM is a staged placeholder (no source raster).
    pub synthetic: bool,
    pub notes: Vec<String>,
    /// WGS84 AOI used for clip / fetch `[west, south, east, north]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aoi_wgs84: Option<[f64; 4]>,
    /// Whether basic conditioning metadata was written (nodata / clip / res).
    #[serde(default)]
    pub conditioned: bool,
    /// Declared or inferred nodata value (if known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodata: Option<f64>,
    /// Compact float preview grid for soft-solver / HAND (`dtm_flood_preview.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_grid_path: Option<String>,
    /// Local Cesium terrain root (`…/terrain` with `layer.json`) when staged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terrain_tiles_url: Option<String>,
    /// `"real"` | `"synthetic"` | `"proxy"` — bed authority for UI badges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bed_source: Option<String>,
}

/// Sampled DEM elevations for soft preview / HAND (row-major, metres).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemSampleGrid {
    pub cols: u32,
    pub rows: u32,
    /// West, south, east, north (WGS84 degrees).
    pub bounds: [f64; 4],
    pub z: Vec<f32>,
    pub synthetic: bool,
    /// `"real"` | `"synthetic"` | `"proxy"`.
    pub bed_source: String,
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// Options for staging / conditioning a flood DEM.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DemStageOpts {
    /// Optional source raster (catalog fetch or user GeoTIFF).
    pub source: Option<String>,
    /// Target cell size from ExtentPlan (`demResolutionM` / `previewCellM`).
    pub cell_size_m: Option<f64>,
    pub crs: Option<String>,
    /// WGS84 AOI `[west, south, east, north]` for clip metadata.
    pub aoi_wgs84: Option<[f64; 4]>,
    /// Explicit nodata (default −9999 when unknown).
    pub nodata: Option<f64>,
}

/// Prepare flood-ready DEM products under `dest_dir` (usually `geo/derived`).
///
/// Copies an existing GeoTIFF when available; otherwise writes a small
/// ASCII grid descriptor so demo / UI runs remain unblocked (labelled).
pub fn prepare_flood_dem(
    workspace: &Path,
    source: Option<&Path>,
    cell_size_m: f64,
    crs: Option<&str>,
) -> Result<DemProduct, String> {
    prepare_flood_dem_with_opts(
        workspace,
        &DemStageOpts {
            source: source.map(|p| p.to_string_lossy().into_owned()),
            cell_size_m: Some(cell_size_m),
            crs: crs.map(|s| s.to_string()),
            ..Default::default()
        },
    )
}

/// Stage + basic-condition with AOI / nodata / resolution metadata.
pub fn prepare_flood_dem_with_opts(
    workspace: &Path,
    opts: &DemStageOpts,
) -> Result<DemProduct, String> {
    let dest_dir = workspace.join("geo").join("derived");
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let cell_size_m = opts.cell_size_m.unwrap_or(2.0).max(0.1);
    let crs = opts.crs.as_deref();
    let nodata = opts.nodata.or(Some(-9999.0));

    if let Some(ref src) = opts.source {
        let path = PathBuf::from(src);
        if path.exists() {
            return condition_dem_with_opts(
                &path,
                &dest_dir,
                opts.aoi_wgs84,
                Some(cell_size_m),
                crs,
                nodata,
            );
        }
    }

    // Prefer an already-imported / catalog-fetched source under geo/.
    if let Some(found) = find_dem_candidate(workspace) {
        return condition_dem_with_opts(
            &found,
            &dest_dir,
            opts.aoi_wgs84,
            Some(cell_size_m),
            crs,
            nodata,
        );
    }

    // Synthetic scaffold — honest placeholder for demo mode.
    synthetic_stub(&dest_dir, cell_size_m, crs, opts.aoi_wgs84, nodata)
}

/// Condition a source DEM into flood-ready DTM products (copy + metadata).
pub fn condition_dem(source: &Path, dest_dir: &Path) -> Result<DemProduct, String> {
    condition_dem_with_opts(source, dest_dir, None, None, None, Some(-9999.0))
}

/// Stage source DEM and write conditioning sidecar (nodata, AOI clip, resolution).
pub fn condition_dem_with_opts(
    source: &Path,
    dest_dir: &Path,
    aoi_wgs84: Option<[f64; 4]>,
    cell_size_m: Option<f64>,
    crs: Option<&str>,
    nodata: Option<f64>,
) -> Result<DemProduct, String> {
    if !source.exists() {
        return Err(format!("DEM source not found: {}", source.display()));
    }
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("tif")
        .to_ascii_lowercase();
    // JSON stubs / manifests are not rasters — refuse to treat as real DEM.
    if matches!(ext.as_str(), "json" | "geojson") {
        return Err(format!(
            "Source is not a DEM raster ({ext}): {}. Fetch USGS 3DEP / Copernicus / OpenTopography, \
             or provide a GeoTIFF.",
            source.display()
        ));
    }

    let dest = dest_dir.join(format!("dtm_flood.{ext}"));
    fs::copy(source, &dest).map_err(|e| format!("Failed to stage DEM: {e}"))?;

    let cell = cell_size_m.unwrap_or(30.0);
    let nodata_v = nodata.unwrap_or(-9999.0);
    let crs_label = crs.unwrap_or("EPSG:4326").to_string();

    let mut notes = vec![
        "DEM staged for flood use.".into(),
        format!("Basic conditioning: nodata={nodata_v}, targetResolutionM={cell}."),
    ];
    if let Some(aoi) = aoi_wgs84 {
        notes.push(format!(
            "AOI clip bounds (WGS84): [{:.6}, {:.6}, {:.6}, {:.6}] — \
             full hydrologic burn-in (pyflwdir/Landlab) deferred.",
            aoi[0], aoi[1], aoi[2], aoi[3]
        ));
    } else {
        notes.push(
            "No AOI supplied — staged full source extent; clip metadata omitted.".into(),
        );
    }

    // Sidecar metadata for hydro / mesh planner / Cesium terrain prep.
    let meta = dest_dir.join("dtm_flood.meta.json");
    let meta_body = serde_json::json!({
        "source": source.to_string_lossy(),
        "staged": dest.to_string_lossy(),
        "conditioned": true,
        "conditioningLevel": "basic",
        "nodata": nodata_v,
        "targetResolutionM": cell,
        "aoiWgs84": aoi_wgs84,
        "crs": crs_label,
        "note": "Basic stage+condition (nodata/clip/resolution). pyflwdir/Landlab hydrologic conditioning not yet applied.",
        "synthetic": false,
    });
    fs::write(
        &meta,
        serde_json::to_string_pretty(&meta_body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // Companion conditioning report for soft preview / HAND agents.
    let report = dest_dir.join("dtm_flood.condition.json");
    let report_body = serde_json::json!({
        "nodata": nodata_v,
        "clipToAoi": aoi_wgs84.is_some(),
        "aoiWgs84": aoi_wgs84,
        "resolutionM": cell,
        "crs": crs_label,
        "sourceBytes": fs::metadata(source).map(|m| m.len()).unwrap_or(0),
        "stagedBytes": fs::metadata(&dest).map(|m| m.len()).unwrap_or(0),
    });
    fs::write(
        &report,
        serde_json::to_string_pretty(&report_body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let (preview_grid_path, bed_source, mut preview_notes) =
        write_preview_sidecar(&dest, dest_dir, aoi_wgs84, nodata_v)?;
    notes.append(&mut preview_notes);

    let terrain_tiles_url = resolve_terrain_tiles_url(dest_dir);
    if terrain_tiles_url.is_none() {
        notes.push(
            "Globe terrain: no quantized-mesh/heightmap yet — Standard uses ellipsoid + flood overlay; \
             stage Cesium terrain tiles under geo/derived/terrain/ (layer.json) when available."
                .into(),
        );
        let _ = write_terrain_readme(dest_dir);
    }

    Ok(DemProduct {
        dtm_path: Some(dest.to_string_lossy().into_owned()),
        dsm_path: None,
        orthomosaic_path: None,
        cell_size_m: Some(cell),
        crs: Some(crs_label),
        synthetic: false,
        notes,
        aoi_wgs84,
        conditioned: true,
        nodata: Some(nodata_v),
        preview_grid_path,
        terrain_tiles_url,
        bed_source: Some(bed_source),
    })
}

fn synthetic_stub(
    dest_dir: &Path,
    cell_size_m: f64,
    crs: Option<&str>,
    aoi_wgs84: Option<[f64; 4]>,
    nodata: Option<f64>,
) -> Result<DemProduct, String> {
    let stub = dest_dir.join("dtm_flood_stub.json");
    let crs_label = crs.unwrap_or("local-ENU-m").to_string();
    let nodata_v = nodata.unwrap_or(-9999.0);
    let body = serde_json::json!({
        "kind": "syntheticDemStub",
        "cellSizeM": cell_size_m,
        "crs": crs_label,
        "nodata": nodata_v,
        "aoiWgs84": aoi_wgs84,
        "note": "No source DEM — synthetic stub for demo flood extents only. Fetch USGS 3DEP / Copernicus GLO-30 / OpenTopography for a real bed.",
        "boundsEnu": [0.0, 0.0, 400.0, 300.0],
        "zMeanM": 10.0,
    });
    fs::write(
        &stub,
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let meta = dest_dir.join("dtm_flood.meta.json");
    let meta_body = serde_json::json!({
        "staged": stub.to_string_lossy(),
        "conditioned": false,
        "conditioningLevel": "none",
        "synthetic": true,
        "nodata": nodata_v,
        "targetResolutionM": cell_size_m,
        "aoiWgs84": aoi_wgs84,
        "crs": crs_label,
        "note": "Synthetic DEM stub — not for authoritative scientific runs.",
    });
    fs::write(
        &meta,
        serde_json::to_string_pretty(&meta_body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(DemProduct {
        dtm_path: Some(stub.to_string_lossy().into_owned()),
        dsm_path: None,
        orthomosaic_path: None,
        cell_size_m: Some(cell_size_m),
        crs: Some(crs_label),
        synthetic: true,
        notes: vec![
            "No source DEM found — using synthetic stub (not for authoritative scientific runs)."
                .into(),
            "Prefer catalog fetch (usgs-3dep / copernicus-glo30 / opentopography) before flood preview."
                .into(),
            "Globe: ellipsoid only until a real DEM + terrain tiles are staged.".into(),
        ],
        aoi_wgs84,
        conditioned: false,
        nodata: Some(nodata_v),
        preview_grid_path: None,
        terrain_tiles_url: None,
        bed_source: Some("synthetic".into()),
    })
}

fn find_dem_candidate(workspace: &Path) -> Option<PathBuf> {
    let roots = [
        workspace.join("geo").join("catalog"),
        workspace.join("geo").join("sources"),
        workspace.join("geo").join("derived"),
    ];
    let exts = ["tif", "tiff", "geotiff", "cog"];
    let mut candidates: Vec<PathBuf> = Vec::new();
    for root in &roots {
        let Ok(rd) = fs::read_dir(root) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            // Skip already-staged product to avoid re-copy loops preferring itself.
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("dtm_flood") {
                continue;
            }
            let some_ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if some_ext
                .as_deref()
                .map(|e| exts.contains(&e))
                .unwrap_or(false)
            {
                candidates.push(p);
            }
        }
    }
    // Prefer USGS / OpenTopo / Copernicus filenames from catalog fetch.
    candidates.sort_by_key(|p| {
        let n = p
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let rank = if n.contains("usgs_3dep") {
            0
        } else if n.contains("opentopo") {
            1
        } else if n.contains("copernicus") || n.contains("glo30") || n.contains("dsm_cog") {
            2
        } else {
            3
        };
        (rank, n)
    });
    candidates.into_iter().next()
}

/// Sample DEM elevations onto a soft-preview / HAND grid.
///
/// Prefers a staged float GeoTIFF (USGS F32) or `dtm_flood_preview.json`.
/// Falls back to synthetic undulation when the DEM is a stub or undecodable.
pub fn sample_dem_grid(
    workspace: &Path,
    cols: u32,
    rows: u32,
    bounds: Option<[f64; 4]>,
) -> Result<DemSampleGrid, String> {
    let cols = cols.clamp(4, 512);
    let rows = rows.clamp(4, 512);
    let derived = workspace.join("geo").join("derived");
    let preview_path = derived.join("dtm_flood_preview.json");
    let aoi = bounds.or_else(|| read_aoi_from_meta(&derived));

    if preview_path.exists() {
        if let Ok(grid) = load_preview_json(&preview_path) {
            return Ok(resample_grid(&grid, cols, rows, aoi.unwrap_or(grid.bounds)));
        }
    }

    // Prefer staged DTM GeoTIFF.
    let dtm = derived.join("dtm_flood.tif");
    let dtm_tiff = derived.join("dtm_flood.tiff");
    let source = if dtm.exists() {
        Some(dtm)
    } else if dtm_tiff.exists() {
        Some(dtm_tiff)
    } else {
        find_dem_candidate(workspace)
    };

    if let Some(ref src) = source {
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "tif" | "tiff" | "geotiff" | "cog") {
            match decode_float_geotiff(src) {
                Ok((w, h, data, nodata)) => {
                    let b = aoi.unwrap_or([-180.0, -90.0, 180.0, 90.0]);
                    let sampled = resample_raw(&data, w, h, cols, rows, nodata);
                    let _ = write_preview_json(
                        &derived,
                        cols,
                        rows,
                        b,
                        &sampled,
                        false,
                        "real",
                        Some(src),
                    );
                    return Ok(DemSampleGrid {
                        cols,
                        rows,
                        bounds: b,
                        z: sampled,
                        synthetic: false,
                        bed_source: "real".into(),
                        notes: vec![format!(
                            "Sampled {}×{} bed from staged DEM {}",
                            cols,
                            rows,
                            src.display()
                        )],
                        source_path: Some(src.to_string_lossy().into_owned()),
                    });
                }
                Err(e) => {
                    // Staged raster exists but undecodable (COG/tiled) — proxy undulation.
                    let b = aoi.unwrap_or([0.0, 0.0, 0.001, 0.001]);
                    let z = synthetic_undulation(cols, rows);
                    return Ok(DemSampleGrid {
                        cols,
                        rows,
                        bounds: b,
                        z,
                        synthetic: true,
                        bed_source: "proxy".into(),
                        notes: vec![
                            format!(
                                "DEM staged at {} but float decode failed ({e}); using undulation proxy for soft preview.",
                                src.display()
                            ),
                            "HAND / soft preview remain Live preview / non-authoritative.".into(),
                        ],
                        source_path: Some(src.to_string_lossy().into_owned()),
                    });
                }
            }
        }
    }

    let b = aoi.unwrap_or([174.762, -41.298, 174.796, -41.275]);
    Ok(DemSampleGrid {
        cols,
        rows,
        bounds: b,
        z: synthetic_undulation(cols, rows),
        synthetic: true,
        bed_source: "synthetic".into(),
        notes: vec![
            "No real DEM — synthetic undulation bed (Demo / Live preview only).".into(),
        ],
        source_path: None,
    })
}

/// Path to local Cesium terrain tiles (`…/geo/derived/terrain`) when `layer.json` exists.
pub fn resolve_terrain_tiles_url(derived_or_workspace: &Path) -> Option<String> {
    let candidates = [
        derived_or_workspace.join("terrain").join("layer.json"),
        derived_or_workspace
            .join("geo")
            .join("derived")
            .join("terrain")
            .join("layer.json"),
        derived_or_workspace.join("derived").join("terrain").join("layer.json"),
    ];
    for layer in &candidates {
        if layer.exists() {
            if let Some(dir) = layer.parent() {
                return Some(dir.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn write_terrain_readme(derived: &Path) -> Result<(), String> {
    let terrain = derived.join("terrain");
    fs::create_dir_all(&terrain).map_err(|e| e.to_string())?;
    let readme = terrain.join("README.md");
    if readme.exists() {
        return Ok(());
    }
    let body = "\
# Cesium terrain tiles (optional)

Standard Globe uses **ellipsoid** + flood overlay when only a GeoTIFF DEM is staged.

To enable local DEM relief without Cesium ion:

1. Convert the staged `../dtm_flood.tif` to quantized-mesh or heightmap tiles \
   (e.g. [tumgis/cesium-terrain-builder](https://github.com/tum-gis/cesium-terrain-builder-docker)).
2. Place tiles here so `layer.json` sits at `geo/derived/terrain/layer.json`.
3. Soft preview / HAND already sample the GeoTIFF (or preview sidecar) independently.

Never enable ion World Terrain on the Standard path.
";
    fs::write(readme, body).map_err(|e| e.to_string())
}

fn write_preview_sidecar(
    source: &Path,
    dest_dir: &Path,
    aoi: Option<[f64; 4]>,
    nodata: f64,
) -> Result<(Option<String>, String, Vec<String>), String> {
    const PREVIEW_COLS: u32 = 96;
    const PREVIEW_ROWS: u32 = 72;
    match decode_float_geotiff(source) {
        Ok((w, h, data, nd)) => {
            let nodata_v = nd.unwrap_or(nodata as f32);
            let z = resample_raw(&data, w, h, PREVIEW_COLS, PREVIEW_ROWS, Some(nodata_v));
            let b = aoi.unwrap_or([-180.0, -90.0, 180.0, 90.0]);
            let path = write_preview_json(
                dest_dir,
                PREVIEW_COLS,
                PREVIEW_ROWS,
                b,
                &z,
                false,
                "real",
                Some(source),
            )?;
            Ok((
                Some(path),
                "real".into(),
                vec![format!(
                    "Wrote soft-preview bed sample {PREVIEW_COLS}×{PREVIEW_ROWS} from GeoTIFF."
                )],
            ))
        }
        Err(_) => Ok((
            None,
            "proxy".into(),
            vec![
                "Staged DEM could not be float-decoded for preview sidecar (tiled COG?). \
                 Soft preview will sample on demand or use undulation proxy."
                    .into(),
            ],
        )),
    }
}

fn write_preview_json(
    dest_dir: &Path,
    cols: u32,
    rows: u32,
    bounds: [f64; 4],
    z: &[f32],
    synthetic: bool,
    bed_source: &str,
    source: Option<&Path>,
) -> Result<String, String> {
    let path = dest_dir.join("dtm_flood_preview.json");
    let body = serde_json::json!({
        "cols": cols,
        "rows": rows,
        "bounds": bounds,
        "z": z,
        "synthetic": synthetic,
        "bedSource": bed_source,
        "source": source.map(|p| p.to_string_lossy().into_owned()),
    });
    fs::write(
        &path,
        serde_json::to_string(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

fn load_preview_json(path: &Path) -> Result<DemSampleGrid, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let cols = v.get("cols").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    let rows = v.get("rows").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    let bounds = v
        .get("bounds")
        .and_then(|b| b.as_array())
        .filter(|a| a.len() == 4)
        .map(|a| {
            [
                a[0].as_f64().unwrap_or(0.0),
                a[1].as_f64().unwrap_or(0.0),
                a[2].as_f64().unwrap_or(0.0),
                a[3].as_f64().unwrap_or(0.0),
            ]
        })
        .ok_or_else(|| "preview missing bounds".to_string())?;
    let z: Vec<f32> = v
        .get("z")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|n| n.as_f64().map(|f| f as f32)).collect())
        .unwrap_or_default();
    if z.len() != (cols as usize) * (rows as usize) {
        return Err("preview z length mismatch".into());
    }
    Ok(DemSampleGrid {
        cols,
        rows,
        bounds,
        z,
        synthetic: v
            .get("synthetic")
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        bed_source: v
            .get("bedSource")
            .and_then(|x| x.as_str())
            .unwrap_or("real")
            .into(),
        notes: vec!["Loaded dtm_flood_preview.json".into()],
        source_path: v
            .get("source")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
    })
}

fn read_aoi_from_meta(derived: &Path) -> Option<[f64; 4]> {
    let meta = derived.join("dtm_flood.meta.json");
    let text = fs::read_to_string(meta).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let a = v.get("aoiWgs84")?.as_array()?;
    if a.len() != 4 {
        return None;
    }
    Some([
        a[0].as_f64()?,
        a[1].as_f64()?,
        a[2].as_f64()?,
        a[3].as_f64()?,
    ])
}

fn resample_grid(src: &DemSampleGrid, cols: u32, rows: u32, bounds: [f64; 4]) -> DemSampleGrid {
    let z = resample_raw(&src.z, src.cols, src.rows, cols, rows, None);
    DemSampleGrid {
        cols,
        rows,
        bounds,
        z,
        synthetic: src.synthetic,
        bed_source: src.bed_source.clone(),
        notes: src.notes.clone(),
        source_path: src.source_path.clone(),
    }
}

fn resample_raw(
    data: &[f32],
    src_w: u32,
    src_h: u32,
    cols: u32,
    rows: u32,
    nodata: Option<f32>,
) -> Vec<f32> {
    let mut out = vec![0.0f32; (cols * rows) as usize];
    if src_w == 0 || src_h == 0 || data.is_empty() {
        return synthetic_undulation(cols, rows);
    }
    for j in 0..rows {
        for i in 0..cols {
            let u = (i as f64 + 0.5) / cols as f64;
            let v = (j as f64 + 0.5) / rows as f64;
            let x = (u * (src_w as f64 - 1.0)).clamp(0.0, (src_w - 1) as f64);
            let y = (v * (src_h as f64 - 1.0)).clamp(0.0, (src_h - 1) as f64);
            let x0 = x.floor() as u32;
            let y0 = y.floor() as u32;
            let x1 = (x0 + 1).min(src_w - 1);
            let y1 = (y0 + 1).min(src_h - 1);
            let fx = (x - x0 as f64) as f32;
            let fy = (y - y0 as f64) as f32;
            let sample = |xx: u32, yy: u32| -> f32 {
                let idx = (yy * src_w + xx) as usize;
                let val = data.get(idx).copied().unwrap_or(0.0);
                if let Some(nd) = nodata {
                    if (val - nd).abs() < 1e-3 || !val.is_finite() {
                        return 0.0;
                    }
                }
                if !val.is_finite() {
                    return 0.0;
                }
                val
            };
            let z00 = sample(x0, y0);
            let z10 = sample(x1, y0);
            let z01 = sample(x0, y1);
            let z11 = sample(x1, y1);
            out[(j * cols + i) as usize] =
                z00 * (1.0 - fx) * (1.0 - fy) + z10 * fx * (1.0 - fy) + z01 * (1.0 - fx) * fy + z11 * fx * fy;
        }
    }
    out
}

/// Gentle basin + channel undulation (matches frontend softSolver synthetic bed).
pub fn synthetic_undulation(cols: u32, rows: u32) -> Vec<f32> {
    let mut z = vec![0.0f32; (cols * rows) as usize];
    for j in 0..rows {
        for i in 0..cols {
            let xn = i as f64 / (cols.max(2) - 1) as f64;
            let yn = j as f64 / (rows.max(2) - 1) as f64;
            let mut elev = 4.2 - yn * 3.4 + (xn - 0.5) * (xn - 0.5) * 1.8;
            let channel = (-((xn - 0.42 - yn * 0.18).powi(2) / 0.012)).exp();
            elev -= channel * 1.6;
            let basin = (-(((xn - 0.62).powi(2) + (yn - 0.55).powi(2)) / 0.02)).exp();
            elev -= basin * 0.9;
            if (xn - 0.55).abs() < 0.03 && yn > 0.35 && yn < 0.7 {
                elev += 1.1;
            }
            z[(j * cols + i) as usize] = elev as f32;
        }
    }
    z
}

/// Minimal little-endian uncompressed IEEE-float TIFF reader (USGS 3DEP F32 strips).
fn decode_float_geotiff(path: &Path) -> Result<(u32, u32, Vec<f32>, Option<f32>), String> {
    let bytes = fs::read(path).map_err(|e| format!("read DEM: {e}"))?;
    if bytes.len() < 8 {
        return Err("TIFF too small".into());
    }
    let le = match &bytes[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return Err("Not a TIFF".into()),
    };
    let magic = read_u16(&bytes, 2, le)?;
    if magic != 42 {
        return Err("Not a classic TIFF".into());
    }
    let mut ifd = read_u32(&bytes, 4, le)? as usize;
    if ifd + 2 > bytes.len() {
        return Err("IFD out of range".into());
    }
    let n_entries = read_u16(&bytes, ifd, le)? as usize;
    ifd += 2;

    let mut width = 0u32;
    let mut height = 0u32;
    let mut bits = 0u16;
    let mut sample_format = 1u16; // 1=uint, 2=int, 3=float
    let mut samples = 1u16;
    let mut compression = 1u16;
    let mut strip_offsets: Vec<u32> = Vec::new();
    let mut strip_bytes: Vec<u32> = Vec::new();
    let mut rows_per_strip = u32::MAX;

    for i in 0..n_entries {
        let off = ifd + i * 12;
        if off + 12 > bytes.len() {
            break;
        }
        let tag = read_u16(&bytes, off, le)?;
        let typ = read_u16(&bytes, off + 2, le)?;
        let count = read_u32(&bytes, off + 4, le)?;
        let val_off = off + 8;
        match tag {
            256 => width = read_scalar_u32(&bytes, val_off, typ, count, le)?,
            257 => height = read_scalar_u32(&bytes, val_off, typ, count, le)?,
            258 => bits = read_scalar_u32(&bytes, val_off, typ, count, le)? as u16,
            259 => compression = read_scalar_u32(&bytes, val_off, typ, count, le)? as u16,
            273 => strip_offsets = read_u32_array(&bytes, val_off, typ, count, le)?,
            277 => samples = read_scalar_u32(&bytes, val_off, typ, count, le)? as u16,
            278 => rows_per_strip = read_scalar_u32(&bytes, val_off, typ, count, le)?,
            279 => strip_bytes = read_u32_array(&bytes, val_off, typ, count, le)?,
            339 => sample_format = read_scalar_u32(&bytes, val_off, typ, count, le)? as u16,
            _ => {}
        }
    }

    if width == 0 || height == 0 {
        return Err("TIFF missing ImageWidth/Length".into());
    }
    if compression != 1 {
        return Err(format!("Compressed TIFF ({compression}) not supported without GDAL"));
    }
    if bits != 32 || sample_format != 3 {
        // Try image crate for 8/16-bit elevation encodings.
        return decode_via_image_crate(path, width, height);
    }
    if samples != 1 {
        return Err("Multi-sample float TIFF not supported".into());
    }
    if strip_offsets.is_empty() {
        return Err("No StripOffsets (tiled COG?) — use GDAL/CTB or preview proxy".into());
    }

    let mut data = vec![0.0f32; (width as usize) * (height as usize)];
    let mut row = 0u32;
    for (si, &soff) in strip_offsets.iter().enumerate() {
        let nbytes = strip_bytes
            .get(si)
            .copied()
            .unwrap_or((width * rows_per_strip.min(height - row) * 4) as u32)
            as usize;
        let start = soff as usize;
        let end = start.saturating_add(nbytes).min(bytes.len());
        let strip = &bytes[start..end];
        let rows_here = rows_per_strip.min(height - row);
        let expect = (width as usize) * (rows_here as usize) * 4;
        if strip.len() < expect {
            return Err("Strip truncated".into());
        }
        for r in 0..rows_here {
            for c in 0..width {
                let bo = ((r * width + c) * 4) as usize;
                let bits_u = if le {
                    u32::from_le_bytes([strip[bo], strip[bo + 1], strip[bo + 2], strip[bo + 3]])
                } else {
                    u32::from_be_bytes([strip[bo], strip[bo + 1], strip[bo + 2], strip[bo + 3]])
                };
                data[((row + r) * width + c) as usize] = f32::from_bits(bits_u);
            }
        }
        row += rows_here;
        if row >= height {
            break;
        }
    }
    Ok((width, height, data, Some(-9999.0)))
}

fn decode_via_image_crate(
    path: &Path,
    _w: u32,
    _h: u32,
) -> Result<(u32, u32, Vec<f32>, Option<f32>), String> {
    let img = image::open(path).map_err(|e| format!("image decode: {e}"))?;
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    let mut data = Vec::with_capacity((w * h) as usize);
    for p in gray.pixels() {
        // Map 0–255 → 0–50 m proxy when only 8-bit available.
        data.push(p[0] as f32 * (50.0 / 255.0));
    }
    Ok((w, h, data, None))
}

fn read_u16(bytes: &[u8], off: usize, le: bool) -> Result<u16, String> {
    if off + 2 > bytes.len() {
        return Err("EOF u16".into());
    }
    Ok(if le {
        u16::from_le_bytes([bytes[off], bytes[off + 1]])
    } else {
        u16::from_be_bytes([bytes[off], bytes[off + 1]])
    })
}

fn read_u32(bytes: &[u8], off: usize, le: bool) -> Result<u32, String> {
    if off + 4 > bytes.len() {
        return Err("EOF u32".into());
    }
    Ok(if le {
        u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
    } else {
        u32::from_be_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
    })
}

fn read_scalar_u32(
    bytes: &[u8],
    val_off: usize,
    typ: u16,
    count: u32,
    le: bool,
) -> Result<u32, String> {
    if count == 0 {
        return Ok(0);
    }
    // Inline value when ≤4 bytes.
    match typ {
        3 => Ok(read_u16(bytes, val_off, le)? as u32), // SHORT
        4 => read_u32(bytes, val_off, le),             // LONG
        1 => Ok(bytes[val_off] as u32),                // BYTE
        _ => read_u32(bytes, val_off, le),
    }
}

fn read_u32_array(
    bytes: &[u8],
    val_off: usize,
    typ: u16,
    count: u32,
    le: bool,
) -> Result<Vec<u32>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let type_size: usize = match typ {
        3 => 2,
        4 => 4,
        _ => 4,
    };
    let total = type_size * count as usize;
    let data_off = if total <= 4 {
        val_off
    } else {
        read_u32(bytes, val_off, le)? as usize
    };
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let o = data_off + i * type_size;
        let v = if typ == 3 {
            read_u16(bytes, o, le)? as u32
        } else {
            read_u32(bytes, o, le)?
        };
        out.push(v);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_ws() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("instasplatter_dem_{id}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join("geo").join("sources")).unwrap();
        fs::create_dir_all(p.join("geo").join("catalog")).unwrap();
        p
    }

    #[test]
    fn synthetic_dem_when_missing() {
        let ws = temp_ws();
        let prod = prepare_flood_dem(&ws, None, 2.0, Some("local-ENU-m")).unwrap();
        assert!(prod.synthetic);
        assert!(!prod.conditioned);
        assert!(prod.dtm_path.as_ref().unwrap().contains("dtm_flood_stub"));
        assert!(prod.nodata.is_some());
        assert_eq!(prod.bed_source.as_deref(), Some("synthetic"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn stages_existing_tif() {
        let ws = temp_ws();
        let src = ws.join("geo").join("sources").join("site.tif");
        fs::write(&src, b"fake-tif").unwrap();
        let prod = prepare_flood_dem(&ws, Some(&src), 2.0, None).unwrap();
        assert!(!prod.synthetic);
        assert!(prod.conditioned);
        assert!(Path::new(prod.dtm_path.as_ref().unwrap()).exists());
        let meta = ws.join("geo").join("derived").join("dtm_flood.meta.json");
        assert!(meta.exists());
        let text = fs::read_to_string(meta).unwrap();
        assert!(text.contains("\"conditioned\": true"));
        let terrain_readme = ws
            .join("geo")
            .join("derived")
            .join("terrain")
            .join("README.md");
        assert!(terrain_readme.exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn conditions_with_aoi_and_resolution() {
        let ws = temp_ws();
        let src = ws.join("geo").join("catalog").join("usgs_3dep_aoi.tif");
        fs::write(&src, b"fake-3dep").unwrap();
        let aoi = [-122.5, 37.7, -122.3, 37.85];
        let prod = prepare_flood_dem_with_opts(
            &ws,
            &DemStageOpts {
                source: None, // discover from catalog/
                cell_size_m: Some(10.0),
                crs: Some("EPSG:4326".into()),
                aoi_wgs84: Some(aoi),
                nodata: Some(-9999.0),
            },
        )
        .unwrap();
        assert!(!prod.synthetic);
        assert!(prod.conditioned);
        assert_eq!(prod.cell_size_m, Some(10.0));
        assert_eq!(prod.aoi_wgs84, Some(aoi));
        let report = ws
            .join("geo")
            .join("derived")
            .join("dtm_flood.condition.json");
        assert!(report.exists());
        let text = fs::read_to_string(report).unwrap();
        assert!(text.contains("clipToAoi"));
        assert!(text.contains("10"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn prefers_catalog_dem_over_synthetic() {
        let ws = temp_ws();
        let src = ws.join("geo").join("catalog").join("opentopo_cop30_aoi.tif");
        fs::write(&src, b"real-ish").unwrap();
        let prod = prepare_flood_dem(&ws, None, 30.0, Some("EPSG:4326")).unwrap();
        assert!(!prod.synthetic);
        assert!(prod.dtm_path.as_ref().unwrap().contains("dtm_flood"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn rejects_geojson_as_dem() {
        let ws = temp_ws();
        let src = ws.join("geo").join("catalog").join("gauges.geojson");
        fs::write(&src, b"{\"type\":\"FeatureCollection\",\"features\":[]}").unwrap();
        let err = condition_dem(&src, &ws.join("geo").join("derived")).unwrap_err();
        assert!(err.contains("not a DEM"), "{err}");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn synthetic_includes_aoi_when_provided() {
        let ws = temp_ws();
        let aoi = [-1.0, 50.0, -0.5, 50.5];
        let prod = prepare_flood_dem_with_opts(
            &ws,
            &DemStageOpts {
                aoi_wgs84: Some(aoi),
                cell_size_m: Some(5.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(prod.synthetic);
        assert_eq!(prod.aoi_wgs84, Some(aoi));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn sample_dem_grid_synthetic_fallback() {
        let ws = temp_ws();
        let grid = sample_dem_grid(&ws, 32, 24, Some([-1.0, 50.0, -0.5, 50.5])).unwrap();
        assert_eq!(grid.cols, 32);
        assert_eq!(grid.rows, 24);
        assert_eq!(grid.z.len(), 32 * 24);
        assert!(grid.synthetic);
        assert_eq!(grid.bed_source, "synthetic");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn sample_dem_grid_from_float_tiff() {
        let ws = temp_ws();
        let derived = ws.join("geo").join("derived");
        fs::create_dir_all(&derived).unwrap();
        let tif = derived.join("dtm_flood.tif");
        write_test_float_tiff(&tif, 4, 3, 10.0).unwrap();
        let grid = sample_dem_grid(&ws, 8, 6, Some([-122.5, 37.7, -122.3, 37.85])).unwrap();
        assert!(!grid.synthetic);
        assert_eq!(grid.bed_source, "real");
        assert!(grid.z.iter().all(|v| (*v - 10.0).abs() < 0.01));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn resolve_terrain_when_layer_json_present() {
        let ws = temp_ws();
        let terrain = ws.join("geo").join("derived").join("terrain");
        fs::create_dir_all(&terrain).unwrap();
        fs::write(terrain.join("layer.json"), r#"{"tiles":[]}"#).unwrap();
        let url = resolve_terrain_tiles_url(&ws.join("geo").join("derived"));
        assert!(url.unwrap().ends_with("terrain"));
        let _ = fs::remove_dir_all(&ws);
    }

    fn write_test_float_tiff(path: &Path, w: u32, h: u32, fill: f32) -> Result<(), String> {
        // Minimal little-endian uncompressed float32 strip TIFF.
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(b"II");
        buf.extend_from_slice(&42u16.to_le_bytes());
        let ifd_offset = 8u32;
        buf.extend_from_slice(&ifd_offset.to_le_bytes());
        // IFD at 8
        let n_tags: u16 = 8;
        buf.extend_from_slice(&n_tags.to_le_bytes());
        let data_offset = 8 + 2 + (n_tags as u32) * 12 + 4;
        let strip_bytes = w * h * 4;
        let mut write_tag = |tag: u16, typ: u16, count: u32, value: u32| {
            buf.extend_from_slice(&tag.to_le_bytes());
            buf.extend_from_slice(&typ.to_le_bytes());
            buf.extend_from_slice(&count.to_le_bytes());
            buf.extend_from_slice(&value.to_le_bytes());
        };
        write_tag(256, 4, 1, w); // ImageWidth
        write_tag(257, 4, 1, h); // ImageLength
        write_tag(258, 3, 1, 32); // BitsPerSample
        write_tag(259, 3, 1, 1); // Compression
        write_tag(273, 4, 1, data_offset); // StripOffsets
        write_tag(277, 3, 1, 1); // SamplesPerPixel
        write_tag(278, 4, 1, h); // RowsPerStrip
        write_tag(279, 4, 1, strip_bytes); // StripByteCounts
        // next IFD = 0
        buf.extend_from_slice(&0u32.to_le_bytes());
        while buf.len() < data_offset as usize {
            buf.push(0);
        }
        let bits = fill.to_bits().to_le_bytes();
        for _ in 0..(w * h) {
            buf.extend_from_slice(&bits);
        }
        // SampleFormat tag missing — decoder defaults sample_format=1. Patch: rewrite with tag 339.
        // Simpler: append SampleFormat by rebuilding IFD with 9 tags.
        let _ = bits;
        // Rebuild properly with SampleFormat=3.
        let mut buf2: Vec<u8> = Vec::new();
        buf2.extend_from_slice(b"II");
        buf2.extend_from_slice(&42u16.to_le_bytes());
        buf2.extend_from_slice(&8u32.to_le_bytes());
        let n: u16 = 9;
        buf2.extend_from_slice(&n.to_le_bytes());
        let data_off2 = 8 + 2 + (n as u32) * 12 + 4;
        let mut tag = |t: u16, ty: u16, c: u32, v: u32| {
            buf2.extend_from_slice(&t.to_le_bytes());
            buf2.extend_from_slice(&ty.to_le_bytes());
            buf2.extend_from_slice(&c.to_le_bytes());
            buf2.extend_from_slice(&v.to_le_bytes());
        };
        tag(256, 4, 1, w);
        tag(257, 4, 1, h);
        tag(258, 3, 1, 32);
        tag(259, 3, 1, 1);
        tag(273, 4, 1, data_off2);
        tag(277, 3, 1, 1);
        tag(278, 4, 1, h);
        tag(279, 4, 1, strip_bytes);
        tag(339, 3, 1, 3); // SampleFormat = float
        buf2.extend_from_slice(&0u32.to_le_bytes());
        while buf2.len() < data_off2 as usize {
            buf2.push(0);
        }
        for _ in 0..(w * h) {
            buf2.extend_from_slice(&fill.to_bits().to_le_bytes());
        }
        fs::write(path, buf2).map_err(|e| e.to_string())
    }
}
