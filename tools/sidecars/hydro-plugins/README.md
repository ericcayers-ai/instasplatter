# Experimental / external hydro engines

These are **not** reconstruction sidecars. Hydro workers live under
`%LOCALAPPDATA%/InstaSplatter/engines/hydro/<name>/`.

| Folder | Tier | License | Notes |
| --- | --- | --- | --- |
| `triton` | Experimental | permissive (verify) | TRITON/Kokkos overland flow — external install |
| `wflow` | Experimental | MIT (verify) | Wflow.jl watershed — external Julia |
| `geoclaw` | Experimental | BSD-3 | Coastal / surge — external |
| `sfincs` | External plugin | **GPL** | Never bundle in Apache installer |
| `hipims` | External plugin | **GPL** | Never bundle |
| `bg-flood` | External plugin | **GPL** | Never bundle |
| `itzi` | External plugin | **GPL** | Never bundle |

## Install protocol

1. Install the engine yourself (binary / Julia / Python env).
2. Place files under `engines/hydro/<folder>/` with a `run` entrypoint.
3. Add `ACCEPTED` (permissive experimental) or `GPL_ACCEPTED` (GPL plugins).
4. Keep a `.stub` marker until the entrypoint is real — host refuses stubs.
5. Promotion to Standard requires the `HydroPromotionGates` checklist in
   `src-tauri/src/geospatial/hydro.rs` (benchmark, conservation, calibration,
   reproducibility, CPU/GPU tolerance, license clearance). GPL engines
   **cannot** promote into the Apache Standard installer.

The ANUGA runner in `tools/sidecars/anuga` is currently a compatibility
scaffold: it checks the ANUGA API with a bounded flat-bed evolve smoke but does
not sample the DEM, apply scenario forcing/boundaries, or calibrate. Its
checkpoints and final display products are synthetic and it always reports
`mode: scaffold`, even when ANUGA is installed. It is not a scientific solver
path. WebGPU/CPU live preview and demo fallback remain **non-authoritative**.
