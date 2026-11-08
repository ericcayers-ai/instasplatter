# Repository inventory and selection — 2026-11-03

Consolidated from four verified sweeps. The per-topic evidence (stars, SPDX licence, last push, one-line
technique, applicability verdict per repo) lives in:

| File | Topic | Data rows | Unique GitHub repos |
|---|---|---|---|
| `docs/research/A-rendering.md` | rasterisers, sorting-free, LOD/streaming, viewers, AA, formats | 64 | 57 |
| `docs/research/B-geospatial.md` | georeferenced 3DGS, CRS/precision, 3D Tiles, DEM sources | 47 | 44 |
| `docs/research/C-vps.md` | VPS, visual localisation vs splats, place recognition, PnP, SLAM | 57 | 51 |
| `docs/research/D-mesh-flood.md` | splat→mesh, surface reconstruction, flood solvers, DEM conditioning | 64 | 62 |
| | **total** | **232** | **183 unique** |

Metadata method and its limits, stated in each file: stars/licence/last-push were fetched from the
GitHub REST API; the unauthenticated quota (60/h) ran out mid-sweep, after which rows were either
fetched through a read-only proxy or marked `[n/r]` (not retrieved) and **must not be quoted**. Nothing
in those tables is from memory. `NOASSERTION` is GitHub's "LICENSE file exists but matches no SPDX
identifier" — it is not a permissive licence and it is not a licence name.

This file is the *decision* layer: what was adopted, what was adapted, what was rejected and why.

## Adopted now (implemented in this pass)

| Item | Licence | Why | Where |
|---|---|---|---|
| Hierarchical chunk culling (Hierarchical-3DGS / Octree-GS principle, clean-room) | concept only (NOASSERTION upstream) | The one change that makes frame cost follow what is on screen: 93.5% less frame time measured | `src/splat/frustum.ts`, `worker.ts`, `renderer.ts`, `docs/PERF.md` |
| Camera-relative / RTC transforms (Cesium `RTC_CENTER` principle) | concept | float32 ENU loses 1 mm at 32.8 km; camera-relative error is 512× smaller at 21 km from origin | `src/geospatial/precision.ts`, splat vertex shader |
| Priority-flood depression filling (Barnes 2014) | public method | every raster solver needs it or water is trapped in splat noise | `src/geospatial/hazards/demDomain.ts` |
| Audusse hydrostatic reconstruction + Toro dry-bed wave speeds | public method | the only way a solver holds a lake at rest *and* advances a dam-break front | `src/geospatial/hazards/swe.ts` |
| ESRI ASCII grid as the flooding interchange format | de-facto standard | what SFINCS / LISFLOOD-FP / Itzï / BG_Flood actually read | `floodSim.ts::domainToAsciiGrid` |
| Analytic benchmarks as CI gates (Ritter 1892, lake at rest) | public method | turns "validated" into an exit code | `floodValidation.smoke.ts` |
| Derived position uncertainty instead of tuned DOP constants | — | the old HDOP/VDOP were `geomFactor × 0.45`; the new bound has a formula and a stated limit | `src/geospatial/vps.ts` |

## Adopt next (ranked, with the reason it is not in this pass)

1. **`playcanvas/splat-transform` (MIT, 1,409★ at sweep time) — Streamed SOG export, then a loader.**
   It is the only mature MIT CLI that writes a multi-LOD
   streamed splat tree, which is the actual answer to 100M+ splats. 2–3 days for export, more for a
   reader. Blocked here only by time, not by licence.
2. **`KHR_gaussian_splatting` + 3D Tiles 2.0** (Khronos spec is royalty-free; CesiumGS Apache-2.0).
   The correct way to put a capture on the globe at its real location. Needs the tileset writer.
3. **`nianticlabs/spz` (MIT)** round-trip. The repo writes SPZ v4 and decodes only inside tests — a
   self-inflicted interoperability hole.
4. **`lzhnb/Analytic-Splatting` (MIT)** closed-form pixel integration. A fragment-shader change that
   removes residual shimmer on the WebGL2 path the app already ships.
5. **`nff747/splat-bvh-core` (Apache-2.0)** WGSL radix sort + LBVH, and a WebGPU backend behind a
   feature flag. Needed only above ~20M splats in one view; see the verdict doc.

## Adapt (concept, not code)

| Item | Why concept only |
|---|---|
| `playcanvas/supersplat` (MIT) Streamed SOG budget policy (2M/4M desktop) | Its LOD budget policy is the reference; the app has no streaming yet |
| `sparkjsdev/spark` (MIT) WebGL2 format breadth | Differential-test oracle once a fixture corpus exists; not a dependency |
| `ubc-vision/stochasticsplats` (MIT) stochastic transparency | The sampling idea is already implemented; the CUDA code is not portable |
| `maturk/dn-splatter` (Apache-2.0) depth+normal priors | Already the reference for `mesh/`'s TSDF path |
| CityGaussian / LoDOfGaussians / CLoD-GS partitioning | `NOASSERTION` or no licence — concept only, and the partition idea is now the chunk index |
| SFINCS / GeoClaw / ANUGA as authoritative solvers | Not bundled: GPL for SFINCS/HiPIMS/BG_Flood, and ANUGA is already a scaffold. Cross-validation is the right use, not bundling |

## Rejected, with the reason

| Item | Reason |
|---|---|
| Vendoring `graphdeco-inria/gaussian-splatting` or `diff-gaussian-rasterization` | Inria non-commercial lineage (`NOASSERTION`); algorithm ancestor only, training goes through Brush/gsplat |
| Vendoring 2DGS / SuGaR / GOF / PGSR / RaDe-GS | Non-commercial or `NOASSERTION`; the mesh path reimplements the ideas from documented methods |
| Replacing the WebGL2 renderer with a WebGPU point-splatting backend now | No measured gain on this hardware, a real regression risk, and no adapter fallback. Decision + evidence in `docs/POINT-SPLATTING-VERDICT.md` |
| `Auratons/renderer_surface_splatting` (GPL-3.0), `Itzï` (GPL-2.0), `RichDEM`/`pysheds` (GPL-3.0) | Copyleft contaminates an Apache-2.0 installer; the methods are reimplemented instead (priority flood) |
| Repos with **no licence file** (`fatPeter/mini-splatting`, `CLoD-GS`, `WebDGS`, `GS2Mesh`) | Cannot legally ship; excluded even as vendored reference |
| `m-niemeyer/radsplat` as a code source | The repo is a project page; the "900+ FPS" claim cannot be reproduced from it |
| Feed-forward NVS products (AnySplat, Gauzilla, Long-LRM) | Different product surface, not a reconstruction backend |
| `Belluxx/Aleph` Google half (satellite/Street View/Earth mesh) | Terms-of-service violation the upstream README admits; the open-data half is already covered by the DEM connectors |

## Previously-mislabelled items now recorded honestly

`reelkit/DIGESTS.md` §61, §72, §83 state that PolyLayout, ABot-Recon and SUPRA-Tex were "recorded in
`instasplatter/docs/PAPER-SWEEP-2024+.md`". They were not (`grep` returns nothing). They are recorded
here instead:

| Item | Source | Relevance | Decision |
|---|---|---|---|
| `amap-cvlab/abot-recon` (Apache-2.0) | reel §72 | Constant-VRAM 12-frame sliding-window reconstruction for 8.6 km / 48,656 frames — the right shape for long drone video | **adapt**: the windowing contract is what `pipeline/`'s frame gate should adopt; weights are VRAM-gated on 8 GB, so not shipped |
| `baranowskibrt/onecanvas` (MIT) | reel §40 | Panorama reprojection for 3D scene understanding | **reference**: needs depth + poses, which this app has, but no product use yet |
| `DbzU9xZxGfR` PolyLayout | reel §61 | Joint multi-room indoor layout | **reference**: indoor layout prior, not aerial/geospatial |
| `supra-3d/supra-tex` (MIT) | reel §83 | Multi-view consistent PBR texturing from one image | **defer**: needs >8 GB VRAM or cloud inference |
| `opengeos/GeoLibre` | reel §60 | Local-first COG/PMTiles/building-extrusion GIS | **adapt**: the local-first COG+PMTiles pattern for the globe view |
| `Belluxx/Aleph` (MIT) | reel §39 | "download a piece of the world" | **adapt (open-data half only)**: OSM + AWS terrain tiles via the existing DEM connectors |

## How to keep this current

- Re-run the sweep with an authenticated GitHub token to fill the `[n/r]` rows (5,000 req/h vs 60).
- Every adopted item above names the file that implements it; if the file goes away, the row is wrong.
- Anything labelled `NOASSERTION` must be resolved by reading its `LICENSE` file before code is copied.
  That has not been done for the 17 such repos in A-rendering, and this document does not pretend
  otherwise.
