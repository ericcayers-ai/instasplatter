//! The `realitylog-capture-v1` receipt (RealityLog seam, migration step M6).
//!
//! The receipt is what makes the seam *verifiable*: it is the direct analogue
//! of the sibling bridge's proof block, which records the exact `argv`, the exit
//! code and the `sha256` of every input and the output. This module is the third
//! member of the host's existing cross-repo contract family, alongside
//! `digital-earth-splat-v1` ([`super::digital_earth_export`]) — same shape, same
//! conventions, same golden-fixture discipline:
//!
//! * a versioned format identifier,
//! * a `build_*` producer that is a pure function of the facts,
//! * a committed fixture under `src-tauri/contracts/realitylog-capture-v1/`,
//! * an integration test that pins producer, fixture and consumer together.
//!
//! What it records: the source checkout **path and HEAD**, the exact `argv` the
//! host handed the source, the exit code, the resolved engine paths together
//! with their **pinned SHA-256s**, and the `sha256` of the produced splat. It
//! deliberately records no secret and no source-tree content: the source's live
//! credential is never read, and nothing under the source is copied.

use crate::engines::{EngineRecord, EnginesManifestRef};
use crate::pipeline::realitylog::CaptureSource;
use crate::splat::SplatCloud;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Format identifier written into the receipt.
pub const REALITYLOG_RECEIPT_FORMAT: &str = "realitylog-capture-v1";

/// The seam's own version, so a future schema change is visible.
pub const REALITYLOG_SEAM_VERSION: u32 = 1;

/// A `realitylog-capture-v1` receipt: one reconstruction job the host drove
/// against a by-path Capture Source, and the artefact it produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureReceipt {
    pub format: String,
    pub seam_version: u32,
    /// The Capture Source checkout the host drove.
    pub source_path: String,
    /// `git rev-parse HEAD` in that checkout, when it is a git repo.
    pub source_head: Option<String>,
    /// The exact argv the host handed the source. Every engine path in it was
    /// injected by the host, never self-resolved by the source.
    pub argv: Vec<String>,
    pub exit_code: i32,
    /// The host's engine authority at run time: name, variant, version, URL,
    /// pinned SHA-256 and resolved path for each pinned archive.
    pub engines: Vec<EngineRecord>,
    /// The produced splat, as the host received it.
    pub splat_path: String,
    pub splat_sha256: String,
    pub splat_bytes: u64,
    pub splat_count: usize,
    /// The source-side session id the artefact belongs to, when known.
    pub capture_session_id: Option<String>,
    /// When the receipt was recorded (Unix seconds).
    pub recorded_at_unix: u64,
}

/// The facts a receipt is built from.
pub struct ReceiptFacts<'a> {
    pub source: &'a CaptureSource,
    pub argv: &'a [String],
    pub exit_code: i32,
    /// The host's engine authority. Accepts either a built manifest or the
    /// engine module's record list.
    pub engines: EnginesManifestRef<'a>,
    pub splat_path: &'a Path,
    /// The splat's bytes, exactly as received; the receipt records their digest.
    pub splat_bytes: &'a [u8],
    pub splat: &'a SplatCloud,
    pub capture_session_id: Option<&'a str>,
    pub recorded_at_unix: u64,
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Build the receipt. Pure: no I/O, no clock, so a test can pin it exactly.
pub fn build_receipt(facts: &ReceiptFacts<'_>) -> CaptureReceipt {
    CaptureReceipt {
        format: REALITYLOG_RECEIPT_FORMAT.to_string(),
        seam_version: REALITYLOG_SEAM_VERSION,
        source_path: facts.source.root.to_string_lossy().into_owned(),
        source_head: facts.source.head.clone(),
        argv: facts.argv.to_vec(),
        exit_code: facts.exit_code,
        engines: facts.engines.records(),
        splat_path: facts.splat_path.to_string_lossy().into_owned(),
        splat_sha256: sha256_hex(facts.splat_bytes),
        splat_bytes: facts.splat_bytes.len() as u64,
        splat_count: facts.splat.len(),
        capture_session_id: facts.capture_session_id.map(str::to_string),
        recorded_at_unix: facts.recorded_at_unix,
    }
}

/// Write the receipt JSON to `dest`, creating the parent directory.
pub fn write_receipt(receipt: &CaptureReceipt, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let json = serde_json::to_string_pretty(receipt).map_err(|e| e.to_string())?;
    fs::write(dest, json).map_err(|e| e.to_string())
}

/// Read a receipt, refusing a receipt from a different contract.
pub fn read_receipt(path: &Path) -> Result<CaptureReceipt, String> {
    let raw =
        fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let receipt: CaptureReceipt = serde_json::from_str(&raw)
        .map_err(|e| format!("{} is not a valid receipt: {e}", path.display()))?;
    if receipt.format != REALITYLOG_RECEIPT_FORMAT {
        return Err(format!(
            "{} carries format {:?}, expected {:?}",
            path.display(),
            receipt.format,
            REALITYLOG_RECEIPT_FORMAT
        ));
    }
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::{build_engines_manifest_in, EnginesManifest};
    use crate::pipeline::realitylog::CaptureSource;
    use std::path::PathBuf;

    fn fixture_source() -> CaptureSource {
        CaptureSource {
            root: PathBuf::from("C:/Programs/hyperscape-lite"),
            entry_point: PathBuf::from(
                "C:/Programs/hyperscape-lite/scripts/reconstruct_session.py",
            ),
            head: Some("d2c8c0a8e6f2e298edad413161b8a0a937312b3d".to_string()),
        }
    }

    fn fixture_engines() -> EnginesManifest {
        build_engines_manifest_in(
            Path::new("C:/Programs/hyperscape-lite/engines"),
            1_760_000_000,
        )
    }

    fn fixture_argv() -> Vec<String> {
        vec![
            "python".to_string(),
            "C:/Programs/hyperscape-lite/scripts/reconstruct_session.py".to_string(),
            "sessions/20260918_013301".to_string(),
            "--steps".to_string(),
            "7000".to_string(),
            "--brush".to_string(),
            "C:/Programs/hyperscape-lite/engines/brush/brush_app.exe".to_string(),
            "--no-push".to_string(),
        ]
    }

    fn fixture_cloud() -> SplatCloud {
        SplatCloud {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 2.0, 3.0]],
            scales_log: vec![[-1.0; 3]; 2],
            rot_wxyz: vec![[1.0, 0.0, 0.0, 0.0]; 2],
            opacity_logit: vec![0.0; 2],
            sh_dc: vec![[0.0; 3]; 2],
            sh_rest: Vec::new(),
            rest_per_channel: 0,
        }
    }

    #[test]
    fn a_receipt_records_the_argv_exit_code_engine_digests_and_splat_digest() {
        let source = fixture_source();
        let engines = fixture_engines();
        let cloud = fixture_cloud();
        let bytes = b"ply-stub-bytes";

        let receipt = build_receipt(&ReceiptFacts {
            source: &source,
            argv: &fixture_argv(),
            exit_code: 0,
            engines: EnginesManifestRef::from(&engines),
            splat_path: Path::new("C:/out/splat_7000.ply"),
            splat_bytes: bytes,
            splat: &cloud,
            capture_session_id: Some("20260918_013301"),
            recorded_at_unix: 1_760_000_100,
        });

        assert_eq!(receipt.format, REALITYLOG_RECEIPT_FORMAT);
        assert_eq!(receipt.seam_version, REALITYLOG_SEAM_VERSION);
        assert_eq!(receipt.exit_code, 0);
        assert_eq!(receipt.splat_count, cloud.len());
        assert_eq!(receipt.splat_bytes, bytes.len() as u64);
        assert_eq!(receipt.splat_sha256, sha256_hex(bytes));
        assert_eq!(receipt.argv, fixture_argv());
        assert_eq!(receipt.source_path, "C:/Programs/hyperscape-lite");
        assert_eq!(
            receipt.source_head.as_deref(),
            Some("d2c8c0a8e6f2e298edad413161b8a0a937312b3d")
        );

        // The engine digests are the pins, not a copy of them.
        assert_eq!(receipt.engines.len(), 3);
        let colmap = receipt
            .engines
            .iter()
            .find(|e| e.name == "colmap" && e.variant == "cuda")
            .unwrap();
        assert_eq!(colmap.version, "4.1.0");
        assert_eq!(
            colmap.sha256,
            "ccd2f8c5b44f3e0ce645170d6abad30ff763ede97eeb0e6e23af1993e624e64b"
        );
        let brush = receipt.engines.iter().find(|e| e.name == "brush").unwrap();
        assert_eq!(brush.version, "0.3.0");

        // No secret, and no source-tree content, can appear: every field is a
        // path, a digest, an argv, or a count.
        let json = serde_json::to_string(&receipt).unwrap();
        for forbidden in ["accessToken", "token", "secret", "safetensors"] {
            assert!(
                !json.contains(forbidden),
                "receipt leaked {forbidden}: {json}"
            );
        }
    }

    #[test]
    fn a_receipt_of_the_wrong_contract_is_refused() {
        let dir = std::env::temp_dir().join("instasplatter_receipt_contract_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("receipt.json");

        let mut receipt = build_receipt(&ReceiptFacts {
            source: &fixture_source(),
            argv: &[],
            exit_code: 3,
            engines: EnginesManifestRef::from(&fixture_engines()),
            splat_path: Path::new("splat.ply"),
            splat_bytes: b"x",
            splat: &SplatCloud::default(),
            capture_session_id: None,
            recorded_at_unix: 5,
        });
        write_receipt(&receipt, &path).unwrap();
        assert_eq!(read_receipt(&path).unwrap(), receipt);

        receipt.format = "digital-earth-splat-v1".to_string();
        write_receipt(&receipt, &path).unwrap();
        assert!(read_receipt(&path).is_err());

        let _ = fs::remove_dir_all(&dir);
    }
}
