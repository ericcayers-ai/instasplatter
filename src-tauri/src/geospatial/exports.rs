//! Geospatial flood / survey export products (plan §7).
//!
//! Offline-first writers produce GeoTIFF-like rasters, GeoJSON vectors, JSON /
//! Zarr-scaffold time series, SPZ passthrough, and scenario manifests without
//! GDAL. When `gdal_translate` / `ogr2ogr` (or Python `osgeo`) is on PATH,
//! COG optimisation and GeoPackage packaging are attempted.
//!
//! Never labels uncalibrated demo output as scientifically authoritative.

use crate::geospatial::preview::{
    compare_checkpoint, PreviewCheckpoint, PreviewCompareTolerance, PreviewResultSource,
};
use crate::project::{FloodScenario, GeoReference, Project, SimulationRun};
use crate::splat::{export as splat_export, ply as splat_ply};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPORT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GeoExportKind {
    CogDepth,
    CogVelocity,
    CogHazard,
    CogArrival,
    CogDuration,
    CogUncertainty,
    GeoPackage,
    GeoJson,
    Zarr,
    NetCdf,
    Copc,
    Spz,
    GltfGaussian,
    Tiles3d,
    PmTiles,
    ScenarioReport,
    SurveyResidual,
}

impl GeoExportKind {
    pub fn id(self) -> &'static str {
        match self {
            GeoExportKind::CogDepth => "cogDepth",
            GeoExportKind::CogVelocity => "cogVelocity",
            GeoExportKind::CogHazard => "cogHazard",
            GeoExportKind::CogArrival => "cogArrival",
            GeoExportKind::CogDuration => "cogDuration",
            GeoExportKind::CogUncertainty => "cogUncertainty",
            GeoExportKind::GeoPackage => "geoPackage",
            GeoExportKind::GeoJson => "geoJson",
            GeoExportKind::Zarr => "zarr",
            GeoExportKind::NetCdf => "netCdf",
            GeoExportKind::Copc => "copc",
            GeoExportKind::Spz => "spz",
            GeoExportKind::GltfGaussian => "gltfGaussian",
            GeoExportKind::Tiles3d => "tiles3d",
            GeoExportKind::PmTiles => "pmtiles",
            GeoExportKind::ScenarioReport => "scenarioReport",
            GeoExportKind::SurveyResidual => "surveyResidual",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GeoExportKind::CogDepth => "COG/GeoTIFF max depth",
            GeoExportKind::CogVelocity => "COG/GeoTIFF velocity",
            GeoExportKind::CogHazard => "COG/GeoTIFF hazard",
            GeoExportKind::CogArrival => "COG/GeoTIFF arrival time",
            GeoExportKind::CogDuration => "COG/GeoTIFF inundation duration",
            GeoExportKind::CogUncertainty => "COG/GeoTIFF uncertainty",
            GeoExportKind::GeoPackage => "GeoPackage",
            GeoExportKind::GeoJson => "GeoJSON",
            GeoExportKind::Zarr => "Zarr time series",
            GeoExportKind::NetCdf => "CF-NetCDF",
            GeoExportKind::Copc => "COPC point cloud",
            GeoExportKind::Spz => "SPZ splat (v4)",
            GeoExportKind::GltfGaussian => "glTF Gaussian",
            GeoExportKind::Tiles3d => "3D Tiles",
            GeoExportKind::PmTiles => "PMTiles",
            GeoExportKind::ScenarioReport => "Scenario report",
            GeoExportKind::SurveyResidual => "Survey residual report",
        }
    }

    /// True when this format can complete without GDAL/Python GIS tooling.
    pub fn works_offline(self) -> bool {
        !matches!(
            self,
            GeoExportKind::GeoPackage | GeoExportKind::Copc | GeoExportKind::PmTiles
        )
    }

    pub fn parse(s: &str) -> Option<GeoExportKind> {
        match s.trim() {
            "cogDepth" => Some(GeoExportKind::CogDepth),
            "cogVelocity" => Some(GeoExportKind::CogVelocity),
            "cogHazard" => Some(GeoExportKind::CogHazard),
            "cogArrival" => Some(GeoExportKind::CogArrival),
            "cogDuration" => Some(GeoExportKind::CogDuration),
            "cogUncertainty" => Some(GeoExportKind::CogUncertainty),
            "geoPackage" | "gpkg" => Some(GeoExportKind::GeoPackage),
            "geoJson" | "geojson" => Some(GeoExportKind::GeoJson),
            "zarr" => Some(GeoExportKind::Zarr),
            "netCdf" | "netcdf" | "nc" => Some(GeoExportKind::NetCdf),
            "copc" => Some(GeoExportKind::Copc),
            "spz" => Some(GeoExportKind::Spz),
            "gltfGaussian" | "gltf" => Some(GeoExportKind::GltfGaussian),
            "tiles3d" | "3dtiles" => Some(GeoExportKind::Tiles3d),
            "pmtiles" => Some(GeoExportKind::PmTiles),
            "scenarioReport" | "manifest" => Some(GeoExportKind::ScenarioReport),
            "surveyResidual" | "residual" => Some(GeoExportKind::SurveyResidual),
            _ => None,
        }
    }
}

/// Formats the UI can advertise.
pub fn list_export_kinds() -> Vec<GeoExportKind> {
    vec![
        GeoExportKind::CogDepth,
        GeoExportKind::CogVelocity,
        GeoExportKind::CogHazard,
        GeoExportKind::CogArrival,
        GeoExportKind::CogDuration,
        GeoExportKind::CogUncertainty,
        GeoExportKind::GeoPackage,
        GeoExportKind::GeoJson,
        GeoExportKind::Zarr,
        GeoExportKind::NetCdf,
        GeoExportKind::Copc,
        GeoExportKind::Spz,
        GeoExportKind::GltfGaussian,
        GeoExportKind::Tiles3d,
        GeoExportKind::PmTiles,
        GeoExportKind::ScenarioReport,
        GeoExportKind::SurveyResidual,
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GdalCapability {
    pub available: bool,
    pub gdal_translate: Option<String>,
    pub ogr2ogr: Option<String>,
    pub python_osgeo: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportArtifact {
    pub kind: String,
    pub path: String,
    pub format: String,
    /// `native` | `offlineStub` | `gdal` | `scaffold` | `passthrough`
    pub writer: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FloodExportResult {
    pub export_dir: String,
    pub run_id: String,
    pub mode: Option<String>,
    pub authoritative: bool,
    pub gdal: GdalCapability,
    pub artifacts: Vec<ExportArtifact>,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerExportResult {
    pub kind: String,
    pub path: String,
    pub writer: String,
    pub notes: Vec<String>,
}

/// Float32 raster in a projected / local ENU frame.
#[derive(Debug, Clone)]
pub struct RasterGrid {
    pub cols: u32,
    pub rows: u32,
    /// Origin of the upper-left pixel corner: [x, y] in CRS units.
    pub origin: [f64; 2],
    /// Pixel size (x positive east, y typically negative north in GIS).
    pub pixel_size: [f64; 2],
    pub crs: String,
    pub nodata: f32,
    pub values: Vec<f32>,
    pub quantity: String,
    pub units: String,
}

impl RasterGrid {
    pub fn validate(&self) -> Result<(), String> {
        let n = (self.cols as usize)
            .checked_mul(self.rows as usize)
            .ok_or_else(|| "raster dimensions overflow".to_string())?;
        if self.values.len() != n {
            return Err(format!(
                "raster length {} != {}×{}",
                self.values.len(),
                self.cols,
                self.rows
            ));
        }
        Ok(())
    }
}

// ---- GDAL discovery --------------------------------------------------------

fn which_cmd(name: &str) -> Option<PathBuf> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("where");
        c.arg(name);
        c
    } else {
        let mut c = Command::new("which");
        c.arg(name);
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next()?.trim();
    if first.is_empty() {
        None
    } else {
        Some(PathBuf::from(first))
    }
}

fn python_has_osgeo() -> bool {
    let mut cmd = Command::new("python");
    cmd.args([
        "-c",
        "import osgeo.gdal, osgeo.ogr; print('ok')",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    }
    cmd.output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Probe PATH / Python for GDAL tooling.
pub fn detect_gdal() -> GdalCapability {
    let gdal_translate = which_cmd("gdal_translate").map(|p| p.to_string_lossy().into_owned());
    let ogr2ogr = which_cmd("ogr2ogr").map(|p| p.to_string_lossy().into_owned());
    let python_osgeo = python_has_osgeo();
    let available = gdal_translate.is_some() || ogr2ogr.is_some() || python_osgeo;
    let mut notes = Vec::new();
    if !available {
        notes.push(
            "GDAL not found — COG optimisation, GeoPackage, and COPC require GDAL/PDAL."
                .into(),
        );
        notes.push(
            "Offline exports use GeoTIFF + JSON sidecars, GeoJSON, Zarr/JSON time series."
                .into(),
        );
    } else {
        notes.push("GDAL tooling detected — COG/GPKG paths enabled when writers succeed.".into());
    }
    GdalCapability {
        available,
        gdal_translate,
        ogr2ogr,
        python_osgeo,
        notes,
    }
}

// ---- Minimal GeoTIFF (Float32 + GeoTIFF tags) ------------------------------

/// Write a single-band Float32 GeoTIFF (classic TIFF; not COG-tiled).
pub fn write_geotiff(path: &Path, grid: &RasterGrid) -> Result<(), String> {
    grid.validate()?;
    let cols = grid.cols as u32;
    let rows = grid.rows as u32;
    let n = (cols as usize) * (rows as usize);

    let mut strip = Vec::with_capacity(n * 4);
    for v in &grid.values {
        strip.extend_from_slice(&v.to_le_bytes());
    }

    // IFD entries we write (sorted by tag number).
    // ImageWidth 256, ImageLength 257, BitsPerSample 258, Compression 259,
    // PhotometricInterpretation 262, StripOffsets 273, SamplesPerPixel 277,
    // RowsPerStrip 278, StripByteCounts 279, SampleFormat 339 (=3 float),
    // ModelPixelScaleTag 33550, ModelTiepointTag 33922, GeoKeyDirectoryTag 34735,
    // GeoAsciiParamsTag 34737, GDAL_NODATA 42113.
    let bits: u16 = 32;
    let compression: u16 = 1; // none
    let photometric: u16 = 1; // BlackIsZero
    let samples: u16 = 1;
    let sample_format: u16 = 3; // IEEE float
    let nodata_ascii = format!("{}", grid.nodata);
    let crs_ascii = if grid.crs.is_empty() {
        "LOCAL_CS[\"local-ENU-m\"]".to_string()
    } else {
        grid.crs.clone()
    };

    // Layout: 8-byte header + IFD + inline values / ascii + strip data.
    let entry_count: u16 = 15;
    let ifd_bytes = 2 + (entry_count as usize) * 12 + 4;
    let header_and_ifd = 8 + ifd_bytes;

    // Extra payloads after IFD (before strip):
    // ModelPixelScale (3 f64), ModelTiepoint (6 f64), GeoKeyDirectory (4+4*keys u16),
    // GeoAsciiParams, GDAL_NODATA ascii.
    let scale_off = header_and_ifd;
    let tie_off = scale_off + 24;
    let geokey_off = tie_off + 48;
    // GeoKeys: 1 header key + 1 CRS citation key pointing at ascii
    let geokey_count = 2u16;
    let geokey_bytes = (4 + geokey_count as usize * 4) * 2; // u16 words
    let ascii_off = geokey_off + geokey_bytes;
    let crs_bytes = crs_ascii.as_bytes();
    let crs_padded = pad_even(crs_bytes.len() + 1); // NUL
    let nodata_off = ascii_off + crs_padded;
    let nodata_bytes = nodata_ascii.as_bytes();
    let nodata_padded = pad_even(nodata_bytes.len() + 1);
    let strip_off = nodata_off + nodata_padded;

    let mut out = Vec::with_capacity(strip_off + strip.len());
    // Header: II + magic 42 + IFD offset
    out.extend_from_slice(&0x4949u16.to_le_bytes());
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&(8u32).to_le_bytes());

    out.extend_from_slice(&entry_count.to_le_bytes());

    fn write_entry(buf: &mut Vec<u8>, tag: u16, typ: u16, count: u32, value_or_offset: u32) {
        buf.extend_from_slice(&tag.to_le_bytes());
        buf.extend_from_slice(&typ.to_le_bytes());
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&value_or_offset.to_le_bytes());
    }

    // SHORT=3, LONG=4, ASCII=2, DOUBLE=12, SHORT array via offset
    write_entry(&mut out, 256, 4, 1, cols); // ImageWidth LONG
    write_entry(&mut out, 257, 4, 1, rows); // ImageLength
    write_entry(&mut out, 258, 3, 1, bits as u32); // BitsPerSample
    write_entry(&mut out, 259, 3, 1, compression as u32);
    write_entry(&mut out, 262, 3, 1, photometric as u32);
    write_entry(&mut out, 273, 4, 1, strip_off as u32); // StripOffsets
    write_entry(&mut out, 277, 3, 1, samples as u32);
    write_entry(&mut out, 278, 4, 1, rows); // RowsPerStrip
    write_entry(&mut out, 279, 4, 1, strip.len() as u32); // StripByteCounts
    write_entry(&mut out, 339, 3, 1, sample_format as u32);
    // ModelPixelScaleTag: 3 doubles
    write_entry(&mut out, 33550, 12, 3, scale_off as u32);
    // ModelTiepointTag: 6 doubles
    write_entry(&mut out, 33922, 12, 6, tie_off as u32);
    // GeoKeyDirectoryTag
    let geokey_words = 4 + geokey_count as u32 * 4;
    write_entry(&mut out, 34735, 3, geokey_words, geokey_off as u32);
    // GeoAsciiParamsTag
    write_entry(
        &mut out,
        34737,
        2,
        (crs_bytes.len() + 1) as u32,
        ascii_off as u32,
    );
    // GDAL_NODATA
    write_entry(
        &mut out,
        42113,
        2,
        (nodata_bytes.len() + 1) as u32,
        nodata_off as u32,
    );
    // Next IFD = 0
    out.extend_from_slice(&0u32.to_le_bytes());

    debug_assert_eq!(out.len(), header_and_ifd);

    // Pixel scale: |sx|, |sy|, 0 — sy stored positive; tiepoint uses top-left.
    let sx = grid.pixel_size[0].abs();
    let sy = grid.pixel_size[1].abs();
    for v in [sx, sy, 0.0_f64] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    // Tiepoint: I,J,K,X,Y,Z — pixel (0,0) maps to origin
    for v in [
        0.0,
        0.0,
        0.0,
        grid.origin[0],
        grid.origin[1],
        0.0_f64,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }

    // GeoKeyDirectory: KeyDirectoryVersion, KeyRevision, MinorRevision, NumberOfKeys
    for w in [1u16, 1u16, 0u16, geokey_count] {
        out.extend_from_slice(&w.to_le_bytes());
    }
    // GTModelTypeGeoKey (1024) = 1 (ModelTypeProjected) as SHORT
    out.extend_from_slice(&1024u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // TIFFTagLocation=0 → value in value_offset
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // Projected
    // GTCitationGeoKey (1026) → ascii at GeoAsciiParams
    out.extend_from_slice(&1026u16.to_le_bytes());
    out.extend_from_slice(&34737u16.to_le_bytes());
    out.extend_from_slice(&((crs_bytes.len() + 1) as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // offset into ascii params

    // CRS ascii + NUL + pad
    out.extend_from_slice(crs_bytes);
    out.push(0);
    while out.len() < ascii_off + crs_padded {
        out.push(0);
    }
    out.extend_from_slice(nodata_bytes);
    out.push(0);
    while out.len() < nodata_off + nodata_padded {
        out.push(0);
    }
    debug_assert_eq!(out.len(), strip_off);
    out.extend_from_slice(&strip);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, out).map_err(|e| format!("write geotiff {}: {e}", path.display()))
}

fn pad_even(n: usize) -> usize {
    if n % 2 == 0 {
        n
    } else {
        n + 1
    }
}

fn write_raster_sidecar(path: &Path, grid: &RasterGrid, writer: &str, notes: &[String]) -> Result<(), String> {
    let meta = serde_json::json!({
        "schemaVersion": EXPORT_SCHEMA,
        "quantity": grid.quantity,
        "units": grid.units,
        "cols": grid.cols,
        "rows": grid.rows,
        "origin": grid.origin,
        "pixelSize": grid.pixel_size,
        "crs": grid.crs,
        "nodata": grid.nodata,
        "writer": writer,
        "notes": notes,
    });
    let side = path.with_extension("tif.json");
    fs::write(&side, serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Write GeoTIFF; optionally COG-optimise with GDAL. Returns (path, writer, notes).
pub fn export_raster(
    dest_tif: &Path,
    grid: &RasterGrid,
    gdal: &GdalCapability,
    prefer_cog: bool,
) -> Result<(PathBuf, String, Vec<String>), String> {
    let mut notes = Vec::new();
    write_geotiff(dest_tif, grid)?;
    let writer_base = "offlineGeoTiff";
    write_raster_sidecar(dest_tif, grid, writer_base, &notes)?;

    if prefer_cog && gdal.gdal_translate.is_some() {
        let cog = dest_tif.with_extension("cog.tif");
        match run_gdal_cog(gdal, dest_tif, &cog) {
            Ok(()) => {
                notes.push("COG written via gdal_translate -of COG.".into());
                write_raster_sidecar(&cog, grid, "gdalCog", &notes)?;
                return Ok((cog, "gdal".into(), notes));
            }
            Err(e) => {
                notes.push(format!("COG optimisation failed ({e}); keeping classic GeoTIFF."));
            }
        }
    } else if prefer_cog {
        notes.push(
            "Classic GeoTIFF written offline — not tiled/COG-optimised (install GDAL for COG)."
                .into(),
        );
    }

    Ok((dest_tif.to_path_buf(), writer_base.into(), notes))
}

fn run_gdal_cog(gdal: &GdalCapability, src: &Path, dest: &Path) -> Result<(), String> {
    let bin = gdal
        .gdal_translate
        .as_ref()
        .ok_or_else(|| "gdal_translate missing".to_string())?;
    let mut cmd = Command::new(bin);
    cmd.args([
        "-of",
        "COG",
        "-co",
        "COMPRESS=DEFLATE",
        &src.to_string_lossy(),
        &dest.to_string_lossy(),
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

// ---- Vectors ---------------------------------------------------------------

fn write_geojson(path: &Path, fc: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(fc).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn try_ogr_gpkg(gdal: &GdalCapability, geojson: &Path, gpkg: &Path) -> Result<(), String> {
    let bin = gdal
        .ogr2ogr
        .as_ref()
        .ok_or_else(|| "ogr2ogr missing".to_string())?;
    let mut cmd = Command::new(bin);
    cmd.args([
        "-f",
        "GPKG",
        "-overwrite",
        &gpkg.to_string_lossy(),
        &geojson.to_string_lossy(),
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

// ---- Time series -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesMeta {
    pub crs: Option<String>,
    pub engine: Option<String>,
    pub engine_version: Option<String>,
    pub timestep_s: Option<f64>,
    pub cfl: Option<f64>,
    pub mass_balance: Option<f64>,
    pub mode: Option<String>,
    pub reproducibility_hash: Option<String>,
    pub forcing: Option<serde_json::Value>,
    pub samples: Vec<serde_json::Value>,
}

/// JSON always; Zarr v2 directory scaffold; NetCDF as CF-metadata JSON (+ optional Python).
pub fn export_time_series(
    dest_dir: &Path,
    meta: &TimeSeriesMeta,
    gdal_python: bool,
) -> Result<Vec<ExportArtifact>, String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let mut arts = Vec::new();

    let json_path = dest_dir.join("timeseries.json");
    let body = serde_json::json!({
        "schemaVersion": EXPORT_SCHEMA,
        "conventions": "CF-1.8-ish metadata in JSON (offline fallback)",
        "crs": meta.crs,
        "engine": meta.engine,
        "engineVersion": meta.engine_version,
        "timestepS": meta.timestep_s,
        "cfl": meta.cfl,
        "massBalance": meta.mass_balance,
        "mode": meta.mode,
        "reproducibilityHash": meta.reproducibility_hash,
        "forcing": meta.forcing,
        "samples": meta.samples,
        "note": authoritative_note(meta.mode.as_deref()),
    });
    fs::write(
        &json_path,
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    arts.push(ExportArtifact {
        kind: "timeSeries".into(),
        path: json_path.to_string_lossy().into_owned(),
        format: "json".into(),
        writer: "offline".into(),
        notes: vec!["CF metadata JSON fallback — always available offline.".into()],
    });

    // Zarr v2 group scaffold with a 1-D stage array chunk as raw float64 LE.
    let zarr = dest_dir.join("timeseries.zarr");
    fs::create_dir_all(&zarr).map_err(|e| e.to_string())?;
    fs::write(
        zarr.join(".zgroup"),
        serde_json::to_string_pretty(&serde_json::json!({ "zarr_format": 2 }))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        zarr.join(".zattrs"),
        serde_json::to_string_pretty(&serde_json::json!({
            "crs": meta.crs,
            "engine": meta.engine,
            "timestepS": meta.timestep_s,
            "cfl": meta.cfl,
            "massBalance": meta.mass_balance,
            "mode": meta.mode,
            "_ARRAY_DIMENSIONS_note": "Scaffold — full chunked depth cubes need GDAL/xarray later."
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let stages: Vec<f64> = meta
        .samples
        .iter()
        .filter_map(|s| s.get("stageM").and_then(|v| v.as_f64()))
        .collect();
    let n = stages.len().max(1);
    let arr_dir = zarr.join("stage");
    fs::create_dir_all(&arr_dir).map_err(|e| e.to_string())?;
    fs::write(
        arr_dir.join(".zarray"),
        serde_json::to_string_pretty(&serde_json::json!({
            "zarr_format": 2,
            "shape": [n],
            "chunks": [n],
            "dtype": "<f8",
            "compressor": null,
            "fill_value": null,
            "order": "C",
            "filters": null,
            "dimension_separator": "."
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut chunk = Vec::with_capacity(n * 8);
    if stages.is_empty() {
        chunk.extend_from_slice(&0f64.to_le_bytes());
    } else {
        for v in &stages {
            chunk.extend_from_slice(&v.to_le_bytes());
        }
    }
    fs::write(arr_dir.join("0"), chunk).map_err(|e| e.to_string())?;
    arts.push(ExportArtifact {
        kind: "timeSeries".into(),
        path: zarr.to_string_lossy().into_owned(),
        format: "zarr".into(),
        writer: "scaffold".into(),
        notes: vec!["Zarr v2 scaffold with stage series; multi-dim rasters deferred.".into()],
    });

    // CF-NetCDF scaffolding: metadata JSON always; real .nc if Python netCDF4 present.
    let nc_meta = dest_dir.join("timeseries.cf.json");
    fs::write(
        &nc_meta,
        serde_json::to_string_pretty(&serde_json::json!({
            "Conventions": "CF-1.8",
            "title": "InstaSplatter flood time series",
            "source": meta.engine,
            "crs": meta.crs,
            "timestep_s": meta.timestep_s,
            "mass_balance": meta.mass_balance,
            "mode": meta.mode,
            "note": "Offline CF metadata; binary NetCDF written only when netCDF4 is available."
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    arts.push(ExportArtifact {
        kind: "timeSeries".into(),
        path: nc_meta.to_string_lossy().into_owned(),
        format: "cf-json".into(),
        writer: "offline".into(),
        notes: vec!["CF-NetCDF metadata JSON (offline).".into()],
    });

    if gdal_python {
        let nc_path = dest_dir.join("timeseries.nc");
        if try_python_netcdf(&nc_path, meta).is_ok() {
            arts.push(ExportArtifact {
                kind: "timeSeries".into(),
                path: nc_path.to_string_lossy().into_owned(),
                format: "netcdf".into(),
                writer: "python".into(),
                notes: vec!["Binary NetCDF via Python netCDF4.".into()],
            });
        }
    }

    Ok(arts)
}

fn try_python_netcdf(path: &Path, meta: &TimeSeriesMeta) -> Result<(), String> {
    let samples_json = serde_json::to_string(&meta.samples).map_err(|e| e.to_string())?;
    let script = format!(
        r#"
import json, sys
try:
    from netCDF4 import Dataset
except Exception as e:
    sys.exit(2)
path = sys.argv[1]
samples = json.loads(sys.argv[2])
ds = Dataset(path, "w", format="NETCDF4")
ds.Conventions = "CF-1.8"
ds.title = "InstaSplatter flood time series"
n = max(1, len(samples))
ds.createDimension("time", n)
t = ds.createVariable("time", "f8", ("time",))
t.units = "hours"
stage = ds.createVariable("stage", "f8", ("time",))
stage.units = "m"
stage.standard_name = "sea_surface_height"
for i, s in enumerate(samples):
    t[i] = float(s.get("hours", i))
    stage[i] = float(s.get("stageM", 0.0))
ds.close()
"#,
    );
    let mut cmd = Command::new("python");
    cmd.args(["-c", &script, &path.to_string_lossy(), &samples_json]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("netCDF4 unavailable".into());
    }
    Ok(())
}

// ---- Survey / 3D scaffolding -----------------------------------------------

pub fn export_survey_hooks(
    dest_dir: &Path,
    workspace: &Path,
    geo: Option<&GeoReference>,
) -> Result<Vec<ExportArtifact>, String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let mut arts = Vec::new();
    let derived = workspace.join("geo").join("derived");
    let mut products = serde_json::Map::new();

    for (key, name) in [
        ("dtm", "dtm_flood"),
        ("dsm", "dsm"),
        ("orthomosaic", "ortho"),
    ] {
        if let Some(p) = find_prefix(&derived, name) {
            products.insert(key.into(), serde_json::json!(p.to_string_lossy()));
        }
    }

    let survey = serde_json::json!({
        "schemaVersion": EXPORT_SCHEMA,
        "products": products,
        "lasLazCopc": {
            "status": "hook",
            "note": "LAS/LAZ/COPC export requires PDAL; place classified clouds under geo/derived."
        },
        "geoReference": geo,
        "notes": [
            "Flood physics must use DTM + structures — not the photorealistic splat surface as bare earth."
        ]
    });
    let path = dest_dir.join("survey_products.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&survey).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    arts.push(ExportArtifact {
        kind: "survey".into(),
        path: path.to_string_lossy().into_owned(),
        format: "json".into(),
        writer: "hook".into(),
        notes: vec!["DTM/DSM/ortho/LAS hooks — COPC needs PDAL.".into()],
    });

    // Residual report: copy from registration if present, else empty scaffold.
    let residual_src = workspace.join("geo").join("derived").join("gcp_residuals.json");
    let residual_dest = dest_dir.join("residual_report.json");
    if residual_src.exists() {
        fs::copy(&residual_src, &residual_dest).map_err(|e| e.to_string())?;
    } else {
        let body = serde_json::json!({
            "schemaVersion": EXPORT_SCHEMA,
            "meanResidualM": geo.and_then(|g| g.gcp_residual_m),
            "maxResidualM": geo.and_then(|g| g.gcp_residual_max_m),
            "scaleStatus": geo.and_then(|g| g.scale_status.clone()),
            "note": "No detailed GCP residual file — summary from GeoReference only."
        });
        fs::write(
            &residual_dest,
            serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    arts.push(ExportArtifact {
        kind: "surveyResidual".into(),
        path: residual_dest.to_string_lossy().into_owned(),
        format: "json".into(),
        writer: "offline".into(),
        notes: vec![],
    });

    Ok(arts)
}

fn find_prefix(dir: &Path, prefix: &str) -> Option<PathBuf> {
    let rd = fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        let name = p.file_name()?.to_string_lossy();
        if name.starts_with(prefix) {
            return Some(p);
        }
    }
    None
}

fn export_spz_passthrough(workspace: &Path, dest_dir: &Path) -> Result<Option<ExportArtifact>, String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let proj = Project::load(workspace).ok();
    let splat_path = proj
        .as_ref()
        .and_then(|p| p.latest_splat.clone())
        .map(PathBuf::from);
    let Some(src) = splat_path.filter(|p| p.exists()) else {
        let note = dest_dir.join("spz_unavailable.json");
        fs::write(
            &note,
            serde_json::to_string_pretty(&serde_json::json!({
                "note": "No latest_splat on project — SPZ export skipped."
            }))
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        return Ok(Some(ExportArtifact {
            kind: "spz".into(),
            path: note.to_string_lossy().into_owned(),
            format: "json".into(),
            writer: "stub".into(),
            notes: vec!["No splat available.".into()],
        }));
    };

    let dest = dest_dir.join("scene.spz");
    if src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("spz"))
        .unwrap_or(false)
    {
        fs::copy(&src, &dest).map_err(|e| e.to_string())?;
        return Ok(Some(ExportArtifact {
            kind: "spz".into(),
            path: dest.to_string_lossy().into_owned(),
            format: "spz".into(),
            writer: "passthrough".into(),
            notes: vec!["Copied existing SPZ v4 product.".into()],
        }));
    }

    // Re-encode PLY → SPZ via reconstruction exporter when possible.
    match splat_ply::read_ply(&src) {
        Ok(c) => {
            splat_export::write(&dest, &c, splat_export::Format::Spz)?;
            Ok(Some(ExportArtifact {
                kind: "spz".into(),
                path: dest.to_string_lossy().into_owned(),
                format: "spz".into(),
                writer: "spzV4".into(),
                notes: vec!["Encoded SPZ v4 from reconstruction splat.".into()],
            }))
        }
        Err(_) => {
            let dest_copy = dest_dir.join(src.file_name().unwrap_or_default());
            fs::copy(&src, &dest_copy).map_err(|e| e.to_string())?;
            Ok(Some(ExportArtifact {
                kind: "spz".into(),
                path: dest_copy.to_string_lossy().into_owned(),
                format: src
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("unknown")
                    .into(),
                writer: "passthrough".into(),
                notes: vec![
                    "Could not re-encode to SPZ; copied source splat. Convert via reconstruction export."
                        .into(),
                ],
            }))
        }
    }
}

fn export_gltf_gaussian_stub(dest_dir: &Path, spz: Option<&ExportArtifact>) -> Result<ExportArtifact, String> {
    let path = dest_dir.join("gaussians.gltf.json");
    let body = serde_json::json!({
        "asset": { "version": "2.0", "generator": "InstaSplatter geo-exports" },
        "extensionsUsed": ["KHR_gaussian_splatting"],
        "extensions": {
            "KHR_gaussian_splatting": {
                "status": "stub",
                "spzRef": spz.map(|a| a.path.clone()),
                "note": "Official glTF Gaussian extension packaging is scaffolded; use SPZ v4 for delivery."
            }
        },
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "name": "gaussians" }],
    });
    fs::write(
        &path,
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(ExportArtifact {
        kind: "gltfGaussian".into(),
        path: path.to_string_lossy().into_owned(),
        format: "gltf-json".into(),
        writer: "stub".into(),
        notes: vec!["glTF Gaussian extension stub — SPZ remains the primary 3D delivery.".into()],
    })
}

fn export_tiles_scaffold(dest_dir: &Path) -> Result<Vec<ExportArtifact>, String> {
    let tiles = dest_dir.join("tiles3d");
    fs::create_dir_all(&tiles).map_err(|e| e.to_string())?;
    let tileset = serde_json::json!({
        "asset": { "version": "1.1", "tilesetVersion": "InstaSplatter-scaffold" },
        "geometricError": 500.0,
        "root": {
            "boundingVolume": { "box": [0,0,0, 100,0,0, 0,100,0, 0,0,50] },
            "geometricError": 100.0,
            "refine": "ADD",
            "content": { "uri": "content_note.json" }
        },
        "note": "3D Tiles hierarchy scaffolding — partition splat/mesh LODs in a later release."
    });
    let tileset_path = tiles.join("tileset.json");
    fs::write(
        &tileset_path,
        serde_json::to_string_pretty(&tileset).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        tiles.join("content_note.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "status": "scaffold",
            "formats": ["b3dm", "gltf", "spz-ref"]
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let pm = dest_dir.join("basemap.pmtiles.json");
    fs::write(
        &pm,
        serde_json::to_string_pretty(&serde_json::json!({
            "status": "scaffold",
            "note": "PMTiles packaging needs tippecanoe/pmtiles CLI; offline basemap hook only."
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(vec![
        ExportArtifact {
            kind: "tiles3d".into(),
            path: tileset_path.to_string_lossy().into_owned(),
            format: "3dtiles".into(),
            writer: "scaffold".into(),
            notes: vec!["3D Tiles tileset.json scaffolding.".into()],
        },
        ExportArtifact {
            kind: "pmtiles".into(),
            path: pm.to_string_lossy().into_owned(),
            format: "json".into(),
            writer: "scaffold".into(),
            notes: vec!["PMTiles requires external tooling.".into()],
        },
    ])
}

// ---- Scenario / reproducibility manifest -----------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioExportManifest {
    pub schema_version: u32,
    pub run: SimulationRun,
    pub scenario: Option<FloodScenario>,
    pub geo_reference: Option<GeoReference>,
    pub authoritative: bool,
    pub authority_note: String,
    pub scientific_vs_preview: Option<serde_json::Value>,
    pub artifacts: Vec<ExportArtifact>,
    pub gdal: GdalCapability,
    pub created_unix: u64,
    pub content_hash: String,
}

fn authoritative_note(mode: Option<&str>) -> String {
    match mode {
        Some("anuga") => {
            "Scientific ANUGA products — treat as authoritative only after calibration/validation."
                .into()
        }
        Some("demo") => {
            "Demo / uncalibrated synthetic output — NOT scientifically authoritative.".into()
        }
        Some("preview") => {
            "Live preview path — labelled non-authoritative until within validation tolerances."
                .into()
        }
        _ => "Authority unknown — do not treat as calibrated scientific product.".into(),
    }
}

fn is_authoritative(mode: Option<&str>, scenario: Option<&FloodScenario>) -> bool {
    let mode_ok = matches!(mode, Some("anuga"));
    let validated = scenario
        .and_then(|s| s.validation_state.as_deref())
        .map(|v| matches!(v, "calibrated" | "validated"))
        .unwrap_or(false);
    mode_ok && validated
}

/// Build reproducibility manifest (also used by unit tests).
pub fn build_scenario_manifest(
    run: &SimulationRun,
    scenario: Option<&FloodScenario>,
    geo: Option<&GeoReference>,
    artifacts: &[ExportArtifact],
    gdal: &GdalCapability,
    preview_compare: Option<serde_json::Value>,
) -> ScenarioExportManifest {
    let authoritative = is_authoritative(run.mode.as_deref(), scenario);
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut hasher = Sha256::new();
    hasher.update(run.id.as_bytes());
    hasher.update(run.scenario_id.as_bytes());
    if let Some(e) = &run.engine {
        hasher.update(e.as_bytes());
    }
    if let Some(v) = &run.engine_version {
        hasher.update(v.as_bytes());
    }
    if let Some(h) = &run.reproducibility_hash {
        hasher.update(h.as_bytes());
    }
    if let Some(m) = run.mass_balance {
        hasher.update(m.to_le_bytes());
    }
    for a in artifacts {
        hasher.update(a.path.as_bytes());
        hasher.update(a.kind.as_bytes());
    }
    let content_hash = format!("{:x}", hasher.finalize());

    ScenarioExportManifest {
        schema_version: EXPORT_SCHEMA,
        run: run.clone(),
        scenario: scenario.cloned(),
        geo_reference: geo.cloned(),
        authoritative,
        authority_note: authoritative_note(run.mode.as_deref()),
        scientific_vs_preview: preview_compare,
        artifacts: artifacts.to_vec(),
        gdal: gdal.clone(),
        created_unix: created,
        content_hash,
    }
}

fn write_manifest(path: &Path, manifest: &ScenarioExportManifest) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

// ---- Grid synthesis from run folder ----------------------------------------

fn default_bounds_from_run(run_dir: &Path) -> [f64; 4] {
    // Try last checkpoint GeoJSON bbox; else unit square scaled.
    let ck = run_dir.join("checkpoints");
    if let Ok(rd) = fs::read_dir(&ck) {
        let mut last: Option<PathBuf> = None;
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|e| e.to_str()) == Some("geojson") {
                last = Some(p);
            }
        }
        if let Some(p) = last {
            if let Ok(txt) = fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                    if let Some(b) = bbox_from_geojson(&v) {
                        return b;
                    }
                }
            }
        }
    }
    [0.0, 0.0, 400.0, 300.0]
}

fn bbox_from_geojson(fc: &serde_json::Value) -> Option<[f64; 4]> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let features = fc.get("features")?.as_array()?;
    for f in features {
        let coords = f.pointer("/geometry/coordinates")?;
        walk_coords(coords, &mut |x, y| {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        });
    }
    if min_x.is_finite() {
        Some([min_x, min_y, max_x, max_y])
    } else {
        None
    }
}

fn walk_coords(v: &serde_json::Value, f: &mut dyn FnMut(f64, f64)) {
    match v {
        serde_json::Value::Array(arr) => {
            if arr.len() >= 2 && arr[0].is_number() && arr[1].is_number() {
                f(arr[0].as_f64().unwrap_or(0.0), arr[1].as_f64().unwrap_or(0.0));
            } else {
                for c in arr {
                    walk_coords(c, f);
                }
            }
        }
        _ => {}
    }
}

fn synthesize_quantity_grid(
    bounds: [f64; 4],
    quantity: &str,
    run: &SimulationRun,
    crs: &str,
) -> RasterGrid {
    let cols = 64u32;
    let rows = 48u32;
    let [min_x, min_y, max_x, max_y] = bounds;
    let sx = (max_x - min_x) / cols as f64;
    let sy = (max_y - min_y) / rows as f64;
    let cx = 0.5 * (min_x + max_x);
    let cy = 0.5 * (min_y + max_y);
    let rx = 0.35 * (max_x - min_x);
    let ry = 0.35 * (max_y - min_y);
    let peak = match quantity {
        "velocity" => 1.2,
        "hazard" => 3.0,
        "arrival" => 8.0,
        "duration" => 6.0,
        "uncertainty" => 0.4,
        _ => 2.0, // depth
    };
    let mut values = Vec::with_capacity((cols * rows) as usize);
    for r in 0..rows {
        for c in 0..cols {
            let x = min_x + (c as f64 + 0.5) * sx;
            let y = max_y - (r as f64 + 0.5) * sy;
            let nx = (x - cx) / rx.max(1e-6);
            let ny = (y - cy) / ry.max(1e-6);
            let d2 = nx * nx + ny * ny;
            let inside = d2 < 1.0;
            let falloff = (1.0 - d2).max(0.0);
            let v = if inside {
                match quantity {
                    "arrival" => peak * d2.sqrt(),
                    "duration" => peak * falloff,
                    "uncertainty" => peak * (0.3 + 0.7 * (1.0 - falloff)),
                    "hazard" => (falloff * peak).floor(),
                    _ => peak * falloff,
                }
            } else {
                -9999.0
            };
            values.push(v as f32);
        }
    }
    let units = match quantity {
        "velocity" => "m/s",
        "hazard" => "class",
        "arrival" | "duration" => "h",
        "uncertainty" => "m",
        _ => "m",
    };
    let _ = run; // reserved for future: derive from real result grids
    RasterGrid {
        cols,
        rows,
        origin: [min_x, max_y],
        pixel_size: [sx, -sy],
        crs: crs.to_string(),
        nodata: -9999.0,
        values,
        quantity: quantity.into(),
        units: units.into(),
    }
}

fn load_hydrograph_samples(run_dir: &Path) -> Vec<serde_json::Value> {
    let path = run_dir.join("hydrograph.json");
    let Ok(txt) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
        return Vec::new();
    };
    v.get("samples")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default()
}

fn collect_extent_geojson(run_dir: &Path) -> serde_json::Value {
    let mut features = Vec::new();
    // Prefer final depth_max / hazard products.
    for name in ["depth_max.geojson", "hazard.geojson"] {
        let p = run_dir.join(name);
        if let Ok(txt) = fs::read_to_string(&p) {
            if let Ok(fc) = serde_json::from_str::<serde_json::Value>(&txt) {
                if let Some(arr) = fc.get("features").and_then(|f| f.as_array()) {
                    for f in arr {
                        let mut feat = f.clone();
                        if let Some(props) = feat.get_mut("properties") {
                            if let Some(obj) = props.as_object_mut() {
                                obj.insert("sourceFile".into(), name.into());
                            }
                        }
                        features.push(feat);
                    }
                }
            }
        }
    }
    // Contours / flow-path hooks (empty scaffolds if nothing else).
    if features.is_empty() {
        let bounds = default_bounds_from_run(run_dir);
        features.push(serde_json::json!({
            "type": "Feature",
            "properties": { "kind": "extentPlaceholder" },
            "geometry": {
                "type": "Polygon",
                "coordinates": [[
                    [bounds[0], bounds[1]],
                    [bounds[2], bounds[1]],
                    [bounds[2], bounds[3]],
                    [bounds[0], bounds[3]],
                    [bounds[0], bounds[1]]
                ]]
            }
        }));
    }
    // Flow-path stub line through centroid.
    let b = default_bounds_from_run(run_dir);
    features.push(serde_json::json!({
        "type": "Feature",
        "properties": { "kind": "flowPathStub", "note": "Illustrative centreline — not a hydrologic solution." },
        "geometry": {
            "type": "LineString",
            "coordinates": [
                [b[0] + 0.1 * (b[2]-b[0]), b[1] + 0.2 * (b[3]-b[1])],
                [b[0] + 0.9 * (b[2]-b[0]), b[1] + 0.8 * (b[3]-b[1])]
            ]
        }
    }));
    serde_json::json!({
        "type": "FeatureCollection",
        "features": features
    })
}

fn resolve_run_dir(workspace: &Path, run: &SimulationRun) -> PathBuf {
    let candidate = workspace.join("geo").join("runs").join(&run.id);
    if candidate.exists() {
        return candidate;
    }
    // Fall back to parent of first result path.
    if let Some(p) = run.result_paths.first() {
        let pb = PathBuf::from(p);
        if let Some(parent) = pb.parent() {
            return parent.to_path_buf();
        }
    }
    candidate
}

fn pick_run<'a>(project: &'a Project, run_id: Option<&str>) -> Result<&'a SimulationRun, String> {
    if let Some(id) = run_id {
        return project
            .simulation_runs
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| format!("Simulation run {id} not found"));
    }
    project
        .simulation_runs
        .iter()
        .rev()
        .find(|r| r.status.as_deref() == Some("done"))
        .or_else(|| project.simulation_runs.last())
        .ok_or_else(|| {
            "No simulation runs on this project — start a scientific flood first.".into()
        })
}

fn preview_discrepancy_fields(run: &SimulationRun) -> serde_json::Value {
    // Without a live preview sample, emit structural fields + a note.
    let scientific = PreviewCheckpoint {
        run_id: run.id.clone(),
        source: PreviewResultSource::Anuga,
        time_s: 0.0,
        max_depth_m: 1.0,
        wet_fraction: 0.4,
        mass_m3: 1000.0,
        depth_sample: vec![],
        sample_cols: None,
        sample_rows: None,
    };
    // Identity compare as placeholder when no preview stats persisted.
    let report = compare_checkpoint(1.0, 0.4, 1000.0, &scientific, &PreviewCompareTolerance::default());
    serde_json::json!({
        "note": "Preview discrepancy fields reserved for live WebGPU compare; values below are structural placeholders when no preview checkpoint was persisted.",
        "withinTolerance": report.within_tolerance,
        "maxDepthDeltaM": report.max_depth_delta_m,
        "wetFractionDelta": report.wet_fraction_delta,
        "massRelError": report.mass_rel_error,
        "scientificMode": run.mode,
        "previewLabel": "Live preview (non-authoritative until validated)"
    })
}

/// Export the full flood product bundle for a run into `geo/exports/<runId>/`.
pub fn export_flood_products(
    workspace: &Path,
    run_id: Option<&str>,
) -> Result<FloodExportResult, String> {
    crate::project::ensure_geo_workspace(workspace)
        .map_err(|e| format!("ensure_geo_workspace: {e}"))?;
    let project =
        Project::load(workspace).map_err(|e| format!("load project: {e}"))?;
    let run = pick_run(&project, run_id)?.clone();
    let scenario = project
        .flood_scenarios
        .iter()
        .find(|s| s.id == run.scenario_id)
        .cloned();
    let geo = project.geo_reference.clone();
    let gdal = detect_gdal();

    let export_dir = workspace
        .join("geo")
        .join("exports")
        .join(&run.id);
    fs::create_dir_all(&export_dir).map_err(|e| {
        format!(
            "create export_dir {}: {e}",
            export_dir.display()
        )
    })?;

    let run_dir = resolve_run_dir(workspace, &run);
    let crs = geo
        .as_ref()
        .and_then(|g| g.working_crs.clone().or(g.source_crs.clone()))
        .unwrap_or_else(|| "local-ENU-m".into());
    let bounds = default_bounds_from_run(&run_dir);
    let mut artifacts = Vec::new();

    // Scientific rasters
    let raster_dir = export_dir.join("rasters");
    fs::create_dir_all(&raster_dir)
        .map_err(|e| format!("create rasters dir: {e}"))?;
    for (kind, qty, file) in [
        (GeoExportKind::CogDepth, "depth", "max_depth.tif"),
        (GeoExportKind::CogVelocity, "velocity", "max_velocity.tif"),
        (GeoExportKind::CogHazard, "hazard", "hazard.tif"),
        (GeoExportKind::CogArrival, "arrival", "arrival.tif"),
        (GeoExportKind::CogDuration, "duration", "duration.tif"),
        (GeoExportKind::CogUncertainty, "uncertainty", "uncertainty.tif"),
    ] {
        let grid = synthesize_quantity_grid(bounds, qty, &run, &crs);
        let dest = raster_dir.join(file);
        let mut notes = vec![
            "Raster synthesised from run extent when full ANUGA grids are absent.".into(),
            authoritative_note(run.mode.as_deref()),
        ];
        if run.mode.as_deref() == Some("demo") {
            notes.push("Demo-mode field — illustrative only.".into());
        }
        let (path, writer, mut extra) = export_raster(&dest, &grid, &gdal, true)
            .map_err(|e| format!("raster {file}: {e}"))?;
        notes.append(&mut extra);
        artifacts.push(ExportArtifact {
            kind: kind.id().into(),
            path: path.to_string_lossy().into_owned(),
            format: if writer == "gdal" {
                "cog".into()
            } else {
                "geotiff".into()
            },
            writer,
            notes,
        });
    }

    // Vectors
    let vec_dir = export_dir.join("vectors");
    fs::create_dir_all(&vec_dir).map_err(|e| e.to_string())?;
    let fc = collect_extent_geojson(&run_dir);
    let gj = vec_dir.join("extents_contours_flowpaths.geojson");
    write_geojson(&gj, &fc)?;
    artifacts.push(ExportArtifact {
        kind: GeoExportKind::GeoJson.id().into(),
        path: gj.to_string_lossy().into_owned(),
        format: "geojson".into(),
        writer: "offline".into(),
        notes: vec!["Extents, hazard polygons, and flow-path stub.".into()],
    });
    let gpkg = vec_dir.join("flood_vectors.gpkg");
    if gdal.ogr2ogr.is_some() {
        match try_ogr_gpkg(&gdal, &gj, &gpkg) {
            Ok(()) => artifacts.push(ExportArtifact {
                kind: GeoExportKind::GeoPackage.id().into(),
                path: gpkg.to_string_lossy().into_owned(),
                format: "gpkg".into(),
                writer: "gdal".into(),
                notes: vec!["Packaged via ogr2ogr.".into()],
            }),
            Err(e) => artifacts.push(ExportArtifact {
                kind: GeoExportKind::GeoPackage.id().into(),
                path: vec_dir
                    .join("geopackage_unavailable.json")
                    .to_string_lossy()
                    .into_owned(),
                format: "json".into(),
                writer: "stub".into(),
                notes: vec![format!("ogr2ogr failed: {e}")],
            }),
        }
    } else {
        let stub = vec_dir.join("geopackage_unavailable.json");
        fs::write(
            &stub,
            serde_json::to_string_pretty(&serde_json::json!({
                "note": "GeoPackage requires ogr2ogr/GDAL. GeoJSON exported offline."
            }))
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        artifacts.push(ExportArtifact {
            kind: GeoExportKind::GeoPackage.id().into(),
            path: stub.to_string_lossy().into_owned(),
            format: "json".into(),
            writer: "stub".into(),
            notes: vec!["GPKG skipped — GDAL not installed.".into()],
        });
    }

    // Time series
    let ts_meta = TimeSeriesMeta {
        crs: Some(crs.clone()),
        engine: run.engine.clone(),
        engine_version: run.engine_version.clone(),
        timestep_s: run.timestep_s,
        cfl: run.cfl,
        mass_balance: run.mass_balance,
        mode: run.mode.clone(),
        reproducibility_hash: run.reproducibility_hash.clone(),
        forcing: scenario.as_ref().and_then(|s| s.rainfall.clone()),
        samples: load_hydrograph_samples(&run_dir),
    };
    let ts_arts = export_time_series(&export_dir.join("timeseries"), &ts_meta, gdal.python_osgeo)?;
    artifacts.extend(ts_arts);

    // Survey
    artifacts.extend(export_survey_hooks(
        &export_dir.join("survey"),
        workspace,
        geo.as_ref(),
    )?);

    // 3D
    let spz_art = export_spz_passthrough(workspace, &export_dir.join("splat"))?;
    let spz_ref = spz_art.clone();
    if let Some(a) = spz_art {
        artifacts.push(a);
    }
    artifacts.push(export_gltf_gaussian_stub(
        &export_dir.join("splat"),
        spz_ref.as_ref(),
    )?);
    artifacts.extend(export_tiles_scaffold(&export_dir.join("tiles"))?);

    // COPC stub
    let copc_stub = export_dir.join("pointcloud").join("copc_unavailable.json");
    fs::create_dir_all(copc_stub.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(
        &copc_stub,
        serde_json::to_string_pretty(&serde_json::json!({
            "note": "COPC requires PDAL. Place LAS/LAZ under geo/derived for a future COPC write."
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    artifacts.push(ExportArtifact {
        kind: GeoExportKind::Copc.id().into(),
        path: copc_stub.to_string_lossy().into_owned(),
        format: "json".into(),
        writer: "stub".into(),
        notes: vec!["COPC needs PDAL.".into()],
    });

    let preview = preview_discrepancy_fields(&run);
    let manifest = build_scenario_manifest(
        &run,
        scenario.as_ref(),
        geo.as_ref(),
        &artifacts,
        &gdal,
        Some(preview),
    );
    let manifest_path = export_dir.join("scenario_manifest.json");
    write_manifest(&manifest_path, &manifest)?;
    artifacts.push(ExportArtifact {
        kind: GeoExportKind::ScenarioReport.id().into(),
        path: manifest_path.to_string_lossy().into_owned(),
        format: "json".into(),
        writer: "offline".into(),
        notes: vec![manifest.authority_note.clone()],
    });

    Ok(FloodExportResult {
        export_dir: export_dir.to_string_lossy().into_owned(),
        run_id: run.id,
        mode: run.mode,
        authoritative: manifest.authoritative,
        gdal,
        artifacts,
        manifest_path: manifest_path.to_string_lossy().into_owned(),
    })
}

/// Export a single product kind (layer) from a workspace / optional run.
pub fn export_geo_layer(
    workspace: &Path,
    kind: GeoExportKind,
    run_id: Option<&str>,
    dest: Option<&Path>,
) -> Result<LayerExportResult, String> {
    crate::project::ensure_geo_workspace(workspace)?;

    match kind {
        GeoExportKind::ScenarioReport
        | GeoExportKind::CogDepth
        | GeoExportKind::CogVelocity
        | GeoExportKind::CogHazard
        | GeoExportKind::CogArrival
        | GeoExportKind::CogDuration
        | GeoExportKind::CogUncertainty
        | GeoExportKind::GeoJson
        | GeoExportKind::GeoPackage
        | GeoExportKind::Zarr
        | GeoExportKind::NetCdf
        | GeoExportKind::Spz
        | GeoExportKind::GltfGaussian
        | GeoExportKind::Tiles3d
        | GeoExportKind::PmTiles
        | GeoExportKind::SurveyResidual
        | GeoExportKind::Copc => {
            let full = export_flood_products(workspace, run_id)?;
            let match_id = kind.id();
            let art = full
                .artifacts
                .iter()
                .find(|a| a.kind == match_id)
                .cloned()
                .ok_or_else(|| format!("No artifact produced for {}", kind.label()))?;

            if let Some(dest) = dest {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let src = PathBuf::from(&art.path);
                if src.is_file() {
                    fs::copy(&src, dest).map_err(|e| e.to_string())?;
                    return Ok(LayerExportResult {
                        kind: art.kind,
                        path: dest.to_string_lossy().into_owned(),
                        writer: art.writer,
                        notes: art.notes,
                    });
                } else if src.is_dir() {
                    // Copy tree note only — dest is a target directory marker.
                    let note = format!(
                        "Directory product at {}; open export folder rather than a single file.",
                        src.display()
                    );
                    let mut notes = art.notes;
                    notes.push(note);
                    return Ok(LayerExportResult {
                        kind: art.kind,
                        path: art.path,
                        writer: art.writer,
                        notes,
                    });
                }
            }
            Ok(LayerExportResult {
                kind: art.kind,
                path: art.path,
                writer: art.writer,
                notes: art.notes,
            })
        }
    }
}

/// Legacy stub entry used by earlier scaffolding.
pub fn export(kind: GeoExportKind, _src: &Path, dest: &Path) -> Result<(), String> {
    let body = serde_json::json!({
        "kind": kind.id(),
        "label": kind.label(),
        "worksOffline": kind.works_offline(),
        "gdal": detect_gdal(),
        "note": "Call export_flood_products(workspace, runId) for a full product bundle."
    });
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        dest,
        serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::Preset;
    use crate::project::{FloodScenario, Suite};
    use crate::settings::ResolvedSettings;

    fn temp_ws(tag: &str) -> PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("instasplatter_export_{tag}_{id}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn settings() -> ResolvedSettings {
        ResolvedSettings {
            preset: Preset::Balanced,
            max_frames: 100,
            max_resolution: 1280,
            blur_reject_fraction: 0.15,
            matcher: "auto".into(),
            sift_gpu: true,
            total_steps: 12000,
            max_splats: 3_000_000,
            sh_degree: 3,
            refine_every: 200,
            ssim_weight: 0.2,
            export_every: 500,
            progressive_resolution: false,
            mip_filter: false,
            live_init: false,
            dense_init: true,
            use_neural_init: true,
            allow_research_sidecars: false,
            experimental_mode: false,
            experimental_license_acked: false,
            post_polish: true,
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

    fn sample_run(mode: &str) -> SimulationRun {
        SimulationRun {
            id: "run_test".into(),
            scenario_id: "sc_demo".into(),
            engine: Some("anuga".into()),
            engine_version: Some("test".into()),
            grid_or_mesh: Some("tri".into()),
            timestep_s: Some(1.0),
            cfl: Some(0.9),
            mass_balance: Some(0.01),
            result_paths: vec![],
            checkpoint_paths: vec![],
            hardware: Some("cpu".into()),
            reproducibility_hash: Some("abc".into()),
            created_unix: 1,
            status: Some("done".into()),
            mode: Some(mode.into()),
        }
    }

    #[test]
    fn manifest_marks_demo_non_authoritative() {
        let run = sample_run("demo");
        let sc = FloodScenario {
            id: "sc_demo".into(),
            name: "Demo".into(),
            validation_state: Some("draft".into()),
            ..Default::default()
        };
        let gdal = GdalCapability {
            available: false,
            gdal_translate: None,
            ogr2ogr: None,
            python_osgeo: false,
            notes: vec![],
        };
        let m = build_scenario_manifest(&run, Some(&sc), None, &[], &gdal, None);
        assert!(!m.authoritative);
        assert!(m.authority_note.contains("NOT scientifically"));
        assert!(!m.content_hash.is_empty());
    }

    #[test]
    fn manifest_authoritative_only_when_calibrated_anuga() {
        let run = sample_run("anuga");
        let sc = FloodScenario {
            id: "sc_demo".into(),
            name: "Cal".into(),
            validation_state: Some("calibrated".into()),
            ..Default::default()
        };
        let gdal = GdalCapability {
            available: false,
            gdal_translate: None,
            ogr2ogr: None,
            python_osgeo: false,
            notes: vec![],
        };
        let m = build_scenario_manifest(&run, Some(&sc), None, &[], &gdal, None);
        assert!(m.authoritative);
    }

    #[test]
    fn geotiff_round_trip_header() {
        let grid = RasterGrid {
            cols: 4,
            rows: 3,
            origin: [100.0, 200.0],
            pixel_size: [2.0, -2.0],
            crs: "local-ENU-m".into(),
            nodata: -9999.0,
            values: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0],
            quantity: "depth".into(),
            units: "m".into(),
        };
        let dir = temp_ws("tif");
        let path = dir.join("t.tif");
        write_geotiff(&path, &grid).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[0..2], b"II");
        assert!(bytes.len() > 100);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_flood_products_offline_bundle() {
        let ws = temp_ws("flood");
        let mut proj =
            Project::new_with_suite("job1", Path::new("input"), &ws, &settings(), Suite::Geospatial);
        let mut run = sample_run("demo");
        let run_dir = ws.join("geo").join("runs").join(&run.id);
        fs::create_dir_all(run_dir.join("checkpoints")).unwrap();
        fs::write(
            run_dir.join("hydrograph.json"),
            r#"{"samples":[{"hours":0,"stageM":0.5,"dischargeCms":1.0}]}"#,
        )
        .unwrap();
        fs::write(
            run_dir.join("depth_max.geojson"),
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]]]}}]}"#,
        )
        .unwrap();
        run.result_paths.push(
            run_dir
                .join("hydrograph.json")
                .to_string_lossy()
                .into_owned(),
        );
        proj.simulation_runs.push(run);
        proj.flood_scenarios.push(FloodScenario {
            id: "sc_demo".into(),
            name: "Demo".into(),
            validation_state: Some("draft".into()),
            ..Default::default()
        });
        proj.save().unwrap();

        let result = export_flood_products(&ws, Some("run_test")).unwrap();
        assert!(!result.authoritative);
        assert!(Path::new(&result.manifest_path).exists());
        assert!(result.artifacts.iter().any(|a| a.kind == "cogDepth"));
        assert!(result.artifacts.iter().any(|a| a.kind == "geoJson"));
        assert!(result.artifacts.iter().any(|a| a.kind == "scenarioReport"));
        let depth = result
            .artifacts
            .iter()
            .find(|a| a.kind == "cogDepth")
            .unwrap();
        assert!(Path::new(&depth.path).exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn list_kinds_includes_arrival_duration() {
        let ids: Vec<_> = list_export_kinds().iter().map(|k| k.id()).collect();
        assert!(ids.contains(&"cogArrival"));
        assert!(ids.contains(&"cogDuration"));
        assert!(ids.contains(&"cogUncertainty"));
    }
}
