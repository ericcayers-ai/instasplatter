//! Contract test for the `digital-earth-splat-v1` manifest bridge.
//!
//! The producer half of this contract lives in
//! [`instasplatter_lib::splat_contract`] (`export_splat_for_digital_earth`).
//! The consumer half lives in the sibling Digital-Earth repo
//! (`lib/splat_import.py` -> `load_splat_placement`), which reads the JSON
//! manifest, resolves `plyPath` next to the manifest, loads the PLY and
//! applies `enuToEcefMatrix` to place the splats in ECEF metres.
//!
//! What this test guarantees:
//! 1. The exporter writes a manifest carrying exactly the camelCase keys and
//!    units the consumer validates (`format`, `plyPath`, `splatCount`,
//!    `originEcef`, `enuToEcefMatrix`, `units`).
//! 2. The PLY written alongside it is a real 3DGS PLY whose vertex count
//!    matches the manifest's `splatCount` -- the consumer rejects a mismatch.
//! 3. The committed golden fixture the consumer test reads
//!    (`contracts/digital-earth-splat-v1/`) still matches what the exporter
//!    produces today, so the two halves cannot drift apart silently.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use instasplatter_lib::splat_contract::{
    digital_earth_export, transforms, DIGITAL_EARTH_MANIFEST_FORMAT,
};

/// The AOI origin used by the committed fixture (Auckland waterfront).
const FIXTURE_ORIGIN: transforms::Geodetic = transforms::Geodetic {
    lon_deg: 174.7633,
    lat_deg: -36.8485,
    height_m: 20.0,
};

/// Values the consumer (`load_manifest`) requires to be present and typed.
const REQUIRED_MANIFEST_KEYS: [&str; 7] = [
    "format",
    "plyPath",
    "splatCount",
    "origin",
    "originEcef",
    "enuToEcefMatrix",
    "units",
];

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("contracts/digital-earth-splat-v1")
}

fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/contract-fixture");
    fs::create_dir_all(&dir).expect("create scratch fixture dir");
    dir
}

/// A deterministic 3DGS cloud: four splats placed around the AOI origin.
fn fixture_cloud() -> instasplatter_lib::splat_contract::SplatCloud {
    use instasplatter_lib::splat_contract::SplatCloud;

    let positions = [
        [0.0_f32, 0.0, 0.0],
        [12.5, -3.25, 1.75],
        [-40.0, 18.0, -2.5],
        [100.0, 0.0, 5.0],
    ];
    let scales_log = [
        [-1.2_f32, -0.8, -0.5],
        [-0.9, -0.9, -0.9],
        [-2.0, -1.5, -1.0],
        [-0.3, -0.4, -0.35],
    ];
    let rot_wxyz = [
        [1.0_f32, 0.0, 0.0, 0.0],
        [0.7071, 0.7071, 0.0, 0.0],
        [0.5, 0.5, 0.5, 0.5],
        [1.0, 0.0, 0.0, 0.0],
    ];
    let opacity_logit = [2.0_f32, -1.0, 0.0, 6.0];
    let sh_dc = [
        [0.0_f32, 0.0, 0.0],
        [1.5, -1.0, 0.5],
        [-0.75, 0.25, 1.25],
        [0.5, 0.5, 0.5],
    ];

    SplatCloud {
        positions: positions.to_vec(),
        scales_log: scales_log.to_vec(),
        rot_wxyz: rot_wxyz.to_vec(),
        opacity_logit: opacity_logit.to_vec(),
        sh_dc: sh_dc.to_vec(),
        sh_rest: Vec::new(),
        rest_per_channel: 0,
    }
}

/// Parsed-as-generic-JSON view of the manifest, so the test asserts on the
/// exact wire keys rather than on the Rust field names.
fn parse_manifest(path: &Path) -> serde_json::Value {
    let raw = fs::read_to_string(path).expect("manifest should be readable UTF-8");
    serde_json::from_str(&raw).expect("manifest should be valid JSON")
}

#[test]
fn exporter_writes_the_consumer_contract() {
    let dir = scratch_dir();
    let ply_path = dir.join("fixture.ply");
    let manifest_path = dir.join("manifest.json");

    let cloud = fixture_cloud();
    instasplatter_lib::splat_contract::ply::write_ply(&ply_path, &cloud)
        .expect("PLY fixtures should be writable");

    let manifest = digital_earth_export::export_splat_for_digital_earth(
        &ply_path,
        FIXTURE_ORIGIN,
        &manifest_path,
    )
    .expect("exporting a non-empty splat cloud should succeed");

    // --- JSON surface the consumer parses ---------------------------------
    let json = parse_manifest(&manifest_path);
    let keys: BTreeSet<&str> = json
        .as_object()
        .expect("manifest root must be an object")
        .keys()
        .map(String::as_str)
        .collect();
    for required in REQUIRED_MANIFEST_KEYS {
        assert!(
            keys.contains(required),
            "manifest is missing key {required}"
        );
    }

    assert_eq!(json["format"], DIGITAL_EARTH_MANIFEST_FORMAT);
    assert_eq!(json["units"], "meters");
    assert_eq!(json["splatCount"], cloud.len());
    assert_eq!(json["origin"]["lonDeg"], FIXTURE_ORIGIN.lon_deg);
    assert_eq!(json["origin"]["latDeg"], FIXTURE_ORIGIN.lat_deg);
    assert_eq!(json["origin"]["heightM"], FIXTURE_ORIGIN.height_m);

    let matrix: Vec<f64> = json["enuToEcefMatrix"]
        .as_array()
        .expect("enuToEcefMatrix must be a flat array")
        .iter()
        .map(|v| v.as_f64().expect("matrix entries must be numbers"))
        .collect();
    assert_eq!(matrix.len(), 16, "matrix must be a flat row-major 4x4");
    let origin_ecef: Vec<f64> = json["originEcef"]
        .as_array()
        .expect("originEcef must be an array")
        .iter()
        .map(|v| v.as_f64().expect("originEcef entries must be numbers"))
        .collect();
    assert_eq!(origin_ecef.len(), 3);

    // The local ENU origin (0,0,0) must map back onto originEcef: this is the
    // whole placement contract the consumer relies on.
    for row in 0..3 {
        assert!(
            (matrix[row * 4 + 3] - origin_ecef[row]).abs() < 1e-6,
            "row {row} translation must equal originEcef row {row}"
        );
    }

    // --- PLY the consumer loads ------------------------------------------
    let ply_path_in_manifest =
        PathBuf::from(json["plyPath"].as_str().expect("plyPath must be a string"));
    assert!(
        ply_path_in_manifest.is_file(),
        "plyPath must resolve to a real file: {}",
        ply_path_in_manifest.display()
    );

    let read_back = instasplatter_lib::splat_contract::ply::read_ply(&ply_path_in_manifest)
        .expect("the exported PLY must be readable as a 3DGS cloud");
    assert_eq!(
        read_back.len(),
        json["splatCount"].as_u64().unwrap() as usize,
        "PLY vertex count must match the manifest's splatCount"
    );

    // --- what the helper returned ----------------------------------------
    assert_eq!(manifest.format, DIGITAL_EARTH_MANIFEST_FORMAT);
    for (index, (returned, serialized)) in manifest
        .enu_to_ecef_matrix
        .iter()
        .zip(matrix.iter())
        .enumerate()
    {
        let tolerance = f64::EPSILON * returned.abs().max(1.0) * 4.0;
        assert!(
            (returned - serialized).abs() <= tolerance,
            "matrix entry {index} changed beyond JSON float precision: {returned} vs {serialized}"
        );
    }
    assert_eq!(manifest.ply_path, ply_path.to_string_lossy());
}

#[test]
fn committed_fixture_matches_the_current_exporter_contract() {
    let fixture = golden_dir();
    let manifest_path = fixture.join("manifest.json");
    let ply_path = fixture.join("splat.ply");

    assert!(
        manifest_path.is_file(),
        "missing committed fixture {} -- regenerate it with the sibling \
         Digital-Earth consumer before shipping a contract change",
        manifest_path.display()
    );
    assert!(ply_path.is_file(), "missing committed PLY fixture");

    let json = parse_manifest(&manifest_path);
    assert_eq!(json["format"], DIGITAL_EARTH_MANIFEST_FORMAT);
    assert_eq!(json["units"], "meters");
    assert_eq!(json["splatCount"], 4);
    assert_eq!(json["plyPath"], "splat.ply");
    assert_eq!(json["origin"]["lonDeg"], FIXTURE_ORIGIN.lon_deg);
    assert_eq!(json["origin"]["latDeg"], FIXTURE_ORIGIN.lat_deg);
    assert_eq!(json["origin"]["heightM"], FIXTURE_ORIGIN.height_m);

    let cloud = instasplatter_lib::splat_contract::ply::read_ply(&ply_path)
        .expect("the fixture PLY must be a readable 3DGS cloud");
    assert_eq!(cloud.len(), 4);
    assert!(
        cloud.rest_per_channel == 0,
        "fixture PLY is expected to be SH-degree 0"
    );

    // The committed matrix must be the one the exporter produces today for
    // FIXTURE_ORIGIN -- otherwise the consumer test is validating a format
    // the producer no longer emits.
    let frame = transforms::EnuFrame::from_geodetic(FIXTURE_ORIGIN);
    let current = frame.enu_to_ecef_matrix();
    let committed: Vec<f64> = json["enuToEcefMatrix"]
        .as_array()
        .expect("enuToEcefMatrix must be a flat array")
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert_eq!(committed.len(), 16);
    for (i, (a, b)) in committed.iter().zip(current.iter()).enumerate() {
        assert!(
            (a - b).abs() < 1e-9,
            "committed matrix entry {i} drifted from the live exporter: {a} vs {b}"
        );
    }
}

#[test]
#[ignore = "regenerate the committed producer fixture after reviewing the consumer contract"]
fn regenerate_committed_fixture() {
    let fixture = golden_dir();
    fs::create_dir_all(&fixture).expect("create contract fixture directory");

    let ply_path = fixture.join("splat.ply");
    let manifest_path = fixture.join("manifest.json");
    let cloud = fixture_cloud();
    instasplatter_lib::splat_contract::ply::write_ply(&ply_path, &cloud)
        .expect("write contract fixture PLY");

    let frame = transforms::EnuFrame::from_geodetic(FIXTURE_ORIGIN);
    digital_earth_export::write_manifest(&cloud, &frame, Path::new("splat.ply"), &manifest_path)
        .expect("write contract fixture manifest");
}
