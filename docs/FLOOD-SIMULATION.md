# Flood simulation: what is implemented, what it is worth, and what it is not

This document exists because the previous state of the repo had a flood module that *looked* finished
(types, meshes, exports, a UI badge) and was not usable as a simulation. The distinction that matters:

> A visual water effect makes water look plausible. A simulation produces numbers that can be wrong in
> a measured way. If you cannot say how wrong, you have a visualisation.

## Pipeline as implemented

```
GaussianPrimitive3D[]                       splat cloud (position, scale, rotation, opacity, colour)
  │ buildDemDomain()                        per-cell ground quantile from the BOTTOM of each Gaussian's
  │                                         vertical extent (strips canopy/floaters), explicit nodata
  │                                         mask, datum + provenance recorded
  ├─ fillDepressionsPriorityFlood()         Barnes (2014) priority-flood; splat noise leaves 1-cell pits
  ├─ rasterizeDomainForSolver()             nodata cells become walls, never invented terrain
  ├─ createSweState() / stepSwe()           well-balanced HLL shallow-water solver
  ├─ wetConnectivity()                      connected-component audit of the wet mask
  └─ extractFloodWaterSurfaceMesh()         free-surface mesh with true surface normals
domainToAsciiGrid()                         ESRI ASCII grid (SFINCS / LISFLOOD-FP / Itzï / BG_Flood)
```

Files: `src/geospatial/hazards/{swe.ts, demDomain.ts, floodSim.ts, floodValidation.ts}`.

## The solver, stated precisely

Cell-centred finite volume, first-order Godunov, **HLL fluxes** with **hydrostatic
reconstruction** (Audusse et al. 2004) and the matching bed-slope source, **Toro dry-bed wave-speed
estimates** at wet/dry interfaces, semi-implicit Manning friction, CFL-limited sub-stepping, all state
in `Float64Array`.

Two bugs found by building the benchmarks, both of which had made the previous module unusable:

1. **A closed boundary needs a momentum reaction, not a zero flux.** Treating walls as "no flux" for
   both mass and momentum accelerated the boundary cells (a flat tank at rest reached 78 m/s).
2. **The plain min/max wave-speed estimate kills the wetting front.** At a wet/dry interface it
   collapses the mass flux to zero; the Ritter front stalled at 70–100 m instead of 237.7 m.

A third finding shaped the design: an HLL scheme with a hydrostatic-reconstruction source whose
sensitivity does not match the HLL pressure term develops an **exponential instability that does not
vanish under dt refinement** (measured: 1e-14 → 1e2 momentum in 64 s at cfl 0.45, 0.1 and 0.02). An
earlier draft used that form; the shipped solver passes the rest-state tests, which is the check that
would have caught it.

## Validation: measured, not asserted

`npx tsx src/geospatial/hazards/floodValidation.smoke.ts` prints 27 checks and fails on any miss.
Current values:

| Benchmark | Result | Threshold |
|---|---|---|
| Lake at rest, flat bed | max \|η−η₀\| = 0 m, max \|u\| = 0 m/s | ≤ 1e-6 |
| Lake at rest, bumpy bed | 3.6e-15 m, 4.2e-14 m/s | ≤ 1e-6 |
| Lake at rest, dry shoreline | 0 m, 1.3e-15 m/s | ≤ 1e-6 |
| Ritter dry-bed dam break, dx = 10 m, 12 s | depth L1 = 0.89% | ≤ 2% |
| Ritter front position | 220 m vs 237.7 m analytic (1.77 cells) | ≤ 3 cells |
| Grid convergence (dx 20/10/5) | L1 0.0101 → 0.0074 → 0.0050, order ≈ 0.44–0.57 | strictly decreasing, order > 0.25 |
| Mass conservation, rain + point source, closed | 1.4e-15 relative | ≤ 1e-9 |
| Positivity repairs | 0 | = 0 |
| Determinism | bitwise identical repeat | true |
| Realised Courant | 0.45 | ≤ 0.45 |

The 0.44–0.57 observed order is the honest number for a first-order scheme on a dry front; it is not
"approximately first order" in the flattering sense, and the docstring in `swe.ts` says so.

## What this is NOT

- **Not a full Saint-Venant code.** First order, no MUSCL reconstruction, no hydraulic structures, no
  infiltration, no wind, no Coriolis, no turbulence, no non-Cartesian grid, no sediment.
- **Not calibrated.** Manning n is a scalar (or a supplied array); there is no land-cover roughness
  map, so a mixed urban/vegetated scene carries a modelling error of the same order as its DEM error.
- **Not datum-complete.** The domain records a `verticalDatum` field and defaults to
  `local-arbitrary`; there is no geoid correction and no CRS transform on the export beyond the ENU
  origin written into the ASCII header.
- **Not authoritative.** No comparison against an external validated engine (ANUGA, GeoClaw, SFINCS)
  has been run. Until one is, this is an engineering estimate that is *internally* consistent.
- **Not connected to the UI.** The result object carries coverage, nodata count, depression-fill
  report, connectivity and mass error; no screen displays them yet (gap #5 in the audit).

## Promotion bar (in order, cheapest first)

1. Show the provenance block and the mass error in the UI next to the water surface.
2. Add a roughness raster path (land-cover → Manning n) with a documented source.
3. Cross-validate one real AOI against ANUGA or GeoClaw; publish depth-extent disagreement.
4. Add the UK EA 2D benchmarking suite cases (floodplain, urban, structures) as regression tests.
5. Only then consider a badge other than "Live preview / engineering estimate".

The rules in `HydroPromotionGates` (`src-tauri/src/geospatial/hydro.rs`) already encode the science
gates for *external* engines; this module needs the equivalent, and items 1–3 are its first three.

## Mesh vs raster, decided

The research sweep (`docs/research/D-mesh-flood.md`) is blunt: for 2D shallow-water flooding the
correct interchange product is a **metric, datum-referenced 2.5D DEM with an explicit nodata mask**,
not a watertight triangle mesh. Mesh watertightness matters only for unstructured-mesh solvers
(ANUGA, TELEMAC, BASEMENT). That is why the primary export here is the ESRI ASCII grid, and why the
ground-extraction, nodata and depression-filling rules got the engineering time instead of a
splat→mesh generator.
