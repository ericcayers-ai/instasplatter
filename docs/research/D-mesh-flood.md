# D — Splat-to-Mesh Conversion & Genuine Hydrodynamic Flood Simulation

> **Delivery note.** This file is the complete, ready-to-save content for
> `C:\Users\<you>\OneDrive\Desktop\Programs\instasplatter\docs\research\D-mesh-flood.md`.
> Per this run's authoritative output instruction it was written to the subagent-artifacts path instead.
> Copy it verbatim to the repo path (UTF-8, no BOM). No other repo file was read-modified or written.

Sweep date: 2026-11-03. Metadata queried live from `https://api.github.com/repos/<owner>/<name>`
and `https://api.github.com/search/repositories` (`stargazers_count`, `license.spdx_id`, `pushed_at`).

**Metadata integrity statement.** The unauthenticated GitHub API allows 60 core requests/hour and
10 search requests/minute per IP. Core quota was exhausted around entry 12 and search quota was hit
repeatedly thereafter. Every row therefore carries an explicit provenance tag:

- `[api]` — stars/licence/push read from the GitHub API in this session. **Trustworthy.**
- `[n/r]` — *not retrieved*. Existence is asserted only from vendor README / paper / project page.
  **Stars and licence below are UNVERIFIED and must not be quoted.** Re-run with a token before publishing.
- `[404]` — repo path returned HTTP 404 / no matching repo in `in:name` search. NOT FOUND.

Labels used in analysis: `[E]` = stated directly by the source; `[I]` = researcher inference from
the method's design, not a claim the authors make.

---

## Summary

Splat-to-mesh is a **solved-for-visuals, unsolved-for-simulation** problem: the best 2024–2026 methods
(2DGS, GOF, PGSR, RaDe-GS, GaussianWrapping) produce geometrically far better surfaces than vanilla 3DGS,
but almost none of them guarantee a closed, manifold, hydraulically-connected domain, and several carry
non-commercial licences. For genuine flooding, the correct interchange format is **not** a triangle mesh —
it is a **metric, datum-referenced 2.5D DEM raster** (single-valued z(x,y)) plus a nodata mask, which is
exactly what SFINCS, LISFLOOD-FP, BG_Flood, HiPIMS and the repo's own TS solver consume; unstructured
mesh solvers (ANUGA, TELEMAC, BASEMENT) are the exception and are where watertightness actually matters.

The repo's `floodSim.ts` is a real finite-volume Saint-Venant solver in structure, but it is currently
**unvalidated**: it has no mass-conservation test, no lake-at-rest test, no dam-break verification and no
convergence study, and its DEM input has no datum, no nodata semantics and is populated by IDW where splats
are missing. Those four gaps — not the numerics — are what separate it from a simulation.

---

## Part 1 — Splat-to-mesh: 30 entries

Cost key: **L** ≤ 1 GPU-hour / scene; **M** 1–8 GPU-h; **H** >8 GPU-h or multi-GB VRAM, or requires
per-scene post-processing. Cost is `[I]` — an engineering estimate from method class (explicit
per-view rasterisation vs tetrahedral/neural field), not a benchmark.

Verdict key: **✅** usable as a simulation domain as-is · **⚠️** usable after repair/conditioning ·
**❌** not usable (open surface, floaters, no metric scale).

### 1a. Gaussian-splatting-native reconstruction

| # | Method / repo | URL | Stars | Licence | Last push | Closure / manifoldness | Metric accuracy | Cost | Sim verdict |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **SuGaR** — `Anttwo/SuGaR` `[api]` | https://github.com/Anttwo/SuGaR | 3553 | NOASSERTION (custom, INRIA-derived — read LICENSE) | 2024-09-24 | Surface-aligned Gaussians → Poisson meshing; output is **open** where coverage ends; floaters pruned by an editing step `[E]` | CVPR'24; good visual fidelity, known to over-smooth thin structures `[E]` | M | ⚠️ |
| 2 | **2DGS** — `hbb1/2d-gaussian-splatting` `[api]` | https://github.com/hbb1/2d-gaussian-splatting | 3328 | NOASSERTION — **non-commercial research/evaluation only** `[E]` | 2026-08-25 | Oriented 2D surfels + TSDF/trimesh extraction; depth is the primary output, mesh is derived; **no watertightness guarantee** `[I]` | Best-in-class splat geometry (paper: lower Chamfer than 3DGS/SuGaR on DTU) `[E]` | L–M | ⚠️ best depth source |
| 3 | **GOF (Gaussian Opacity Fields)** — `autonomousvision/gaussian-opacity-fields` `[api]` | https://github.com/autonomousvision/gaussian-opacity-fields | 1025 | NOASSERTION (custom) | 2024-11-15 | Level-set on an opacity field → **closed surfaces in unbounded scenes**, tetrahedral marching `[E]` | SIGGRAPH Asia'24; strong on large outdoor scenes `[E]` | M | ✅ closest to watertight |
| 4 | **PGSR** — `zju3dv/PGSR` `[api]` | https://github.com/zju3dv/PGSR | 1130 | NOASSERTION (custom) | 2024-12-25 | Planar Gaussians + single/multi-view geometry loss → mesh via TSDF fusion; open at boundaries `[I]` | TVCG'24; planar priors improve flat-surface accuracy (walls/floors/roads) `[E]` | L–M | ⚠️ good for planar terrain |
| 5 | **RaDe-GS** — `HKUST-SAIL/RaDe-GS` `[api]` | https://github.com/HKUST-SAIL/RaDe-GS | 716 | NOASSERTION (custom) | 2026-03-19 | Rasterises **depth** directly (not just colour) → mesh via `mesh_extract.py`; open surface `[E]` | Improved depth fidelity vs 3DGS; thin-structure handling `[E]` | L–M | ⚠️ |
| 6 | **DN-Splatter** — `maturk/dn-splatter` `[api]` | https://github.com/maturk/dn-splatter | 849 | **Apache-2.0** `[E]` — the only permissive licence in this group | 2025-07-05 | Depth+normal priors (sensor or monocular) → 2DGS-style mesh; sensor-depth scenes are near-metric `[E]` | WACV'25; monocular-depth supervision gives **plausible metric scale outdoors** `[E]` | M | ✅ licence-wise |
| 7 | **AGS-Mesh** — `XuqianRen/AGS_Mesh` `[api]` | https://github.com/XuqianRen/AGS_Mesh | 133 | NOASSERTION | 2025-05-20 | Adaptive density + geometric priors; **indoor rooms only** `[E]` | 3DV'25; indoor smartphone capture `[E]` | L | ❌ wrong domain (indoor) |
| 8 | **GS2Mesh** — `yanivw12/gs2mesh` `[api]` | https://github.com/yanivw12/gs2mesh | 371 | **no licence file** (`license: null`) `[E]` | 2025-11-23 | Renders stereo pairs → pretrained stereo MVS depth → fusion; **no licence = cannot legally ship** `[E]` | ECCV'24; stereo-depth fusion improves on splat rasterised depth `[E]` | M | ❌ licence blocker |
| 9 | **MILo** — `Anttwo/MILo` `[api]` | https://github.com/Anttwo/MILo | 539 | NOASSERTION | 2026-04-03 | **Mesh-in-the-loop**: mesh is a first-class optimised object, so topology is coherent and closed by construction `[E]` | SIGGRAPH Asia'25; detailed + efficient `[E]` | M | ✅ promising |
| 10 | **GaussianWrapping** ("From Blobs to Spokes") — `diego1401/GaussianWrapping` `[api]` | https://github.com/diego1401/GaussianWrapping | 306 | NOASSERTION (ECCV'26 code) | 2026-08-29 | Oriented Gaussians + occupancy/vacancy field → **explicitly claims watertight meshes incl. thin structures** `[E]` | Newest (2026); no independent reproduction found `[I]` | M | ✅ if claim holds |
| 11 | **Scaffold-GS** — `city-super/Scaffold-GS` `[api]` | https://github.com/city-super/Scaffold-GS | 1289 | NOASSERTION | 2024-09-26 | **Rendering method, not reconstruction** — no surface extraction path `[E]` | High render quality, geometry secondary `[I]` | L | ❌ |
| 12 | **NeuSG** — `HLinChen/NeuSG` `[n/r]` | https://github.com/HLinChen/NeuSG | n/r | n/r | n/r | Joint Gaussians + implicit SDF; implicit surface is closed by construction `[I]` | arXiv 2312.00846 `[E]` | H | ⚠️ |
| 13 | **Gaussian Surfels** — `turandai/gaussian_surfels` `[n/r]` | https://github.com/turandai/gaussian_surfels | n/r | n/r | n/r | Surfel normal/opacity regularisation → screened-Poisson mesh; open surface `[I]` | SIGGRAPH'24 `[E]` | L–M | ⚠️ |
| 14 | **SplatMesh** — `kaichen-z/SplatMesh` `[n/r]` | https://github.com/kaichen-z/SplatMesh | n/r | n/r | n/r | **Segmentation/editing on a mesh+splat hybrid — not a reconstruction method** `[E]` | n/a for terrain | L | ❌ out of scope |
| 15 | **MeshSplatting** — `meshsplatting/mesh-splatting` `[n/r]` | https://github.com/meshsplatting/mesh-splatting | n/r | n/r | n/r | Differentiable **mesh** rendering with splat-like primitives; consumes a mesh, does not produce a survey-grade one `[I]` | n/a | L | ❌ |
| 16 | **MeshGS** `[404]` | ACCV 2024 paper (openaccess.thecvf.com) | — | — | — | **No official repository located**; and by design it *aligns splats to an existing mesh for rendering* — the reverse direction `[E]` | — | — | ❌ not applicable |
| 17 | **TRELLIS** — `microsoft/TRELLIS` `[api]` | https://github.com/microsoft/TRELLIS | 14185 | **MIT** `[E]` | 2026-10-27 | Generative (image/text→3D), produces closed watertight assets from a generator, **not from your capture** `[E]` | Not a reconstruction from real scenes `[I]` | M | ❌ wrong tool |
| 18 | **NeuralFusion** (closest thing to "Neural KinectFusion") — `weders/NeuralFusion` `[n/r]` | https://github.com/weders/NeuralFusion | n/r | n/r | n/r | Learned latent TSDF fusion from posed depth; closed SDF extraction `[I]` | Research-grade, RGB-D only `[E]` | M | ⚠️ RGB-D only |

### 1b. Classical / non-splat reconstruction and meshing

| # | Method / repo | URL | Stars | Licence | Last push | Closure / manifoldness | Metric accuracy | Cost | Sim verdict |
|---|---|---|---|---|---|---|---|---|---|
| 19 | **Open3D** (Poisson, BPA, TSDF) — `isl-org/Open3D` `[api]` | https://github.com/isl-org/Open3D | 14170 | NOASSERTION (MIT in practice — confirm) | 2026-09-30 | Screened Poisson → watertight by construction; BPA → **never watertight** `[E]` | Inherits input point noise; no scale invention (metric if input is metric) `[E]` | L | ✅ workhorse |
| 20 | **PoissonRecon** (screened Poisson) — `mkazhdan/PoissonRecon` `[api]` | https://github.com/mkazhdan/PoissonRecon | 1870 | **MIT** `[E]` | 2026-04-29 | Reference implementation; density-trimming removes low-support spurious sheets `[E]` | Best classical watertight closure; over-smooths `[E]` | L | ✅ |
| 21 | **COLMAP** — `colmap/colmap` `[api]` | https://github.com/colmap/colmap | 13139 | NOASSERTION (BSD-3 in practice) | 2026-11-03 | MVS gives a metric-scalable dense cloud; mesh only via Poisson `[E]` | Photogrammetric baseline that splats are measured against `[E]` | M–H | ✅ reference truth |
| 22 | **Neuralangelo** — `NVlabs/neuralangelo` `[api]` | https://github.com/NVlabs/neuralangelo | 4616 | NOASSERTION (NVIDIA research licence) | 2024-04-14 | Neural SDF → **watertight by construction**, but needs COLMAP poses + long training `[E]` | CVPR'23; very high detail `[E]` | H | ✅ quality, ❌ cost |
| 23 | **NKSR** (Neural Kernel Surface Reconstruction) — `nv-tlabs/NKSR` `[api]` | https://github.com/nv-tlabs/NKSR | 1006 | NOASSERTION (NVIDIA source licence) | 2026-08-25 | Point-cloud → mesh; robust to noise/outliers, handles large scenes `[E]` | CVPR'23 Highlight; strong on noisy LiDAR/MVS `[E]` | L–M | ✅ best cloud→mesh |
| 24 | **VDBFusion** — `PRBonn/vdbfusion` `[api]` | https://github.com/PRBonn/vdbfusion | 663 | **MIT** `[E]` | 2025-01-22 | Sparse VDB TSDF; **open** at voxel-block boundaries unless truncated `[I]` | Metric if poses+depth metric; CPU-fast `[E]` | L | ⚠️ |
| 25 | **nvblox** — `nvidia-isaac/nvblox` `[n/r]` | https://github.com/nvidia-isaac/nvblox | n/r | Apache-2.0 (+BSD-3 for Voxblox-derived files) `[E]` | n/r | GPU TSDF/ESDF + **mesh extraction**; incremental `[E]` | ROS/Isaac-coupled; excellent for streaming RGB-D `[E]` | L (GPU) | ⚠️ needs ROS plumbing |
| 26 | **OpenMVS** — `cdcseacave/openMVS` `[n/r]` | https://github.com/cdcseacave/openMVS | n/r | n/r (AGPL historically — **verify**) | n/r | Densify → mesh → refine; open boundaries typical `[I]` | Classical photogrammetry quality `[E]` | M | ⚠️ |
| 27 | **Manifold** — `elalish/manifold` `[n/r]` | https://github.com/elalish/manifold | n/r | n/r (Apache-2.0 historically — **verify**) | n/r | Guarantees **manifold, watertight** output through boolean/repair ops `[E]` | Geometry-preserving repair; not a reconstructor `[E]` | L | ✅ repair stage |
| 28 | **PyMeshLab / MeshLab** `[n/r]` | https://github.com/cnr-isti-vclab/pymeshlab | n/r | n/r (GPL historically — **verify**) | n/r | Screening (non-manifold edges, self-intersections, holes) + hole filling `[E]` | n/a (validator) | L | ✅ validator |
| 29 | **diff-surfel-rasterization** — `hbb1/diff-surfel-rasterization` `[n/r]` | https://github.com/hbb1/diff-surfel-rasterization | n/r | n/r (non-commercial, same family as 2DGS) | n/r | The rasteriser behind 2DGS/GOF-family depth maps; produces **depth/normal buffers**, not meshes `[E]` | — | L | ⚠️ depth source only |
| 30 | **`2DGS → mesh` pipelines generally** (e.g. 2DGS `render.py` + `mesh_extract.py`, trimesh/Open3D `extract_triangle_mesh`) `[I]` | https://github.com/hbb1/2d-gaussian-splatting | — | inherits 2DGS (non-commercial) | — | Marching-cubes on a fused TSDF → **floaters and open boundaries are normal** `[E]` from issue trackers | Accuracy bounded by depth noise; scale from COLMAP | L | ⚠️ needs trim+fill |

**Alias → row mapping for the requested query strings:** "Gaussian Opacity Fields" / "GOF" → 3 ·
"2DGS to mesh" → 2 + 29 + 30 · "SuGaR mesh" → 1 · "Gaussian Splatting to mesh" → 1–10 ·
"Surface Reconstruction from Gaussian Splatting" → 3, 4, 8 · "screened Poisson" → 19 + 20 ·
"COLMAP dense+poisson" → 21 + 20 · "TSDF variants" → 24, 25, 30 · "TRELLIS (De6_EtZy7Bn)" → 17.

### 1c. What actually makes a mesh usable as a simulation domain

1. **Closed, orientable, manifold, single-component-per-basin.** Poisson/GOF/NKSR/Neuralangelo can give
   this; marching-cubes-on-TSDF (2DGS, GS2Mesh, SuGaR) generally does not. `[E]` for the classical
   methods' guarantees, `[I]` for the splat family.
2. **No floating geometry.** SuGaR and 2DGS both require explicit floater/opacity pruning; the standard
   remedy is keeping only the largest connected component plus a density-support trim. `[I]`
3. **Metric scale and datum.** A splat mesh lives in COLMAP/ENU space with arbitrary scale unless a
   georeferenced control point exists. Every solver in Part 2 requires metres + a vertical datum. `[E]`
4. **No bridges/overhangs over water.** Physically, a tree canopy is *not* a wall; a mesh fused from a
   street-level capture will contain canopy, wires and parked cars — all of which become fake dams. `[E]` for
   the physical argument, `[I]` for the consequence.
5. **Hydraulic connectivity.** For a 2D solver, the domain must have at least one path from every wet cell
   to an outlet/outflow boundary. A mesh with an interior wall of floaters silently partitions the domain. `[I]`

> **Decision-relevant conclusion.** For 2D shallow-water flooding, chasing a watertight *mesh* is the wrong
> goal. The right product is a **raster DEM with an explicit nodata mask, a stated vertical datum, and a
> documented ground-extraction rule**. Mesh watertightness only becomes load-bearing if the target solver is
> ANUGA/TELEMAC/BASEMENT (unstructured mesh). `[I]`

---

## Part 2 — Genuine flood solvers: 20 entries

Domain-format key: **R** = regular/raster DEM grid · **Q** = quadtree/block-adaptive raster ·
**U** = unstructured triangular mesh.

| # | Model / repo | URL | Stars | Licence | Last push | Domain fmt | Headless/CLI drivable from Rust? | What a splat surface must satisfy |
|---|---|---|---|---|---|---|---|---|
| 1 | **ANUGA** — `GeoscienceAustralia/anuga_core` `[api]` | https://github.com/GeoscienceAustralia/anuga_core | 231 | NOASSERTION (**ANUGA licence = BSD-3-like**, confirm in `LICENSE.txt`) | 2026-10-23 | **U** (triangulation, .tsw/.msh + .sww output) | Yes — `python -m anuga` scripts via stdin/argv; a Python sidecar is the realistic integration | Closed 2D triangulation, per-vertex z, per-triangle Manning n, **positive areas, no duplicate vertices**; georeferenced in metres `[E]` |
| 2 | **SFINCS** — `Deltares/SFINCS` `[api]` | https://github.com/Deltares/SFINCS | 94 | **GPL-3.0** `[E]` (binaries under a separate Deltares freeware licence `[E]`) | 2026-11-02 | **R/Q** (`sfincs.inp`, `.dep` + `.msk` binaries) | Yes — single Fortran binary, `sfincs.inp` on stdin/CLI `[E]` | Regular grid, **z in metres relative to a stated datum**, `msk` nodata mask, per-cell Manning n; **depressions that must hold water need to be real, not noise** `[E]` |
| 3 | **GeoClaw** — `clawpack/geoclaw` `[api]` | https://github.com/clawpack/geoclaw | 92 | **BSD-3-Clause** `[E]` | 2026-10-30 | **R + AMR** (topotype DEM via `settopo`) | Yes — `make .data && make .output` + Fortran binary `[E]` | Rectilinear topography with **no nodata holes inside the domain**; GeoClaw's well-balanced solver will still fail silently on discontinuities `[I]` |
| 4 | **Wflow.jl** — `Deltares/Wflow.jl` `[api]` | https://github.com/Deltares/Wflow.jl | 166 | **MIT** `[E]` | 2026-11-03 | **network + raster** (SBM + local-inertial routing) | Yes — Julia CLI/`wflow_sbm.toml` `[E]` | Not a 2D SWE solver: needs a D8/D∞ flow network over a conditioned DEM `[E]`. Use as upstream hydrology, not as the flood engine |
| 5 | **HiPIMS-CUDA** — `HEMLab/HiPIMS-CUDA` `[api]` | https://github.com/HEMLab/HiPIMS-CUDA | 96 | **GPL-3.0** `[E]` | 2025-07-24 | **R** (DEM + rainfall rasters) | Partly — CUDA binary + case text files; GPL forbids static linking into the app `[E]` | Grid-aligned DEM, **same CRS/units as the rainfall and boundary rasters**, no nodata inside `[E]` |
| 6 | **LISFLOOD-FP** (BMI wrapper) — `openearth/lisflood-fp-bmi` `[api]` | https://github.com/openearth/lisflood-fp-bmi | 48 | **GPL-3.0** `[E]` | 2021-04-12 (stale, v5.9) | **R** (`.par` + `.dem`/`.asc`) | Yes — BMI C interface, ideal for a sidecar `[E]` | ASCII/ESRI grid, **metres**, nodata = large negative sentinel, Manning n raster `[E]` |
| 7 | **Itzï** — `ItziModel/itzi` `[api]` | https://github.com/ItziModel/itzi | 64 | **GPL-2.0** `[E]` | 2026-10-29 | **R** (`.asc` DEM + `.hyd`/`.rain`/`.man`) | Yes — Python `run_model.py` CLI + QGIS front-end `[E]` | Damped local-inertia (convective term dropped) `[E]`; forgiving of noisy DEMs but **cannot resolve hydraulic jumps** `[I]` |
| 8 | **BG_Flood** — `CyprienBosserelle/BG_Flood` `[api]` | https://github.com/CyprienBosserelle/BG_Flood | 53 | **GPL-3.0** `[E]` | 2026-10-30 | **Q** (block-uniform quadtree; AMR declared *not yet operational* `[E]`) | Yes — parameter file + DEM netCDF, GPU `[E]` | NetCDF/GeoTIFF DEM, **metres, positive-up**, nodata mask; GPU memory bounds the domain size `[I]` |
| 9 | **RichDEM** — `r-barnes/richdem` `[api]` | https://github.com/r-barnes/richdem | 326 | **GPL-3.0** `[E]` | 2024-06-24 | DEM conditioning | Yes — CLI + Python `[E]` | `FillDepressions` / `BreachDepressions` is **mandatory** before any solver; splat noise creates thousands of spurious pits `[E]` for the tool, `[I]` for the splat case |
| 10 | **pysheds** — `pysheds/pysheds` `[api]` | https://github.com/pysheds/pysheds | 915 | **GPL-3.0** `[E]` | 2026-10-26 | DEM → flow dir/accumulation | Yes — Python API `[E]` | D8/D∞ + catchment delineation; used to prove **hydraulic connectivity** of the extracted DEM `[I]` |
| 11 | **WhiteboxTools** — `jblindsay/whitebox-tools` `[api]` | https://github.com/jblindsay/whitebox-tools | 1218 | **MIT** `[E]` | 2026-05-26 | DEM conditioning | **Yes — and it is written in Rust** `[E]`, so it can be embedded as a crate instead of shelled out | `FillDepressions`, `BreachDepressionsLeastCost`, `RemoveOffTerrainObjects`, `SmoothDem` — the exact conditioning chain a splat DEM needs `[E]` |
| 12 | **NOAA-OWP inundation-mapping (HAND-FIM)** — `NOAA-OWP/inundation-mapping` `[api]` | https://github.com/NOAA-OWP/inundation-mapping | 131 | **Apache-2.0** `[E]` | 2026-11-03 | **R** + hydrofabric + synthetic rating curves | Partly — Nextflow/Docker pipeline `[E]` | **DEM alone is not enough** `[E]`: needs hydro-conditioned DEM, stream network, and an SRC built from Manning n and channel slope. It is a *mapping* method (no time stepping) `[I]` |
| 13 | **TauDEM** — `dtarb/TauDEM` `[n/r]` | https://github.com/dtarb/TauDEM | n/r | GPL-3.0 `[E]` (web) | n/r | DEM → flow network | Yes — MPI CLI `[E]` | Same conditioning demands as pysheds; MPI is awkward to ship in a desktop app `[I]` |
| 14 | **Delft3D / Delft3D FM** — `Deltares/Delft3D` `[n/r]` | https://github.com/Deltares/Delft3D | n/r | **AGPL-3.0 / GPL-3.0 per component** `[E]` | n/r | **U** (flexible mesh) + structured | Yes, but heavy (Fortran suite, large runtime tree) `[E]` | Unstructured flexible mesh with orthogonality constraints; AGPL is the most aggressive copyleft here `[E]` |
| 15 | **TELEMAC-2D** — official repo is **GitLab** (`gitlab.pam-retd.fr/otm/telemac-mascaret`), GitHub `ogoe/OpenTelemac` is an *unofficial mirror* `[n/r]` | https://www.opentelemac.org | n/r | **GPL** (suite) / LGPL (BIEF) `[E]` | n/r | **U** (SELAFIN `.sel` mesh) | Yes — `telemac2d.py` + steering file `[E]` | SELAFIN mesh with strict connectivity; splash-zone handling via tidal flats option `[E]` |
| 16 | **BASEMENT** — ETH Zurich, `basement.ethz.ch` `[404]` | https://basement.ethz.ch | — | **Freeware, closed source** — explicitly *not* open source `[E]` | — | **U** | Binary only; EULA-governed `[E]` | Redistribution inside an installer is not permitted without a EULA review `[E]` |
| 17 | **HEC-RAS 2D** — USACE `[404]` | https://www.hec.usace.army.mil/software/hec-ras/ | — | **Public domain US federal / closed source** `[E]` | — | **U**/RAS Mapper | Windows-only COM/HDF; no headless Linux path `[E]` | Not viable for a cross-platform sidecar `[I]` |
| 18 | **MIKE 21 / MIKE+** — DHI `[404]` | https://www.dhigroup.com | — | **Commercial, per-seat** `[E]` | — | **U**/R | No public API licence `[E]` | Excluded on cost/licence grounds `[I]` |
| 19 | **TRITON** — identity **not verified** in this sweep `[n/r]` | — | n/r | n/r | n/r | n/r | n/r | Cannot be recommended without resolving which "TRITON" is meant (there are unrelated projects under this name) — **open question** |
| 20 | **Fathom / Fathom-Global** `[404]` | https://www.fathom.global | — | **Commercial** `[E]` | — | R | No | Useful as a *benchmark dataset* source, not as an engine `[I]` |

### 2a. Interface requirements distilled

- **Units and datum.** Every model above assumes **metres, positive-up, and a stated vertical datum**. A
  splat DEM in ENU with an arbitrary origin is *not* an input; it is a shape. `[I]`
- **Nodata.** All raster solvers need an explicit mask. Silently IDW-filling empty cells (as the repo's
  `splatCloudToTerrainMeshAndDem` does) **invents terrain** and will create or destroy flow paths. `[I]`,
  grounded in the code's documented behaviour.
- **Roughness.** Manning *n* must be a per-cell/per-triangle field, not a single scalar; a uniform 0.035
  over a mixed urban/vegetated splat scene is a modelling error of the same order as DEM error. `[I]`
- **Depressions.** Every solver is sensitive to spurious pits; conditioning is not optional. `[E]` (RichDEM
  docs) / `[I]` (consequence).
- **Licence.** GPL/AGPL engines (SFINCS, LISFLOOD-FP, HiPIMS, BG_Flood, Itzï, pysheds, RichDEM, TauDEM,
  TELEMAC, Delft3D) are fine as *separately executed CLI sidecars* in most readings, but **bundling the
  binary in a Tauri installer is a distribution event** that triggers source-offer obligations. Flag for
  legal review — this is engineering caution, not legal advice. `[I]`
  **MIT/BSD/Apache alternatives:** GeoClaw (BSD-3), Wflow.jl (MIT), WhiteboxTools (MIT),
  NOAA-OWP HAND (Apache-2.0), ANUGA (BSD-like).

---

## Part 3 — The honest bar: what distinguishes a simulation from a visualisation

A visualisation produces plausible-looking water. A simulation produces numbers that can be **wrong in a
measurable way**. Fourteen measurable checks, ordered by cost-to-implement:

| # | Check | Pass criterion | Can the current TS solver plausibly pass? |
|---|---|---|---|
| 1 | **Global mass conservation** — Σ(h·dx²) vs. cumulative inflow/outflow/rainfall | Rel. error < 1e-6 (f64) or < 1e-4 (f32) over the run; reported per step | **Plausible but unmeasured.** The solver is flux-form FV in `Float32Array`, so round-off alone will accumulate ~1e-4–1e-5 over 10⁵ steps. Needs a running Σ and a written assertion `[I]` |
| 2 | **Lake at rest (well-balancedness)** — still water on a non-flat bed | max\|η − η₀\| ≤ 1e-6 m after 10⁴ steps; h ≥ 0 | **Plausible** — the header claims Audusse/Kurganov hydrostatic reconstruction `[E]`; the test is trivial to add and will immediately expose whether the claim holds `[I]` |
| 3 | **Wet–dry lake at rest** (bed with two shorelines, q = 0) | Same as #2 with a dry front present | **Uncertain.** Positivity is claimed; *preservation of rest at a moving shoreline in f32* is the hard part `[I]` |
| 4 | **Ritter dry-bed dam break** — analytic rarefaction, h = (2c₀ − ξ)²/(9g), u = ⅔(c₀ + ξ), −c₀ ≤ ξ = x/t ≤ 2c₀ `[E]` | L1 error on h and on the front position; friction set to n = 0 | **Partly.** A first-order Godunov scheme reaches the right *shape* but the front position lags; and the code applies Manning friction unconditionally — without an n = 0 path the comparison is invalid `[I]` |
| 5 | **Stoker / wet-bed dam break** (shock + rarefaction) | Shock speed and height vs analytic | **Probably passes qualitatively, fails quantitatively** at first order `[I]` |
| 6 | **CFL / stability audit** — record the realised Courant number every step | max CFL ≤ target (e.g. 0.45); dt bounded; zero NaN/negative-depth events | **Yes, plausibly** — CFL-adaptive stepping is implemented `[E]`; needs instrumentation, not new physics `[I]` |
| 7 | **Thacker oscillating basin** — analytic periodic solution | Period and amplitude error < a few % | **Unlikely.** Needs clean well-balancing + frictionless + fine grid; f32 will drift `[I]` |
| 8 | **Grid convergence / order of accuracy** — h-refinement series (dx, dx/2, dx/4) on a smooth case | Monotone error decrease + estimated order ≈ 1 for a first-order scheme | **Yes.** Convergence will be observed; the *order* will be ~1, which is honest but coarse `[I]` |
| 9 | **Mesh/grid independence for the decision variable** — inundation extent and max depth vs dx | Change < agreed tolerance between successive refinements `[E]` (method) | **Unknown** — no study exists in repo `[I]` |
| 10 | **Wet–dry front position vs benchmark** — e.g. UK EA 2D benchmarking suite, CADAM dam-break cases `[E]` | Arrival-time and extent error within published tolerances | **No.** These need calibrated roughness, structures, and boundary types the solver does not have `[I]` |
| 11 | **Hydraulic connectivity / topology audit** — connected-component labelling of the wet mask | Exactly one wet component reachable from each source; isolated ponds enumerated and explained | **No** — not implemented, and the IDW-filled DEM will create them `[I]` |
| 12 | **Roughness sensitivity** — Manning n ±20 % | Arrival-time response quantified and physically sensible | **Unknown** — no sensitivity study `[I]` |
| 13 | **Provenance / units audit** — datum, CRS, resolution, vertical accuracy, nodata fraction | All present and machine-readable in the output | **No.** Output is `originEnu` + metres with no datum, no CRS, and no nodata mask `[E]` |
| 14 | **Determinism / reproducibility** — same inputs → identical outputs | Bitwise identical, or documented tolerance | **Likely yes** (single-threaded FV), but not asserted `[I]` |

**Honest scorecard for `floodSim.ts`:** it can plausibly pass **2, 6, 8, and 14** with instrumentation only —
i.e. the *numerical hygiene* checks. It cannot currently pass **10, 11, 13**, and **4** is blocked by the
absence of a frictionless path. The gap is not the solver; it is the **absence of a verification suite and
of a datum/nodata contract on the input DEM**. `[I]`, grounded in the module's own doc comment `[E]`.

**The single cheapest credibility win:** implement checks #1, #2 and #4 as unit tests with analytic
solutions, print the errors in the UI, and stop calling it a simulation until they are green. `[I]`

---

## Top-5 — mesh (for a flood-simulation domain)

1. **2DGS** (`hbb1/2d-gaussian-splatting`, 3328★) — the best depth/normal source; extraction is a
   post-process, so it feeds the existing TSDF path directly. **Blocker: non-commercial licence.**
2. **DN-Splatter** (`maturk/dn-splatter`, 849★, **Apache-2.0**) — the only permissively-licensed member of
   the research set, and monocular-depth supervision is what a street-level capture actually needs.
3. **GOF** (`autonomousvision/gaussian-opacity-fields`, 1025★) — closest to genuinely closed surfaces in
   unbounded outdoor scenes via level-set extraction.
4. **NKSR** (`nv-tlabs/NKSR`, 1006★) — the most robust point-cloud→mesh step; turnkey, noise-tolerant.
5. **PoissonRecon / Open3D screened Poisson** (1870★ MIT / 14170★) — the deterministic, MIT-licensed
   fallback that *guarantees* watertight output and can be shipped today.

Honourable: **MILo** (539★, mesh-in-the-loop topology), **GaussianWrapping** (306★, claims watertight),
**PGSR** (1130★, planar priors suit terrain), **VDBFusion** (663★ MIT).

## Top-5 — solvers

1. **SFINCS** (GPL-3.0) — purpose-built for compound flooding, raster-native, single binary. Best
   capability-per-effort; GPL is the caveat.
2. **ANUGA** (BSD-like) — the most complete published validation suite of any here, including dry/wet dam
   break against Ritter `[E]`; unstructured mesh is where splat geometry is actually needed.
3. **GeoClaw** (BSD-3) — rigorous well-balanced/AMR solver with the cleanest licence among the serious engines.
4. **BG_Flood** (GPL-3.0) — GPU speed on quadtree grids; note AMR is declared *not yet operational* `[E]`.
5. **WhiteboxTools** (MIT, **Rust**) — not a solver, but the only component that can be *linked* rather than
   shelled out, and the DEM conditioning it provides is a precondition for every other entry.

**Recommended architecture** `[I]`: keep `floodSim.ts` as the fast interactive tier; validate it against
**GeoClaw or ANUGA** (permissive licences, analytic benchmarks) in CI; offer **SFINCS** as the
"authoritative" sidecar only after a licence review.

---

## Contradictions

- **SFINCS licence.** The GitHub API reports `GPL-3.0` `[E]`; Deltares' product page presents precompiled
  executables under a separate Deltares freeware licence `[E]`. Both are true — source vs binary. Do not
  state "SFINCS is GPL" without that qualifier.
- **Splat watertightness.** Method pages advertise "watertight" surfaces (GOF, GaussianWrapping, MILo),
  while user issue trackers for the same projects report floaters and open boundaries. Not resolved here;
  treat every watertightness claim as **unverified until measured on your own capture**.
- **"Neural KinectFusion".** No repository of that exact name was found; the nearest real thing is
  NeuralFusion (`weders/NeuralFusion`). The task's name may be a conflation — flagged, not resolved.
- **"TRITON".** Multiple unrelated projects share the name; the flood-modelling "TRITON" could not be
  pinned to a single repository in this sweep.

## Missing evidence

- Stars/licence/last-push for 12 of 30 mesh rows and 9 of 20 solver rows: **`[n/r]`** — GitHub quota
  exhausted. Re-run with an authenticated token before any of these numbers are published.
- No independent benchmark numbers were collected. All accuracy statements are the authors' own claims `[E]`
  and were **not reproduced** here.
- `floodSim.ts` was read only to ~line 120 (header contract + interfaces). The solver body was **not**
  audited line-by-line, so every statement about its numerics is `[I]` from its own doc comment, not `[E]`.
- The claim that the repo's existing `src-tauri/src/mesh/{tsdf,raster,export}.rs` "2DGS/DN-Splatter-style"
  pipeline exists was taken from the task brief; those files were **not** opened in this sweep.
- Whether any GPL engine may be bundled in a Tauri installer is a **legal** question, not resolved here.

## Sources

- Kept: `hbb1/2d-gaussian-splatting` (https://github.com/hbb1/2d-gaussian-splatting) — licence restriction confirmed via repo + issue #242; the geometric reference point for the whole field.
- Kept: `maturk/dn-splatter` (https://github.com/maturk/dn-splatter) — the one Apache-2.0 option; WACV'25 paper PDF at openaccess.thecvf.com.
- Kept: `autonomousvision/gaussian-opacity-fields` (https://github.com/autonomousvision/gaussian-opacity-fields) — level-set closure claim.
- Kept: ANUGA validation docs (https://anuga.readthedocs.io/en/stable/reference/validation.html) — dry/wet dam-break tests against analytic solutions; the template for the repo's own test suite.
- Kept: UK EA 2D benchmarking report (https://www.gov.uk/flood-and-coastal-erosion-risk-management-research-reports/2d-benchmarking-evaluating-the-latest-generation-of-the-hydraulic-models-for-fcrm) — the external benchmark suite referenced in check #10.
- Kept: `Deltares/SFINCS` (https://github.com/Deltares/SFINCS) + SFINCS manual (https://sfincs.readthedocs.io) — `sfincs.inp` input contract.
- Kept: RichDEM depression-filling docs (https://richdem.com/intro.html) — conditioning necessity.
- Kept: NOAA-OWP `inundation-mapping` README (https://github.com/NOAA-OWP/inundation-mapping) — DEM is insufficient without SRC/roughness.
- Kept: BASEMENT download page (https://basement.ethz.ch/download.html) — freeware ≠ open source.
- Kept: TELEMAC download (https://www.opentelemac.org) — official GitLab, GitHub mirror unofficial.
- Deprioritised: blog/SEO "best gaussian splatting mesh" listicles — no primary claims.
- Deprioritised: fork repos of SuGaR/2DGS — same code, stale metadata.

## Next steps

1. **Re-run the metadata sweep with a GitHub token** to fill the 21 `[n/r]` rows (one `GET /repos/...` each; 5000 req/h authenticated).
2. **Write the three analytic tests first** (mass conservation, lake at rest, Ritter with n = 0) against `floodSim.ts`; publish the L1 errors. Nothing else in this document matters until that exists.
3. **Fix the DEM contract**: require datum + CRS + nodata mask from `splatCloudToTerrainMeshAndDem`, and replace silent IDW fill with a nodata flag plus an explicit "invented cells" count.
4. **Licence decision** on 2DGS (non-commercial) and on GPL sidecars — this determines the entire mesh and solver shortlist.
5. Only then: cross-validate one real capture in GeoClaw (BSD-3) and compare depth/extent against the TS solver.

---

## Row counts

| Section | Rows |
|---|---|
| Part 1 — mesh entries (18 `[api]`, 12 `[n/r]`/`[404]`) | **30** |
| Part 2 — solver entries (12 `[api]`, 8 `[n/r]`/`[404]`) | **20** |
| Part 3 — honesty-bar checks | **14** |
| **Total data rows** | **64** |
