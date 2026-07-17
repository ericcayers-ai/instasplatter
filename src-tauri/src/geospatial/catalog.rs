//! Managed data connectors and local catalog.
//!
//! Real fetch where public APIs allow without secrets; graceful errors when
//! key/network/AOI are missing. DEM connectors feed `dem` stage/condition.

use crate::geospatial::data::{GeoBounds, GeoFormat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const HTTP_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_DEM_PIXELS: u32 = 2048;
const USGS_3DEP_EXPORT: &str =
    "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage";
const OPENTOPO_GLOBALDEM: &str = "https://portal.opentopography.org/API/globaldem";
const NWIS_IV: &str = "https://waterservices.usgs.gov/nwis/iv/";
const NWIS_SITE: &str = "https://waterservices.usgs.gov/nwis/site/";
const FEMA_NFHL_QUERY: &str =
    "https://hazards.fema.gov/gis/nfhl/rest/services/public/NFHL/MapServer/28/query";
const EARTH_SEARCH: &str = "https://earth-search.aws.element84.com/v1/search";
const COPERNICUS_S3: &str = "https://copernicus-dem-30m.s3.amazonaws.com";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub format: GeoFormat,
    pub license: Option<String>,
    pub bounds: Option<GeoBounds>,
    pub resolution_m: Option<f64>,
    pub url: Option<String>,
    pub stale: bool,
    /// Local path after a successful fetch (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    /// Connector id (`usgs-3dep`, `fema-nfhl`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<String>,
    /// Attribution / citation string for UI chrome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// Short status note (e.g. needs API key, staged manifest only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Built-in connector names shown in the UI.
pub fn connector_names() -> Vec<&'static str> {
    CONNECTORS.iter().map(|c| c.title).collect()
}

#[derive(Debug, Clone, Copy)]
struct ConnectorDef {
    id: &'static str,
    title: &'static str,
    provider: &'static str,
    format: GeoFormat,
    license: &'static str,
    attribution: &'static str,
    kind: ConnectorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectorKind {
    Dem,
    HydroBasins,
    Gauges,
    FloodZones,
    Stac,
}

const CONNECTORS: &[ConnectorDef] = &[
    ConnectorDef {
        id: "usgs-3dep",
        title: "USGS 3DEP",
        provider: "USGS",
        format: GeoFormat::GeoTiff,
        license: "Public domain / USGS",
        attribution: "USGS 3DEP Elevation",
        kind: ConnectorKind::Dem,
    },
    ConnectorDef {
        id: "copernicus-glo30",
        title: "Copernicus DEM",
        provider: "Copernicus / AWS Open Data",
        format: GeoFormat::Cog,
        license: "Copernicus DEM licence — cite Copernicus",
        attribution: "Copernicus DEM GLO-30",
        kind: ConnectorKind::Dem,
    },
    ConnectorDef {
        id: "opentopography",
        title: "OpenTopography",
        provider: "OpenTopography",
        format: GeoFormat::GeoTiff,
        license: "OpenTopography API ToS (key optional)",
        attribution: "OpenTopography GlobalDEM",
        kind: ConnectorKind::Dem,
    },
    ConnectorDef {
        id: "hydrosheds",
        title: "HydroSHEDS",
        provider: "HydroSHEDS",
        format: GeoFormat::GeoJson,
        license: "Cite HydroSHEDS / HydroBASINS",
        attribution: "HydroSHEDS / HydroBASINS",
        kind: ConnectorKind::HydroBasins,
    },
    ConnectorDef {
        id: "usgs-nwis-gauges",
        title: "USGS gauges",
        provider: "USGS NWIS",
        format: GeoFormat::GeoJson,
        license: "Public domain / USGS",
        attribution: "USGS National Water Information System",
        kind: ConnectorKind::Gauges,
    },
    ConnectorDef {
        id: "fema-nfhl",
        title: "FEMA NFHL",
        provider: "FEMA",
        format: GeoFormat::GeoJson,
        license: "US public",
        attribution: "FEMA National Flood Hazard Layer",
        kind: ConnectorKind::FloodZones,
    },
    ConnectorDef {
        id: "osm-waterways",
        title: "OSM waterways",
        provider: "OpenStreetMap",
        format: GeoFormat::GeoJson,
        license: "ODbL — © OpenStreetMap contributors",
        attribution: "© OpenStreetMap contributors (ODbL)",
        kind: ConnectorKind::HydroBasins,
    },
    ConnectorDef {
        id: "earth-search-stac",
        title: "Earth Search STAC",
        provider: "Element84 Earth Search",
        format: GeoFormat::GeoJson,
        license: "STAC / open (per-collection)",
        attribution: "Earth Search STAC (Element84)",
        kind: ConnectorKind::Stac,
    },
];

/// Options for listing / fetching catalog assets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CatalogFetchOpts {
    /// WGS84 AOI `[west, south, east, north]`.
    pub aoi_wgs84: Option<[f64; 4]>,
    /// Target DEM cell size (m) from ExtentPlan when fetching rasters.
    pub cell_size_m: Option<f64>,
    /// Optional user GeoTIFF / GeoJSON path (Copernicus / HydroSHEDS fallback).
    pub user_file: Option<String>,
    /// OpenTopography API key (else `OPENTOPOGRAPHY_API_KEY` env).
    pub api_key: Option<String>,
}

fn connector_by_id(id: &str) -> Option<&'static ConnectorDef> {
    CONNECTORS.iter().find(|c| c.id == id)
}

fn entry_from_def(def: &ConnectorDef, bounds: Option<GeoBounds>) -> CatalogEntry {
    CatalogEntry {
        id: def.id.to_string(),
        title: def.title.to_string(),
        provider: def.provider.to_string(),
        format: def.format,
        license: Some(def.license.to_string()),
        bounds,
        resolution_m: match def.kind {
            ConnectorKind::Dem if def.id == "usgs-3dep" => Some(10.0),
            ConnectorKind::Dem if def.id == "copernicus-glo30" => Some(30.0),
            ConnectorKind::Dem => Some(30.0),
            _ => None,
        },
        url: None,
        stale: false,
        local_path: None,
        connector_id: Some(def.id.to_string()),
        attribution: Some(def.attribution.to_string()),
        notes: None,
    }
}

/// List built-in connectors as catalog entries (optionally scoped to an AOI).
pub fn list_entries(aoi_wgs84: Option<[f64; 4]>) -> Vec<CatalogEntry> {
    let bounds = aoi_wgs84.map(GeoBounds::from_array);
    CONNECTORS
        .iter()
        .map(|c| {
            let mut e = entry_from_def(c, bounds);
            if c.id == "usgs-3dep" {
                if let Some(b) = bounds {
                    if !aoi_intersects_conus(b) {
                        e.notes = Some(
                            "USGS 3DEP is optimized for CONUS/US territories; coverage may be empty outside the US."
                                .into(),
                        );
                    }
                } else {
                    e.notes = Some("Provide an AOI (WGS84) to fetch a clipped DEM.".into());
                }
            }
            if c.id == "opentopography" && opentopo_api_key(None).is_none() {
                e.notes = Some(
                    "Set OPENTOPOGRAPHY_API_KEY (or pass apiKey) for GlobalDEM downloads."
                        .into(),
                );
            }
            e
        })
        .collect()
}

/// List cached catalog entries under a workspace directory (`geo/catalog` or `geo/sources`).
pub fn list_cached(workspace_geo_sources: &str) -> Vec<CatalogEntry> {
    let root = PathBuf::from(workspace_geo_sources);
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(&root) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with(".catalog.json") {
            if let Ok(text) = fs::read_to_string(&p) {
                if let Ok(e) = serde_json::from_str::<CatalogEntry>(&text) {
                    out.push(e);
                    continue;
                }
            }
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let fmt = GeoFormat::from_extension(&ext);
        if matches!(
            fmt,
            GeoFormat::GeoTiff | GeoFormat::Cog | GeoFormat::GeoJson | GeoFormat::Unknown
        ) && matches!(ext.as_str(), "tif" | "tiff" | "cog" | "geojson" | "json")
            && !name.ends_with(".meta.json")
            && !name.ends_with(".condition.json")
            && !name.ends_with(".catalog.json")
        {
            out.push(CatalogEntry {
                id: format!("local-{}", short_hash(p.to_string_lossy().as_bytes())),
                title: name.to_string(),
                provider: "local".into(),
                format: fmt,
                license: None,
                bounds: None,
                resolution_m: None,
                url: None,
                stale: false,
                local_path: Some(p.to_string_lossy().into_owned()),
                connector_id: Some("local".into()),
                attribution: None,
                notes: Some("Cached local asset".into()),
            });
        }
    }
    out
}

/// Fetch a remote asset into `dest_dir` (typically `workspace/geo/catalog`).
pub fn fetch_asset(entry_id: &str, dest_dir: &str) -> Result<CatalogEntry, String> {
    fetch_asset_with_opts(
        entry_id,
        dest_dir,
        &CatalogFetchOpts::default(),
    )
}

/// Fetch with AOI / resolution / optional key or user file.
pub fn fetch_asset_with_opts(
    entry_id: &str,
    dest_dir: &str,
    opts: &CatalogFetchOpts,
) -> Result<CatalogEntry, String> {
    let dest = PathBuf::from(dest_dir);
    fs::create_dir_all(&dest).map_err(|e| format!("Cannot create catalog dir: {e}"))?;

    let id = entry_id.trim();
    // Allow both bare connector ids and aliases from older UI strings.
    let resolved = resolve_entry_id(id);
    let def = connector_by_id(&resolved).ok_or_else(|| {
        format!(
            "Unknown catalog entry `{entry_id}`. Known: {}",
            CONNECTORS
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    let aoi = opts.aoi_wgs84.map(GeoBounds::from_array);
    let mut entry = entry_from_def(def, aoi);

    let result = match def.id {
        "usgs-3dep" => fetch_usgs_3dep(&dest, aoi, opts.cell_size_m),
        "copernicus-glo30" => fetch_copernicus(&dest, aoi, opts.user_file.as_deref()),
        "opentopography" => fetch_opentopography(
            &dest,
            aoi,
            opts.cell_size_m,
            opts.api_key.as_deref(),
        ),
        "hydrosheds" => fetch_hydrosheds(&dest, aoi, opts.user_file.as_deref()),
        "usgs-nwis-gauges" => fetch_nwis_gauges(&dest, aoi),
        "fema-nfhl" => fetch_fema_nfhl(&dest, aoi),
        "osm-waterways" => fetch_osm_waterways(&dest, aoi),
        "earth-search-stac" => fetch_earth_search(&dest, aoi),
        other => Err(format!("Connector `{other}` is not implemented.")),
    };

    match result {
        Ok((path, url, notes)) => {
            entry.local_path = Some(path.to_string_lossy().into_owned());
            entry.url = url;
            entry.notes = notes;
            entry.stale = false;
            write_entry_sidecar(&dest, &entry)?;
            Ok(entry)
        }
        Err(e) => Err(e),
    }
}

fn resolve_entry_id(id: &str) -> String {
    let lower = id.to_ascii_lowercase().replace(' ', "-");
    match lower.as_str() {
        "usgs-3dep" | "usgs3dep" | "3dep" => "usgs-3dep".into(),
        "copernicus-dem" | "copernicus-glo30" | "copernicus" | "glo-30" | "glo30" => {
            "copernicus-glo30".into()
        }
        "opentopography" | "opentopo" => "opentopography".into(),
        "hydrosheds" | "hydrobasins" => "hydrosheds".into(),
        "usgs-gauges" | "usgs-nwis-gauges" | "nwis" | "gauges" => "usgs-nwis-gauges".into(),
        "noaa-mrms-/-nwm" | "noaa-mrms" => "usgs-nwis-gauges".into(), // gauges path for hydrograph forcing
        "fema-nfhl" | "nfhl" | "fema" => "fema-nfhl".into(),
        "osm-waterways" | "osm" | "waterways" | "overture-/-osm" | "overture" => {
            "osm-waterways".into()
        }
        "earth-search-stac" | "earth-search" | "stac" => "earth-search-stac".into(),
        "ecmwf-open-data" => "earth-search-stac".into(),
        other => other.to_string(),
    }
}

fn write_entry_sidecar(dest: &Path, entry: &CatalogEntry) -> Result<(), String> {
    let path = dest.join(format!("{}.catalog.json", entry.id));
    let text = serde_json::to_string_pretty(entry).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(concat!("InstaSplatter/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))
}

fn http_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let client = http_client()?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("Network error fetching catalog asset: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        let snippet: String = body.chars().take(240).collect();
        return Err(format!(
            "Catalog fetch failed (HTTP {status}) for {url}: {snippet}"
        ));
    }
    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("Failed reading response body: {e}"))
}

fn http_get_json(url: &str) -> Result<serde_json::Value, String> {
    let bytes = http_get_bytes(url)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid JSON from {url}: {e}"))
}

fn http_post_json(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let client = http_client()?;
    let resp = client
        .post(url)
        .json(body)
        .send()
        .map_err(|e| format!("Network error (POST {url}): {e}"))?;
    let status = resp.status();
    let text = resp.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        let snippet: String = text.chars().take(240).collect();
        return Err(format!("STAC search failed (HTTP {status}): {snippet}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("Invalid STAC JSON: {e}"))
}

fn require_aoi(aoi: Option<GeoBounds>) -> Result<GeoBounds, String> {
    aoi.ok_or_else(|| {
        "An AOI is required (WGS84 west,south,east,north). Commit a flood AOI or pass aoiWgs84."
            .into()
    })
}

fn validate_aoi(b: GeoBounds) -> Result<GeoBounds, String> {
    if !b.min_x.is_finite()
        || !b.min_y.is_finite()
        || !b.max_x.is_finite()
        || !b.max_y.is_finite()
    {
        return Err("AOI bounds must be finite numbers.".into());
    }
    if b.max_x <= b.min_x || b.max_y <= b.min_y {
        return Err("AOI must have positive width and height (west<east, south<north).".into());
    }
    if b.min_x < -180.0 || b.max_x > 180.0 || b.min_y < -90.0 || b.max_y > 90.0 {
        return Err("AOI must be WGS84 lon/lat within [-180,180] × [-90,90].".into());
    }
    let span_lon = b.max_x - b.min_x;
    let span_lat = b.max_y - b.min_y;
    if span_lon > 5.0 || span_lat > 5.0 {
        return Err(format!(
            "AOI is too large for a single fetch ({span_lon:.2}° × {span_lat:.2}°). \
             Shrink to ≤5° on each side."
        ));
    }
    Ok(b)
}

fn aoi_intersects_conus(b: GeoBounds) -> bool {
    // Rough CONUS+AK+HI+PR envelope for messaging (not a hard gate).
    let lon_ok = b.max_x > -180.0 && b.min_x < -64.0;
    let lat_ok = b.max_y > 17.0 && b.min_y < 72.0;
    lon_ok && lat_ok
}

fn dem_pixel_size(aoi: GeoBounds, cell_size_m: Option<f64>) -> (u32, u32, f64) {
    let cell = cell_size_m.unwrap_or(30.0).max(1.0);
    let mid_lat = (aoi.min_y + aoi.max_y) * 0.5;
    let m_per_deg_lat = 111_320.0;
    let m_per_deg_lon = 111_320.0 * mid_lat.to_radians().cos().abs().max(0.2);
    let width_m = (aoi.max_x - aoi.min_x) * m_per_deg_lon;
    let height_m = (aoi.max_y - aoi.min_y) * m_per_deg_lat;
    let mut w = (width_m / cell).ceil().max(1.0) as u32;
    let mut h = (height_m / cell).ceil().max(1.0) as u32;
    let max_dim = w.max(h);
    if max_dim > MAX_DEM_PIXELS {
        let scale = MAX_DEM_PIXELS as f64 / max_dim as f64;
        w = ((w as f64) * scale).floor().max(1.0) as u32;
        h = ((h as f64) * scale).floor().max(1.0) as u32;
    }
    let effective = ((width_m / w as f64) + (height_m / h as f64)) * 0.5;
    (w, h, effective)
}

fn short_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())[..12].to_string()
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut f = fs::File::create(path).map_err(|e| e.to_string())?;
    f.write_all(bytes).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// USGS 3DEP
// ---------------------------------------------------------------------------

fn fetch_usgs_3dep(
    dest: &Path,
    aoi: Option<GeoBounds>,
    cell_size_m: Option<f64>,
) -> Result<(PathBuf, Option<String>, Option<String>), String> {
    let aoi = validate_aoi(require_aoi(aoi)?)?;
    let (w, h, effective) = dem_pixel_size(aoi, cell_size_m);
    let bbox = format!(
        "{},{},{},{}",
        aoi.min_x, aoi.min_y, aoi.max_x, aoi.max_y
    );
    let url = format!(
        "{USGS_3DEP_EXPORT}?bbox={bbox}&bboxSR=4326&imageSR=4326&size={w},{h}\
         &format=tiff&pixelType=F32&noDataInterpretation=esriNoDataMatchAny\
         &interpolation=RSP_BilinearInterpolation&f=image"
    );
    let bytes = http_get_bytes(&url)?;
    if bytes.len() < 64 {
        return Err(
            "USGS 3DEP returned an empty response. Check network or AOI coverage.".into(),
        );
    }
    // Reject obvious HTML error pages.
    if bytes.starts_with(b"<") || bytes.starts_with(b"<!DOCTYPE") {
        return Err(
            "USGS 3DEP returned HTML instead of a GeoTIFF (service error or rate limit)."
                .into(),
        );
    }
    let out = dest.join("usgs_3dep_aoi.tif");
    write_bytes(&out, &bytes)?;
    let meta = dest.join("usgs_3dep_aoi.meta.json");
    let meta_body = serde_json::json!({
        "connector": "usgs-3dep",
        "aoiWgs84": aoi.as_array(),
        "sizePx": [w, h],
        "effectiveResolutionM": effective,
        "nodata": "service NoData / esriNoDataMatchAny",
        "attribution": "USGS 3DEP Elevation",
        "url": url,
    });
    fs::write(
        &meta,
        serde_json::to_string_pretty(&meta_body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        out,
        Some(url),
        Some(format!(
            "Clipped USGS 3DEP GeoTIFF staged (~{effective:.1} m cells, {w}×{h} px)."
        )),
    ))
}

// ---------------------------------------------------------------------------
// Copernicus GLO-30 (AWS Open Data COG) + user file
// ---------------------------------------------------------------------------

fn fetch_copernicus(
    dest: &Path,
    aoi: Option<GeoBounds>,
    user_file: Option<&str>,
) -> Result<(PathBuf, Option<String>, Option<String>), String> {
    if let Some(path) = user_file {
        let src = PathBuf::from(path);
        if !src.exists() {
            return Err(format!(
                "Copernicus user file not found: {}. Provide a GLO-30 GeoTIFF/COG path.",
                src.display()
            ));
        }
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("tif");
        let out = dest.join(format!("copernicus_glo30_user.{ext}"));
        if src.canonicalize().ok() == out.canonicalize().ok() {
            // Already staged at destination.
        } else {
            fs::copy(&src, &out).map_err(|e| format!("Failed copying user DEM: {e}"))?;
        }
        return Ok((
            out,
            Some(src.to_string_lossy().into_owned()),
            Some("Staged user-provided Copernicus / GLO-30 DEM file.".into()),
        ));
    }

    let aoi = validate_aoi(require_aoi(aoi)?)?;
    // Download the 1° tile covering the AOI centroid (full tile; dem.rs clips in meta).
    let lat = ((aoi.min_y + aoi.max_y) * 0.5).floor() as i32;
    let lon = ((aoi.min_x + aoi.max_x) * 0.5).floor() as i32;
    let (tile_name, url) = copernicus_tile_url(lat, lon);
    let bytes = http_get_bytes(&url).map_err(|e| {
        format!(
            "{e} — Copernicus GLO-30 tile `{tile_name}` unavailable (ocean / restricted / network). \
             Pass userFile with a local GLO-30 GeoTIFF as a fallback."
        )
    })?;
    if bytes.len() < 1024 {
        return Err(format!(
            "Copernicus tile `{tile_name}` response too small. Pass userFile with a local DEM."
        ));
    }
    let out = dest.join(format!("{tile_name}.tif"));
    write_bytes(&out, &bytes)?;
    let meta = dest.join(format!("{tile_name}.meta.json"));
    let meta_body = serde_json::json!({
        "connector": "copernicus-glo30",
        "tile": tile_name,
        "aoiWgs84": aoi.as_array(),
        "resolutionM": 30.0,
        "attribution": "Copernicus DEM GLO-30",
        "cite": "Copernicus DEM — cite Copernicus Programme",
        "url": url,
        "note": "Full 1° COG tile staged; dem conditioning records AOI clip bounds.",
    });
    fs::write(
        &meta,
        serde_json::to_string_pretty(&meta_body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        out,
        Some(url),
        Some(format!(
            "Copernicus GLO-30 tile `{tile_name}` staged from AWS Open Data."
        )),
    ))
}

fn copernicus_tile_url(lat: i32, lon: i32) -> (String, String) {
    let ns = if lat >= 0 { "N" } else { "S" };
    let ew = if lon >= 0 { "E" } else { "W" };
    let lat_abs = lat.unsigned_abs();
    let lon_abs = lon.unsigned_abs();
    let northing = format!("{ns}{lat_abs:02}_00");
    let easting = format!("{ew}{lon_abs:03}_00");
    let tile = format!("Copernicus_DSM_COG_10_{northing}_{easting}_DEM");
    let url = format!("{COPERNICUS_S3}/{tile}/{tile}.tif");
    (tile, url)
}

// ---------------------------------------------------------------------------
// OpenTopography
// ---------------------------------------------------------------------------

fn opentopo_api_key(explicit: Option<&str>) -> Option<String> {
    explicit
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            std::env::var("OPENTOPOGRAPHY_API_KEY")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

fn fetch_opentopography(
    dest: &Path,
    aoi: Option<GeoBounds>,
    _cell_size_m: Option<f64>,
    api_key: Option<&str>,
) -> Result<(PathBuf, Option<String>, Option<String>), String> {
    let key = opentopo_api_key(api_key).ok_or_else(|| {
        "OpenTopography requires an API key. Set OPENTOPOGRAPHY_API_KEY or pass apiKey \
         (register at https://opentopography.org/)."
            .to_string()
    })?;
    let aoi = validate_aoi(require_aoi(aoi)?)?;
    let url = format!(
        "{OPENTOPO_GLOBALDEM}?demtype=COP30&south={}&north={}&west={}&east={}&outputFormat=GTiff&API_Key={}",
        aoi.min_y, aoi.max_y, aoi.min_x, aoi.max_x, key
    );
    // Redact key in stored URL
    let public_url = format!(
        "{OPENTOPO_GLOBALDEM}?demtype=COP30&south={}&north={}&west={}&east={}&outputFormat=GTiff&API_Key=***",
        aoi.min_y, aoi.max_y, aoi.min_x, aoi.max_x
    );
    let bytes = http_get_bytes(&url).map_err(|e| {
        format!(
            "{e} — Check OPENTOPOGRAPHY_API_KEY validity and AOI size limits."
        )
    })?;
    if bytes.starts_with(b"<") || bytes.starts_with(b"{") {
        let msg: String = String::from_utf8_lossy(&bytes).chars().take(300).collect();
        return Err(format!("OpenTopography error response: {msg}"));
    }
    let out = dest.join("opentopo_cop30_aoi.tif");
    write_bytes(&out, &bytes)?;
    Ok((
        out,
        Some(public_url),
        Some("OpenTopography COP30 GeoTIFF clipped to AOI.".into()),
    ))
}

// ---------------------------------------------------------------------------
// HydroSHEDS / HydroBASINS
// ---------------------------------------------------------------------------

fn fetch_hydrosheds(
    dest: &Path,
    aoi: Option<GeoBounds>,
    user_file: Option<&str>,
) -> Result<(PathBuf, Option<String>, Option<String>), String> {
    if let Some(path) = user_file {
        let src = PathBuf::from(path);
        if !src.exists() {
            return Err(format!(
                "HydroSHEDS user file not found: {}. Provide a HydroBASINS GeoJSON/GPKG path.",
                src.display()
            ));
        }
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("geojson");
        let out = dest.join(format!("hydrosheds_user.{ext}"));
        if src.canonicalize().ok() == out.canonicalize().ok() {
            // Already in place.
        } else {
            fs::copy(&src, &out).map_err(|e| format!("Failed copying HydroSHEDS file: {e}"))?;
        }
        return Ok((
            out,
            Some(src.to_string_lossy().into_owned()),
            Some("Staged user-provided HydroSHEDS / HydroBASINS file.".into()),
        ));
    }

    let aoi = validate_aoi(require_aoi(aoi)?)?;
    let continent = hydrosheds_continent(aoi);
    let product_url = format!(
        "https://data.hydrosheds.org/file/hydrobasins/standard/hybas_{continent}_lev08_v1c.zip"
    );

    // Continent zips are huge — stage an honest AOI request + citation GeoJSON
    // rather than silently downloading multi-GB archives.
    let out = dest.join("hydrosheds_aoi_request.geojson");
    let body = serde_json::json!({
        "type": "FeatureCollection",
        "name": "hydrosheds_aoi_request",
        "features": [{
            "type": "Feature",
            "properties": {
                "connector": "hydrosheds",
                "product": "HydroBASINS",
                "level": 8,
                "continent": continent,
                "productUrl": product_url,
                "citation": "Lehner, B., Grill G. (2013). Global river hydrography and network routing. Cite HydroSHEDS / HydroBASINS.",
                "note": "Full continent HydroBASINS packages are large. This file records the AOI and download URL; place a clipped basin GeoJSON via userFile for local overlay, or download/clip offline.",
            },
            "geometry": {
                "type": "Polygon",
                "coordinates": [[
                    [aoi.min_x, aoi.min_y],
                    [aoi.max_x, aoi.min_y],
                    [aoi.max_x, aoi.max_y],
                    [aoi.min_x, aoi.max_y],
                    [aoi.min_x, aoi.min_y]
                ]]
            }
        }]
    });
    fs::write(
        &out,
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // Probe product URL (HEAD/GET small) so users know if the endpoint is reachable.
    let reachable = http_client()
        .and_then(|c| {
            c.head(&product_url)
                .send()
                .map_err(|e| e.to_string())
                .and_then(|r| {
                    if r.status().is_success() || r.status().as_u16() == 403 {
                        Ok(())
                    } else {
                        Err(format!("HTTP {}", r.status()))
                    }
                })
        })
        .is_ok();

    let notes = if reachable {
        format!(
            "HydroBASINS AOI request staged (continent `{continent}`). \
             Product URL reachable — download/clip offline or pass userFile. Cite HydroSHEDS."
        )
    } else {
        format!(
            "HydroBASINS AOI request staged (continent `{continent}`). \
             Could not verify product URL (network). Pass userFile with a clipped basin layer."
        )
    };
    Ok((out, Some(product_url), Some(notes)))
}

fn hydrosheds_continent(aoi: GeoBounds) -> &'static str {
    let lon = (aoi.min_x + aoi.max_x) * 0.5;
    let lat = (aoi.min_y + aoi.max_y) * 0.5;
    // HydroBASINS continent codes: af, ar, as, au, eu, gr, na, sa, si
    if (-170.0..-50.0).contains(&lon) && (7.0..84.0).contains(&lat) {
        "na"
    } else if (-92.0..-32.0).contains(&lon) && (-56.0..15.0).contains(&lat) {
        "sa"
    } else if (-25.0..60.0).contains(&lon) && (-35.0..38.0).contains(&lat) {
        "af"
    } else if (-12.0..70.0).contains(&lon) && (35.0..82.0).contains(&lat) {
        "eu"
    } else if (40.0..180.0).contains(&lon) && (-12.0..60.0).contains(&lat) {
        "as"
    } else if (110.0..180.0).contains(&lon) && (-50.0..0.0).contains(&lat) {
        "au"
    } else if lon < -50.0 {
        "na"
    } else {
        "eu"
    }
}

// ---------------------------------------------------------------------------
// USGS NWIS gauges
// ---------------------------------------------------------------------------

fn fetch_nwis_gauges(
    dest: &Path,
    aoi: Option<GeoBounds>,
) -> Result<(PathBuf, Option<String>, Option<String>), String> {
    let aoi = validate_aoi(require_aoi(aoi)?)?;
    let bbox = format!(
        "{},{},{},{}",
        aoi.min_x, aoi.min_y, aoi.max_x, aoi.max_y
    );
    // Site inventory (RDB) is reliable; also try instantaneous values JSON.
    let site_url = format!(
        "{NWIS_SITE}?format=rdb&bBox={bbox}&siteType=ST&siteStatus=active&hasDataTypeCd=iv"
    );
    let iv_url = format!(
        "{NWIS_IV}?format=json&bBox={bbox}&parameterCd=00060,00065&siteStatus=active"
    );

    let mut features = Vec::new();
    let mut source_url = site_url.clone();

    let rdb_notes = match http_get_bytes(&site_url) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            features.extend(parse_nwis_rdb_sites(&text));
            format!("{} NWIS sites from RDB inventory.", features.len())
        }
        Err(e) => format!("NWIS site RDB failed ({e}); trying IV JSON…"),
    };

    let notes = if features.is_empty() {
        match http_get_json(&iv_url) {
            Ok(json) => {
                features.extend(parse_nwis_iv_json(&json));
                source_url = iv_url.clone();
                format!(
                    "{} gauge time-series sites from NWIS IV JSON.",
                    features.len()
                )
            }
            Err(e) => {
                return Err(format!(
                    "USGS NWIS gauge fetch failed. {rdb_notes} IV error: {e}"
                ));
            }
        }
    } else {
        rdb_notes
    };

    let out = dest.join("usgs_nwis_gauges.geojson");
    let body = serde_json::json!({
        "type": "FeatureCollection",
        "name": "usgs_nwis_gauges",
        "features": features,
        "properties": {
            "connector": "usgs-nwis-gauges",
            "attribution": "USGS National Water Information System",
            "aoiWgs84": aoi.as_array(),
            "sourceUrl": source_url,
        }
    });
    fs::write(
        &out,
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    if features.is_empty() {
        Ok((
            out,
            Some(source_url),
            Some(
                "No active stream gauges found in AOI (empty FeatureCollection staged)."
                    .into(),
            ),
        ))
    } else {
        Ok((out, Some(source_url), Some(notes)))
    }
}

fn parse_nwis_rdb_sites(text: &str) -> Vec<serde_json::Value> {
    let mut features = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if headers.is_empty() {
            headers = line.split('\t').map(|s| s.to_string()).collect();
            continue;
        }
        // Skip RDB format definition row (e.g. 5s 15s …)
        if line
            .split('\t')
            .next()
            .map(|c| c.chars().all(|ch| ch.is_ascii_digit() || ch == 's' || ch == 'n' || ch == 'd'))
            .unwrap_or(false)
            && line.contains('s')
            && !line.contains("USGS")
        {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < headers.len() {
            continue;
        }
        let get = |name: &str| {
            headers
                .iter()
                .position(|h| h == name)
                .and_then(|i| cols.get(i).copied())
                .unwrap_or("")
        };
        let lon: f64 = get("dec_long_va").parse().unwrap_or(f64::NAN);
        let lat: f64 = get("dec_lat_va").parse().unwrap_or(f64::NAN);
        if !lon.is_finite() || !lat.is_finite() {
            continue;
        }
        let site_no = get("site_no");
        let name = get("station_nm");
        features.push(serde_json::json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [lon, lat] },
            "properties": {
                "siteNo": site_no,
                "name": name,
                "agency": get("agency_cd"),
                "siteUrl": format!("https://waterdata.usgs.gov/monitoring-location/{site_no}/"),
            }
        }));
    }
    features
}

fn parse_nwis_iv_json(json: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut features = Vec::new();
    let Some(series) = json
        .pointer("/value/timeSeries")
        .and_then(|v| v.as_array())
    else {
        return features;
    };
    for ts in series {
        let site_no = ts
            .pointer("/sourceInfo/siteCode/0/value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let name = ts
            .pointer("/sourceInfo/siteName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let lon = ts
            .pointer("/sourceInfo/geoLocation/geogLocation/longitude")
            .and_then(|v| v.as_f64())
            .unwrap_or(f64::NAN);
        let lat = ts
            .pointer("/sourceInfo/geoLocation/geogLocation/latitude")
            .and_then(|v| v.as_f64())
            .unwrap_or(f64::NAN);
        if !lon.is_finite() || !lat.is_finite() {
            continue;
        }
        let param = ts
            .pointer("/variable/variableCode/0/value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        features.push(serde_json::json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [lon, lat] },
            "properties": {
                "siteNo": site_no,
                "name": name,
                "parameterCd": param,
                "siteUrl": format!("https://waterdata.usgs.gov/monitoring-location/{site_no}/"),
            }
        }));
    }
    features
}

// ---------------------------------------------------------------------------
// OSM waterways (Overpass) — ODbL overlay, not simulation
// ---------------------------------------------------------------------------

fn fetch_osm_waterways(
    dest: &Path,
    aoi: Option<GeoBounds>,
) -> Result<(PathBuf, Option<String>, Option<String>), String> {
    let aoi = validate_aoi(require_aoi(aoi)?)?;
    // Overpass bbox: south,west,north,east
    let bbox = format!(
        "{},{},{},{}",
        aoi.min_y, aoi.min_x, aoi.max_y, aoi.max_x
    );
    let query = format!(
        "[out:json][timeout:45];(way[\"waterway\"]({bbox});relation[\"waterway\"]({bbox}););out geom;"
    );
    let url = "https://overpass-api.de/api/interpreter";
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("data={query}"))
        .send()
        .map_err(|e| format!("OSM Overpass request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "OSM Overpass HTTP {} — try again or draw a smaller AOI.",
            resp.status()
        ));
    }
    let json: serde_json::Value = resp
        .json()
        .map_err(|e| format!("OSM Overpass JSON parse: {e}"))?;
    let elements = json
        .get("elements")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    let mut features = Vec::new();
    for el in &elements {
        let tags = el.get("tags").cloned().unwrap_or(serde_json::json!({}));
        let name = tags.get("name").and_then(|v| v.as_str()).unwrap_or("waterway");
        let waterway = tags
            .get("waterway")
            .and_then(|v| v.as_str())
            .unwrap_or("stream");
        let geom = if let Some(geometry) = el.get("geometry").and_then(|g| g.as_array()) {
            let coords: Vec<serde_json::Value> = geometry
                .iter()
                .filter_map(|p| {
                    let lon = p.get("lon")?.as_f64()?;
                    let lat = p.get("lat")?.as_f64()?;
                    Some(serde_json::json!([lon, lat]))
                })
                .collect();
            if coords.len() < 2 {
                continue;
            }
            serde_json::json!({ "type": "LineString", "coordinates": coords })
        } else {
            continue;
        };
        features.push(serde_json::json!({
            "type": "Feature",
            "properties": {
                "name": name,
                "waterway": waterway,
                "source": "osm-overpass",
                "license": "ODbL"
            },
            "geometry": geom
        }));
    }

    let out = dest.join("osm_waterways.geojson");
    let body = serde_json::json!({
        "type": "FeatureCollection",
        "name": "osm_waterways",
        "features": features,
        "properties": {
            "connector": "osm-waterways",
            "attribution": "© OpenStreetMap contributors (ODbL)",
            "aoiWgs84": aoi.as_array(),
            "note": "Network overlay only — not a hydrologic solution."
        }
    });
    fs::write(
        &out,
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let notes = if features.is_empty() {
        "No OSM waterways in AOI (empty FeatureCollection). Stub geometry may still show in UI."
            .into()
    } else {
        format!("{} OSM waterway features (ODbL).", features.len())
    };
    Ok((out, Some(url.into()), Some(notes)))
}

// ---------------------------------------------------------------------------
// FEMA NFHL
// ---------------------------------------------------------------------------

fn fetch_fema_nfhl(
    dest: &Path,
    aoi: Option<GeoBounds>,
) -> Result<(PathBuf, Option<String>, Option<String>), String> {
    let aoi = validate_aoi(require_aoi(aoi)?)?;
    let geometry = format!(
        "{},{},{},{}",
        aoi.min_x, aoi.min_y, aoi.max_x, aoi.max_y
    );
    let url = format!(
        "{FEMA_NFHL_QUERY}?geometry={geometry}&geometryType=esriGeometryEnvelope\
         &inSR=4326&spatialRel=esriSpatialRelIntersects&outFields=FLD_ZONE,ZONE_SUBTY,SFHA_TF,STATIC_BFE\
         &returnGeometry=true&outSR=4326&f=geojson&resultRecordCount=2000"
    );
    let bytes = http_get_bytes(&url)?;
    // Validate GeoJSON-ish
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("FEMA NFHL response not JSON: {e}"))?;
    if json.get("error").is_some() {
        let msg = json
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown FEMA error");
        return Err(format!("FEMA NFHL query error: {msg}"));
    }
    let count = json
        .get("features")
        .and_then(|f| f.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let out = dest.join("fema_nfhl_flood_hazard_zones.geojson");
    write_bytes(&out, &bytes)?;
    // Sidecar note: overlay only, not simulation
    let meta = dest.join("fema_nfhl_flood_hazard_zones.meta.json");
    let meta_body = serde_json::json!({
        "connector": "fema-nfhl",
        "layer": 28,
        "role": "flood-zone-overlay",
        "notSimulation": true,
        "attribution": "FEMA National Flood Hazard Layer",
        "featureCount": count,
        "aoiWgs84": aoi.as_array(),
    });
    fs::write(
        &meta,
        serde_json::to_string_pretty(&meta_body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        out,
        Some(url),
        Some(format!(
            "FEMA NFHL flood hazard zones overlay ({count} features). Not a simulation."
        )),
    ))
}

// ---------------------------------------------------------------------------
// Earth Search STAC
// ---------------------------------------------------------------------------

fn fetch_earth_search(
    dest: &Path,
    aoi: Option<GeoBounds>,
) -> Result<(PathBuf, Option<String>, Option<String>), String> {
    let aoi = validate_aoi(require_aoi(aoi)?)?;
    let body = serde_json::json!({
        "bbox": [aoi.min_x, aoi.min_y, aoi.max_x, aoi.max_y],
        "collections": ["sentinel-2-l2a"],
        "limit": 10,
        "query": {
            "eo:cloud_cover": { "lt": 40 }
        }
    });
    let json = http_post_json(EARTH_SEARCH, &body).map_err(|e| {
        format!(
            "{e} — Earth Search STAC is optional; check network or try again later."
        )
    })?;
    let features = json
        .get("features")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let count = features.as_array().map(|a| a.len()).unwrap_or(0);
    let out = dest.join("earth_search_stac_items.geojson");
    let fc = serde_json::json!({
        "type": "FeatureCollection",
        "name": "earth_search_stac",
        "features": features,
        "properties": {
            "connector": "earth-search-stac",
            "collections": ["sentinel-2-l2a"],
            "attribution": "Earth Search STAC (Element84)",
            "aoiWgs84": aoi.as_array(),
            "note": "Item metadata + asset hrefs; download imagery assets separately as needed.",
        }
    });
    fs::write(
        &out,
        serde_json::to_string_pretty(&fc).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        out,
        Some(EARTH_SEARCH.to_string()),
        Some(format!(
            "Earth Search STAC: {count} Sentinel-2 L2A items (metadata) for AOI."
        )),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("instasplatter_catalog_{label}_{id}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn connector_names_include_p0() {
        let names = connector_names();
        assert!(names.iter().any(|n| n.contains("3DEP")));
        assert!(names.iter().any(|n| n.contains("Copernicus")));
        assert!(names.iter().any(|n| n.contains("HydroSHEDS")));
        assert!(names.iter().any(|n| n.contains("NFHL") || n.contains("FEMA")));
        assert!(names.iter().any(|n| n.contains("gauges") || n.contains("USGS")));
        assert!(names.iter().any(|n| n.contains("STAC") || n.contains("Earth")));
        assert!(names.iter().any(|n| n.contains("OpenTopography")));
        assert!(names.iter().any(|n| n.contains("OSM") || n.contains("waterway")));
    }

    #[test]
    fn list_entries_sets_bounds() {
        let aoi = [-122.5, 37.7, -122.3, 37.85];
        let entries = list_entries(Some(aoi));
        assert_eq!(entries.len(), CONNECTORS.len());
        let usgs = entries.iter().find(|e| e.id == "usgs-3dep").unwrap();
        assert!(usgs.bounds.is_some());
        assert_eq!(usgs.attribution.as_deref(), Some("USGS 3DEP Elevation"));
    }

    #[test]
    fn fetch_requires_aoi() {
        let dir = temp_dir("no_aoi");
        let err = fetch_asset("usgs-3dep", dir.to_str().unwrap()).unwrap_err();
        assert!(err.to_lowercase().contains("aoi"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fetch_rejects_huge_aoi() {
        let dir = temp_dir("huge");
        let err = fetch_asset_with_opts(
            "usgs-3dep",
            dir.to_str().unwrap(),
            &CatalogFetchOpts {
                aoi_wgs84: Some([-130.0, 20.0, -60.0, 50.0]),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.contains("too large"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_aliases() {
        assert_eq!(resolve_entry_id("USGS 3DEP"), "usgs-3dep");
        assert_eq!(resolve_entry_id("Copernicus DEM"), "copernicus-glo30");
        assert_eq!(resolve_entry_id("FEMA NFHL"), "fema-nfhl");
        assert_eq!(resolve_entry_id("osm"), "osm-waterways");
        assert_eq!(resolve_entry_id("waterways"), "osm-waterways");
    }

    #[test]
    fn copernicus_tile_naming() {
        let (tile, url) = copernicus_tile_url(37, -123);
        assert_eq!(tile, "Copernicus_DSM_COG_10_N37_00_W123_00_DEM");
        assert!(url.contains(&tile));
        assert!(url.ends_with(".tif"));
    }

    #[test]
    fn hydrosheds_stages_request_geojson() {
        let dir = temp_dir("hydro");
        let (path, url, notes) = fetch_hydrosheds(
            &dir,
            Some(GeoBounds::from_array([-122.5, 37.7, -122.3, 37.85])),
            None,
        )
        .unwrap();
        assert!(path.exists());
        assert!(url.unwrap().contains("hydrobasins"));
        assert!(notes.unwrap().contains("HydroBASINS"));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("FeatureCollection"));
        assert!(text.contains("Cite HydroSHEDS") || text.contains("HydroSHEDS"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hydrosheds_user_file() {
        let dir = temp_dir("hydro_user");
        let src = dir.join("basins.geojson");
        fs::write(&src, r#"{"type":"FeatureCollection","features":[]}"#).unwrap();
        let (path, _, notes) = fetch_hydrosheds(&dir, None, Some(src.to_str().unwrap())).unwrap();
        assert!(path.exists());
        assert!(notes.unwrap().contains("user-provided"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn copernicus_user_file() {
        let dir = temp_dir("cop_user");
        let src = dir.join("glo30.tif");
        fs::write(&src, b"fake-cog-bytes").unwrap();
        let (path, _, notes) =
            fetch_copernicus(&dir, None, Some(src.to_str().unwrap())).unwrap();
        assert!(path.exists());
        assert!(notes.unwrap().contains("user-provided"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn opentopo_without_key_errors_clearly() {
        // Ensure env key does not leak into this test.
        let prev = std::env::var("OPENTOPOGRAPHY_API_KEY").ok();
        std::env::remove_var("OPENTOPOGRAPHY_API_KEY");
        let dir = temp_dir("ot");
        let err = fetch_opentopography(
            &dir,
            Some(GeoBounds::from_array([-122.5, 37.7, -122.3, 37.85])),
            Some(30.0),
            None,
        )
        .unwrap_err();
        assert!(err.contains("API key"), "{err}");
        if let Some(v) = prev {
            std::env::set_var("OPENTOPOGRAPHY_API_KEY", v);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_rdb_minimal() {
        let rdb = "\
# comment
agency_cd\tsite_no\tstation_nm\tdec_lat_va\tdec_long_va
5s\t15s\t50s\t12n\t13n
USGS\t11123456\tTEST RIVER\t37.75\t-122.40
";
        let feats = parse_nwis_rdb_sites(rdb);
        assert_eq!(feats.len(), 1);
        assert_eq!(feats[0]["properties"]["siteNo"], "11123456");
    }

    #[test]
    fn dem_pixel_size_caps() {
        let aoi = GeoBounds::from_array([-122.5, 37.7, -122.0, 38.2]);
        let (w, h, _) = dem_pixel_size(aoi, Some(1.0));
        assert!(w <= MAX_DEM_PIXELS);
        assert!(h <= MAX_DEM_PIXELS);
    }

    #[test]
    fn list_cached_finds_files() {
        let dir = temp_dir("cache");
        fs::write(dir.join("site.tif"), b"x").unwrap();
        let cached = list_cached(dir.to_str().unwrap());
        assert!(cached.iter().any(|e| e.local_path.is_some()));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Network-backed smoke: skip when offline / CI without egress.
    #[test]
    fn usgs_3dep_fetch_smoke_or_skip() {
        if std::env::var("INSTASPLATTER_CATALOG_NETWORK").ok().as_deref() != Some("1") {
            return;
        }
        let dir = temp_dir("3dep_net");
        let result = fetch_asset_with_opts(
            "usgs-3dep",
            dir.to_str().unwrap(),
            &CatalogFetchOpts {
                aoi_wgs84: Some([-122.45, 37.75, -122.40, 37.80]),
                cell_size_m: Some(30.0),
                ..Default::default()
            },
        );
        match result {
            Ok(e) => {
                assert!(e.local_path.is_some());
                let p = PathBuf::from(e.local_path.unwrap());
                assert!(p.exists());
                assert!(fs::metadata(&p).unwrap().len() > 100);
            }
            Err(e) => panic!("expected network fetch to succeed when enabled: {e}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
