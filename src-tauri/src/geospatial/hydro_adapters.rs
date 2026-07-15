//! Experimental / external hydrodynamic adapter façade.
//!
//! Registry, promotion gates, and GPL external-plugin protocol live in
//! [`crate::geospatial::hydro`]. This module re-exports them under stable
//! adapter-facing names (`HydroEngineId`, …) so callers that landed during
//! parallel staging keep compiling without a second copy of the types.

#[allow(unused_imports)] // public façade for external/experimental callers
pub use crate::geospatial::hydro::{
    engine_descriptor, engine_registry, external_plugin_protocol, refuse_gpl_bundle,
    try_promote_to_standard, ExternalHydroInstallProtocol, HydroEngine as HydroEngineId,
    HydroEngineDescriptor, HydroEngineTier, HydroInstallKind, HydroPromotionGates,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_reexports_registry() {
        let regs = engine_registry();
        assert!(regs.iter().any(|d| d.id == HydroEngineId::Anuga));
        assert!(regs.iter().any(|d| d.id == HydroEngineId::Triton));
        assert!(regs.iter().any(|d| d.id == HydroEngineId::Sfincs && !d.bundled));
    }

    #[test]
    fn gpl_still_refuses_bundle_via_facade() {
        assert!(refuse_gpl_bundle(HydroEngineId::Sfincs));
        let p = external_plugin_protocol(HydroEngineId::Sfincs).expect("protocol");
        assert!(p.refuse_if_bundled_request);
    }
}
