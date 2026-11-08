//! Contract test for the `realitylog-capture-v1` receipt bridge.
//!
//! The producer half lives in [`instasplatter_lib::splat_contract::realitylog_export`].
//! The consumer of a receipt is the operator (and any future audit tool): it
//! reads the JSON and can re-verify every claim in it — the source path and
//! HEAD, the exact `argv`, the exit code, the resolved engine paths with their
//! pinned SHA-256s, and the `sha256` of the splat that was produced.
//!
//! What this test guarantees:
//! 1. The producer emits exactly the documented camelCase keys, with the engine
//!    digests taken from the pins rather than re-typed.
//! 2. The committed golden fixture under `contracts/realitylog-capture-v1/`
//!    still matches what the producer emits today for the same facts, and the
//!    fixture's `splatSha256` really is the digest of the fixture's own
//!    `splat.ply` — so the two halves of the fixture cannot drift apart.
//! 3. The receipt carries no secret and no source-tree content.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use instasplatter_lib::splat_contract::realitylog::CaptureSource;
use instasplatter_lib::splat_contract::realitylog_export::{
    build_receipt, read_receipt, write_receipt, ReceiptFacts, REALITYLOG_RECEIPT_FORMAT,
    REALITYLOG_SEAM_VERSION,
};
use instasplatter_lib::splat_contract::{
    build_engines_manifest_in, ply, EnginesManifestRef, SplatCloud,
};

/// The flow the fixture names: a real past test run's source state.
const FIXTURE_SOURCE_ROOT: &str = "C:/Programs/hyperscape-lite";
const FIXTURE_SOURCE_HEAD: &str = "d2c8c0a8e6f2e298edad413161b8a0a937312b3d";
const FIXTURE_RECORDED_AT: u64 = 1_760_000_000;
const FIXTURE_SESSION: &str = "20260918_013301";
const FIXTURE_ENGINES_ROOT: &str = "C:/Programs/hyperscape-lite/engines";

/// Every key the receipt contract declares.
const REQUIRED_RECEIPT_KEYS: [&str; 13] = [
    "format",
    "seamVersion",
    "sourcePath",
    "sourceHead",
    "argv",
    "exitCode",
    "engines",
    "splatPath",
    "splatSha256",
    "splatBytes",
    "splatCount",
    "captureSessionId",
    "recordedAtUnix",
];

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("contracts/realitylog-capture-v1")
}

fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/realitylog-receipt-scratch");
    fs::create_dir_all(&dir).expect("create scratch receipt dir");
    dir
}

/// A deterministic 3DGS cloud, four splats, mirroring the sibling contract
/// fixture's discipline (small, fixed, byte-stable).
fn fixture_cloud() -> SplatCloud {
    SplatCloud {
        positions: vec![
            [0.0, 0.0, 0.0],
            [12.5, -3.25, 1.75],
            [-40.0, 18.0, -2.5],
            [100.0, 0.0, 5.0],
        ],
        scales_log: vec![
            [-1.2, -0.8, -0.5],
            [-0.9, -0.9, -0.9],
            [-2.0, -1.5, -1.0],
            [-0.3, -0.4, -0.35],
        ],
        rot_wxyz: vec![
            [1.0, 0.0, 0.0, 0.0],
            [0.7071, 0.7071, 0.0, 0.0],
            [0.5, 0.5, 0.5, 0.5],
            [1.0, 0.0, 0.0, 0.0],
        ],
        opacity_logit: vec![2.0, -1.0, 0.0, 6.0],
        sh_dc: vec![
            [0.0, 0.0, 0.0],
            [1.5, -1.0, 0.5],
            [-0.75, 0.25, 1.25],
            [0.5, 0.5, 0.5],
        ],
        sh_rest: Vec::new(),
        rest_per_channel: 0,
    }
}

fn fixture_source() -> CaptureSource {
    CaptureSource {
        root: PathBuf::from(FIXTURE_SOURCE_ROOT),
        entry_point: PathBuf::from(format!(
            "{FIXTURE_SOURCE_ROOT}/scripts/reconstruct_session.py"
        )),
        head: Some(FIXTURE_SOURCE_HEAD.to_string()),
    }
}

fn fixture_argv() -> Vec<String> {
    vec![
        "python".to_string(),
        format!("{FIXTURE_SOURCE_ROOT}/scripts/reconstruct_session.py"),
        format!("sessions/{FIXTURE_SESSION}"),
        "cap".to_string(),
        "--steps".to_string(),
        "7000".to_string(),
        "--brush".to_string(),
        format!("{FIXTURE_ENGINES_ROOT}/brush/brush_app.exe"),
        "--mq3drecon".to_string(),
        "C:/Programs/mq3drecon/.venv312/Scripts/mq3drecon.exe".to_string(),
        "--no-push".to_string(),
        "--dry-run".to_string(),
    ]
}

/// The producer's output for the fixture's own facts, with the splat bytes read
/// from the committed fixture. `registrar` lets the regenerate test write those
/// bytes first.
fn fixture_receipt(
    splat_bytes: &[u8],
) -> instasplatter_lib::splat_contract::realitylog_export::CaptureReceipt {
    let source = fixture_source();
    let engines = build_engines_manifest_in(Path::new(FIXTURE_ENGINES_ROOT), FIXTURE_RECORDED_AT);
    build_receipt(&ReceiptFacts {
        source: &source,
        argv: &fixture_argv(),
        exit_code: 0,
        engines: EnginesManifestRef::from(&engines),
        splat_path: Path::new("splat.ply"),
        splat_bytes,
        splat: &fixture_cloud(),
        capture_session_id: Some(FIXTURE_SESSION),
        recorded_at_unix: FIXTURE_RECORDED_AT,
    })
}

fn parse_json(path: &Path) -> serde_json::Value {
    let raw = fs::read_to_string(path).expect("fixture should be readable UTF-8");
    serde_json::from_str(&raw).expect("fixture should be valid JSON")
}

#[test]
fn producer_writes_the_receipt_contract() {
    let dir = scratch_dir();
    let ply_path = dir.join("splat_7000.ply");
    let receipt_path = dir.join("receipt.json");

    let cloud = fixture_cloud();
    ply::write_ply(&ply_path, &cloud).expect("the fixture PLY should be writable");
    let bytes = fs::read(&ply_path).expect("the written PLY should be readable");

    let source = fixture_source();
    let engines = build_engines_manifest_in(Path::new(FIXTURE_ENGINES_ROOT), FIXTURE_RECORDED_AT);
    let receipt = build_receipt(&ReceiptFacts {
        source: &source,
        argv: &fixture_argv(),
        exit_code: 0,
        engines: EnginesManifestRef::from(&engines),
        splat_path: &ply_path,
        splat_bytes: &bytes,
        splat: &cloud,
        capture_session_id: Some(FIXTURE_SESSION),
        recorded_at_unix: FIXTURE_RECORDED_AT,
    });
    write_receipt(&receipt, &receipt_path).expect("the receipt should be writable");

    // --- the JSON surface a consumer parses ------------------------------
    let json = parse_json(&receipt_path);
    let keys: BTreeSet<&str> = json
        .as_object()
        .expect("receipt root must be an object")
        .keys()
        .map(String::as_str)
        .collect();
    for required in REQUIRED_RECEIPT_KEYS {
        assert!(keys.contains(required), "receipt is missing key {required}");
    }

    assert_eq!(json["format"], REALITYLOG_RECEIPT_FORMAT);
    assert_eq!(json["seamVersion"], REALITYLOG_SEAM_VERSION);
    assert_eq!(json["sourceHead"], FIXTURE_SOURCE_HEAD);
    assert_eq!(json["exitCode"], 0);
    assert_eq!(json["splatCount"], cloud.len());
    assert_eq!(json["splatBytes"], bytes.len() as u64);
    assert_eq!(json["captureSessionId"], FIXTURE_SESSION);

    // The digest is of the real bytes on disk, so a consumer can re-verify it.
    let recorded = json["splatSha256"]
        .as_str()
        .expect("splatSha256 must be a string");
    assert_eq!(
        recorded,
        instasplatter_lib::splat_contract::realitylog_export::sha256_hex(&bytes)
    );
    assert_eq!(recorded.len(), 64, "sha256 must be lowercase hex");
    assert!(recorded
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

    // The argv is recorded verbatim, and every engine path in it was injected.
    let argv: Vec<String> = json["argv"]
        .as_array()
        .expect("argv must be an array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(argv, fixture_argv());
    assert!(argv.contains(&"--brush".to_string()));
    assert!(argv.contains(&"--mq3drecon".to_string()));

    // The engine records carry the pins, not a copy of them.
    let engines_json = json["engines"]
        .as_array()
        .expect("engines must be an array");
    assert_eq!(engines_json.len(), 3);
    let colmap = engines_json
        .iter()
        .find(|e| e["name"] == "colmap" && e["variant"] == "cuda")
        .expect("the cuda COLMAP record must be present");
    assert_eq!(colmap["version"], "4.1.0");
    assert_eq!(
        colmap["sha256"],
        "ccd2f8c5b44f3e0ce645170d6abad30ff763ede97eeb0e6e23af1993e624e64b"
    );

    // A receipt is a record of a run, not a container for tree content.
    let raw = fs::read_to_string(&receipt_path).unwrap();
    for forbidden in ["accessToken", "Bearer", "secret", "password", "safetensors"] {
        assert!(!raw.contains(forbidden), "receipt leaked {forbidden}");
    }

    // ... and it round-trips through the reader.
    assert_eq!(read_receipt(&receipt_path).unwrap(), receipt);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn committed_fixture_matches_the_current_producer_contract() {
    let fixture = golden_dir();
    let receipt_path = fixture.join("receipt.json");
    let ply_path = fixture.join("splat.ply");

    assert!(
        receipt_path.is_file(),
        "missing committed fixture {} -- regenerate it with `cargo test -- --ignored \
         regenerate_committed_receipt` after reviewing the contract",
        receipt_path.display()
    );
    assert!(ply_path.is_file(), "missing committed PLY fixture");

    // The committed receipt must be byte-for-byte what the producer emits today
    // for the same facts, including the digest of the committed PLY.
    let committed_bytes = fs::read(&ply_path).expect("the fixture PLY must be readable");
    let produced = fixture_receipt(&committed_bytes);
    let committed = read_receipt(&receipt_path).expect("the committed receipt must parse");
    assert_eq!(
        committed, produced,
        "the committed receipt drifted from the live producer"
    );

    // And the committed JSON carries exactly the declared keys.
    let json = parse_json(&receipt_path);
    let keys: BTreeSet<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let expected: BTreeSet<&str> = REQUIRED_RECEIPT_KEYS.into_iter().collect();
    assert_eq!(
        keys, expected,
        "the committed receipt's key set is not the contract"
    );

    // The fixture's splat really is the cloud the receipt describes.
    let cloud = ply::read_ply(&ply_path).expect("the fixture PLY must decode");
    assert_eq!(cloud.len(), json["splatCount"].as_u64().unwrap() as usize);
    assert_eq!(cloud.len(), 4);
    assert_eq!(json["splatPath"], "splat.ply");
}

#[test]
#[ignore = "regenerate the committed receipt fixture after reviewing the contract"]
fn regenerate_committed_receipt() {
    let fixture = golden_dir();
    fs::create_dir_all(&fixture).expect("create contract fixture directory");

    let ply_path = fixture.join("splat.ply");
    let receipt_path = fixture.join("receipt.json");

    let cloud = fixture_cloud();
    ply::write_ply(&ply_path, &cloud).expect("write contract fixture PLY");
    let bytes = fs::read(&ply_path).expect("read back the fixture PLY");
    let receipt = fixture_receipt(&bytes);
    write_receipt(&receipt, &receipt_path).expect("write contract fixture receipt");
}
