//! DEM conditioning and flood-ready terrain products.
//!
//! Stages a DTM for scientific / demo flood runs. Full pyflwdir/Landlab
//! conditioning lands with the GIS workers; this module always produces a
//! usable `DemProduct` descriptor so hydro can proceed.

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
    let dest_dir = workspace.join("geo").join("derived");
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

    if let Some(src) = source {
        if src.exists() {
            return condition_dem(src, &dest_dir);
        }
    }

    // Prefer an already-imported source under geo/sources.
    if let Some(found) = find_dem_candidate(workspace) {
        return condition_dem(&found, &dest_dir);
    }

    // Synthetic scaffold — honest placeholder for demo mode.
    let stub = dest_dir.join("dtm_flood_stub.json");
    let body = serde_json::json!({
        "kind": "syntheticDemStub",
        "cellSizeM": cell_size_m,
        "crs": crs.unwrap_or("local-ENU-m"),
        "note": "No source DEM — synthetic stub for demo flood extents only.",
        "boundsEnu": [0.0, 0.0, 400.0, 300.0],
        "zMeanM": 10.0,
    });
    fs::write(&stub, serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    Ok(DemProduct {
        dtm_path: Some(stub.to_string_lossy().into_owned()),
        dsm_path: None,
        orthomosaic_path: None,
        cell_size_m: Some(cell_size_m),
        crs: Some(crs.unwrap_or("local-ENU-m").to_string()),
        synthetic: true,
        notes: vec![
            "No source DEM found — using synthetic stub (not for authoritative scientific runs)."
                .into(),
        ],
    })
}

/// Condition a source DEM into flood-ready DTM products.
pub fn condition_dem(source: &Path, dest_dir: &Path) -> Result<DemProduct, String> {
    if !source.exists() {
        return Err(format!("DEM source not found: {}", source.display()));
    }
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("tif")
        .to_ascii_lowercase();
    let dest = dest_dir.join(format!("dtm_flood.{ext}"));
    fs::copy(source, &dest).map_err(|e| format!("Failed to stage DEM: {e}"))?;

    // Sidecar metadata for hydro / mesh planner.
    let meta = dest_dir.join("dtm_flood.meta.json");
    let meta_body = serde_json::json!({
        "source": source.to_string_lossy(),
        "staged": dest.to_string_lossy(),
        "conditioned": false,
        "note": "pyflwdir/Landlab conditioning not yet applied — staged copy only.",
    });
    fs::write(&meta, serde_json::to_string_pretty(&meta_body).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    Ok(DemProduct {
        dtm_path: Some(dest.to_string_lossy().into_owned()),
        dsm_path: None,
        orthomosaic_path: None,
        cell_size_m: None,
        crs: None,
        synthetic: false,
        notes: vec!["DEM staged for flood use (conditioning deferred).".into()],
    })
}

fn find_dem_candidate(workspace: &Path) -> Option<PathBuf> {
    let roots = [
        workspace.join("geo").join("sources"),
        workspace.join("geo").join("derived"),
    ];
    let exts = ["tif", "tiff", "geotiff", "cog"];
    for root in &roots {
        let Ok(rd) = fs::read_dir(root) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_file() {
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
                return Some(p);
            }
        }
    }
    None
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
        p
    }

    #[test]
    fn synthetic_dem_when_missing() {
        let ws = temp_ws();
        let prod = prepare_flood_dem(&ws, None, 2.0, Some("local-ENU-m")).unwrap();
        assert!(prod.synthetic);
        assert!(prod.dtm_path.as_ref().unwrap().contains("dtm_flood_stub"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn stages_existing_tif() {
        let ws = temp_ws();
        let src = ws.join("geo").join("sources").join("site.tif");
        fs::write(&src, b"fake-tif").unwrap();
        let prod = prepare_flood_dem(&ws, Some(&src), 2.0, None).unwrap();
        assert!(!prod.synthetic);
        assert!(Path::new(prod.dtm_path.as_ref().unwrap()).exists());
        let _ = fs::remove_dir_all(&ws);
    }
}
