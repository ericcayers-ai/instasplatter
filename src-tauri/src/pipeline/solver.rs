//! Dual-mode camera / densify / polish policy (Standard vs Experimental).
//!
//! Standard stays commercially redistributable. Experimental requires an
//! explicit license ack and unlocks NC research sidecars (Ω, MASt3R, DUSt3R, Difix).

use super::sidecars::SidecarStatus;
use crate::settings::ResolvedSettings;

/// Human label for status chips / banner.
pub fn camera_chip(solver: &str) -> String {
    match solver {
        "vggt-omega" => "Cameras: VGGT-Ω".into(),
        "mast3r" => "Cameras: MASt3R-SfM".into(),
        "dust3r" => "Cameras: DUSt3R".into(),
        "vggt-commercial" => "Cameras: VGGT-Commercial".into(),
        "colmap" => "Cameras: COLMAP".into(),
        "colmap-fallback" => "Cameras: COLMAP (fallback)".into(),
        "live-init" => "Cameras: Live tracking".into(),
        other => format!("Cameras: {other}"),
    }
}

/// Ordered pose solvers for Standard Mode (VGGT-Commercial first).
pub fn standard_pose_chain(st: &SidecarStatus) -> Vec<&'static str> {
    let mut v = Vec::new();
    if st.vggt_commercial {
        v.push("vggt-commercial");
    }
    v
}

/// Ordered pose solvers for Experimental Mode (fail-down).
pub fn experimental_pose_chain(st: &SidecarStatus) -> Vec<&'static str> {
    let mut v = Vec::new();
    if st.vggt_omega {
        v.push("vggt-omega");
    }
    if st.mast3r {
        v.push("mast3r");
    }
    if st.dust3r {
        v.push("dust3r");
    }
    if st.vggt_commercial {
        v.push("vggt-commercial");
    }
    v
}

/// Whether a launcher is non-commercial / research-gated.
pub fn is_research_sidecar(name: &str) -> bool {
    matches!(
        name,
        "vggt-omega" | "vggt-research" | "mast3r" | "dust3r" | "difix"
    )
}

/// Select densify launcher order. Experimental merges every present source;
/// Standard prefers commercial/Apache ones (RoMa handled separately).
pub fn densify_neural_order(settings: &ResolvedSettings, st: &SidecarStatus) -> Vec<&'static str> {
    let mut v = Vec::new();
    if settings.experimental_mode {
        if st.vggt_omega {
            v.push("vggt-omega");
        }
        if st.mast3r {
            v.push("mast3r");
        }
        if st.dust3r {
            v.push("dust3r");
        }
        if st.vggt_commercial {
            v.push("vggt-commercial");
        }
        if st.depth_anything_v2 {
            v.push("depth-anything-v2");
        }
        if st.vggt_research {
            v.push("vggt-research");
        }
    } else {
        // Standard: commercial + Apache densifiers only (RoMa is separate).
        if st.vggt_commercial {
            v.push("vggt-commercial");
        }
        if st.depth_anything_v2 {
            v.push("depth-anything-v2");
        }
    }
    v
}

/// Polish order. Experimental: Difix then Fixer (both). Standard: Fixer only.
pub fn polish_order(settings: &ResolvedSettings, st: &SidecarStatus) -> Vec<&'static str> {
    let mut v = Vec::new();
    if settings.experimental_mode && st.difix {
        v.push("difix");
    }
    if st.fixer {
        v.push("fixer");
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::sidecars::SidecarStatus;
    use crate::profiler::{GpuVendor, HardwareProfile, Preset};
    use crate::settings::{resolve, Settings};

    fn profile() -> HardwareProfile {
        HardwareProfile {
            gpu_name: "Test".into(),
            gpu_vendor: GpuVendor::Nvidia,
            vram_mb: 16_000,
            has_cuda: true,
            cpu_name: "CPU".into(),
            cpu_threads: 8,
            ram_mb: 32_000,
            auto_preset: Preset::Balanced,
        }
    }

    fn empty_st() -> SidecarStatus {
        SidecarStatus {
            depth_anything_v2: false,
            vggt_commercial: false,
            vggt_omega: false,
            vggt_research: false,
            mast3r: false,
            dust3r: false,
            roma_v2: false,
            fixer: false,
            difix: false,
        }
    }

    #[test]
    fn experimental_pose_chain_orders_fail_down() {
        let mut st = empty_st();
        st.vggt_omega = true;
        st.mast3r = true;
        st.dust3r = true;
        st.vggt_commercial = true;
        assert_eq!(
            experimental_pose_chain(&st),
            vec!["vggt-omega", "mast3r", "dust3r", "vggt-commercial"]
        );
    }

    #[test]
    fn standard_pose_chain_is_commercial_only() {
        let mut st = empty_st();
        st.vggt_omega = true;
        st.vggt_commercial = true;
        assert_eq!(standard_pose_chain(&st), vec!["vggt-commercial"]);
    }

    #[test]
    fn research_gate_idents() {
        assert!(is_research_sidecar("vggt-omega"));
        assert!(is_research_sidecar("mast3r"));
        assert!(!is_research_sidecar("vggt-commercial"));
        assert!(!is_research_sidecar("roma-v2"));
    }

    #[test]
    fn experimental_resolve_forces_research_and_max_floors() {
        let s = Settings {
            experimental_mode: Some(true),
            experimental_license_acked: Some(true),
            preset: Some("balanced".into()),
            ..Default::default()
        };
        let r = resolve(&s, &profile());
        assert!(r.experimental_mode);
        assert!(r.allow_research_sidecars);
        assert_eq!(r.preset, Preset::Max);
        assert!(r.max_frames >= Preset::Max.params().max_frames);
        assert!(r.max_splats >= Preset::Max.params().max_splats);
        assert_eq!(r.roma_quality, "precise");
    }

    #[test]
    fn experimental_without_ack_stays_off() {
        let s = Settings {
            experimental_mode: Some(true),
            experimental_license_acked: Some(false),
            ..Default::default()
        };
        let r = resolve(&s, &profile());
        assert!(!r.experimental_mode);
        assert!(!r.allow_research_sidecars);
    }

    #[test]
    fn polish_order_experimental_puts_difix_first() {
        let s = Settings {
            experimental_mode: Some(true),
            experimental_license_acked: Some(true),
            ..Default::default()
        };
        let r = resolve(&s, &profile());
        let mut st = empty_st();
        st.fixer = true;
        st.difix = true;
        assert_eq!(polish_order(&r, &st), vec!["difix", "fixer"]);
    }

    #[test]
    fn standard_densify_excludes_nc_even_when_installed() {
        let s = Settings {
            allow_research_sidecars: Some(true),
            experimental_mode: Some(false),
            ..Default::default()
        };
        let r = resolve(&s, &profile());
        let mut st = empty_st();
        st.vggt_omega = true;
        st.mast3r = true;
        st.dust3r = true;
        st.vggt_commercial = true;
        st.depth_anything_v2 = true;
        assert_eq!(
            densify_neural_order(&r, &st),
            vec!["vggt-commercial", "depth-anything-v2"]
        );
        assert!(!r.allow_research_sidecars);
    }
}
