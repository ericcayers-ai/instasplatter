# Research: Geospatial Gaussian Splatting, Georeferencing, CRS & Coordinate Handling (Part B)

GitHub ecosystem sweep for **InstaSplat** — georeferenced drone/aerial 3DGS, GCP/GNSS alignment,
large-scene coordinate precision, 3D Tiles / Cesium terrain pipelines, DEM sources, and geo splat viewers.

- **Prepared:** 2026-11-03
- **Companion docs:** `docs/RESEARCH-STACK.md` (stack), `docs/PAPER-SWEEP-2024+.md` (papers), Part A sweep (non-geo infra)
- **Existing code in scope:** `src-tauri/src/geospatial/{transforms,dem,sat_splat,splat_dem}.rs`, `src/geospatial/enu.ts`

---

## Summary

Nothing in the open-source ecosystem does *georeferenced 3DGS* well yet as a turnkey library: the
credible options split into (a) **research code that reconstructs from satellite/drone imagery but
stops at "COLMAP-local + a similarity transform"** (Skyfall-GS, SatelliteSfM, Ortho-3DGS, DroneSplat,
Urban-GS, HorizonGS — most non-commercial), and (b) **geospatial plumbing that is permissive and
production-grade** (COLMAP `model_aligner`, GDAL/PROJ/pyproj, Open3D/fast_gicp, quantized-mesh-encoder,
3d-tiles-tools, CesiumJS). The single most important 2026 development for InstaSplat is that
**`KHR_gaussian_splatting` is now a ratified Khronos glTF extension and CesiumJS renders splat tilesets
through it (plus `KHR_gaussian_splatting_compression_spz`)** — that, not a bespoke PLY-drape, is the
correct export contract for the v0.10 globe view. Precision-wise, float32 ENU is fine to ~8 km
(sub-mm) and to ~16.8 km (mm); the practical limit for InstaSplat scenes is set by DEM/GNSS error
(metres), not float32, but anything beyond ~10–17 km must be chunked into per-tile frames.

**Top-5 adopt-now:** `KhronosGroup/glTF` KHR_gaussian_splatting, `CesiumGS/3d-tiles-tools`,
`kylebarron/quantized-mesh-encoder`, `colmap/colmap` (`model_aligner`), `OSGeo/gdal`.

---

## How the metadata was obtained (deviation, disclosed)

The task specified `https://api.github.com/repos/<owner>/<name>` for stars / license / last push.

- **`api.github.com` was unavailable for this entire run**: unauthenticated requests from this host
  returned `HTTP 403 {"message":"API rate limit exceeded for <redacted-ip>"}` on every attempt
  (verified twice, including a forced-direct connection). No authenticated token is available here.
- **Substitute used for `stargazers_count` and `pushed_at`:** `https://ungh.cc/repos/<owner>/<name>`,
  a read-only mirror of the GitHub REST API returning the same `repo`, `stars`, `forks`, `pushedAt`
  fields. Values below are **as fetched on 2026-11-03**.
- **License identification:** read directly from each repository's license file at
  `https://raw.githubusercontent.com/<owner>/<name>/HEAD/<LICENSE file>`; where the filename was
  uncertain the root tree was listed first. "no license file found" means the root tree was checked
  and contains no `LICENSE*`/`COPYING*` file — that is a *finding*, not a guess.
- Anything I could not verify is marked **NOT FOUND** / **NOT VERIFIED**. No star count, license or
  repo name in this document is estimated or invented.

**Verdict legend** (aligned with the project's Standard/Experimental license policy in `RESEARCH-STACK.md`):

| Verdict | Meaning for InstaSplat |
|---|---|
| **adopt-now** | Permissive license, ship it in the Standard path this cycle |
| **adapt** | Take the technique/format, write our own clean-room implementation (license or architecture mismatch) |
| **reference-only** | Read it, cite it, do not link or copy code (NC/GPL/AGPL/no-license, or scope mismatch) |
| **reject** | Unusable: dead, redundant with in-repo code, or wrong problem |

---

## Findings

### 1. Georeferenced / satellite / drone / aerial 3DGS

| # | Repo | ★ | License | Last push | Technique (what it actually does) | Verdict |
|---|---|---|---|---|---|---|
| 1 | [jayin92/Skyfall-GS](https://github.com/jayin92/Skyfall-GS) | 1039 | Apache-2.0 (root `LICENSE`; repo also ships `LICENSE_inria.md`) | 2026-09-26 | ECCV'26. Satellite-imagery → immersive urban 3DGS: two-stage curriculum, stage 2 = iterative dataset update with diffusion (FlowEdit) to fix cross-view-consistent geometry. Ships JAX + NYC datasets. | **adapt.** The *pipeline shape* (coarse satellite geometry, then refine) matches `sat_splat.rs`; Apache-2.0 root license is Standard-safe, but the vendored `diff-gaussian-rasterization` submodule inherits INRIA non-commercial terms — treat submodule as Experimental and port only the loss/schedule ideas. |
| 2 | [jayin92/SatelliteSfM](https://github.com/jayin92/SatelliteSfM) | 30 | **no license file found** (root tree has no LICENSE/COPYING) | 2026-03-07 | Solves satellite SfM: converts **RPC** (rational polynomial camera) models of pushbroom satellite images to pinhole, then runs standard SfM; handles ENU and Lat/Lon/Alt frames. | **reference-only.** This is the missing piece for satellite-imagery-only AOIs (RPC → pinhole), and exactly what InstaSplat would need for a "satellite capture" mode — but no license = all rights reserved. Clean-room the RPC→pinhole math instead. |
| 3 | [BITyia/DroneSplat](https://github.com/BITyia/DroneSplat) | 163 | MIT | 2025-04-07 | CVPR'25 highlight: robust 3DGS from *in-the-wild* drone imagery (title/description-level evidence). | **adapt.** MIT is Standard-safe. Useful for the "messy real drone capture" robustness path (exposure/pose noise), not for georeferencing. Low maintenance (last push 2025-04) — copy ideas, not the dependency. |
| 4 | [alexzhenglong/Ortho-3DGS](https://github.com/alexzhenglong/Ortho-3DGS) | 30 | MIT | 2026-06-27 | True digital orthophoto from UAV imagery: depth-regulated 3DGS + non-invasive orthorectification + progressive chunking, without modifying the CUDA rasterizer. README states no CRS/GCP handling. | **adapt.** Best available recipe for splat → orthophoto/DSM product (the `splat_dem.rs` direction). MIT. Orthorectification via virtual-camera geometry is directly reusable; add our own CRS/GCP layer. |
| 5 | [manudelu/georeferenced_gsplat](https://github.com/manudelu/georeferenced_gsplat) | 6 | **no license file found** | 2025-10-30 | "Gaussian Splatting pipeline with georeferencing and correct scaling" — drone photo GPS metadata → real-world-aligned, metrically scaled splats. | **reference-only.** Closest title match to the exact InstaSplat problem, but 6★, no license, no doc depth. Useful only as a checklist of pipeline steps. |
| 6 | [wangm-buaa/Urban-GS](https://github.com/wangm-buaa/Urban-GS) | 25 | Gaussian-Splatting License (INRIA, non-commercial) | 2026-06-18 | CVPR'26 highlight: unified aerial-to-street 3DGS, compact representation for high fidelity at both scales. | **reference-only.** NC license blocks Standard; the aerial↔street "two-density" idea is worth noting for mixed drone + ground captures. |
| 7 | [Linketic/CityGaussian](https://github.com/Linketic/CityGaussian) | 1279 | CC BY-NC-SA 4.0 | 2026-08-16 | ECCV'24/ICLR'25 "CityGaussian series": large-scale reconstruction with **block partitioning + per-block normalization + LoD**. `main` = CityGaussianV2 (V1 on `V1-Original` branch). The previously cited `LTriomphe/CityGaussian` now 404s → renamed to `Linketic`. | **reference-only.** NC. The block-partition/per-block-origin scheme is the canonical mitigation for large-extent float precision (see §Precision budget). Already listed as Experimental-only in `RESEARCH-STACK.md` — consistent. |
| 8 | [InternRobotics/HorizonGS](https://github.com/InternRobotics/HorizonGS) | 164 | Gaussian-Splatting License (INRIA, non-commercial) | 2025-07-05 | CVPR'25: unified 3DGS for **large-scale aerial-to-ground** scenes; dataset of 7 calibrated scenes (5 synthetic, 2 real). | **reference-only.** NC. Its aerial↔ground camera mixing is the closest analogue to drone + pedestrian-level captures; dataset may still be usable for evaluation under its own terms (verify separately). |
| 9 | [NanCheng2001/SkySplat-main](https://github.com/NanCheng2001/SkySplat-main) | 52 | **no license file found** | 2026-06-09 | AAAI'26: *generalizable* 3DGS from **multi-temporal sparse satellite images** (feed-forward, few views). | **reference-only.** No license. Conceptually relevant only if InstaSplat ever does single-pass satellite stereo; not a registration/CRS tool. |
| 10 | [kangpeilun/VastGaussian](https://github.com/kangpeilun/VastGaussian) | 511 | Apache-2.0 | 2026-05-27 | **Unofficial** implementation of VastGaussian (city-scale: progressive partitioning, appearance decoupling, LiDAR-guided geometry). Default branch `refactor`. Note: `city-super/VastGaussian` 404s. | **adapt.** Apache-2.0 makes this the *only* permissively-licensed city-scale 3DGS partitioning codebase found. "Unofficial" + `refactor` default branch = treat as a reference implementation, not a dependency. |
| 11 | [city-super/Octree-GS](https://github.com/city-super/Octree-GS) | 862 | Gaussian-Splatting License (INRIA, non-commercial) | 2025-08-19 | TPAMI'25: **Octree-structured LoD Gaussians** for consistent real-time rendering at multiple scales. | **reference-only.** NC. The LoD-anchor design is the right model for Cesium-style distance-based streaming (pairs with `KHR_gaussian_splatting` LoD tiles). |
| 12 | [graphdeco-inria/hierarchical-3d-gaussians](https://github.com/graphdeco-inria/hierarchical-3d-gaussians) | 1479 | custom "Hierarchical-3d-Gaussians License" (non-commercial) | 2025-06-10 | SIGGRAPH'24: hierarchical 3D Gaussian representation for real-time rendering of **very large datasets** (chunked hierarchy + LoD, out-of-core). | **reference-only.** NC, but the definitive reference for "how do you split a 10 km scene into chunks and stream them". Read the chunk/LoD design before designing tileset structure. |
| 13 | [cmusatyalab/mega-nerf](https://github.com/cmusatyalab/mega-nerf) | 505 | MIT | 2024-01-10 | Mega-NeRF: large-scale aerial NeRF (pre-3DGS) with **spatial partitioning into non-overlapping cells + per-cell appearance/centroid handling**; Mill-19 / UrbanScene datasets. (Repo has no description; technique from the paper, not re-verified this run.) | **reference-only.** MIT but NeRF-era. The cell-partitioning + per-cell centroid scheme is exactly the "chunk before you train" pattern; nothing here is 3DGS-specific. |
| 14 | gdaosu/SatSplat | **NOT FOUND** | — | — | `https://github.com/gdaosu/SatSplat` returns **404** as of 2026-11-03, although the SatSplat project page (`gdaosu.github.io/satsplat`) still links to it as the code release ("Geometrically-Accurate Gaussian Splatting for Satellite Imagery"). | **NOT FOUND.** Do not cite or depend on it. Re-check at the next sweep; the paper exists but the code is not published at the advertised URL. |

**Related names that do *not* exist as claimed** (see §Missing evidence): `CRISP`, `UAV-GS`, `GeoSplat`.
Nearest real analogues: Ortho-3DGS (#4), DroneSplat (#3), HorizonGS (#8), `georeferenced_gsplat` (#5).

### 2. Alignment, GCP constraints and registration (splat → truth)

| # | Repo | ★ | License | Last push | Technique (what it actually does) | Verdict |
|---|---|---|---|---|---|---|
| 15 | [dozeri83/geo-register-plugin](https://github.com/dozeri83/geo-register-plugin) | 43 | **GPL-3.0** (verified from `LICENSE`) | 2026-09-28 | LichtFeld Studio plugin: **RANSAC + IRLS-fit Umeyama similarity transform** mapping scene coordinates → **ECEF**, from image GPS/CSV/Metashape poses; rejects outliers; exports georeferenced 3D Tiles or LAS/LAZ with a 3D Tiles root transform. | **reject as code / adapt the math.** GPL-3.0 cannot enter the Apache installer (same rule as LichtFeld, already documented). But this is the *exact* InstaSplat algorithm (Umeyama + robust outlier rejection → ECEF). Clean-room implement: (1) gather poses, (2) RANSAC Umeyama (scene→ECEF: scale + rotation + translation), (3) report residuals against the existing `GCP residual threshold` constants, (4) write the tileset `root.transform`. |
| 16 | [CZ-CHO/tiler](https://github.com/CZ-CHO/tiler) | 1 | Apache-2.0 | 2026-06-30 | Converts GraphDECO / `KHR_gaussian_splatting` PLY → **SPZ-compressed GLB 3D Tiles** with adaptive LOD simplification; accepts `--coordinate "[lat,long,height]"` to emit the standard **ENU root transform** in `tileset.json`. | **adapt.** Apache-2.0 and the closest end-to-end "PLY → Cesium-ready tileset" tool; 1★/single-author means don't vendor it, but clone the *behaviour*: our exporter must emit `tileset.json` with an ENU→ECEF root matrix from the AOI origin (already computable via `transforms::enu_to_ecef_matrix`). |
| 17 | [mapillary/OpenSfM](https://github.com/mapillary/OpenSfM) | 3816 | BSD-2-Clause | 2026-11-02 | SfM pipeline with documented **georeferencing**: reconstruction in local ENU, conversions between ENU / LLA / ECEF, GPS-constrained alignment. | **adapt.** BSD-2 is Standard-safe. Read `doc/georeferencing.md` as the design reference for our own `GeoReference` schema (the cleanest statement of the ENU↔LLA↔ECEF contract in the ecosystem). Don't take the Python dependency. |
| 18 | [colmap/colmap](https://github.com/colmap/colmap) | 13139 | new BSD (BSD-3-Clause) | 2026-11-03 | `colmap model_aligner`: aligns a reconstruction to geo coordinates via `--ref_is_gps 1 --alignment_type ecef --alignment_max_error 3.0`, needs ≥3 geo-referenced images; RANSAC-based similarity fit. (Verified from `doc/faq.rst`.) | **adopt-now.** COLMAP is already the Standard-mode SfM backbone. Adding a `model_aligner` invocation gives Sim(3)+RANSAC georeferencing for free, before the splat stage — exactly the WGS84→ENU contract InstaSplat needs, with `--alignment_max_error` as the outlier gate. Caveat (inference): output is in **ECEF metres** (~6.4e6), so re-centre into the AOI ENU frame immediately. |
| 19 | [OpenDroneMap/ODM](https://github.com/OpenDroneMap/ODM) | 6577 | **AGPL-3.0** | 2026-10-29 | End-to-end drone pipeline: orthophoto, georeferenced DEM, `odm_georeferenced_model.laz`, `odm_textured_model_geo.obj`. `--gcp` takes a file whose header is `EPSG:<code>` or a `+proj` string followed by `geo_x geo_y geo_z im_x im_y image_name`. (Docs confirm GCP input + georeferencing; they do *not* state that GCPs drive a BA adjustment.) | **reference-only.** AGPL blocks embedding. But ODM's **GCP file format** (CRS header + image-space ↔ ground-space pairs) is worth adopting verbatim as InstaSplat's GCP interchange format — it is what surveyors already have on disk. |
| 20 | [isl-org/Open3D](https://github.com/isl-org/Open3D) | 14170 | MIT | 2026-09-30 | Point-cloud registration toolkit: point-to-plane **ICP**, generalized ICP, global registration (RANSAC/FPFH), robust kernels, TSDF. | **adopt-now (sidecar).** MIT, pip-installable. The pragmatic engine for **splat-ground → DEM/LiDAR ICP refinement** after coarse Umeyama alignment — gives us the residual report the `splat_dem` path currently lacks. Python sidecar keeps it out of the Rust binary. |
| 21 | [koide3/fast_gicp](https://github.com/koide3/fast_gicp) | 1704 | BSD-3-Clause | 2025-04-24 | Fast **GICP/VGICP** variants with OpenMP/CUDA, multi-threaded, LiDAR-scale clouds. | **reference-only.** Excellent but C++/ROS-shaped and unmaintained since 2025-04; Open3D (#20) covers the need with less integration cost. Fallback only if ICP wall-clock becomes the bottleneck on 10⁷-point splat-derived clouds. |
| 22 | borglab/gtsam | **NOT VERIFIED** | **NOT VERIFIED** | **NOT VERIFIED** | Deliberately dropped from this sweep: pose-graph/BA factor graphs are already covered by COLMAP (#18) and the in-repo 6-DoF VPS path. No metadata fetched, so not cited as a candidate. | **defer.** Re-evaluate only if we need *joint* pose + scale + GCP optimization in one factor graph. |

*Researcher note:* the "GCP-constrained bundle adjustment" requirement is best served by **COLMAP (#18) for pose/scene alignment** plus **our own residual gate** (already exists as `GCP residual threshold constants` in `RESEARCH-STACK.md` §Automated gates), rather than by adopting a survey-grade BA stack. No permissively-licensed repo was found that performs *true* GCP-constrained BA over 3DGS parameters.

### 3. Precision & large-coordinate rendering

| # | Repo | ★ | License | Last push | Technique (what it actually does) | Verdict |
|---|---|---|---|---|---|---|
| 23 | [CesiumGS/cesium](https://github.com/CesiumGS/cesium) | 15907 | Apache-2.0 | 2026-11-03 | CesiumJS. Precision model: f64 transforms + **`RTC_CENTER`/`CESIUM_RTC` anchoring** + **`czm_translateRelativeToEye`** (high/low split, camera-relative) + multi-frustum for depth range. 2026: renders **splat tilesets** via `KHR_gaussian_splatting` and `KHR_gaussian_splatting_compression_spz`; splats proposed for 3D Tiles 2.0 (Cesium blog 2026-04-27). | **adopt-now.** Already the designated globe engine (Apache-2.0, blank ion token on Standard). The actionable part is the precision *contract*: never hand CesiumJS raw float32 ECEF; always emit a per-tile `transform`/`RTC_CENTER` anchor in f64 JSON. |
| 24 | [CesiumGS/3d-tiles](https://github.com/CesiumGS/3d-tiles) | 2637 | **no root LICENSE file found** (spec repo) | 2026-10-23 | The 3D Tiles specification (OGC Community Standard; 3D Tiles 1.1 = implicit tiling + metadata). Legacy tile formats define `RTC_CENTER` as **`float32[3]`**, with `POSITION` values relative to it. | **adopt-now as a spec, not as code.** Cite the RTC definition when designing our exporter; the license gap means we link to the spec text, we don't vendor it. |
| 25 | [CesiumGS/3d-tiles-tools](https://github.com/CesiumGS/3d-tiles-tools) | 544 | Apache-2.0 | 2026-10-20 | Node CLI/library: glTF/GLB ↔ 3D Tiles conversion, tileset optimization, legacy-format upgrade, stats/validation helpers. | **adopt-now.** The packaging step between "splat GLB with `KHR_gaussian_splatting`" and "tileset.json CesiumJS will load". Apache-2.0, maintained weekly. |
| 26 | [NASA-AMMOS/3DTilesRendererJS](https://github.com/NASA-AMMOS/3DTilesRendererJS) | 2511 | Apache-2.0 | 2026-10-28 | three.js / Babylon.js / R3F renderer for 3D Tiles with its own tile traversal + **`Ellipsoid`/WGS84 + region/tileset transform** math. | **adapt.** Not our renderer (we use CesiumJS for the globe), but its tileset-transform math is a clean JS reference for the ENU→ECEF placement `enu.ts` lacks, and it is the fallback if Cesium ion constraints ever bite. |
| 27 | [KhronosGroup/glTF](https://github.com/KhronosGroup/glTF) | 7876 | CC-BY-4.0 (spec text; `LICENSE.adoc`) | 2026-10-30 | `extensions/2.0/Khronos/KHR_gaussian_splatting` — **Complete, Ratified**: primitive marked as splat field with `POSITION` + `ROTATION`/`SCALE`/`OPACITY` + `SH_DEGREE_0_COEF_0` … `SH_DEGREE_3_COEF_[0-6]` (higher degrees conditional). It is a **glTF extension, not a 3D Tiles extension**. | **adopt-now.** This is the export target. CC-BY-4.0 = attribution only (compatible with a commercial app; add to About/attribution). Implementing it removes the need for a custom splat renderer in the globe view entirely. |
| 28 | [playcanvas/supersplat](https://github.com/playcanvas/supersplat) | 10564 | MIT | 2026-10-31 | 3DGS editor (PlayCanvas engine): view, crop, transform, compress (SPZ), export; handles large splat files in-browser. | **adapt.** Not a geo tool, but the best permissive reference for **SPZ compression + splat editing operations** in TS, and a sanity-check viewer for our exporters. Its transform gizmo is a UX model for "nudge the splat onto the DEM". |

### 4. 3D Tiles / quantized-mesh / terrain tiling pipelines

| # | Repo | ★ | License | Last push | Technique (what it actually does) | Verdict |
|---|---|---|---|---|---|---|
| 29 | [CesiumGS/quantized-mesh](https://github.com/CesiumGS/quantized-mesh) | 307 | **no root LICENSE file found** | 2026-05-19 | The **quantized-mesh 1.0 terrain spec**: per-tile local UV + 16-bit zigzag-encoded heights + optional edge indices/oct-encoded normals; the format `CesiumTerrainProvider` consumes via `layer.json`. | **adopt-now as a spec.** This is the terrain format for the v0.10 local-DEM globe path (already flagged in `RESEARCH-STACK.md` as `quantized-mesh/heightmap`). License gap → cite, don't vendor. |
| 30 | [kylebarron/quantized-mesh-encoder](https://github.com/kylebarron/quantized-mesh-encoder) | 111 | MIT | 2026-10-05 | Fast Python encoder for quantized-mesh; README walks DEM/heightmap → mesh → `.terrain` tiles. | **adopt-now.** Replaces a Dockerised CTB build with a pip dependency in the DEM sidecar: DEM raster → (encoder) → tiles + `layer.json`. MIT, actively pushed. Highest-leverage terrain item in this sweep. |
| 31 | [kylebarron/pydelatin](https://github.com/kylebarron/pydelatin) | 87 | MIT | 2026-11-02 | Python bindings to `hmm`: fast **terrain mesh generation** from heightmaps (adaptive triangulation). | **adopt-now (pairs with #30).** The mesh-generator half of the DEM→terrain pipeline; MIT, same author, compatible API shape. |
| 32 | [mapbox/martini](https://github.com/mapbox/martini) | 661 | ISC | 2026-06-24 | **RTIN** (right-triangulated irregular networks) terrain mesh generation in JS/WASM — real-time heightmap decimation with error-bounded LOD. | **adapt.** ISC (Standard-safe). Use if we want to build terrain meshes *in the frontend* from a heightmap we already hold (no sidecar), or for splat-derived terrain; the client-side alternative to #30/#31. |
| 33 | [geo-data/cesium-terrain-builder](https://github.com/geo-data/cesium-terrain-builder) | 888 | Apache-2.0 | **2023-03-05** | C++/CLI `ctb-tile`: GDAL raster → Cesium terrain tiles (+ quantized-mesh output). The classic CTB. | **reject (stale).** Last push 2023-03; GDAL-version coupling and a Docker/C++ build for something #30 does in Python. Note: **`tumgis/cesium-terrain-builder` returns 404** — `RESEARCH-STACK.md`'s license map should point at `geo-data/cesium-terrain-builder` (Apache-2.0, verified) or be replaced by #30 entirely. |
| 34 | [coronis-computing/emodnet_qmgc](https://github.com/coronis-computing/emodnet_qmgc) | 30 | **GPL-3.0** | 2026-11-02 | C++ library: GDAL raster → quantized-mesh tile pyramid, with Python helpers. | **reject.** GPL-3.0 excludes it from the Standard installer (same rule as SFINCS/HiPIMS). Algorithmically interesting (huge rasters), but #30 wins on license. |
| 35 | CesiumGS/3d-tiles-validator | **NOT VERIFIED** | **NOT VERIFIED** | **NOT VERIFIED** | Not fetched this run — superseded by #25, which ships validation helpers. | **defer.** Validate via `3d-tiles-tools` (#25) first; pull the standalone validator only if we need CI-grade conformance reports. |
| 36 | [protomaps/PMTiles](https://github.com/protomaps/PMTiles) | 3100 | BSD-3 | 2026-09-16 | Single-file, range-request tile archive ("pyramids of map tiles in a single file on static storage") + readers in JS/Go/Python. | **adapt.** Good fit for shipping offline basemap/terrain tile packs inside a workspace (one file, not a directory tree). BSD-3. Terrain-in-PMTiles is possible but not standard — verify before committing (see Missing evidence). |
| 37 | [tilezen/joerd](https://github.com/tilezen/joerd) | 427 | MIT | **2021-03-22** | The tooling behind **Mapzen/AWS Terrain Tiles** (AWS Open Data `elevation-tiles-prod`): download, merge, warp, and generate DEM tiles from many sources. | **reference-only.** Dead since 2021 and pipeline-shaped, but it documents the *source-compositing order* (SRTM/NED/ETOPO/3DEP/…) that makes global terrain tiles. Use the AWS tile bucket as a data source, not this repo as code. |
| 38 | [KhronosGroup/glTF](https://github.com/KhronosGroup/glTF) — SPZ extension note | 7876 | CC-BY-4.0 | 2026-10-30 | `KHR_gaussian_splatting_compression_spz`: gzip + quantization of splat attributes (Cesium reports up to ~90 % size reduction vs PLY; SPZ 2.0 stores rotations as normalized quaternions). | **adopt-now (with #27).** Compression is what makes a km-scale splat tileset loadable on the globe; it is also a precision *reducer*, so choose quantization steps deliberately (opacity/scale especially). |

### 5. DEM libraries, basemaps and splat viewers

| # | Repo | ★ | License | Last push | Technique (what it actually does) | Verdict |
|---|---|---|---|---|---|---|
| 39 | [geotiffjs/geotiff.js](https://github.com/geotiffjs/geotiff.js) | 1056 | MIT | 2026-09-13 | Pure-JS GeoTIFF parser (browser + node): reads tags, GeoTIFF georeferencing keys, windowed/decoded reads. | **adopt-now.** Already the planned DEM path in `RESEARCH-STACK.md` (MIT verified). Use it for front-end sampling of a local COG (AOI crop → heightmap) instead of round-tripping through Rust; keep the f64 origin on the Rust side. |
| 40 | [OSGeo/gdal](https://github.com/OSGeo/gdal) | 6130 | MIT | 2026-11-02 | The translator: warp/reproject (`gdalwarp`), vertical datum/geoid pipelines, COG writing, GeoTIFF/LAZ/GeoPackage I/O, `gdalinfo` provenance. | **adopt-now.** Backbone for DEM ingest: reproject 3DEP/GLO-30 to the AOI metric CRS, resample, build COG, and (critically) **attach the CRS/vertical-datum metadata that `dem.rs` currently records only as a string**. |
| 41 | [rasterio/rasterio](https://github.com/rasterio/rasterio) | 2591 | BSD-3-Clause | 2026-11-02 | Pythonic GDAL: windowed reads, `Affine`/`CRS` objects, reproject, masked arrays, COG access. | **adopt-now (sidecar).** Safer API surface than raw GDAL for the DEM conditioning sidecar (`sample_dem_grid` equivalents, nodata as masked arrays, per-AOI windows). BSD-3. |
| 42 | [OSGeo/PROJ](https://github.com/OSGeo/PROJ) | 2032 | MIT | 2026-11-02 | Coordinate transformation engine: EPSG database, `+proj` pipelines, **geoid grids (EGM96/EGM2008)**, ITRF/plate-motion epochs. | **adapt.** `transforms.rs` deliberately avoids PROJ ("kept dependency-free … so registration stays redistributable"). Keep that for WGS84↔ENU↔ECEF (closed-form, ~200 lines), and call PROJ only for *projected* CRS warps + **ellipsoidal↔orthometric (geoid) height** — the one conversion we genuinely cannot hand-roll. MIT. |
| 43 | [pyproj4/pyproj](https://github.com/pyproj4/pyproj) | 1233 | MIT | 2026-10-29 | Python bindings to PROJ with `Transformer`, `CRS.from_epsg`, `itransform`, always-xy/always-xyz flags. | **adapt.** The actual call site for #42 inside the DEM/GCP sidecar (including parsing ODM-style `EPSG:xxxx` GCP headers, #19). MIT. |
| 44 | [visgl/deck.gl](https://github.com/visgl/deck.gl) | 14728 | MIT | 2026-11-03 | WebGL2/WebGPU visualization framework; `TerrainLayer` (draped mesh from an elevation decoder), `TileLayer`, `COGLayer`, `ScenegraphLayer`. | **reference-only.** InstaSplat's 2D view is MapLibre and 3D is CesiumJS; adding deck.gl would be a third renderer. Worth reading `TerrainLayer`'s elevation-decoder interface if we ever need GPU-draped imagery over our own DEM in the ENU workspace. |
| 45 | [maplibre/maplibre-gl-js](https://github.com/maplibre/maplibre-gl-js) | 11995 | BSD-3-Clause | 2026-11-02 | Vector/raster map renderer with `raster-dem` sources + `terrain` for client-side terrain/hillshade. (Terrain API = researcher knowledge, not re-verified in source this run.) | **adopt-now (already in stack).** The existing 2D MapLibre view gains real terrain cheaply via a `raster-dem` source pointed at our staged DEM tiles — same tiles as Cesium, no new pipeline. BSD-3. |
| 46 | [openlayers/openlayers](https://github.com/openlayers/openlayers) | 12640 | BSD-2-Clause | 2026-11-02 | Map library with WebGL tile layers, `ol/source/GeoTIFF`, and 3D-ish camera controls. | **reject.** Redundant with MapLibre + CesiumJS; a third client-side map stack for no capability we lack. |
| 47 | [mkkellogg/GaussianSplats3D](https://github.com/mkkellogg/GaussianSplats3D) | 2938 | MIT | 2025-10-19 | three.js 3DGS viewer: `.ply`/`.splat`/`.ksplat` loading, streaming/LOD, `DropInViewer`, progressive loading of large scenes. | **adapt.** Our ENU workspace viewer is the natural home for this if we want splats without Cesium; `.ksplat` streaming is a permissive alternative to SPZ, and `DropInViewer` is the fastest in-app sanity check for an exported splat. MIT. |

### Row accounting (authoritative)

- **47 numbered rows** across the five tables.
- **43 unique repos carry verified metadata** (stars + last push + license status).
- **2 rows are metadata-incomplete by design:** #14 `gdaosu/SatSplat` (404 — NOT FOUND) and
  #35 `CesiumGS/3d-tiles-validator` (not fetched), plus #22 `borglab/gtsam` (deliberately dropped,
  NOT VERIFIED).
- **1 row is an extension note on an already-counted repo:** #38 = `KhronosGroup/glTF` (same as #27).

---

## Precision budget

### The float32 arithmetic (this is the whole argument)

IEEE-754 binary32 = 1 sign + 8 exponent + 23 fraction bits ⇒ **24-bit significand**. The gap between
representable neighbours at magnitude `x` is

```
ULP(x) = 2^(floor(log2 x) − 23)        relative spacing ≤ 2^-24 ≈ 5.96e-8
```

So a **millimetre** survives only while

```
1e-3 m ≤ x · 2^-24   ⇒   x ≤ 1e-3 · 2^24 = 16 777.216 m ≈ 16.8 km
```

| Distance from ENU origin | float32 ULP | What breaks |
|---|---|---|
| 100 m | 7.6 µm | nothing |
| 1 km | **0.061 mm** | nothing (demo/survey fine) |
| 8.19 km (2¹³) | **0.98 mm** | last point at which mm is representable |
| 16.8 km (2¹⁴) | **1.95 mm** | **mm-class precision is gone beyond this** (the 16.7 km figure) |
| 100 km | **7.8 mm** | cm-class RTK claims become unverifiable |
| 1 000 km | **6.3 cm** | regional scenes visibly jitter |
| 6 378 km (raw ECEF) | **0.5 m** | ECEF as float32 is **dead on arrival** — 0.5 m spacing at the Earth's surface |

Corollaries that matter more than the table:

1. **Never upload ECEF as float32.** Cesium's own guidance is the same: subtract a nearby centre in
   f64 *before* building the `Float32Array`, and note that the legacy `RTC_CENTER` field is itself
   `float32[3]` — a rendering anchor, not a survey record.
2. **Longitude/latitude as float32 is also ~1 m.** `180° · 2^-24 ≈ 1.07e-5° ≈ 1.19 m` at the equator.
   Store `lon/lat/height` in f64 only (`transforms.rs` already uses `f64` throughout — correct).
3. **`src/geospatial/enu.ts` is the weak link.** It converts with a flat `metresPerDegree(lat)` factor
   (`wgs84ToEnu` / `enuToWgs84`) — a flat-earth approximation with **no ellipsoid curvature and no
   rotation into the local tangent frame**. Over a 1 km AOI the error is sub-decimetre and invisible;
   over 10–50 km it becomes metres, and it silently disagrees with the ellipsoidal `transforms.rs` used
   on the Rust side. *Recommendation (inference from reading both files):* have `enu.ts` consume the
   `EnuFrame` origin + `enu_to_ecef_matrix` from `transforms.rs` and do a proper ENU→ECEF→geodetic
   round-trip, or at minimum document it as "AOI-local approximation, ≤2 km".
4. **The dominant error is not float32.** With 1 cm RTK and 10 m-posting DEMs, the georeferencing budget
   is consumed by (a) GNSS/RTK residuals (≈1–4 cm), (b) DEM vertical error (metres — 3DEP 1/3″ and
   Copernicus GLO-30 are decimetre-to-metre-class vertical products depending on terrain/vegetation),
   and (c) splat-vs-DEM vertical bias from the reconstruction itself. Float32 binds only past ~8–17 km.
   *(Interpretation; DEM RMSE ranges are researcher knowledge, not measured here — see Missing evidence.)*
5. **Terrain quantization budget.** quantized-mesh stores heights as 16-bit zigzag: 65 536 levels over a
   tile's height range. A 1 km tile spanning 800 m relief → **1.2 cm** steps; the same 16 bits across a
   3 km tile spanning 3 000 m → **4.6 cm**. Per-tile bounding volumes keep this local — which is exactly
   why per-tile encoding, not one global mesh, is the answer.

### Mitigation each candidate actually uses

| Candidate | Mitigation |
|---|---|
| CesiumJS (#23) | f64 transforms in JS; `RTC_CENTER`/`CESIUM_RTC` anchor per tile; `czm_translateRelativeToEye` (high/low split) for camera-relative vertex math; multi-frustum for depth, not coordinate, precision |
| 3D Tiles spec (#24) | per-tile `transform` (f64 in JSON) + `RTC_CENTER` (`float32[3]`, relative offsets) |
| 3d-tiles-tools (#25) / tiler (#16) | emit tileset root `transform` = ENU→ECEF matrix; keep tile-local vertex data small |
| quantized-mesh (#29) + encoder (#30) + pydelatin (#31) + martini (#32) | per-tile local UV + 16-bit heights; bounding volumes per tile; no global coordinate ever encoded |
| SPZ compression (#38) | attribute quantization — a precision knob to set deliberately per attribute (rotation/scale/opacity) |
| COLMAP `model_aligner` (#18) | solves Sim(3) in f64 but **outputs ECEF** (≈6.4e6 m) → must re-centre to AOI ENU before training/export (inference) |
| geo-register-plugin (#15) | RANSAC+IRLS Umeyama scene→ECEF, then stores the result as a 3D Tiles root transform (anchors rather than bakes) |
| CityGaussian (#7), Hierarchical (#12), VastGaussian (#10), Octree-GS (#11), Mega-NeRF (#13) | partition the scene into blocks/cells with **per-block origins/normalization**; train and stream per block |
| InstaSplat today | `transforms.rs` = f64 closed-form geodetic↔ECEF↔ENU (good); `enu.ts` = flat approximation (must be upgraded); no chunking yet; `dem.rs` carries `terrain_tiles_url` + `bed_source` but no vertical datum |

**Practical InstaSplat rule (inference):** single ENU frame ≤ **8 km** for sub-mm vertex resolution,
≤ **16.8 km** for mm; cap a workspace at **~10 km** and tile beyond that, publishing each tile with its
own f64 anchor. For AOIs ≥ ~2° per side, `sat_splat.rs` already refuses (§`validate_aoi`), which
conveniently keeps current products inside the safe envelope.

---

## Contradictions

1. **"3D Tiles Gaussian splatting extension" — it is a glTF extension.** `KHR_gaussian_splatting` is a
   *glTF 2.0* extension (Ratified, Khronos); 3D Tiles consumes it as glTF/GLB tile content. Cesium's
   blog frames splats as proposed for **3D Tiles 2.0**, while several community posts describe splats
   as a 3D Tiles 1.1 feature. Recorded, unresolved: 1.1 definitively adds implicit tiling + metadata;
   splat support rides on glTF.
2. **ODM GCP semantics.** The `--gcp` docs page confirms GCP input + georeferencing; community claims
   describe "GCP-constrained bundle adjustment". The docs page fetched does *not* state that GCPs drive
   the BA. Unresolved.
3. **CityGaussian repository identity.** `LTriomphe/CityGaussian` now **404s**; the live repo is
   `Linketic/CityGaussian` (ECCV'24 + ICLR'25 series, V2 on `main`). Any doc pointing at the old path
   (including older InstaSplat notes) is stale.
4. **VastGaussian.** `city-super/VastGaussian` **404s**; the maintained code is `kangpeilun/VastGaussian`,
   which self-describes as an *unofficial* implementation — and is nonetheless **Apache-2.0**, while the
   official 3DGS-derived repos are non-commercial. Permissiveness here does not imply officialness.
5. **`tumgis/cesium-terrain-builder` 404 vs `RESEARCH-STACK.md`**, which lists it as Apache-2.0
   "verify image/tool". The canonical CTB is `geo-data/cesium-terrain-builder` (Apache-2.0, verified)
   but **last pushed 2023-03**. Recommend replacing that row with `quantized-mesh-encoder` (MIT, 2026).
6. **Skyfall-GS license surface.** Root `LICENSE` is Apache-2.0, but the repo also ships `LICENSE_inria.md`
   and vendors `diff-gaussian-rasterization-depth` submodules; the standard reading is Apache-2.0 for
   *this* code with INRIA non-commercial terms on the vendored rasterizer. Treating "Skyfall-GS is
   Apache-2.0" as blanket permission to vendor the full stack would be wrong.
7. **Star counts are point-in-time.** Values here came from a mirror on 2026-11-03; they will differ from
   `api.github.com` and from any cached snapshot. Do not treat them as stable identifiers.

## Missing evidence

- **`CRISP`** — no repo, paper, or project page could be verified for an aerial/satellite Gaussian-splatting
  project by that name. Closest 2026 matches surfaced were SharpSplat, SkySplat, Urban-GS, ProDiG, AeroDGS.
  **Treat "CRISP" as unverified / possibly mis-remembered.**
- **`UAV-GS` / `UAVGS`** — no such repository found. Nearest real: DroneSplat (#3), Ortho-3DGS (#4),
  Horizon-GS (#8).
- **`GeoSplat`** — only `manudelu/georeferenced_gsplat` (#5, no license) and `zqlin0521/GeoGS`
  (sparse-view *urban* geometry prior, not georeferencing) exist; `PKU-VCL-Geometry/GeoSplatting` is a
  different (geometry-only) paper. None is a "georeferenced 3DGS toolkit".
- **`gdaosu/SatSplat`** — 404; project page links it. Code release unverified.
- **CesiumGS/3d-tiles and CesiumGS/quantized-mesh licenses** — no root LICENSE/COPYING file exists; the
  spec text is OGC-hosted. Attribution path not established this run (spec repos cited, not vendored).
- **SatelliteSfM, SkySplat-main, georeferenced_gsplat** — **no license file at all**. Under default
  copyright these are "all rights reserved"; they must not be copied into the product.
- **DEM vertical accuracy figures** (3DEP 1/3″ ≈ 10 m posting; Copernicus GLO-30 = 30 m DSM;
  decimetre-to-metre RMSE depending on cover) — researcher knowledge, **not verified in this run**. The
  precision-budget conclusion (DEM ≫ float32) is robust to the exact RMSE, but specific numbers need a
  USGS/Copernicus product-spec citation before appearing in user-facing docs.
- **Vertical datum / geoid (highest-risk gap).** No listed repo handles ellipsoidal↔orthometric
  conversion except PROJ/pyproj (#42/#43) via geoid grids; nobody in the 3DGS cluster does. InstaSplat's
  `height_m` is ellipsoidal in `transforms.rs` — mixing it with orthometric DEM heights (3DEP tiles are
  typically orthometric) is a latent metre-scale bug. **Unresolved.**
- **`maplibre` terrain**, **`deck.gl` TerrainLayer**, **`Open3D` ICP tuning** — API claims from researcher
  knowledge of those libraries, not re-verified against source this run.
- **Performance claims** ("SPZ up to ~90 % smaller", Cesium ion LOD) come from Cesium's own blog, not a
  measured benchmark on InstaSplat data.

## Sources

**Kept (primary/authoritative):**
- `ungh.cc/repos/*` — GitHub REST mirror used for stars/pushedAt because `api.github.com` was rate-limited this run (per-entry URLs above).
- `raw.githubusercontent.com/*/HEAD/LICENSE*` — license files, read directly (per-entry).
- [KHR_gaussian_splatting README](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_gaussian_splatting/README.md) — Ratified status + attribute layout; the export contract.
- [Cesium blog: 3D Gaussian Splats + LOD (2026-04-27)](https://cesium.com/blog/2026/04/27/3d-gaussian-splats-lod/) — CesiumJS support via `KHR_gaussian_splatting` + `_compression_spz`; 3D Tiles 2.0 proposal; SPZ quantization.
- [3D Tiles PointCloud spec (`RTC_CENTER`)](https://github.com/CesiumGS/3d-tiles/blob/main/specification/TileFormats/PointCloud/README.adoc) — RTC semantics and the `float32[3]` caveat.
- [czm_translateRelativeToEye](https://cesium.com/downloads/cesiumjs/releases/b16/Documentation/czm_translateRelativeToEye.html) — high/low camera-relative precision path.
- [COLMAP FAQ (`model_aligner`)](https://raw.githubusercontent.com/colmap/colmap/main/doc/faq.rst) — `--ref_is_gps`, `--alignment_type ecef`, `--alignment_max_error`, ≥3 images.
- [ODM GCP arguments](https://docs.opendronemap.org/arguments/gcp/) — `--gcp` file format with `EPSG:` header.
- [OpenSfM georeferencing docs](https://github.com/OpenSfM/OpenSfM/blob/master/doc/georeferencing.md) — ENU/LLA/ECEF contract.
- [geo-register-plugin README](https://github.com/dozeri83/geo-register-plugin) — RANSAC+IRLS Umeyama → ECEF, 3D Tiles root transform.
- [tiler README](https://github.com/CZ-CHO/tiler) — PLY → SPZ GLB 3D Tiles + WGS84→ENU root transform.
- [Ortho-3DGS README](https://github.com/alexzhenglong/Ortho-3DGS) — depth-regulated orthorectification, chunking, no CRS.
- [SatelliteSfM README](https://github.com/jayin92/SatelliteSfM) — RPC → pinhole, ENU + Lat/Lon/Alt.

**Rejected / deprioritized:**
- `LTriomphe/CityGaussian`, `city-super/VastGaussian`, `tumgis/cesium-terrain-builder`, `gdaosu/SatSplat` — **404** (renamed/removed); replaced by `Linketic/CityGaussian`, `kangpeilun/VastGaussian`, `geo-data/cesium-terrain-builder`, and NOT FOUND respectively.
- `OSGeo/gdal` description vs `LICENSE.TXT` — trust the license file (MIT, confirmed).
- Search-engine "answer" summaries claiming CRISP/UAV-GS exist — unverifiable, discarded.
- `coronis-computing/emodnet_qmgc` — GPL-3.0, functionally superseded by #30.
- `tilezen/joerd` — dead since 2021; kept only as documentation of terrain-source compositing.
- `openlayers/openlayers` — redundant third map stack.

## Next steps

1. **Write the exporter against `KHR_gaussian_splatting` (+SPZ)** and validate with `3d-tiles-tools`,
   with `tileset.json` `root.transform` = `transforms::enu_to_ecef_matrix()`. Shortest path to splats on
   the CesiumJS globe, and it removes any custom globe splat renderer.
2. **Settle the vertical datum question** (ellipsoidal vs orthometric) before shipping any DEM-aligned
   product; add an explicit `vertical_datum` field beside `DemProduct.crs` and convert with PROJ geoid
   grids in the sidecar.
3. **Implement RANSAC Umeyama (scene→ECEF) in Rust** with a residual gate, mirroring #15/#18 semantics;
   feed it COLMAP `model_aligner --alignment_type ecef` output OR raw GNSS/GCP pairs.
4. **Replace the CTB row** in `RESEARCH-STACK.md` with `quantized-mesh-encoder` + `pydelatin` (MIT,
   active) and downgrade `tumgis`/`geo-data` CTB to "legacy reference".
5. **Upgrade `enu.ts`** to ellipsoidal frames (or hard-cap AOI extent with a documented tolerance).
6. Re-run this sweep for `SatSplat`, `CRISP`, and any Cesium 3D Tiles 2.0 splat finalisation at the next
   milestone — all three are moving.

---

## Top-5 adopt-now

1. **[KhronosGroup/glTF — `KHR_gaussian_splatting`](https://github.com/KhronosGroup/glTF)** — 7876★,
   spec CC-BY-4.0, last push 2026-10-30. Ratified glTF splat extension that CesiumJS already renders;
   defines `POSITION`/`ROTATION`/`SCALE`/`OPACITY`/SH attributes. **Adopt as the export target this cycle.**
2. **[CesiumGS/3d-tiles-tools](https://github.com/CesiumGS/3d-tiles-tools)** — 544★, Apache-2.0,
   2026-10-20. Packaging/validation between our GLB output and a loadable `tileset.json`. **Adopt.**
3. **[kylebarron/quantized-mesh-encoder](https://github.com/kylebarron/quantized-mesh-encoder)** — 111★,
   MIT, 2026-10-05. DEM → `.terrain` tiles in Python; retires the Dockerised CTB path and unblocks the
   v0.10 local-DEM globe terrain. **Adopt (with `pydelatin`, 87★ MIT).**
4. **[colmap/colmap](https://github.com/colmap/colmap) `model_aligner`** — 13139★, BSD-3, 2026-11-03.
   `--ref_is_gps 1 --alignment_type ecef` gives Sim(3)+RANSAC georeferencing on the SfM we already run;
   the cheapest correct path to WGS84→ENU for drone captures. **Adopt.**
5. **[OSGeo/gdal](https://github.com/OSGeo/gdal)** — 6130★, MIT, 2026-11-02. Reprojection, resampling,
   COG output, CRS/vertical-datum metadata for 3DEP/GLO-30 ingest. **Adopt (via `rasterio` in the sidecar).**

*Runner-up:* `isl-org/Open3D` (14170★, MIT) for splat→DEM/LiDAR ICP refinement with a residual report.
