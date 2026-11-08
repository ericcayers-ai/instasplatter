//! Installer packaging invariants (RealityLog seam, migration step M1).
//!
//! `THIRDPARTY.md` states, in this repo's own licence ledger, that the research
//! sidecar adapters are *not* bundled in the base installer:
//!
//! > Sidecars shipped as installable adapters. None are bundled in the base installer.
//!
//! Until 2026-10-11 that sentence was false. `tauri.conf.json` shipped
//! `"../tools/sidecars/*": "sidecars/"` as a bundle resource, which put every
//! adapter directory into the installer — including ANUGA (GPL-2.0, per
//! `THIRDPARTY.md`) and the non-commercial research adapters (CC BY-NC / NC,
//! per `tools/sidecars/research/README.md`). An Apache-2.0 installer that
//! carries a GPL-2.0 adapter and an NC adapter is a licence defect that ships to
//! users, and it contradicted the ledger in the same tree.
//!
//! The fix is the ledger's own claim: bundle no sidecar adapter at all. The
//! runtime already resolves adapters from the user engine directory first
//! (`pipeline/sidecars.rs::sidecar_search_dirs`, `geospatial/hydro.rs`,
//! `pipeline/gsplat.rs`), so a user-installed adapter keeps working; only the
//! *bundled* copies are gone. Dev builds still fall back to the repo's own
//! `tools/sidecars/` tree, exactly as before.
//!
//! The tests below embed the two files instead of re-reading a copy, so the
//! assertion is against the artefacts that actually ship.

/// `src-tauri/tauri.conf.json`, the bundle configuration that ships.
pub const TAURI_CONF: &str = include_str!("../tauri.conf.json");

/// `THIRDPARTY.md`, the repo's own third-party licence ledger.
pub const THIRDPARTY: &str = include_str!("../../THIRDPARTY.md");

/// The ledger sentence this module exists to keep mechanically true.
pub const SIDECAR_BUNDLE_CLAIM: &str =
    "Sidecars shipped as installable adapters. None are bundled in the base installer.";

/// Every sidecar adapter directory under `tools/sidecars/`.
///
/// `adapter_list_covers_the_tree` fails if the tree gains or loses a directory,
/// so a new adapter cannot appear without this list — and the restricted list
/// below — being reconsidered.
pub const SIDECAR_ADAPTERS: [&str; 33] = [
    "_common",
    "anuga",
    "city-gaussian",
    "depth-anything-3",
    "depth-anything-v2",
    "difix",
    "dust3r",
    "easi3r",
    "fixer",
    "gof",
    "gs-2d",
    "gsplat-train",
    "horizon-gs",
    "hydro-plugins",
    "lightglue",
    "mapanything",
    "mast3r",
    "mast3r-slam",
    "milo",
    "monst3r",
    "pgsr",
    "pi3x",
    "rade-gs",
    "research",
    "roma-v2",
    "slam3r",
    "stream-vggt",
    "sugar",
    "swmm",
    "urban-gs",
    "vggt-commercial",
    "vggt-long",
    "vggt-omega",
];

/// Adapters whose terms make them unshippable in a distributed Apache-2.0
/// installer, with the repo's own source for each claim:
///
/// * GPL-2.0 — `THIRDPARTY.md` ("ANUGA | GPL-2.0 | ... GPL_ACCEPTED marker
///   required") and `tools/sidecars/research/README.md` ("Hydro experimental /
///   GPL plugins: see hydro-plugins/README.md").
/// * Non-commercial — `tools/sidecars/research/README.md`, whose licence table
///   marks these CC BY-NC / CC BY-NC-SA / "research/NC".
/// * Application-gated / model-licensed rather than OSI-permissive —
///   `tools/sidecars/README.md` ("vggt-commercial | Gated commercial") and
///   `THIRDPARTY.md` ("Fixer | NVIDIA Open Model License").
///
/// The list is a *lower bound* on what must not ship: the bundle carries no
/// adapter at all, so it is satisfied with room to spare. It exists so that a
/// future partial allowlist cannot quietly re-admit one of these.
pub const LICENCE_RESTRICTED_ADAPTERS: [&str; 25] = [
    // GPL-2.0 / GPL-coupled
    "anuga",
    "hydro-plugins",
    // Non-commercial research adapters
    "city-gaussian",
    "difix",
    "dust3r",
    "easi3r",
    "gof",
    "horizon-gs",
    "mast3r",
    "mast3r-slam",
    "milo",
    "monst3r",
    "pgsr",
    "pi3x",
    "rade-gs",
    "research",
    "slam3r",
    "stream-vggt",
    "sugar",
    "urban-gs",
    "vggt-long",
    "vggt-omega",
    "vggt-research",
    // Gated commercial / model licence
    "vggt-commercial",
    "fixer",
];

/// Every string that appears in `bundle.resources` — both the source globs and
/// their destination directories, since a destination of `sidecars/<name>/`
/// would ship that adapter just as surely as a source path would.
pub fn bundled_resource_entries() -> Result<Vec<String>, String> {
    let parsed: serde_json::Value = serde_json::from_str(TAURI_CONF)
        .map_err(|e| format!("tauri.conf.json is not JSON: {e}"))?;
    let resources = match parsed.get("bundle").and_then(|b| b.get("resources")) {
        None | Some(serde_json::Value::Null) => return Ok(Vec::new()),
        Some(value) => value,
    };

    let mut entries = Vec::new();
    match resources {
        serde_json::Value::Object(map) => {
            for (from, to) in map {
                entries.push(from.clone());
                if let Some(to) = to.as_str() {
                    entries.push(to.to_string());
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(s) = item.as_str() {
                    entries.push(s.to_string());
                }
            }
        }
        other => return Err(format!("bundle.resources has an unexpected shape: {other}")),
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of M1: no adapter directory reaches the installer.
    #[test]
    fn no_sidecar_adapter_is_bundled() {
        let entries = bundled_resource_entries().expect("tauri.conf.json must parse");
        for entry in &entries {
            let normalized = entry.replace('\\', "/").to_ascii_lowercase();
            assert!(
                !normalized.contains("tools/sidecars"),
                "bundle.resources still ships the sidecar tree: {entry} -- THIRDPARTY.md \
                 states none are bundled in the base installer"
            );
            for adapter in SIDECAR_ADAPTERS {
                assert!(
                    !normalized.contains(&format!("sidecars/{adapter}")),
                    "bundle.resources ships the {adapter} adapter: {entry}"
                );
            }
        }
    }

    /// The named defect from the design: GPL-2.0 and NC adapters in an
    /// Apache-2.0 installer.
    #[test]
    fn no_licence_restricted_adapter_is_reachable_from_the_bundle() {
        let entries = bundled_resource_entries().expect("tauri.conf.json must parse");
        for restricted in LICENCE_RESTRICTED_ADAPTERS {
            for entry in &entries {
                let normalized = entry.replace('\\', "/").to_ascii_lowercase();
                assert!(
                    !normalized.contains(&format!("sidecars/{restricted}")),
                    "licence-restricted adapter `{restricted}` is bundled via {entry}"
                );
            }
        }
        // The two the design named by hand, asserted explicitly so a future
        // refactor of the lists above cannot lose them.
        for entry in &entries {
            assert!(
                !entry.contains("sidecars/anuga"),
                "ANUGA (GPL-2.0) is bundled: {entry}"
            );
            assert!(
                !entry.contains("vggt-research") && !entry.contains("vggt-omega"),
                "an NC VGGT adapter is bundled: {entry}"
            );
        }
    }

    /// `THIRDPARTY.md`'s claim must be present verbatim *and* be true now.
    #[test]
    fn thirdparty_ledger_claim_is_present_and_true() {
        assert!(
            THIRDPARTY.contains(SIDECAR_BUNDLE_CLAIM),
            "THIRDPARTY.md no longer carries the sidecar-bundle claim this gate pins: \
             {SIDECAR_BUNDLE_CLAIM:?}"
        );
        let entries = bundled_resource_entries().expect("tauri.conf.json must parse");
        assert!(
            entries.iter().all(|e| !e.contains("tools/sidecars")),
            "THIRDPARTY.md claims no sidecar is bundled, but bundle.resources is {entries:?}"
        );
    }

    /// The adapter list is derived from the tree; keep it honest.
    #[test]
    fn adapter_list_covers_the_tree() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tools")
            .join("sidecars");
        let mut on_disk: Vec<String> = std::fs::read_dir(&root)
            .expect("tools/sidecars must be readable")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();

        let mut declared: Vec<String> = SIDECAR_ADAPTERS.iter().map(|s| s.to_string()).collect();
        declared.sort();

        assert_eq!(
            declared, on_disk,
            "SIDECAR_ADAPTERS drifted from tools/sidecars/ -- add the new adapter to \
             SIDECAR_ADAPTERS and decide whether it belongs in LICENCE_RESTRICTED_ADAPTERS"
        );
    }
}
