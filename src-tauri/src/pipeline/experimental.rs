//! Experimental reconstruction adapters — capture-aware routing catalogs.
//!
//! Experimental mode evaluates installed research engines that match the
//! capture profile. Every accepted candidate still passes canonical-frame
//! alignment and validation gates in `solver` / `sidecars` before fusion.
//! Surface/mesh and 4D engines are separate adapters (never blind-merged
//! into the static COLMAP/ENU densify path).

use super::sidecars::SidecarStatus;
use super::solver::CaptureProfile;
use serde::{Deserialize, Serialize};

/// Role of an experimental research adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExperimentalEngineKind {
    /// Feed-forward / SLAM pose hypothesis → COLMAP/ENU frame.
    Pose,
    /// Dense point / depth evidence for confidence fusion.
    Densify,
    /// Surface / mesh (2DGS, GOF, SuGaR, …) — separate product path.
    SurfaceMesh,
    /// Dynamic / 4D result — never fused into static init.ply.
    FourD,
    /// Large-scene partition / LOD (CityGaussian, Urban-GS, Horizon).
    LargeScene,
}

/// Static catalog entry (docs + routing). Readiness is probed via sidecars.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ExperimentalEngineSpec {
    pub id: &'static str,
    pub kind: ExperimentalEngineKind,
    pub license: &'static str,
    pub label: &'static str,
}

/// Full experimental reconstruction catalog (stubs until weights are wired).
pub const EXPERIMENTAL_ENGINES: &[ExperimentalEngineSpec] = &[
    // Static unordered
    ExperimentalEngineSpec {
        id: "vggt-omega",
        kind: ExperimentalEngineKind::Pose,
        license: "CC-BY-NC-4.0",
        label: "VGGT-Ω",
    },
    ExperimentalEngineSpec {
        id: "mast3r",
        kind: ExperimentalEngineKind::Pose,
        license: "CC-BY-NC-SA",
        label: "MASt3R-SfM",
    },
    ExperimentalEngineSpec {
        id: "dust3r",
        kind: ExperimentalEngineKind::Pose,
        license: "CC-BY-NC-SA",
        label: "DUSt3R",
    },
    ExperimentalEngineSpec {
        id: "pi3x",
        kind: ExperimentalEngineKind::Pose,
        license: "CC-BY-NC",
        label: "Pi3X",
    },
    // Long video / SLAM
    ExperimentalEngineSpec {
        id: "stream-vggt",
        kind: ExperimentalEngineKind::Pose,
        license: "research/NC",
        label: "StreamVGGT",
    },
    ExperimentalEngineSpec {
        id: "vggt-long",
        kind: ExperimentalEngineKind::Pose,
        license: "research/NC",
        label: "VGGT-Long",
    },
    ExperimentalEngineSpec {
        id: "mast3r-slam",
        kind: ExperimentalEngineKind::Pose,
        license: "CC-BY-NC-SA",
        label: "MASt3R-SLAM",
    },
    ExperimentalEngineSpec {
        id: "slam3r",
        kind: ExperimentalEngineKind::Pose,
        license: "research/NC",
        label: "SLAM3R",
    },
    // Dynamic / 4D
    ExperimentalEngineSpec {
        id: "monst3r",
        kind: ExperimentalEngineKind::FourD,
        license: "research/NC",
        label: "MonST3R",
    },
    ExperimentalEngineSpec {
        id: "easi3r",
        kind: ExperimentalEngineKind::FourD,
        license: "research/NC",
        label: "Easi3R",
    },
    // Large aerial / urban
    ExperimentalEngineSpec {
        id: "city-gaussian",
        kind: ExperimentalEngineKind::LargeScene,
        license: "research/NC",
        label: "CityGaussian V2",
    },
    ExperimentalEngineSpec {
        id: "urban-gs",
        kind: ExperimentalEngineKind::LargeScene,
        license: "research/NC",
        label: "Urban-GS",
    },
    ExperimentalEngineSpec {
        id: "horizon-gs",
        kind: ExperimentalEngineKind::LargeScene,
        license: "research/NC",
        label: "Horizon-GS",
    },
    // Surface / mesh (separate adapters)
    ExperimentalEngineSpec {
        id: "gs-2d",
        kind: ExperimentalEngineKind::SurfaceMesh,
        license: "Apache-2.0",
        label: "2DGS",
    },
    ExperimentalEngineSpec {
        id: "gof",
        kind: ExperimentalEngineKind::SurfaceMesh,
        license: "Inria-NC",
        label: "GOF",
    },
    ExperimentalEngineSpec {
        id: "pgsr",
        kind: ExperimentalEngineKind::SurfaceMesh,
        license: "research/NC",
        label: "PGSR",
    },
    ExperimentalEngineSpec {
        id: "rade-gs",
        kind: ExperimentalEngineKind::SurfaceMesh,
        license: "research/NC",
        label: "RaDe-GS",
    },
    ExperimentalEngineSpec {
        id: "sugar",
        kind: ExperimentalEngineKind::SurfaceMesh,
        license: "GS-adjacent",
        label: "SuGaR",
    },
    ExperimentalEngineSpec {
        id: "milo",
        kind: ExperimentalEngineKind::SurfaceMesh,
        license: "research/NC",
        label: "MILo",
    },
];

/// Checklist required before an experimental densify/pose artifact may fuse.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalValidationGate {
    pub canonical_frame_aligned: bool,
    pub hypothesis_gates_passed: bool,
    pub schema_v2_artifact: bool,
    pub scale_status_declared: bool,
    pub license_acknowledged: bool,
}

impl ExperimentalValidationGate {
    pub fn all_clear(&self) -> bool {
        self.canonical_frame_aligned
            && self.hypothesis_gates_passed
            && self.schema_v2_artifact
            && self.scale_status_declared
            && self.license_acknowledged
    }

    /// Build a gate report from an accepted hypothesis + schema-v2 write.
    /// `license_ok` is true when the host already allowed the launcher
    /// (Standard commercial/Apache, or Experimental with NC ack).
    pub fn for_fusion_candidate(
        license_ok: bool,
        hypothesis_ok: bool,
        wrote_schema_v2: bool,
        frame_is_colmap_enu: bool,
        scale_declared: bool,
    ) -> Self {
        Self {
            canonical_frame_aligned: frame_is_colmap_enu,
            hypothesis_gates_passed: hypothesis_ok,
            schema_v2_artifact: wrote_schema_v2,
            scale_status_declared: scale_declared,
            license_acknowledged: license_ok,
        }
    }
}

fn ready(st: &SidecarStatus, id: &str) -> bool {
    match id {
        "vggt-omega" => st.vggt_omega,
        "mast3r" => st.mast3r,
        "dust3r" => st.dust3r,
        "pi3x" => st.pi3x,
        "stream-vggt" => st.stream_vggt,
        "vggt-long" => st.vggt_long,
        "mast3r-slam" => st.mast3r_slam,
        "slam3r" => st.slam3r,
        "monst3r" => st.monst3r,
        "easi3r" => st.easi3r,
        "city-gaussian" => st.city_gaussian,
        "urban-gs" => st.urban_gs,
        "horizon-gs" => st.horizon_gs,
        "gs-2d" => st.gs_2d,
        "gof" => st.gof,
        "pgsr" => st.pgsr,
        "rade-gs" => st.rade_gs,
        "sugar" => st.sugar,
        "milo" => st.milo,
        "vggt-commercial" => st.vggt_commercial,
        "mapanything" => st.mapanything,
        "vggt-research" => st.vggt_research,
        _ => false,
    }
}

fn push_ready(out: &mut Vec<&'static str>, st: &SidecarStatus, id: &'static str) {
    if ready(st, id) {
        out.push(id);
    }
}

/// Pose / SfM candidates for an experimental capture profile (not a blind merge).
pub fn experimental_pose_for_profile(
    profile: CaptureProfile,
    st: &SidecarStatus,
) -> Vec<&'static str> {
    let mut v = Vec::new();
    match profile {
        CaptureProfile::UnorderedPhotos | CaptureProfile::FisheyeOrRollingShutter => {
            // Static unordered: Ω / MASt3R / DUSt3R / Pi3X (+ commercial repair).
            for id in ["vggt-omega", "mast3r", "dust3r", "pi3x"] {
                push_ready(&mut v, st, id);
            }
            push_ready(&mut v, st, "vggt-commercial");
            push_ready(&mut v, st, "mapanything");
        }
        CaptureProfile::OrderedVideo => {
            // Short ordered clips still prefer Ω then commercial.
            push_ready(&mut v, st, "vggt-omega");
            push_ready(&mut v, st, "vggt-commercial");
            push_ready(&mut v, st, "mapanything");
            push_ready(&mut v, st, "mast3r");
        }
        CaptureProfile::LongVideo => {
            // Long video / SLAM family — evaluate streaming adapters first.
            for id in ["stream-vggt", "vggt-long", "mast3r-slam", "slam3r"] {
                push_ready(&mut v, st, id);
            }
            push_ready(&mut v, st, "vggt-omega");
            push_ready(&mut v, st, "vggt-commercial");
        }
        CaptureProfile::DynamicScene => {
            // Dynamic: Ω + MonST3R / Easi3R pose hints; 4D stays on a side path.
            push_ready(&mut v, st, "vggt-omega");
            push_ready(&mut v, st, "monst3r");
            push_ready(&mut v, st, "easi3r");
            push_ready(&mut v, st, "mast3r");
        }
        CaptureProfile::LargeScene => {
            // Large aerial/urban: partition engines are separate; pose still scored.
            push_ready(&mut v, st, "vggt-omega");
            push_ready(&mut v, st, "mast3r");
            push_ready(&mut v, st, "vggt-commercial");
            push_ready(&mut v, st, "mapanything");
        }
        CaptureProfile::GpsRtkDrone => {
            // Pose-prior stays first at the orchestrator; neural repairs follow.
            push_ready(&mut v, st, "vggt-omega");
            push_ready(&mut v, st, "vggt-commercial");
            push_ready(&mut v, st, "mapanything");
            push_ready(&mut v, st, "mast3r");
            push_ready(&mut v, st, "pi3x");
        }
    }
    v.dedup();
    v
}

/// Densify evidence sources for experimental fusion (profile-filtered).
pub fn experimental_densify_for_profile(
    profile: CaptureProfile,
    st: &SidecarStatus,
) -> Vec<&'static str> {
    let mut v = Vec::new();
    match profile {
        CaptureProfile::LongVideo => {
            for id in ["stream-vggt", "vggt-long", "mast3r-slam"] {
                push_ready(&mut v, st, id);
            }
        }
        CaptureProfile::DynamicScene => {
            push_ready(&mut v, st, "vggt-omega");
            // MonST3R/Easi3R densify only as tagged four-d evidence — not static fuse.
        }
        CaptureProfile::UnorderedPhotos
        | CaptureProfile::FisheyeOrRollingShutter
        | CaptureProfile::OrderedVideo
        | CaptureProfile::GpsRtkDrone => {
            for id in ["vggt-omega", "mast3r", "dust3r", "pi3x"] {
                push_ready(&mut v, st, id);
            }
        }
        CaptureProfile::LargeScene => {
            push_ready(&mut v, st, "vggt-omega");
            push_ready(&mut v, st, "mast3r");
        }
    }
    push_ready(&mut v, st, "vggt-commercial");
    push_ready(&mut v, st, "mapanything");
    if st.depth_anything_3 {
        v.push("depth-anything-3");
    } else if st.depth_anything_v2 {
        v.push("depth-anything-v2");
    }
    push_ready(&mut v, st, "vggt-research");
    v.dedup();
    v
}

/// Separate 4D adapters — never concatenated into static init.ply fusion.
pub fn experimental_four_d_candidates(st: &SidecarStatus) -> Vec<&'static str> {
    let mut v = Vec::new();
    push_ready(&mut v, st, "monst3r");
    push_ready(&mut v, st, "easi3r");
    v
}

/// Large-scene partition / LOD adapters (engine-specific outputs).
pub fn experimental_large_scene_candidates(st: &SidecarStatus) -> Vec<&'static str> {
    let mut v = Vec::new();
    for id in ["city-gaussian", "urban-gs", "horizon-gs"] {
        push_ready(&mut v, st, id);
    }
    v
}

/// Surface / mesh experimental engines (separate from densify fusion).
pub fn experimental_surface_candidates(st: &SidecarStatus) -> Vec<&'static str> {
    let mut v = Vec::new();
    for id in ["gs-2d", "gof", "pgsr", "rade-gs", "sugar", "milo"] {
        push_ready(&mut v, st, id);
    }
    v
}

/// Human-readable routing table row for docs / diagnostics.
pub fn routing_table() -> Vec<(CaptureProfile, &'static str, &'static [&'static str])> {
    vec![
        (
            CaptureProfile::UnorderedPhotos,
            "static unordered",
            &["vggt-omega", "mast3r", "dust3r", "pi3x"],
        ),
        (
            CaptureProfile::LongVideo,
            "long video / SLAM",
            &["stream-vggt", "vggt-long", "mast3r-slam", "slam3r"],
        ),
        (
            CaptureProfile::DynamicScene,
            "dynamic (+ separate 4D)",
            &["vggt-omega", "monst3r", "easi3r"],
        ),
        (
            CaptureProfile::LargeScene,
            "large aerial / urban",
            &["city-gaussian", "urban-gs", "horizon-gs"],
        ),
        (
            CaptureProfile::OrderedVideo,
            "short ordered video",
            &["vggt-omega", "vggt-commercial", "mapanything"],
        ),
        (
            CaptureProfile::GpsRtkDrone,
            "GPS/RTK drone",
            &["colmap-pose-prior", "vggt-omega", "vggt-commercial"],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st_all_research() -> SidecarStatus {
        SidecarStatus {
            vggt_omega: true,
            mast3r: true,
            dust3r: true,
            pi3x: true,
            stream_vggt: true,
            vggt_long: true,
            mast3r_slam: true,
            slam3r: true,
            monst3r: true,
            easi3r: true,
            city_gaussian: true,
            urban_gs: true,
            horizon_gs: true,
            gs_2d: true,
            gof: true,
            pgsr: true,
            rade_gs: true,
            sugar: true,
            milo: true,
            vggt_commercial: true,
            mapanything: true,
            depth_anything_3: true,
            ..Default::default()
        }
    }

    #[test]
    fn static_unordered_prefers_omega_family() {
        let v = experimental_pose_for_profile(CaptureProfile::UnorderedPhotos, &st_all_research());
        assert_eq!(&v[..4], &["vggt-omega", "mast3r", "dust3r", "pi3x"]);
    }

    #[test]
    fn long_video_routes_stream_slam_first() {
        let v = experimental_pose_for_profile(CaptureProfile::LongVideo, &st_all_research());
        assert_eq!(
            &v[..4],
            &["stream-vggt", "vggt-long", "mast3r-slam", "slam3r"]
        );
    }

    #[test]
    fn dynamic_keeps_four_d_separate() {
        let pose = experimental_pose_for_profile(CaptureProfile::DynamicScene, &st_all_research());
        assert!(pose.contains(&"vggt-omega"));
        assert!(pose.contains(&"monst3r"));
        let four = experimental_four_d_candidates(&st_all_research());
        assert_eq!(four, vec!["monst3r", "easi3r"]);
    }

    #[test]
    fn large_scene_exposes_partition_adapters() {
        let large = experimental_large_scene_candidates(&st_all_research());
        assert_eq!(large, vec!["city-gaussian", "urban-gs", "horizon-gs"]);
        let surface = experimental_surface_candidates(&st_all_research());
        assert!(surface.contains(&"gs-2d"));
        assert!(surface.contains(&"sugar"));
    }

    #[test]
    fn fusion_gate_requires_all_checks() {
        let ok = ExperimentalValidationGate::for_fusion_candidate(
            true, true, true, true, true,
        );
        assert!(ok.all_clear());
        let bad = ExperimentalValidationGate::for_fusion_candidate(
            true, true, true, false, true,
        );
        assert!(!bad.all_clear());
        let no_license = ExperimentalValidationGate::for_fusion_candidate(
            false, true, true, true, true,
        );
        assert!(!no_license.all_clear());
    }

    #[test]
    fn catalog_covers_plan_families() {
        let ids: Vec<_> = EXPERIMENTAL_ENGINES.iter().map(|e| e.id).collect();
        for need in [
            "pi3x",
            "stream-vggt",
            "monst3r",
            "city-gaussian",
            "gs-2d",
            "milo",
        ] {
            assert!(ids.contains(&need), "missing {need}");
        }
        assert!(!routing_table().is_empty());
        assert!(EXPERIMENTAL_ENGINES.iter().any(|e| e.kind == ExperimentalEngineKind::FourD));
        assert!(EXPERIMENTAL_ENGINES
            .iter()
            .any(|e| e.label.contains("CityGaussian")));
    }
}
