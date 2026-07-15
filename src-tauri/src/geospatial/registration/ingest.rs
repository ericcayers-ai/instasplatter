//! Telemetry / GCP file parsers for drone georegistration.

use super::GcpPoint;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// One GPS/IMU sample (covariance retained — never treated as exact).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryPoint {
    pub lon_deg: f64,
    pub lat_deg: f64,
    pub height_m: f64,
    /// Diagonal std (m). Defaults [5,5,10] for consumer GNSS, tighter for RTK.
    pub covariance_m: [f64; 3],
    pub unix_s: Option<f64>,
    pub heading_deg: Option<f64>,
    pub pitch_deg: Option<f64>,
    pub roll_deg: Option<f64>,
    pub focal_mm: Option<f64>,
    pub image_name: Option<String>,
    pub source: String,
}

impl Default for TelemetryPoint {
    fn default() -> Self {
        Self {
            lon_deg: 0.0,
            lat_deg: 0.0,
            height_m: 0.0,
            covariance_m: [5.0, 5.0, 10.0],
            unix_s: None,
            heading_deg: None,
            pitch_deg: None,
            roll_deg: None,
            focal_mm: None,
            image_name: None,
            source: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryFormat {
    Exif,
    DjiSrt,
    DjiMrk,
    Pos,
    Csv,
    Gpx,
    Kml,
    GcpCsv,
    Unknown,
}

impl TelemetryFormat {
    pub fn id(self) -> &'static str {
        match self {
            TelemetryFormat::Exif => "exif",
            TelemetryFormat::DjiSrt => "dji-srt",
            TelemetryFormat::DjiMrk => "dji-mrk",
            TelemetryFormat::Pos => "pos",
            TelemetryFormat::Csv => "csv",
            TelemetryFormat::Gpx => "gpx",
            TelemetryFormat::Kml => "kml",
            TelemetryFormat::GcpCsv => "gcp-csv",
            TelemetryFormat::Unknown => "unknown",
        }
    }

    pub fn from_path(path: &Path) -> TelemetryFormat {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.contains("gcp") && ext == "csv" {
            return TelemetryFormat::GcpCsv;
        }
        match ext.as_str() {
            "srt" => TelemetryFormat::DjiSrt,
            "mrk" => TelemetryFormat::DjiMrk,
            "pos" | "ppk" | "txt" if name.contains("pos") || name.contains("ppk") || ext == "pos" || ext == "ppk" => {
                TelemetryFormat::Pos
            }
            "pos" | "ppk" => TelemetryFormat::Pos,
            "csv" => TelemetryFormat::Csv,
            "gpx" => TelemetryFormat::Gpx,
            "kml" | "kmz" => TelemetryFormat::Kml,
            _ => TelemetryFormat::Unknown,
        }
    }
}

pub fn list_telemetry_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_telemetry(dir, &mut out, 0);
    out.sort();
    out
}

fn walk_telemetry(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "tiles" | "runs" | "exports" | "derived") {
                continue;
            }
            walk_telemetry(&p, out, depth + 1);
            continue;
        }
        let fmt = TelemetryFormat::from_path(&p);
        if !matches!(fmt, TelemetryFormat::Unknown | TelemetryFormat::Exif) {
            out.push(p);
        }
    }
}

pub fn ingest_path(path: &Path) -> Result<(TelemetryFormat, Vec<TelemetryPoint>), String> {
    let fmt = TelemetryFormat::from_path(path);
    let pts = match fmt {
        TelemetryFormat::DjiSrt => parse_dji_srt(path)?,
        TelemetryFormat::DjiMrk => parse_dji_mrk(path)?,
        TelemetryFormat::Pos => parse_pos(path)?,
        TelemetryFormat::Csv => parse_telemetry_csv(path)?,
        TelemetryFormat::Gpx => parse_gpx(path)?,
        TelemetryFormat::Kml => parse_kml(path)?,
        TelemetryFormat::GcpCsv => {
            // GCP CSV is survey marks, not flight telemetry.
            return Ok((fmt, Vec::new()));
        }
        TelemetryFormat::Exif | TelemetryFormat::Unknown => {
            return Err(format!("Unsupported telemetry file: {}", path.display()));
        }
    };
    Ok((fmt, pts))
}

pub fn parse_gcp_csv(path: &Path) -> Result<Vec<GcpPoint>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| "GCP CSV is empty".to_string())?
        .to_ascii_lowercase();
    let cols: Vec<&str> = split_csv_line(&header);
    let idx = |names: &[&str]| {
        cols.iter()
            .position(|c| names.iter().any(|n| c.trim() == *n))
    };
    let id_i = idx(&["id", "name", "label", "gcp"]);
    let lat_i = idx(&["lat", "latitude", "y"]);
    let lon_i = idx(&["lon", "lng", "longitude", "x"]);
    let alt_i = idx(&["alt", "altitude", "height", "z", "elev", "elevation"]);
    let x_i = idx(&["easting", "east"]);
    let y_i = idx(&["northing", "north"]);

    let geographic = lat_i.is_some() && lon_i.is_some();
    let mut out = Vec::new();
    for (row, line) in lines.enumerate() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let cells: Vec<&str> = split_csv_line(line);
        let get = |i: Option<usize>| -> Option<f64> {
            i.and_then(|j| cells.get(j)).and_then(|s| parse_f64(s))
        };
        let id = id_i
            .and_then(|j| cells.get(j).map(|s| s.trim().to_string()))
            .unwrap_or_else(|| format!("gcp{}", row + 1));
        if geographic {
            let lat = get(lat_i).ok_or_else(|| format!("GCP row {row}: missing lat"))?;
            let lon = get(lon_i).ok_or_else(|| format!("GCP row {row}: missing lon"))?;
            let alt = get(alt_i).unwrap_or(0.0);
            // Heuristic: if |lon|>90 and header used x/y as lon/lat swap… keep as-is.
            out.push(GcpPoint {
                id,
                survey_xyz: [lon, lat, alt],
                survey_crs: "EPSG:4326".into(),
                covariance_m: Some([0.02, 0.02, 0.05]),
                ..Default::default()
            });
        } else if let (Some(x), Some(y)) = (get(x_i).or(get(lon_i)), get(y_i).or(get(lat_i))) {
            let z = get(alt_i).unwrap_or(0.0);
            out.push(GcpPoint {
                id,
                survey_xyz: [x, y, z],
                survey_crs: "local-ENU-m".into(),
                covariance_m: Some([0.02, 0.02, 0.05]),
                ..Default::default()
            });
        }
    }
    Ok(out)
}

pub fn read_exif_gps(path: &Path) -> Result<Option<TelemetryPoint>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = BufReader::new(file);
    let exif = match exif::Reader::new().read_from_container(&mut buf) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };

    let lat = dms_to_deg(&exif, exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef)?;
    let lon = dms_to_deg(&exif, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef)?;
    let (Some(lat), Some(lon)) = (lat, lon) else {
        return Ok(None);
    };

    let alt = exif
        .get_field(exif::Tag::GPSAltitude, exif::In::PRIMARY)
        .and_then(|f| rational_to_f64(&f.value))
        .unwrap_or(0.0);
    let alt_ref = exif
        .get_field(exif::Tag::GPSAltitudeRef, exif::In::PRIMARY)
        .map(|f| format!("{}", f.display_value().with_unit(&exif)))
        .unwrap_or_default();
    let height = if alt_ref.contains('1') || alt_ref.to_ascii_lowercase().contains("below") {
        -alt
    } else {
        alt
    };

    let heading = exif
        .get_field(exif::Tag::GPSImgDirection, exif::In::PRIMARY)
        .and_then(|f| rational_to_f64(&f.value))
        .or_else(|| {
            exif.get_field(exif::Tag::GPSTrack, exif::In::PRIMARY)
                .and_then(|f| rational_to_f64(&f.value))
        });

    let focal = exif
        .get_field(exif::Tag::FocalLength, exif::In::PRIMARY)
        .and_then(|f| rational_to_f64(&f.value));

    let unix_s = read_exif_unix(path).ok().flatten();

    // Gravity / accelerometer orientation is camera-maker specific; capture when present as XMP later.
    Ok(Some(TelemetryPoint {
        lon_deg: lon,
        lat_deg: lat,
        height_m: height,
        covariance_m: [3.0, 3.0, 8.0],
        unix_s,
        heading_deg: heading,
        focal_mm: focal,
        image_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned()),
        source: "exif".into(),
        ..Default::default()
    }))
}

pub fn read_exif_unix(path: &Path) -> Result<Option<f64>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = BufReader::new(file);
    let exif = match exif::Reader::new().read_from_container(&mut buf) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    for tag in [
        exif::Tag::DateTimeOriginal,
        exif::Tag::DateTimeDigitized,
        exif::Tag::DateTime,
    ] {
        if let Some(field) = exif.get_field(tag, exif::In::PRIMARY) {
            let s = field.display_value().to_string().replace('\'', "");
            if let Some(t) = parse_exif_datetime(&s) {
                return Ok(Some(t));
            }
        }
    }
    Ok(None)
}

fn dms_to_deg(
    exif: &exif::Exif,
    tag: exif::Tag,
    ref_tag: exif::Tag,
) -> Result<Option<f64>, String> {
    let Some(field) = exif.get_field(tag, exif::In::PRIMARY) else {
        return Ok(None);
    };
    let Some(dms) = dms_rationals(&field.value) else {
        return Ok(None);
    };
    let mut deg = dms[0] + dms[1] / 60.0 + dms[2] / 3600.0;
    if let Some(r) = exif.get_field(ref_tag, exif::In::PRIMARY) {
        let s = r.display_value().to_string().to_ascii_uppercase();
        if s.contains('S') || s.contains('W') {
            deg = -deg;
        }
    }
    Ok(Some(deg))
}

fn dms_rationals(value: &exif::Value) -> Option<[f64; 3]> {
    match value {
        exif::Value::Rational(v) if v.len() >= 3 => Some([
            v[0].num as f64 / v[0].denom.max(1) as f64,
            v[1].num as f64 / v[1].denom.max(1) as f64,
            v[2].num as f64 / v[2].denom.max(1) as f64,
        ]),
        _ => None,
    }
}

fn rational_to_f64(value: &exif::Value) -> Option<f64> {
    match value {
        exif::Value::Rational(v) if !v.is_empty() => {
            Some(v[0].num as f64 / v[0].denom.max(1) as f64)
        }
        exif::Value::Ascii(v) if !v.is_empty() => {
            String::from_utf8_lossy(&v[0]).trim().parse().ok()
        }
        _ => None,
    }
}

/// DJI subtitle telemetry (HTML-ish key/values inside SRT cues).
pub fn parse_dji_srt(path: &Path) -> Result<Vec<TelemetryPoint>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let lat = extract_f64(block, &["latitude:", "[latitude:", "lat:"]);
        let lon = extract_f64(block, &["longitude:", "[longitude:", "lon:", "lng:"]);
        let abs_alt = extract_f64(block, &["abs_alt:", "abs alt:", "[abs_alt:"]);
        let rel_alt = extract_f64(block, &["rel_alt:", "rel alt:", "[rel_alt:"]);
        let height = abs_alt.or(rel_alt);
        let (Some(lat), Some(lon), Some(height)) = (lat, lon, height) else {
            continue;
        };
        let heading = extract_f64(block, &["gb_yaw:", "yaw:", "[gb_yaw:"]);
        let unix_s = extract_datetime_unix(block);
        let rtk = block.to_ascii_lowercase().contains("rtk");
        let cov = if rtk {
            [0.05, 0.05, 0.1]
        } else {
            [2.0, 2.0, 5.0]
        };
        out.push(TelemetryPoint {
            lon_deg: lon,
            lat_deg: lat,
            height_m: height,
            covariance_m: cov,
            unix_s,
            heading_deg: heading,
            source: "dji-srt".into(),
            ..Default::default()
        });
    }
    if out.is_empty() {
        return Err("No DJI SRT GPS cues parsed".into());
    }
    Ok(out)
}

/// DJI MRK / RTK marker file (tab or comma separated).
pub fn parse_dji_mrk(path: &Path) -> Result<Vec<TelemetryPoint>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cells: Vec<&str> = if line.contains('\t') {
            line.split('\t').collect()
        } else {
            split_csv_line(line)
        };
        // Common: idx time lat lon ellip_h stdN stdE stdH …
        // or: idx time lon lat …
        let nums: Vec<f64> = cells.iter().filter_map(|c| parse_f64(c)).collect();
        if nums.len() < 4 {
            // Try lat/lon as textual fields at fixed positions.
            if cells.len() >= 5 {
                let lat = parse_f64(cells.get(2).unwrap_or(&"")).or_else(|| parse_coord_token(cells[2]));
                let lon = parse_f64(cells.get(3).unwrap_or(&"")).or_else(|| parse_coord_token(cells[3]));
                let h = parse_f64(cells.get(4).unwrap_or(&""));
                if let (Some(lat), Some(lon), Some(h)) = (lat, lon, h) {
                    let std_n = cells.get(5).and_then(|c| parse_f64(c)).unwrap_or(0.05);
                    let std_e = cells.get(6).and_then(|c| parse_f64(c)).unwrap_or(0.05);
                    let std_u = cells.get(7).and_then(|c| parse_f64(c)).unwrap_or(0.1);
                    out.push(TelemetryPoint {
                        lon_deg: lon,
                        lat_deg: lat,
                        height_m: h,
                        covariance_m: [std_e.abs(), std_n.abs(), std_u.abs()],
                        unix_s: cells.get(1).and_then(|c| parse_loose_time(c)),
                        source: "dji-mrk".into(),
                        ..Default::default()
                    });
                }
            }
            continue;
        }
        // Heuristic: latitude is in [-90,90], longitude may be larger.
        let (lat, lon, h) = if nums[1].abs() <= 90.0 && nums[2].abs() <= 180.0 {
            (nums[1], nums[2], nums[3])
        } else if nums[0].abs() <= 90.0 {
            (nums[0], nums[1], nums[2])
        } else {
            continue;
        };
        let std_e = nums.get(4).copied().unwrap_or(0.05).abs().max(0.01);
        let std_n = nums.get(5).copied().unwrap_or(0.05).abs().max(0.01);
        let std_u = nums.get(6).copied().unwrap_or(0.1).abs().max(0.01);
        out.push(TelemetryPoint {
            lon_deg: lon,
            lat_deg: lat,
            height_m: h,
            covariance_m: [std_e, std_n, std_u],
            source: "dji-mrk".into(),
            ..Default::default()
        });
    }
    if out.is_empty() {
        return Err("No MRK samples parsed".into());
    }
    Ok(out)
}

/// Generic POS / PPK text (space or comma): time lat lon height [std…]
pub fn parse_pos(path: &Path) -> Result<Vec<TelemetryPoint>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('%') || line.starts_with('#') {
            continue;
        }
        let cells: Vec<&str> = line
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .collect();
        if cells.len() < 4 {
            continue;
        }
        // Skip date+time columns: find first lat-like number.
        let mut start = 0usize;
        while start + 2 < cells.len() {
            if let (Some(a), Some(b)) = (parse_f64(cells[start]), parse_f64(cells[start + 1])) {
                if a.abs() <= 90.0 && b.abs() <= 180.0 {
                    break;
                }
                if b.abs() <= 90.0 && a.abs() <= 180.0 {
                    break;
                }
            }
            start += 1;
            if start > 4 {
                break;
            }
        }
        if start + 2 >= cells.len() {
            continue;
        }
        let a = parse_f64(cells[start]);
        let b = parse_f64(cells[start + 1]);
        let h = cells.get(start + 2).and_then(|c| parse_f64(c));
        let (Some(a), Some(b), Some(h)) = (a, b, h) else {
            continue;
        };
        let (lat, lon) = if a.abs() <= 90.0 && b.abs() <= 180.0 {
            (a, b)
        } else {
            (b, a)
        };
        let std_e = cells
            .get(start + 3)
            .and_then(|c| parse_f64(c))
            .unwrap_or(0.1)
            .abs()
            .max(0.02);
        let std_n = cells
            .get(start + 4)
            .and_then(|c| parse_f64(c))
            .unwrap_or(std_e)
            .abs()
            .max(0.02);
        let std_u = cells
            .get(start + 5)
            .and_then(|c| parse_f64(c))
            .unwrap_or(std_e * 2.0)
            .abs()
            .max(0.02);
        let unix_s = parse_loose_time(cells[0])
            .or_else(|| {
                if cells.len() > 1 {
                    parse_loose_time(&format!("{} {}", cells[0], cells[1]))
                } else {
                    None
                }
            });
        out.push(TelemetryPoint {
            lon_deg: lon,
            lat_deg: lat,
            height_m: h,
            covariance_m: [std_e, std_n, std_u],
            unix_s,
            source: "pos".into(),
            ..Default::default()
        });
    }
    if out.is_empty() {
        return Err("No POS samples parsed".into());
    }
    Ok(out)
}

pub fn parse_telemetry_csv(path: &Path) -> Result<Vec<TelemetryPoint>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| "CSV is empty".to_string())?
        .to_ascii_lowercase();
    let cols: Vec<&str> = split_csv_line(&header);
    let idx = |names: &[&str]| {
        cols.iter()
            .position(|c| names.iter().any(|n| c.trim() == *n))
    };
    let lat_i = idx(&["lat", "latitude"]).ok_or("CSV missing lat column")?;
    let lon_i = idx(&["lon", "lng", "longitude"]).ok_or("CSV missing lon column")?;
    let alt_i = idx(&["alt", "altitude", "height", "elev", "elevation", "z"]);
    let time_i = idx(&["time", "timestamp", "unix", "gps_time", "datetime"]);
    let std_h_i = idx(&["std_h", "h_acc", "horizontal_accuracy", "accuracy"]);
    let std_v_i = idx(&["std_v", "v_acc", "vertical_accuracy"]);
    let yaw_i = idx(&["yaw", "heading", "course"]);

    let mut out = Vec::new();
    for line in lines {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let cells = split_csv_line(line);
        let lat = cells.get(lat_i).and_then(|c| parse_f64(c));
        let lon = cells.get(lon_i).and_then(|c| parse_f64(c));
        let (Some(lat), Some(lon)) = (lat, lon) else {
            continue;
        };
        let height = alt_i
            .and_then(|i| cells.get(i))
            .and_then(|c| parse_f64(c))
            .unwrap_or(0.0);
        let std_h = std_h_i
            .and_then(|i| cells.get(i))
            .and_then(|c| parse_f64(c))
            .unwrap_or(2.0)
            .abs()
            .max(0.05);
        let std_v = std_v_i
            .and_then(|i| cells.get(i))
            .and_then(|c| parse_f64(c))
            .unwrap_or(std_h * 2.0)
            .abs()
            .max(0.05);
        let unix_s = time_i
            .and_then(|i| cells.get(i))
            .and_then(|c| parse_loose_time(c));
        let heading = yaw_i
            .and_then(|i| cells.get(i))
            .and_then(|c| parse_f64(c));
        out.push(TelemetryPoint {
            lon_deg: lon,
            lat_deg: lat,
            height_m: height,
            covariance_m: [std_h, std_h, std_v],
            unix_s,
            heading_deg: heading,
            source: "csv".into(),
            ..Default::default()
        });
    }
    if out.is_empty() {
        return Err("No CSV GPS rows parsed".into());
    }
    Ok(out)
}

pub fn parse_gpx(path: &Path) -> Result<Vec<TelemetryPoint>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (i, _) in text.match_indices("<trkpt") {
        let rest = &text[i..];
        let end = rest.find(">").unwrap_or(rest.len().min(200));
        let attrs = &rest[..end];
        let lat = attr_f64(attrs, "lat");
        let lon = attr_f64(attrs, "lon");
        let (Some(lat), Some(lon)) = (lat, lon) else {
            continue;
        };
        let chunk_end = rest.find("</trkpt>").unwrap_or(512.min(rest.len()));
        let chunk = &rest[..chunk_end];
        let height = xml_tag_f64(chunk, "ele").unwrap_or(0.0);
        let unix_s = xml_tag_str(chunk, "time").and_then(|s| parse_loose_time(&s));
        out.push(TelemetryPoint {
            lon_deg: lon,
            lat_deg: lat,
            height_m: height,
            covariance_m: [3.0, 3.0, 8.0],
            unix_s,
            source: "gpx".into(),
            ..Default::default()
        });
    }
    // Also waypoints.
    for (i, _) in text.match_indices("<wpt") {
        let rest = &text[i..];
        let end = rest.find(">").unwrap_or(rest.len().min(200));
        let attrs = &rest[..end];
        let lat = attr_f64(attrs, "lat");
        let lon = attr_f64(attrs, "lon");
        let (Some(lat), Some(lon)) = (lat, lon) else {
            continue;
        };
        let chunk_end = rest.find("</wpt>").unwrap_or(512.min(rest.len()));
        let chunk = &rest[..chunk_end];
        let height = xml_tag_f64(chunk, "ele").unwrap_or(0.0);
        out.push(TelemetryPoint {
            lon_deg: lon,
            lat_deg: lat,
            height_m: height,
            covariance_m: [2.0, 2.0, 5.0],
            source: "gpx".into(),
            ..Default::default()
        });
    }
    if out.is_empty() {
        return Err("No GPX track points parsed".into());
    }
    Ok(out)
}

pub fn parse_kml(path: &Path) -> Result<Vec<TelemetryPoint>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (i, _) in text.match_indices("<coordinates>") {
        let rest = &text[i + "<coordinates>".len()..];
        let end = rest.find("</coordinates>").unwrap_or(rest.len().min(50_000));
        let body = rest[..end].trim();
        for token in body.split(|c: char| c.is_whitespace()) {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            let parts: Vec<&str> = token.split(',').collect();
            if parts.len() < 2 {
                continue;
            }
            let lon = parse_f64(parts[0]);
            let lat = parse_f64(parts[1]);
            let h = parts.get(2).and_then(|c| parse_f64(c)).unwrap_or(0.0);
            let (Some(lon), Some(lat)) = (lon, lat) else {
                continue;
            };
            out.push(TelemetryPoint {
                lon_deg: lon,
                lat_deg: lat,
                height_m: h,
                covariance_m: [5.0, 5.0, 10.0],
                source: "kml".into(),
                ..Default::default()
            });
        }
    }
    if out.is_empty() {
        return Err("No KML coordinates parsed".into());
    }
    Ok(out)
}

pub fn parse_time_from_name(stem: &str) -> Option<f64> {
    // DJI_YYYYMMDDHHMMSS or *_YYYYMMDD_HHMMSS*
    let digits: String = stem.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 14 {
        let y: i32 = digits[0..4].parse().ok()?;
        let mo: u32 = digits[4..6].parse().ok()?;
        let d: u32 = digits[6..8].parse().ok()?;
        let h: u32 = digits[8..10].parse().ok()?;
        let mi: u32 = digits[10..12].parse().ok()?;
        let s: u32 = digits[12..14].parse().ok()?;
        return civil_to_unix(y, mo, d, h, mi, s as f64);
    }
    None
}

fn extract_f64(text: &str, keys: &[&str]) -> Option<f64> {
    let lower = text.to_ascii_lowercase();
    for key in keys {
        let k = key.to_ascii_lowercase();
        if let Some(i) = lower.find(&k) {
            let after = &text[i + key.len()..];
            let num: String = after
                .chars()
                .skip_while(|c| !c.is_ascii_digit() && *c != '-' && *c != '.')
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e' || *c == 'E')
                .collect();
            if let Ok(v) = num.parse::<f64>() {
                return Some(v);
            }
        }
    }
    None
}

fn extract_datetime_unix(text: &str) -> Option<f64> {
    // 2023-01-01 12:00:00.000 or ISO
    for line in text.lines() {
        if let Some(t) = parse_loose_time(line.trim()) {
            return Some(t);
        }
    }
    None
}

fn parse_coord_token(s: &str) -> Option<f64> {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-' )
        .collect();
    parse_f64(&cleaned)
}

fn parse_f64(s: &str) -> Option<f64> {
    let t = s.trim().trim_matches(|c| c == '"' || c == '\'');
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok()
}

fn split_csv_line(line: &str) -> Vec<&str> {
    // Simple split — good enough for survey CSVs without embedded commas in quotes.
    line.split(',').collect()
}

fn attr_f64(attrs: &str, name: &str) -> Option<f64> {
    let key = format!("{name}=\"");
    let i = attrs.find(&key)?;
    let rest = &attrs[i + key.len()..];
    let end = rest.find('"')?;
    parse_f64(&rest[..end])
}

fn xml_tag_f64(chunk: &str, tag: &str) -> Option<f64> {
    xml_tag_str(chunk, tag).and_then(|s| parse_f64(&s))
}

fn xml_tag_str(chunk: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let i = chunk.find(&open)?;
    let rest = &chunk[i + open.len()..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim().to_string())
}

fn parse_loose_time(s: &str) -> Option<f64> {
    let t = s.trim().trim_matches('"');
    if let Ok(v) = t.parse::<f64>() {
        // Heuristic: unix seconds / millis
        if v > 1.0e11 {
            return Some(v / 1000.0);
        }
        if v > 1.0e9 {
            return Some(v);
        }
    }
    // ISO / EXIF: YYYY-MM-DDTHH:MM:SS or YYYY:MM:DD HH:MM:SS
    let cleaned = t.replace('T', " ").replace('Z', "");
    let cleaned = cleaned.replace('/', "-");
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.len() >= 2 {
        let date = parts[0].replace(':', "-");
        let time = parts[1];
        let dp: Vec<&str> = date.split('-').collect();
        let tp: Vec<&str> = time.split(':').collect();
        if dp.len() == 3 && tp.len() >= 2 {
            let y: i32 = dp[0].parse().ok()?;
            let mo: u32 = dp[1].parse().ok()?;
            let d: u32 = dp[2].parse().ok()?;
            let h: u32 = tp[0].parse().ok()?;
            let mi: u32 = tp[1].parse().ok()?;
            let sec: f64 = tp.get(2).and_then(|s| parse_f64(s)).unwrap_or(0.0);
            return civil_to_unix(y, mo, d, h, mi, sec);
        }
    }
    parse_exif_datetime(t)
}

fn parse_exif_datetime(s: &str) -> Option<f64> {
    // "YYYY:MM:DD HH:MM:SS"
    let t = s.trim().trim_matches('"').trim_matches('\'');
    let bytes = t.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let y: i32 = t[0..4].parse().ok()?;
    let mo: u32 = t[5..7].parse().ok()?;
    let d: u32 = t[8..10].parse().ok()?;
    let h: u32 = t[11..13].parse().ok()?;
    let mi: u32 = t[14..16].parse().ok()?;
    let sec: f64 = t[17..19].parse().ok()?;
    civil_to_unix(y, mo, d, h, mi, sec)
}

/// Civil UTC → unix seconds (approximate; good enough for frame matching).
fn civil_to_unix(y: i32, month: u32, day: u32, hour: u32, min: u32, sec: f64) -> Option<f64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut y = y;
    let mut m = month as i32;
    if m <= 2 {
        y -= 1;
        m += 12;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = ((153 * (m as u64 - 3) + 2) / 5 + day as u64 - 1) as u64;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era as i64 * 146_097 + doe as i64 - 719_468;
    let whole = days * 86_400 + hour as i64 * 3600 + min as i64 * 60;
    Some(whole as f64 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_simple_gpx() {
        let dir = std::env::temp_dir().join(format!("is_gpx_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.gpx");
        let mut f = fs::File::create(&p).unwrap();
        write!(
            f,
            r#"<?xml version="1.0"?>
<gpx><trk><trkseg>
<trkpt lat="-36.85" lon="174.76"><ele>40</ele><time>2024-01-01T12:00:00Z</time></trkpt>
</trkseg></trk></gpx>"#
        )
        .unwrap();
        let pts = parse_gpx(&p).unwrap();
        assert_eq!(pts.len(), 1);
        assert!((pts[0].lat_deg + 36.85).abs() < 1e-6);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_dji_srt_sample() {
        let dir = std::env::temp_dir().join(format!("is_srt_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("flight.SRT");
        let mut f = fs::File::create(&p).unwrap();
        write!(
            f,
            "1\n00:00:00,000 --> 00:00:00,033\n\
             <font size=\"28\">FrameCnt: 1, DiffTime: 33ms\n\
             2024-06-01 12:00:00.000\n\
             [latitude: -36.848460] [longitude: 174.763332] [rel_alt: 42.1 abs_alt: 58.4] [gb_yaw: 12.5]\n\
             </font>\n\n\
             2\n00:00:00,033 --> 00:00:00,066\n\
             <font size=\"28\">FrameCnt: 2\n\
             2024-06-01 12:00:00.033\n\
             [latitude: -36.848500] [longitude: 174.763400] [rel_alt: 42.2 abs_alt: 58.5] [gb_yaw: 13.0] RTK\n\
             </font>\n"
        )
        .unwrap();
        let pts = parse_dji_srt(&p).unwrap();
        assert_eq!(pts.len(), 2);
        assert!((pts[0].lat_deg + 36.84846).abs() < 1e-5);
        assert!((pts[0].lon_deg - 174.763332).abs() < 1e-5);
        assert!((pts[0].height_m - 58.4).abs() < 1e-3);
        assert_eq!(pts[0].source, "dji-srt");
        assert!(pts[1].covariance_m[0] < 0.1); // RTK tighter
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_dji_mrk_sample() {
        let dir = std::env::temp_dir().join(format!("is_mrk_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("markers.MRK");
        let mut f = fs::File::create(&p).unwrap();
        writeln!(
            f,
            "1\t12:00:00.000\t-36.848460\t174.763332\t58.400\t0.040\t0.050\t0.080"
        )
        .unwrap();
        writeln!(
            f,
            "2\t12:00:01.000\t-36.848500\t174.763400\t58.500\t0.041\t0.051\t0.081"
        )
        .unwrap();
        let pts = parse_dji_mrk(&p).unwrap();
        assert_eq!(pts.len(), 2);
        assert!((pts[0].lat_deg + 36.84846).abs() < 1e-5);
        assert!((pts[0].lon_deg - 174.763332).abs() < 1e-5);
        assert!((pts[0].height_m - 58.4).abs() < 1e-3);
        assert_eq!(pts[0].source, "dji-mrk");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn civil_unix_known_epoch() {
        let t = civil_to_unix(1970, 1, 1, 0, 0, 0.0).unwrap();
        assert!((t - 0.0).abs() < 1.0);
    }
}
