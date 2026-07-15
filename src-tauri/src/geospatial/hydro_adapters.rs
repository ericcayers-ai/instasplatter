//! Experimental / external hydrodynamic adapter registry.
//!
//! TRITON, Wflow, GeoClaw are external permissive installs. SFINCS / HiPIMS /
//! BG_Flood / Itzï use a GPL external-plugin protocol and are never bundled
//! into the Apache installer. Promotion to Standard requires
//! [`HydroPromotionGates`].

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HydroEngineId {
    Anuga,
    Swmm,
    WebGpuPreview,
    Triton,
    Wflow,
    GeoClaw,
    Sfincs,
    Hipims,
    BgFlood,
    Itzi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HydroEngineTier {
    Standard,
    Preview,
    Experimental,
    ExternalPlugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HydroInstallKind {
    BundledWorker,
    ExternalPermissive,
    ExternalGplPlugin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HydroEngineDescriptor {
    pub id: HydroEngineId,
    pub label: String,
    pub tier: HydroEngineTier,
    pub license: String,
    pub install_kind: HydroInstallKind,
    pub install_folder: String,
    pub bundled: bool,
    pub notes: String,
}

/// Checklist required before promoting an experimental hydro engine to Standard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HydroPromotionGates {
    pub lake_at_rest: bool,
    pub wet_dry_analytical: bool,
    pub dam_break_analytical: bool,
    pub rainfall_infiltration: bool,
    pub mesh_convergence: bool,
    pub mass_conservation: bool,
    pub urban_obstacles: bool,
    pub calibrated_historical: bool,
    pub anuga_cross_comparison: bool,
    pub cpu_gpu_tolerance: bool,
    pub reproducibility_hash: bool,
    pub license_cleared_for_standard: bool,
}

impl HydroPromotionGates {
    pub fn all_clear(&self) -> bool {
        self.lake_at_rest
            && self.wet_dry_analytical
            && self.dam_break_analytical
            && self.rainfall_infiltration
            && self.mesh_convergence
            && self.mass_conservation
            && self.urban_obstacles
            && self.calibrated_historical
            && self.anuga_cross_comparison
            && self.cpu_gpu_tolerance
            && self.reproducibility_hash
            && self.license_cleared_for_standard
    }

    pub fn failing(&self) -> Vec<&'static str> {
        let checks: [(&'static str, bool); 12] = [
            ("lake_at_rest", self.lake_at_rest),
            ("wet_dry_analytical", self.wet_dry_analytical),
            ("dam_break_analytical", self.dam_break_analytical),
            ("rainfall_infiltration", self.rainfall_infiltration),
            ("mesh_convergence", self.mesh_convergence),
            ("mass_conservation", self.mass_conservation),
            ("urban_obstacles", self.urban_obstacles),
            ("calibrated_historical", self.calibrated_historical),
            ("anuga_cross_comparison", self.anuga_cross_comparison),
            ("cpu_gpu_tolerance", self.cpu_gpu_tolerance),
            ("reproducibility_hash", self.reproducibility_hash),
            (
                "license_cleared_for_standard",
                self.license_cleared_for_standard,
            ),
        ];
        checks
            .into_iter()
            .filter_map(|(n, ok)| if ok { None } else { Some(n) })
            .collect()
    }
}

pub fn engine_registry() -> Vec<HydroEngineDescriptor> {
    vec![
        HydroEngineDescriptor {
            id: HydroEngineId::Anuga,
            label: "ANUGA".into(),
            tier: HydroEngineTier::Standard,
            license: "Apache-2.0".into(),
            install_kind: HydroInstallKind::BundledWorker,
            install_folder: "anuga".into(),
            bundled: false,
            notes: "Authoritative 2D shallow-water scientific solver.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngineId::Swmm,
            label: "EPA SWMM".into(),
            tier: HydroEngineTier::Standard,
            license: "Public Domain".into(),
            install_kind: HydroInstallKind::BundledWorker,
            install_folder: "swmm".into(),
            bundled: false,
            notes: "Urban drainage / network exchange.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngineId::WebGpuPreview,
            label: "WebGPU live preview".into(),
            tier: HydroEngineTier::Preview,
            license: "Apache-2.0".into(),
            install_kind: HydroInstallKind::BundledWorker,
            install_folder: "webgpu-preview".into(),
            bundled: false,
            notes: "Display-rate interpolated preview; not authoritative.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngineId::Triton,
            label: "TRITON / Kokkos".into(),
            tier: HydroEngineTier::Experimental,
            license: "BSD-style (verify)".into(),
            install_kind: HydroInstallKind::ExternalPermissive,
            install_folder: "triton".into(),
            bundled: false,
            notes: "Accelerated permissive rainfall / overland flow. Not bundled.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngineId::Wflow,
            label: "Wflow.jl".into(),
            tier: HydroEngineTier::Experimental,
            license: "MIT (verify)".into(),
            install_kind: HydroInstallKind::ExternalPermissive,
            install_folder: "wflow".into(),
            bundled: false,
            notes: "Watershed / runoff workflows. External Julia install.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngineId::GeoClaw,
            label: "GeoClaw".into(),
            tier: HydroEngineTier::Experimental,
            license: "BSD-3".into(),
            install_kind: HydroInstallKind::ExternalPermissive,
            install_folder: "geoclaw".into(),
            bundled: false,
            notes: "Coastal / surge specialization. External install.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngineId::Sfincs,
            label: "SFINCS".into(),
            tier: HydroEngineTier::ExternalPlugin,
            license: "GPL".into(),
            install_kind: HydroInstallKind::ExternalGplPlugin,
            install_folder: "sfincs".into(),
            bundled: false,
            notes: "External GPL plugin — never ship in Apache installer.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngineId::Hipims,
            label: "HiPIMS".into(),
            tier: HydroEngineTier::ExternalPlugin,
            license: "GPL".into(),
            install_kind: HydroInstallKind::ExternalGplPlugin,
            install_folder: "hipims".into(),
            bundled: false,
            notes: "External GPL plugin — never ship in Apache installer.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngineId::BgFlood,
            label: "BG_Flood".into(),
            tier: HydroEngineTier::ExternalPlugin,
            license: "GPL".into(),
            install_kind: HydroInstallKind::ExternalGplPlugin,
            install_folder: "bg-flood".into(),
            bundled: false,
            notes: "External GPL plugin — never ship in Apache installer.".into(),
        },
        HydroEngineDescriptor {
            id: HydroEngineId::Itzi,
            label: "Itzï".into(),
            tier: HydroEngineTier::ExternalPlugin,
            license: "GPL".into(),
            install_kind: HydroInstallKind::ExternalGplPlugin,
            install_folder: "itzi".into(),
            bundled: false,
            notes: "External GPL plugin — never ship in Apache installer.".into(),
        },
    ]
}

pub fn engine_descriptor(id: HydroEngineId) -> Option<HydroEngineDescriptor> {
    engine_registry().into_iter().find(|d| d.id == id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalHydroInstallProtocol {
    pub engine: HydroEngineId,
    pub expected_folder: String,
    pub accepted_marker: String,
    pub entrypoint: String,
    pub refuse_if_stub: bool,
    pub refuse_if_bundled_request: bool,
    pub instructions: String,
}

pub fn external_plugin_protocol(id: HydroEngineId) -> Option<ExternalHydroInstallProtocol> {
    let d = engine_descriptor(id)?;
    if !matches!(
        d.install_kind,
        HydroInstallKind::ExternalGplPlugin | HydroInstallKind::ExternalPermissive
    ) {
        return None;
    }
    let gpl = matches!(d.install_kind, HydroInstallKind::ExternalGplPlugin);
    Some(ExternalHydroInstallProtocol {
        engine: id,
        expected_folder: format!("engines/hydro/{}", d.install_folder),
        accepted_marker: if gpl {
            "GPL_ACCEPTED".into()
        } else {
            "ACCEPTED".into()
        },
        entrypoint: "run".into(),
        refuse_if_stub: true,
        refuse_if_bundled_request: gpl,
        instructions: format!(
            "Install {} yourself under %LOCALAPPDATA%/InstaSplatter/engines/hydro/{}. \
             Place a {} marker after reviewing the license. \
             {} InstaSplatter never bundles this binary.",
            d.label,
            d.install_folder,
            if gpl { "GPL_ACCEPTED" } else { "ACCEPTED" },
            if gpl {
                "GPL engines are external plugins only."
            } else {
                "Experimental permissive engine."
            }
        ),
    })
}

pub fn refuse_gpl_bundle(id: HydroEngineId) -> bool {
    engine_descriptor(id)
        .map(|d| matches!(d.install_kind, HydroInstallKind::ExternalGplPlugin))
        .unwrap_or(false)
}

pub fn try_promote_to_standard(
    id: HydroEngineId,
    gates: &HydroPromotionGates,
) -> Result<(), String> {
    let d = engine_descriptor(id).ok_or_else(|| format!("Unknown hydro engine: {id:?}"))?;
    if matches!(d.install_kind, HydroInstallKind::ExternalGplPlugin) {
        return Err(format!(
            "{} is GPL — cannot promote into the Apache Standard installer.",
            d.label
        ));
    }
    if !matches!(d.tier, HydroEngineTier::Experimental) {
        return Err(format!(
            "{} is not an experimental hydro adapter ({:?}).",
            d.label, d.tier
        ));
    }
    if !gates.all_clear() {
        return Err(format!(
            "Promotion gates incomplete for {}: missing {:?}.",
            d.label,
            gates.failing()
        ));
    }
    Err(format!(
        "{} passed checklist scaffolding, but Standard promotion is not wired in this build.",
        d.label
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_standard_experimental_and_gpl() {
        let regs = engine_registry();
        assert!(regs.iter().any(|d| d.id == HydroEngineId::Anuga));
        assert!(regs.iter().any(|d| d.id == HydroEngineId::Triton));
        assert!(regs.iter().any(|d| d.id == HydroEngineId::Wflow));
        assert!(regs.iter().any(|d| d.id == HydroEngineId::GeoClaw));
        assert!(regs
            .iter()
            .any(|d| d.id == HydroEngineId::Sfincs && !d.bundled));
        assert!(regs.iter().all(|d| !d.bundled));
    }

    #[test]
    fn gpl_engines_refuse_bundle_and_need_protocol() {
        for id in [
            HydroEngineId::Sfincs,
            HydroEngineId::Hipims,
            HydroEngineId::BgFlood,
            HydroEngineId::Itzi,
        ] {
            assert!(refuse_gpl_bundle(id));
            let p = external_plugin_protocol(id).expect("protocol");
            assert!(p.refuse_if_bundled_request);
            assert!(p.accepted_marker.contains("GPL"));
        }
    }

    #[test]
    fn promotion_gates_block_incomplete_checklist() {
        let mut gates = HydroPromotionGates::default();
        let err = try_promote_to_standard(HydroEngineId::Triton, &gates).unwrap_err();
        assert!(err.contains("Promotion gates incomplete"));
        gates = HydroPromotionGates {
            lake_at_rest: true,
            wet_dry_analytical: true,
            dam_break_analytical: true,
            rainfall_infiltration: true,
            mesh_convergence: true,
            mass_conservation: true,
            urban_obstacles: true,
            calibrated_historical: true,
            anuga_cross_comparison: true,
            cpu_gpu_tolerance: true,
            reproducibility_hash: true,
            license_cleared_for_standard: true,
        };
        assert!(gates.all_clear());
        let err = try_promote_to_standard(HydroEngineId::Triton, &gates).unwrap_err();
        assert!(err.contains("not wired"));
    }

    #[test]
    fn gpl_cannot_promote_even_with_gates() {
        let gates = HydroPromotionGates {
            lake_at_rest: true,
            wet_dry_analytical: true,
            dam_break_analytical: true,
            rainfall_infiltration: true,
            mesh_convergence: true,
            mass_conservation: true,
            urban_obstacles: true,
            calibrated_historical: true,
            anuga_cross_comparison: true,
            cpu_gpu_tolerance: true,
            reproducibility_hash: true,
            license_cleared_for_standard: true,
        };
        let err = try_promote_to_standard(HydroEngineId::Sfincs, &gates).unwrap_err();
        assert!(err.contains("GPL"));
    }
}
