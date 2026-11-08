//! RealityLog artefact pull (RealityLog seam, migration step M4).
//!
//! Edge A of the seam: the host is an HTTP **client** of the source's existing
//! capture-library service. There is no new server on either side — the source
//! already serves `GET /library` and `GET /captures/<id>/<file>`
//! (`quest-client/pc_receiver/capture_library.py`, contract `PROTOCOL.md` §12),
//! and `reqwest` was already a host dependency.
//!
//! Two rules make this safe:
//!
//! 1. **The manifest schema belongs to the source.** [`MANIFEST_KEYS`] is pinned
//!    to the nine keys the source's own `tests/test_manifest_contract.py`
//!    asserts against `PROTOCOL.md` §12.1; an entry with a different key set is
//!    refused rather than partially read.
//! 2. **PLY only.** The host has no SOG reader and its glTF-gaussian writer is a
//!    stub, so a non-PLY `splat_format` fails closed ([`select_ply`]). The seam
//!    crosses the artefact, not the renderer.

use crate::splat::ply;
use crate::splat::SplatCloud;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::time::Duration;

/// The manifest route on the source's library service.
pub const LIBRARY_PATH: &str = "/library";

/// The nine §12.1 manifest keys, in the source's own order
/// (`capture_library.py::_manifest_entry`).
pub const MANIFEST_KEYS: [&str; 9] = [
    "session_id",
    "created_at",
    "splat_url",
    "splat_bytes",
    "splat_format",
    "thumbnail_url",
    "content_origin",
    "original_frames_included",
    "frame_count",
];

/// One §12.1 manifest entry. `deny_unknown_fields` plus the explicit key-set
/// check in [`parse_library_manifest`] is what makes "exactly these fields"
/// enforceable rather than aspirational.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryEntry {
    pub session_id: String,
    pub created_at: String,
    pub splat_url: String,
    pub splat_bytes: u64,
    pub splat_format: String,
    pub thumbnail_url: Option<String>,
    pub content_origin: String,
    pub original_frames_included: bool,
    pub frame_count: u64,
}

/// The key set of one manifest object, as it appeared on the wire.
pub fn entry_keys(value: &serde_json::Value) -> Result<BTreeSet<String>, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("manifest entry is not a JSON object: {value}"))?;
    Ok(object.keys().cloned().collect())
}

/// The §12.1 key set, as this module requires it.
pub fn required_keys() -> BTreeSet<String> {
    MANIFEST_KEYS.iter().map(|k| (*k).to_string()).collect()
}

/// Parse and validate a library manifest.
///
/// Every entry must carry exactly [`MANIFEST_KEYS`] — no more, no fewer. That
/// is the same assertion the source's own contract test makes, so a rename on
/// either side fails here rather than degrading silently.
pub fn parse_library_manifest(json: &str) -> Result<Vec<LibraryEntry>, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("library manifest is not JSON: {e}"))?;
    let array = parsed
        .as_array()
        .ok_or_else(|| "library manifest must be a JSON array of session entries".to_string())?;

    let expected = required_keys();
    for entry in array {
        let keys = entry_keys(entry)?;
        if keys != expected {
            let missing: Vec<&String> = expected.difference(&keys).collect();
            let extra: Vec<&String> = keys.difference(&expected).collect();
            return Err(format!(
                "library manifest entry has the wrong fields — the source's PROTOCOL.md §12.1 \
                 contract is {MANIFEST_KEYS:?}. missing={missing:?} unexpected={extra:?}"
            ));
        }
    }

    serde_json::from_value(parsed).map_err(|e| format!("library manifest entry is malformed: {e}"))
}

/// The one format this seam crosses. Everything else fails closed, because the
/// host cannot read it: no SOG reader exists in this repo at all, the
/// glTF-`KHR_gaussian_splatting` writer is a stub, and `.ksplat` has no support.
pub const CARRIED_FORMAT: &str = "ply";

/// Refuse anything that is not a PLY, naming the actual reason.
pub fn select_ply(entry: &LibraryEntry) -> Result<(), String> {
    let format = entry.splat_format.trim().to_ascii_lowercase();
    if format == CARRIED_FORMAT {
        return Ok(());
    }
    Err(format!(
        "session {} advertises splat_format {:?}; the RealityLog seam carries PLY only. \
         The host has no SOG reader, its glTF-gaussian writer is a stub, and it cannot read \
         .ksplat, so this artefact is refused rather than half-read.",
        entry.session_id, entry.splat_format
    ))
}

/// The first entry whose format the seam can carry, or a fail-closed error.
pub fn pick_ply(entries: &[LibraryEntry]) -> Result<LibraryEntry, String> {
    let mut refusals = Vec::new();
    for entry in entries {
        match select_ply(entry) {
            Ok(()) => return Ok(entry.clone()),
            Err(reason) => refusals.push(reason),
        }
    }
    if refusals.is_empty() {
        Err("the capture-library manifest lists no sessions".to_string())
    } else {
        Err(refusals.join("; "))
    }
}

/// Join a base URL with a manifest path (`/captures/<id>/<file>`).
pub fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Could not build an HTTP client for the capture library: {e}"))
}

/// `GET /library` and validate the manifest.
pub fn fetch_manifest(base_url: &str) -> Result<Vec<LibraryEntry>, String> {
    let url = join_url(base_url, LIBRARY_PATH);
    let response = client()?
        .get(&url)
        .send()
        .map_err(|e| format!("Could not reach the capture library at {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "The capture library at {url} answered HTTP {}",
            response.status()
        ));
    }
    let body = response
        .text()
        .map_err(|e| format!("Could not read the manifest from {url}: {e}"))?;
    parse_library_manifest(&body)
}

/// A pulled splat: the manifest entry that advertised it, the exact bytes'
/// digest (the receipt records this), and the decoded cloud.
#[derive(Debug, Clone)]
pub struct PulledSplat {
    pub entry: LibraryEntry,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
    pub cloud: SplatCloud,
}

/// `GET /captures/<id>/<file>` for one PLY entry, decoded through the host's
/// own production reader (`splat::ply::read_ply_bytes`).
pub fn pull_ply(base_url: &str, entry: &LibraryEntry) -> Result<PulledSplat, String> {
    select_ply(entry)?;
    let url = join_url(base_url, &entry.splat_url);
    let response = client()?
        .get(&url)
        .send()
        .map_err(|e| format!("Could not fetch {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url} answered HTTP {}", response.status()));
    }
    let body = response
        .bytes()
        .map_err(|e| format!("Could not read {url}: {e}"))?;

    let mut hasher = Sha256::new();
    hasher.update(&body);
    let sha256 = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    let cloud = ply::read_ply_bytes(&body).map_err(|e| {
        format!(
            "{url} was advertised as PLY by session {} but does not decode as a 3DGS cloud: {e}",
            entry.session_id
        )
    })?;

    Ok(PulledSplat {
        entry: entry.clone(),
        url,
        bytes: body.len() as u64,
        sha256,
        cloud,
    })
}

/// `GET /library`, pick the first carryable entry, pull it.
pub fn pull_best_ply(base_url: &str) -> Result<PulledSplat, String> {
    let entries = fetch_manifest(base_url)?;
    let entry = pick_ply(&entries)?;
    pull_ply(base_url, &entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The source's own committed fixture, embedded verbatim. This is the same
    /// file `quest-client/pc_receiver/tests/fixtures/library_manifest.gate.json`
    /// holds in the source checkout, which the source's `test_manifest_contract.py`
    /// validates against `PROTOCOL.md` §12.1.
    const GATE_FIXTURE: &str =
        include_str!("../../tests/fixtures/realitylog/library_manifest.gate.json");

    #[test]
    fn the_committed_fixture_parses_with_exactly_the_source_declared_keys() {
        let entries = parse_library_manifest(GATE_FIXTURE).expect("the gate fixture must parse");
        assert_eq!(entries.len(), 2);

        // Key-set equality is asserted for the raw JSON too, so a key the
        // struct silently tolerated cannot slip through.
        let raw: serde_json::Value = serde_json::from_str(GATE_FIXTURE).unwrap();
        for entry in raw.as_array().unwrap() {
            assert_eq!(entry_keys(entry).unwrap(), required_keys());
        }

        let first = &entries[0];
        assert_eq!(first.session_id, "20260918_165835_live_stream");
        assert_eq!(first.created_at, "2026-09-18T16:58:35Z");
        assert_eq!(
            first.splat_url,
            "/captures/20260918_165835_live_stream/splat.spz"
        );
        assert_eq!(first.splat_bytes, 12345);
        assert_eq!(first.splat_format, "spz");
        assert_eq!(
            first.thumbnail_url.as_deref(),
            Some("/captures/20260918_165835_live_stream/thumbnail.jpg")
        );
        assert_eq!(first.content_origin, "captured");
        assert!(first.original_frames_included);
        assert_eq!(first.frame_count, 5);

        let second = &entries[1];
        assert_eq!(second.splat_format, "ply");
        assert_eq!(second.thumbnail_url, None);
        assert!(!second.original_frames_included);
        assert_eq!(second.frame_count, 0);
    }

    /// A renamed, added or removed key is refused — the drift class the source's
    /// own contract test exists to catch.
    #[test]
    fn a_manifest_with_a_wrong_key_set_is_refused() {
        let renamed = GATE_FIXTURE.replace("\"frame_count\"", "\"frames\"");
        let err = parse_library_manifest(&renamed).unwrap_err();
        assert!(err.contains("wrong fields"), "{err}");
        assert!(err.contains("frame_count"), "{err}");

        let extra = GATE_FIXTURE.replace("\"content_origin\"", "\"content_originx\"");
        assert!(parse_library_manifest(&extra).is_err());

        assert!(
            parse_library_manifest("{}").is_err(),
            "an object is not a manifest"
        );
    }

    /// The seam carries PLY and nothing else, and says why.
    #[test]
    fn a_non_ply_entry_is_refused_with_the_reason() {
        let entries = parse_library_manifest(GATE_FIXTURE).unwrap();
        // The spz entry is first in the fixture, so picking "the first entry"
        // would be wrong: pick_ply must skip it and take the ply one.
        assert!(select_ply(&entries[0]).is_err());
        let chosen = pick_ply(&entries).expect("the ply entry must be selected");
        assert_eq!(chosen.session_id, "20260918_013301");
        assert_eq!(chosen.splat_format, "ply");

        let err = select_ply(&entries[0]).unwrap_err();
        assert!(err.contains("carries PLY only"), "{err}");

        // A manifest whose only entry is a non-PLY format fails closed.
        let spz_only = format!("[{}]", serde_json::to_string(&entries[0]).unwrap());
        let err = pick_ply(&parse_library_manifest(&spz_only).unwrap()).unwrap_err();
        assert!(err.contains("PLY only"), "{err}");
    }

    #[test]
    fn urls_join_without_doubling_the_separator() {
        assert_eq!(
            join_url("http://127.0.0.1:8766", LIBRARY_PATH),
            "http://127.0.0.1:8766/library"
        );
        assert_eq!(
            join_url("http://127.0.0.1:8766/", LIBRARY_PATH),
            "http://127.0.0.1:8766/library"
        );
        assert_eq!(
            join_url("http://127.0.0.1:8766", "/captures/a/splat.ply"),
            "http://127.0.0.1:8766/captures/a/splat.ply"
        );
    }

    /// The seam's format decision is recorded in the host's docs.
    const SEAM_DOC: &str = include_str!("../../../docs/REALITYLOG-SEAM.md");

    /// M5's gate: the seam path resolves to PLY through the host's own format
    /// mapper, a non-PLY `splat_format` is refused at the same place, and the
    /// out-of-scope formats are declared rather than merely unimplemented.
    #[test]
    fn the_seam_path_resolves_to_ply_and_refuses_anything_else() {
        use crate::splat::export::Format;
        use std::path::Path;

        let entries = parse_library_manifest(GATE_FIXTURE).unwrap();
        let chosen = pick_ply(&entries).expect("the fixture has one PLY entry");

        // The seam path IS a PLY path, by the host's own extension mapper.
        assert_eq!(Format::from_path(Path::new(&chosen.splat_url)), Format::Ply);
        assert_eq!(Format::from_path(Path::new("splat.ply")), Format::Ply);

        // The entry-level decision is the same decision.
        assert!(select_ply(&chosen).is_ok());
        for refused in ["spz", "sog", "ksplat", "gltf", "ply_", ""] {
            let mut entry = chosen.clone();
            entry.splat_format = refused.to_string();
            assert!(
                select_ply(&entry).is_err(),
                "splat_format {refused:?} must be refused by the seam"
            );
        }

        // `Format::parse` is the host's whole format vocabulary: SOG and
        // .ksplat have no representation at all, and SPZ's decoder lives only
        // inside `mod tests` in `splat/export.rs`, so it is not a read path.
        assert_eq!(Format::parse("ply"), Some(Format::Ply));
        assert_eq!(Format::parse("spz"), Some(Format::Spz));
        assert_eq!(Format::parse("sog"), None);
        assert_eq!(Format::parse("ksplat"), None);
        assert_eq!(Format::parse("gltf"), None);

        // The decision is documented, and the doc names both the carried
        // format and the ones deliberately left out.
        assert!(
            SEAM_DOC.contains("PLY"),
            "the seam doc must name the crossing format"
        );
        assert!(
            SEAM_DOC.contains("SOG"),
            "the seam doc must declare SOG out of scope"
        );
        assert!(
            SEAM_DOC.contains("ksplat"),
            "the seam doc must declare .ksplat out of scope"
        );
        assert!(
            SEAM_DOC.contains("not ported"),
            "the seam doc must record that the source's viewer is not ported"
        );
    }

    /// The live M4 gate. Ignored by default: it needs the source checkout
    /// (`REALITYLOG_ROOT`), its venv interpreter, and a real tracked PLY to
    /// serve. Run it with `cargo test -- --ignored realitylog_library`.
    ///
    /// What it proves, in the order the gate states it:
    /// 1. the source's own `python -m pc_receiver.capture_library` serves the
    ///    scratch capture root, and the host's parse of the live manifest has
    ///    exactly the key set of the source's committed fixture;
    /// 2. a **real tracked PLY** (the source's own Brush checkpoint) is pulled
    ///    over HTTP and decoded by the host's production reader;
    /// 3. the host's splat count equals the count the **source's own reader**
    ///    reports for the same bytes.
    #[test]
    #[ignore = "needs REALITYLOG_ROOT pointing at a real hyperscape-lite checkout"]
    fn live_library_pull_matches_the_sources_own_reader() {
        let root = match std::env::var_os(crate::pipeline::realitylog::REALITYLOG_ROOT_ENV) {
            Some(root) => std::path::PathBuf::from(root),
            None => {
                eprintln!("REALITYLOG_ROOT is not set; skipping the live library gate");
                return;
            }
        };

        let scratch =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/realitylog-library-gate");
        let _ = std::fs::remove_dir_all(&scratch);
        let captures = scratch.join("captures");
        // Session 1: no carryable splat, so the manifest still has a session the
        // seam must refuse.
        let spz_session = captures.join("20260918_165835_live_stream");
        std::fs::create_dir_all(&spz_session).unwrap();
        std::fs::write(spz_session.join("splat.spz"), vec![0u8; 12345]).unwrap();
        // Session 2: a real tracked 3DGS PLY, published where the library finds it.
        let ply_session = captures.join("20260918_013301");
        std::fs::create_dir_all(ply_session.join("brush_out")).unwrap();
        let source_ply = root
            .join("docs/evidence/p7.1-live-checkpoints-20260920T101026Z/checkpoint_00001000.ply");
        assert!(
            source_ply.is_file(),
            "the tracked PLY fixture is missing: {}",
            source_ply.display()
        );
        let served_ply = ply_session.join("brush_out/splat_7000.ply");
        std::fs::copy(&source_ply, &served_ply)
            .expect("copy the tracked PLY into the scratch root");

        // --- what the source's own reader says about those bytes -----------
        let python = crate::pipeline::realitylog::resolve_converter_python(&root);
        let oracle = std::process::Command::new(&python)
            .arg("-c")
            .arg(
                "import sys, pathlib; sys.path.insert(0, sys.argv[1]); \
                 from prune_floaters import read_ply; \
                 print(read_ply(pathlib.Path(sys.argv[2])).vertex_count)",
            )
            .arg(root.join("scripts").to_string_lossy().into_owned())
            .arg(served_ply.to_string_lossy().into_owned())
            .output()
            .expect("the source's venv interpreter must run");
        assert!(
            oracle.status.success(),
            "the source's own reader failed: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let source_count: usize = String::from_utf8_lossy(&oracle.stdout)
            .trim()
            .parse()
            .expect("the source's reader prints one integer");

        // --- start the source's own library service ------------------------
        let port = 8791u16;
        let base = format!("http://127.0.0.1:{port}");
        let mut server = std::process::Command::new(&python)
            .args(["-m", "pc_receiver.capture_library"])
            .arg(captures.to_string_lossy().into_owned())
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .current_dir(root.join("quest-client"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the source's library service must start");

        let mut entries = None;
        for _ in 0..60 {
            match fetch_manifest(&base) {
                Ok(found) => {
                    entries = Some(found);
                    break;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(250)),
            }
        }
        let entries = match entries {
            Some(entries) => entries,
            None => {
                let _ = server.kill();
                panic!("the source's library service never answered {base}{LIBRARY_PATH}");
            }
        };

        // (1) live manifest: the same key set as the source's committed fixture.
        let fixture_entries = parse_library_manifest(GATE_FIXTURE).unwrap();
        let raw: serde_json::Value = serde_json::from_str(GATE_FIXTURE).unwrap();
        let fixture_keys = entry_keys(&raw.as_array().unwrap()[0]).unwrap();
        for entry in &entries {
            let live = serde_json::to_value(entry).unwrap();
            assert_eq!(
                entry_keys(&live).unwrap(),
                fixture_keys,
                "the live manifest's key set drifted from the committed fixture"
            );
        }
        assert_eq!(entries.len(), fixture_entries.len());

        // (2)+(3) pull the real PLY and compare against the source's reader.
        let pulled = match pull_best_ply(&base) {
            Ok(pulled) => pulled,
            Err(e) => {
                let _ = server.kill();
                panic!("pull failed: {e}");
            }
        };
        let _ = server.kill();

        assert_eq!(pulled.entry.splat_format, "ply");
        assert_eq!(pulled.bytes, std::fs::metadata(&served_ply).unwrap().len());
        assert_eq!(
            pulled.bytes, pulled.entry.splat_bytes,
            "the bytes served do not match the manifest's own splat_bytes"
        );
        assert_eq!(
            pulled.cloud.len(),
            source_count,
            "the host read {} splats; the source's own reader reports {source_count}",
            pulled.cloud.len()
        );
        assert!(
            pulled.cloud.rest_per_channel > 0,
            "a Brush checkpoint is not SH-degree 0"
        );

        eprintln!(
            "live pull: session={} bytes={} sha256={} splats={} (source reader: {})",
            pulled.entry.session_id,
            pulled.bytes,
            pulled.sha256,
            pulled.cloud.len(),
            source_count
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }
}
