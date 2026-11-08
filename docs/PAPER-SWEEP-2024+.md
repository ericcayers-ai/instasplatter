# Paper / repo sweep evidence (2024–2026)

**Sweep date:** 2026-07-14  
**Workspace:** `C:\Users\<you>\OneDrive\Desktop\Programs\instasplatter`  
**Method:** (1) Read existing `docs/RESEARCH-STACK.md`, `ROADMAP.md`, `ROADMAP-V2.md`.  
(2) Web search for post-2024 3DGS / NVS / feed-forward SfM / splat→mesh work.  
(3) Cross-check against the Awesome 3DGS index ([mrnerf/awesome-3D-gaussian-splatting](https://mrnerf.github.io/awesome-3D-gaussian-splatting/)) for breadth.  
(4) Codebase audit (`rg` / graphify) for real symbols vs docs-only.

This file is the **verifiable inventory**. Do not treat prose claims as proof; use the Status + Grep columns.

## Policy used during the sweep

| Rule | Consequence |
| --- | --- |
| Prefer Apache/MIT/BSD or Meta commercial AUP / NVIDIA Open Model | Default-ON when installed |
| CC BY-NC / Inria NC / GS-license-adjacent | Research opt-in, reimplement, or reject shipping |
| Complementary methods should **AND** together | Not mutually exclusive toggles |
| Brush is stock CLI unless `brush-custom` | In-loop SOTA stays fork-blocked |

## How to re-verify

```powershell
# Status symbols / settings
rg -n "dense_init|use_neural_init|allow_research|mip_filter|progressive_resolution|post_polish|vggt_omega|brush-custom|try_polish|densify_after_sfm|apply_3d_filter|poisson_style_fallback" src-tauri src
# Sidecar launchers
rg -n "depth-anything-v2|vggt-commercial|vggt-omega|fixer|difix" src-tauri tools
# Mesh stack
rg -n "MeshOptions|smooth_passes|min_component|poisson_fallback|CONFIDENCE|alpha" src-tauri/src/mesh
```

---

## Master inventory (40 entries)

Columns: **Status** ∈ {integrated, wired-default-ON, opt-in, deferred, rejected}  
**License** abbreviated. **Integration** = code path or reason not shipped.

| # | Name | Year / venue | Link | Category | License | Status | Integration in this repo | Grep / proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 3D Gaussian Splatting | 2023 SIGGRAPH | [repo](https://github.com/graphdeco-inria/gaussian-splatting) | densify/train | Inria NC | rejected (as vender) | Algorithm ancestor only; train via Brush Apache | `brush_exe`, `pipeline/brush.rs` |
| 2 | Brush | 2024–2025 | [ArthurBrussee/brush](https://github.com/ArthurBrussee/brush) v0.3.0 | densify/train | Apache-2.0 | wired-default-ON | Primary trainer CLI | `engines.rs::BRUSH`, `brush_exe()` |
| 3 | COLMAP | 4.1 | [colmap/colmap](https://github.com/colmap/colmap) | SfM/init | BSD | wired-default-ON | Batch SfM + patch-match MVS | `colmap.rs`, `dense.rs::patch_match_stereo` |
| 4 | GLOMAP | 2024 | [colmap/glomap](https://github.com/colmap/glomap) | SfM/init | BSD | deferred | Classical incremental SfM already native; GLOMAP binary not wired | `sfm/mod.rs` live path instead |
| 5 | VGGT | CVPR 2025 | [facebookresearch/vggt](https://github.com/facebookresearch/vggt) | SfM/init | NC weights | opt-in | `vggt-research` launcher only if Research ON | `sidecars.rs` `vggt-research` |
| 6 | VGGT-1B-Commercial | Jul 2025 | HF `facebook/VGGT-1B-Commercial` | SfM/init | Meta AUP | wired-default-ON | **Standard primary poses** + densify when `ACCEPTED` | `try_neural_poses`, `vggt_commercial` |
| 7 | VGGT-Ω | CVPR 2026 Oral | [vggt-omega](https://github.com/facebookresearch/vggt-omega) arXiv:2605.15195 | SfM/init | CC BY-NC-4.0 | opt-in | **Experimental** preferred pose chain head | `vggt_omega`, `experimental_mode` |
| 8 | MASt3R / DUSt3R | 2024 | naver dust3r/mast3r | SfM/init | CC BY-NC-SA | opt-in | **Experimental** pose + dense merge (not Standard) | `mast3r`, `dust3r`, `solver.rs` |
| 9 | π³ / Pi3 / Pi3X | 2025 | [yyfz/Pi3](https://github.com/yyfz/Pi3) | SfM/init | CC BY-NC weights | opt-in | **Experimental** static unordered stub `pi3x` | `pi3x`, `experimental.rs` |
| 10 | GlueMap / feed-forward SfM glue | 2025–26 | [colmap/gluemap](https://github.com/colmap/gluemap) | SfM/init | mixed | deferred | Would wrap Pi3/VGGT; NC backbones | docs only |
| 11 | InstantSplat | 2024 | InstantSplat repos | SfM/init | often NC/GS | deferred | Pose-free idea → VGGT sidecar instead | ROADMAP Phase 3 |
| 12 | AnySplat | 2025 | feed-forward GS | densify/train | verify | deferred | Out of scope (feed-forward NVS product) | — |
| 13 | Depth Anything V2 | 2024 | [Depth-Anything-V2](https://github.com/DepthAnything/Depth-Anything-V2) | densify/train | Apache-2.0 | wired-default-ON | Dense-init sidecar; **composed with MVS + RoMa** | `depth_anything_v2`, `try_neural_points` |
| 13b | RoMa v2 densify | 2024–26 | [Parskatt/RoMaV2](https://github.com/Parskatt/RoMaV2) | densify/train | MIT (+ DINOv3 meta custom) | wired-default-ON | Lichtfeld-*recipe* clean-room sidecars; **not** GPL plugin | `roma-v2`, `try_roma_densify` |
| 13c | Lichtfeld Densification Plugin | 2025 | [shadygm/Lichtfeld-Densification-Plugin](https://github.com/shadygm/Lichtfeld-Densification-Plugin) | densify/train | **GPL-3.0** | rejected (as vendor) | Recipe inspiration only — never copy sources | docs / roma-v2 README |
| 14 | Dense sparse→Gaussian seed | our | — | densify/train | Apache-friendly | integrated | Always write `init.ply` | `dense.rs::points_to_gaussians` |
| 15 | COLMAP patch-match MVS | COLMAP | colmap stereo | densify/train | BSD | wired-default-ON | After neural merge when CUDA | `densify_after_sfm` |
| 16 | DashGaussian | CVPR 2025 | [DashGaussian](https://github.com/YouyuChen0207/DashGaussian) | densify/train | CC BY-NC-SA | wired-default-ON | **Reimplemented** progressive stages | `plan_stages`, `progressive_resolution` |
| 17 | Mip-Splatting | CVPR 2024 | [mip-splatting](https://github.com/autonomousvision/mip-splatting) | AA/filter | Inria NC | wired-default-ON | **Reimplemented** 3D filter + bake | `mipfilter.rs::apply_3d_filter` |
| 18 | AbsGS | 2024 | arXiv:2404.10484 | densify/train | research | wired-default-ON | AbsGrad densify = fork; opac/scale L1 via CLI | `opac_loss_weight`, `scale_loss_weight` |
| 19 | 3DGS-MCMC | NeurIPS 2024 | [3dgs-mcmc](https://github.com/ubc-vision/3dgs-mcmc) | densify/train | Inria NC | wired-default-ON (partial) | Brush ships MCMC-style noise; knobs exposed | `mean_noise_weight`, Brush changelog |
| 20 | Taming-3DGS | 2024 | humansensinglab | densify/train | MIT (perf) | deferred | Needs Brush fork densify | `tools/brush-fork/README.md` |
| 21 | FastGS | 2025 | FastGS 100s train | densify/train | MIT-ish | deferred | Brush fork budgeted densify | brush-fork priorities |
| 22 | Mini-Splatting / Speedy-Splat | 2024 | various | densify/train | Apache / NC mix | deferred | Pruning reference only | ROADMAP-V2 license table |
| 23 | SpotLessSplats | 2024 | arXiv:2406.20055 | robustness | Apache-derived | deferred | Mask loss inside Brush | brush-fork #4 |
| 24 | WildGaussians | NeurIPS 2024 | [wild-gaussians](https://github.com/jkulhanek/wild-gaussians) | appearance | MIT + GS deps | deferred | Appearance embeddings in fork | brush-fork #5 |
| 25 | SAM 2 | 2024 | facebookresearch/sam2 | robustness | Apache-2.0 | deferred | Sidecar for SpotLess later | RESEARCH-STACK |
| 26 | Bilateral grid / bilagrid | NeRF-W era | various | appearance | permissive recipes | deferred | Needs in-loop color model | ROADMAP §5 |
| 27 | Analytic-Splatting | 2024+ | verify | AA/filter | unclear | deferred | — | RESEARCH-STACK |
| 28 | 2DGS | SIGGRAPH 2024 | [hbb1/2d-gaussian-splatting](https://github.com/hbb1/2d-gaussian-splatting) | mesh | Apache-friendly raster | wired-default-ON | Mesh recipe (TSDF depths) | `mesh/mod.rs`, `raster.rs` |
| 29 | DN-Splatter | WACV 2025 | [maturk/dn-splatter](https://github.com/maturk/dn-splatter) | mesh | Apache (Nerfstudio) | wired-default-ON | Confidence floor, cleanup insp. | `CONFIDENCE` / alpha≥0.62 |
| 30 | AGS-Mesh | 3DV 2025 | arXiv:2411.19271 | mesh | with DN-Splatter | wired-default-ON | Oriented-point fallback | `poisson_style_fallback` |
| 31 | SuGaR | 2024 | Anttwo/SuGaR | mesh | GS-adjacent | rejected | NC / GS license | ROADMAP-V2 |
| 32 | GOF / PGSR / RaDe-GS | 2024 | various | mesh | Inria NC | rejected | Reference only | ROADMAP-V2 |
| 33 | GS-2M | 2025 | ndming/GS-2M | mesh | MIT-leaning | deferred | PBR mesh later | ROADMAP Phase 4.4 |
| 34 | MeshSplat / FatesGS | 2025 | awesome list | mesh | verify | deferred | Feed-forward mesh not core | — |
| 35 | Difix3D+ | CVPR 2025 | [nv-tlabs/Difix3D](https://github.com/nv-tlabs/Difix3D) | post-process | research/gated | opt-in | **Experimental** polish before Fixer | `difix`, `polish_order` |
| 36 | NVIDIA Fixer | 2025–26 | [HF nvidia/Fixer](https://huggingface.co/nvidia/Fixer) | post-process | NVIDIA Open Model (**commercial OK**) | wired-default-ON | When launcher installed | `post_polish`, `fixer` sidecar |
| 46 | Dual-mode Experimental UI | 2026 | our | UX / policy | Apache-2.0 | integrated | TitleBar toggle, NC modal, banner, Max floors | `experimental_mode`, TitleBar |
| 37 | GSFixer / GSFix3D | 2025 | [GVCLab/GSFixer](https://github.com/GVCLab/GSFixer) | post-process | Apache + GS NC + RAIL | rejected | Bundled GS license blocks commercial | RESEARCH-STACK |
| 38 | on-the-fly-nvs | Inria | graphdeco | SfM/init | Inria NC | deferred | Algorithm → native live init | `sfm::run_incremental` |
| 39 | CUT3R | 2024+ | — | SfM/init | CC BY-NC-SA | deferred | Architecture template only | ROADMAP-V2 2.2 |
| 40 | Live PLY lerp viewport | our | — | post-process / UX | — | integrated | Web worker lerp | `src/splat/worker.ts` `lerpCloud` |
| 41 | SparseSplat / LGTM / VG²GT | 2025–26 | arXiv feed-forward | densify/train | research | deferred | Feed-forward product path | awesome list 2026 |
| 42 | Dense-SfM / Fast3R | 2025 | various | SfM/init | verify | deferred | Classical+COLMAP sufficient for now | — |
| 43 | CityGaussian / Urban-GS / Horizon | 2024+ | — | large scene / NC | NC | opt-in | **Experimental** partition/LOD stubs | `city-gaussian`, `urban-gs`, `horizon-gs` |
| 44 | Frame blur gate | our | — | robustness | — | integrated | Video ingest | `ingest.rs`, `blur_reject_fraction` |
| 45 | Batch queue | our | — | UX | — | integrated | Serialize GPU jobs | queue modules / UI |

*Entries 1–45 intentionally exceed 30; literature supports far more.*

---

## Codebase audit: REAL code vs docs-only

### Real code (greppable paths exist)

| Idea | Proof files / symbols |
| --- | --- |
| Progressive (DashGaussian-style) | `pipeline/brush.rs::plan_stages`, setting `progressive_resolution` default `true` |
| Mip 3D filter | `splat/mipfilter.rs::apply_3d_filter`, bake in `finalize` |
| AbsGS-like opac/scale L1 | `settings.rs` → Brush `--opac-loss-weight` / `--scale-loss-weight` |
| MCMC-style mean noise | Brush stock + `--mean-noise-weight` from `strictness` |
| Dense init (MVS + sparse seed) | `pipeline/dense.rs::densify_after_sfm` |
| Neural sidecars | `pipeline/sidecars.rs` |
| Compose neural **and** MVS **and** RoMa | `densify_after_sfm` merges clouds (v0.5+) |
| Dual-mode Experimental | `settings::experimental_mode`, `solver.rs` |
| Live incremental SfM | `sfm/mod.rs::run_incremental` |
| Mesh TSDF + cleanup + fallback | `mesh/mod.rs`, `tsdf.rs`, `raster.rs` |
| Live lerp | `src/splat/worker.ts`, `renderer.ts` |
| Custom Brush override | `engines.rs::brush_exe` → `brush-custom` |
| Fixer polish hook | `sidecars::try_polish`, setting `post_polish` |

### Docs / fork-blocked (honest gaps)

| Idea | Why not real yet |
| --- | --- |
| AbsGS densify metric | Needs Brush densify patch |
| True 2D mip in rasterizer | Needs Brush rasterizer patch |
| SpotLess / WildGaussians | Needs train-step losses / embeddings |
| Taming / FastGS budget densify | Needs Brush fork |
| Full screened Poisson | Deferred multigrid |
| UV atlas texture | Packer not written |
| SAM2 class masks | Sidecar not built |
| Difix research distill loop | Heavy; Fixer launcher path only |
| GLOMAP binary | Not downloaded/wired |
| MASt3R / Pi3 | NC — Experimental stubs only (`tools/sidecars/mast3r`) |

---

## Target composed pipelines (v0.5 dual-mode)

**Standard:**

```
Video → frame gate
  → VGGT-Commercial (primary) → optional COLMAP BA / COLMAP fallback
  → RoMa v2 ∧ DAV2/VGGT-C ∧ COLMAP MVS ∧ sparse → merged init.ply
  → gsplat or Brush (progressive ∧ mip ∧ AbsGS ∧ MCMC)
  → Fixer polish if installed
  → Live PLY lerp → Mesh
```

**Experimental (after license ack):**

```
Video → frame gate → CaptureProfile
  → profile-matched research hypotheses (Ω/MASt3R/DUSt3R/Pi3X | StreamVGGT/SLAM | MonST3R…)
  → score + canonical COLMAP/ENU + validation gates
  → confidence-fuse densify (schema v2) — never blind concatenate
  → optional separate 4D / large-scene / surface adapters
  → Difix → Fixer
  → gsplat Max (all strategies) or Brush Max
  → Live PLY lerp → Mesh
```

See `pipeline/experimental.rs` for the routing table; hydro adapters in `geospatial/hydro.rs`.

---

## Sweep evidence artifacts (non-hand-waving)

| Source | What was used |
| --- | --- |
| Local docs | `docs/RESEARCH-STACK.md`, `ROADMAP.md`, `ROADMAP-V2.md` (read in full before coding) |
| Web | Searches for VGGT/Ω, AbsGS, SpotLess, MCMC, Difix, Fixer, DashGaussian, FastGS, DN-Splatter, AGS-Mesh, 2DGS, GSFixer, GlueMap, Pi3 |
| Awesome index | Scraped ## headings for 2024/2025/2026 titles (80+ candidates; curated into table) |
| HF Fixer card | Confirmed **NVIDIA Open Model License**, “ready for commercial/non-commercial use” (2026-07-14 fetch) |
| Repo grep | Symbols listed in columns above |

Companion: [RESEARCH-STACK.md](./RESEARCH-STACK.md) (ship policy + status summary).

## Addendum — gsplat audit (2026-07-14)

Cloned `https://github.com/nerfstudio-project/gsplat` (Apache-2.0) into
`_refs/gsplat` (gitignored). Features mapped into:
- sidecar `tools/sidecars/gsplat-train/` (`run.py`, `train_mini.py`)
- Rust `pipeline/gsplat.rs`
- settings `trainer` / `gsplat_*` (compose ON)

See RESEARCH-STACK **gsplat parity** table for row-by-row status.


---

## Correction — 2026-11-03 (read before trusting Entries 47–62)

The 2026-11-01 addendum below records entries 47, 48, 49, 61 as **integrated**. A measurement pass
on 2026-11-03 found that status wrong for three of them:

- **#47 / #61 (Gaussian Point Splatting, "64-bit atomic visibility buffer").** WGSL defines no `u64`
  type, and the recorded shader had a depth/colour race (`atomicMin(depth)` then a separate colour
  store). Nothing outside the smoke test imported `src/splat/pointSplatting.ts`, so no renderer path
  ever ran it. Corrected shader (two-pass, 32-bit) and the adopt/reject decision now live in
  `docs/POINT-SPLATTING-VERDICT.md`; the module is a **reference implementation**, not shipped.
- **#48 (splat-to-mesh Saint-Venant flood).** The solver leaked 33% of injected volume on a closed
  domain and could not hold a lake at rest. It is replaced by `src/geospatial/hazards/swe.ts` (HLL +
  Audusse hydrostatic reconstruction + Toro dry-bed wave speeds), validated by 27 measured checks in
  `floodValidation.smoke.ts`. Details and the honest limits: `docs/FLOOD-SIMULATION.md`.
- **#49 (6-DoF VPS).** Nothing imported `vps.ts`, and `hdopM`/`vdopM` were `geomFactor * 0.45` — tuned
  constants, not a derivation. Now wired (`VpsPanel.tsx` + IPC), with a derived first-order bound and
  seeded random-subset RANSAC.
- **#50–#62 rows** are references/deferred as recorded; none of them is shipped code.

The repo-wide audit of documentation-vs-code drift, with the commands that produced each finding, is
`docs/AUDIT-2026-11-03.md` §9.

---

## Addendum — 2026-11-01 Deep GitHub Gaussian Splatting, Splat-to-Mesh Flood & 6-DoF VPS Sweep (Entries 47–62)

**Sweep date:** 2026-11-01  
**Primary Additions Implemented in `src/`:**
1. **Gaussian Point Splatting (SIGGRAPH '26, `JorisAR/gaussian-point-splatting`):** Clean-room implementation in `src/splat/pointSplatting.ts` (`packDepthColor64`, `unpackDepthColor64`, `computeAdaptivePointCount`, `cullGeospatialSplatChunks`, `rasterizeGaussianPointSplatting`, `WGSL_GAUSSIAN_POINT_SPLATTING_SHADER`). Replaces global $O(M \log M)$ radix sort with stochastic/quasi-random Halton point emission per Gaussian and 64-bit atomic depth-first visibility (`atomicMin`), scaling to city/geospatial scenes (100M–425M+ splats) with zero sorting pop.
2. **Splat-to-Mesh Terrain Extraction & Saint-Venant Flood Simulation (`src/geospatial/hazards/floodSim.ts`):** Bridges `GaussianPrimitive3D[]` clouds directly into a ground-envelope DEM & 3D indexed mesh (`splatCloudToTerrainMeshAndDem`) and runs a mass-conserving 2D Shallow Water Equations (Saint-Venant hydrostatic reconstruction + Manning friction) solver (`stepShallowWaterSaintVenant`, `runSplatToMeshFloodSimulation`, `extractFloodWaterSurfaceMesh`).
3. **Full Ground-Level + Geospatial 6-DoF Visual Positioning System (`src/geospatial/vps.ts`):** 4-stage coarse-to-fine visual localization (`retrieveCandidateVpsSubmaps`, `matchQueryToSplatAnchors`, `solvePnpRansac6DoF`, `localizeCameraVps`) operating across street-level scans and aerial/geospatial views with ENU $\leftrightarrow$ WGS84 registration.

| # | Name | Year / Venue | Link | Category | License | Status | Integration in this repo | Grep / proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 47 | **Gaussian Point Splatting** | SIGGRAPH 2026 | [JorisAR/gaussian-point-splatting](https://github.com/JorisAR/gaussian-point-splatting) | large-scene / sorting-free raster | Research ref (clean-room TS/WGSL) | **integrated** | Sorting-free stochastic point splatting + 64-bit atomic depth+color buffer for geospatial scale | `src/splat/pointSplatting.ts`, `rasterizeGaussianPointSplatting` |
| 48 | **Splat-to-Mesh Hydrostatic Flood Sim** | 2026 | in-repo (`floodSim.ts`) | mesh / hydro physics | Apache-2.0 | **integrated** | Lower-quantile ground DEM + mesh extraction from splats $\rightarrow$ 2D Saint-Venant SWE solver $\rightarrow$ 3D water surface mesh | `src/geospatial/hazards/floodSim.ts`, `runSplatToMeshFloodSimulation` |
| 49 | **6-DoF Ground + Geospatial Splat VPS** | 2026 | in-repo (`vps.ts`) | localization / VPS | Apache-2.0 | **integrated** | NetVLAD/GeoCLIP submap retrieval + Lowe/mutual 2D-3D matching + PnP-RANSAC + LM refinement + WGS84 registration | `src/geospatial/vps.ts`, `localizeCameraVps` |
| 50 | **3DGRUT** (3D Gaussian Ray Tracing + Unscented Transform) | 2025–26 NVIDIA | [nv-tlabs/3dgrut](https://github.com/nv-tlabs/3dgrut) | raytracing / fisheye / VR | Apache-2.0 | deferred (ref) | Exact shutter / fisheye / secondary ray reference for VR & wide-FOV cameras | `PAPER-SWEEP-2024+.md` |
| 51 | **StopThePop** | SIGGRAPH 2024 | [r4dl/StopThePop-Rasterizer](https://github.com/r4dl/StopThePop-Rasterizer) | AA / sorting | MIT / GS-deriv | superseded by #47 | Per-pixel hierarchical sort; superseded for geospatial scale by sorting-free Gaussian Point Splatting (#47) | `pointSplatting.ts` |
| 52 | **RadSplat** | 3DV 2025 | [m-niemeyer/radsplat](https://github.com/m-niemeyer/radsplat) | pruning / speed | Apache-2.0 / ref | opt-in | Ray-contribution max-operator pruning for 900+ FPS compact splats | `RESEARCH-STACK.md` |
| 53 | **Hierarchical-3DGS** | SIGGRAPH 2024 | [graphdeco-inria/hierarchical-3d-gaussians](https://github.com/graphdeco-inria/hierarchical-3d-gaussians) | city / geospatial LOD | Inria NC | reimplemented | Chunked ENU bounding-sphere hierarchy & frustum culling in `cullGeospatialSplatChunks` | `pointSplatting.ts::cullGeospatialSplatChunks` |
| 54 | **Octree-GS** | 2024–25 | [city-super/Octree-GS](https://github.com/city-super/Octree-GS) | LOD / large scene | Apache-2.0 | integrated (cull) | Anchor-based LOD & view-distance octree culling | `pointSplatting.ts` |
| 55 | **Grendel-GS** | NeurIPS 2024 | [nyu-systems/Grendel-GS](https://github.com/nyu-systems/Grendel-GS) | distributed train | Apache-2.0 | deferred | Multi-GPU sparse all-to-all splat distribution (out of single-GPU RTX 4060 scope) | — |
| 56 | **FlashGS** | CVPR 2025 | [FlashGS/FlashGS](https://github.com/FlashGS/FlashGS) | fast raster | MIT | deferred | Exact ellipse intersection culling in CUDA | — |
| 57 | **Spark** | 2025 | [sparkjsdev/spark](https://github.com/sparkjsdev/spark) | WebGL/Three.js | MIT | reference | Web-native streaming splat chunks | `RESEARCH-STACK.md` |
| 58 | **SuperSplat** | 2024–26 | [playcanvas/supersplat](https://github.com/playcanvas/supersplat) | editor / SOG | MIT | reference | PlayCanvas splat inspection & cleanup UX | `RESEARCH-STACK.md` |
| 59 | **SplatLoc / GS-Loc / 6DGS** | 2024–25 | [various 3DGS visual localization repos] | VPS / 6-DoF | research / Apache | **reimplemented** | Clean-room 2D-3D anchor descriptor matching + PnP-RANSAC + LM polish in `src/geospatial/vps.ts` | `src/geospatial/vps.ts` |
| 60 | **Gauzilla / Long-LRM** | 2025–26 | feed-forward large-scale GS | feed-forward | research | deferred | Feed-forward scene reconstruction | — |
| 61 | **64-Bit Atomic Visibility Buffer** | SIGGRAPH 2026 | `JorisAR/gaussian-point-splatting` | WebGPU compute | Apache-2.0 (ours) | **integrated** | `packDepthColor64` + `WGSL_GAUSSIAN_POINT_SPLATTING_SHADER` | `src/splat/pointSplatting.ts` |
| 62 | **Saint-Venant Hydrostatic Reconstruction** | Audusse 2004 / 2026 | `src/geospatial/hazards/floodSim.ts` | flood physics | Apache-2.0 | **integrated** | Positivity-preserving well-balanced SWE solver over splat-derived DEM & mesh | `src/geospatial/hazards/floodSim.ts` |
