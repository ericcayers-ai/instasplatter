# Research stack (v0.9+) — dual suite + dual mode

Evidence-first inventory: **[PAPER-SWEEP-2024+.md](./PAPER-SWEEP-2024+.md)**.

InstaSplatter is a **dual-suite** app. Each suite keeps a **Standard** vs **Experimental** policy. In-app **About** summarizes stacks, licenses, and attribution (including Esri World Imagery).

| Suite | Default job |
|---|---|
| **Reconstruction** | Capture → scored cameras → confidence-fused dense → **live stages** (frustums / sparse / dense / splat) |
| **Geospatial** | Draw **AOI anywhere** → **3D ENU workspace** (default) or 2D satellite → ANUGA/SWMM + live preview → exports |

| | **Standard Mode** (default) | **Experimental Mode** (license ack) |
|---|---|---|
| Intent | Best quality that stays commercially redistributable | Absolute max quality; personal/research; NC OK after ack |
| Cameras | Capture-aware commercial chain (VGGT-C, MapAnything, COLMAP priors) | Profile routing: static Ω/MASt3R/DUSt3R/Pi3X; long video StreamVGGT/VGGT-Long/MASt3R-SLAM/SLAM3R; dynamic Ω/MonST3R/Easi3R (+ separate 4D); large aerial CityGaussian/Urban-GS/Horizon |
| Dense init | **RoMa v2 densify** ∧ DA3 ∧ COLMAP MVS ∧ sparse | Profile-matched densifiers, then **confidence-fuse** (schema v2 + gates) — never raw concatenate |
| Surface / 4D | Stock mesh path | Separate adapters (`gs-2d`, GOF/PGSR/…, MonST3R/Easi3R) |
| Polish | NVIDIA Fixer when installed | **Difix then Fixer** |
| Trainer | gsplat Auto on CUDA else Brush | Force gsplat Max + strategies; Brush Max if no gsplat |
| Flood | **ANUGA** scientific (+ **SWMM** network); demo fallback labelled non-authoritative | TRITON / Wflow / GeoClaw external permissive; SFINCS/HiPIMS/BG_Flood/Itzï **GPL plugin only** |
| Preview | WebGPU/CPU soft solver — always “Live preview” until compare gates pass | Same; promotion blocked without `HydroPromotionGates` |
| UI | Quiet chrome; Geospatial defaults to 3D ENU | TitleBar Experimental ON + discrete NC banner + solver chips |

Weights / PyTorch sidecars stay **user-installed** under
`%LOCALAPPDATA%/InstaSplatter/engines/sidecars/`. Hydro plugins:
`%LOCALAPPDATA%/InstaSplatter/engines/hydro/`. Base NSIS installer stays lean
(COLMAP + Brush only).

## Standard Mode pipeline (reconstruction)

```
Video → frame gate
  → capture-aware commercial poses → optional COLMAP BA
      else COLMAP SfM (or live-init → COLMAP)
  → Live sparse cloud + frustums in 3D viewport
  → RoMa v2 densify ∧ DA3/VGGT-C ∧ COLMAP MVS ∧ sparse → init.ply
  → Live dense cloud as densify merges
  → gsplat (CUDA) or Brush: progressive ∧ mip ∧ AbsGS opac/scale ∧ MCMC
  → Fixer polish when installed
  → Live PLY lerp viewport → Mesh / SPZ v4 export
```

## Experimental Mode pipeline (reconstruction)

```
Video → frame gate → detect CaptureProfile
  → profile-matched research pose hypotheses (scored)
  → canonical COLMAP/ENU + validation gates (reject before fusion)
  → confidence-fuse densifiers (schema v2) — never raw concatenate
  → optional 4D / large-scene / surface adapters on separate paths
  → Difix then Fixer
  → gsplat Max (MCMC **or** AbsGrad, exclusive; +AA+appearance+bilagrid) or Brush Max
  → Live PLY lerp viewport → Mesh / SPZ v4 export
```

Routing table lives in `src-tauri/src/pipeline/experimental.rs`.

## Geospatial flood path (v0.9)

```
Draw AOI (WGS84) anywhere → commit_flood_aoi
  → soft-solver domain = AOI (not Wellington-locked)
  → Esri World Imagery underlay (Carto/OSM low-bandwidth fallback)
  → optional splat_bounds_enu from latest PLY into extent plan
  → 3D ENU workspace (default): terrain + depth water + splat gizmos
      or 2D MapLibre satellite + AOI + flood ImageSource
  → Scientific: ANUGA (+ optional SWMM) → sim:// checkpoints → SimulationRun
      else labelled demo extents (never authoritative)
  → Preview: WebGPU or CPU soft solver → display-rate interp
      authority badge: Live preview | Demo | Scientific
  → Exports: COG / GeoPackage / Zarr-meta / residual report + manifest
      authoritative=true only for calibrated ANUGA scientific products
```

Basemap attribution (Esri World Imagery) is required and shown in the geo chrome / About.

## License map (hard constraints)

| Component | License | Standard | Experimental |
|---|---|---|---|
| VGGT-1B-Commercial | Meta AUP (commercial) | Primary SfM | Fallback in chain |
| VGGT-Ω | CC BY-NC-4.0 | Never | Preferred static SfM |
| MASt3R / DUSt3R / Pi3X | CC BY-NC(-SA) | Never | Static unordered |
| StreamVGGT / VGGT-Long / SLAM* | research/NC | Never | Long video |
| MonST3R / Easi3R | research/NC | Never | Dynamic + separate 4D |
| CityGaussian / Urban-GS / Horizon | research/NC | Never | Large aerial adapters |
| 2DGS / GOF / SuGaR / … | mix | Never (vendored) | Surface adapters |
| RoMa v2 code | **MIT** | Densify sidecar | Densify (`precise`) |
| Lichtfeld Densification Plugin | **GPL-3.0** | **Do not copy** — recipe only | Same |
| Difix3D+ | Research/gated | Off | Preferred polish (then Fixer) |
| NVIDIA Fixer | NVIDIA Open Model | ON when installed | ON when installed |
| COLMAP / DA3 / gsplat / Brush | BSD / Apache / Apache | ON | ON |
| ANUGA | Apache-2.0 | Scientific flood | Scientific flood |
| EPA SWMM | Public domain | Network exchange | Network exchange |
| WebGPU/CPU preview | Apache-2.0 | Preview only | Preview only |
| Esri World Imagery | Esri terms + attribution | Basemap (Standard) | Same |
| TRITON / Wflow / GeoClaw | permissive (verify) | Never | External hydro install |
| SFINCS / HiPIMS / BG_Flood / Itzï | **GPL** | Never | External plugin only |

## Sidecar install

See `tools/sidecars/README.md` and `tools/sidecars/research/README.md`.

## Hydro adapters + promotion

Registry + GPL external-plugin protocol: `src-tauri/src/geospatial/hydro.rs`
(façade: `hydro_adapters.rs`). Docs: `tools/sidecars/hydro-plugins/README.md`,
`tools/sidecars/anuga/README.md`, `tools/sidecars/swmm/README.md`.

`HydroPromotionGates` (lake-at-rest, wet/dry, dam-break, mass conservation,
ANUGA cross-compare, license clearance, …) must all clear before any
experimental hydro engine can be considered for Standard. GPL engines
**cannot** promote into the Apache installer. Preview soft-solver exposes
`lakeAtRestMassOk` / `massRelError` hooks for graphics continuity — they do
**not** count as scientific validation.

## Automated gates that pass in-repo (v0.9)

- Fusion / Sim(3) dense fusion unit tests
- ENU/ECEF CRS round-trips (`geospatial::transforms`)
- Project v1→v2 migration + geospatial workspace dirs
- AOI → ENU domain / `commit_flood_aoi` persistence
- `model_transform` project round-trip
- Hydro promotion / GPL refuse-bundle tests
- Export manifest `authoritative` flags (demo false; calibrated ANUGA only)
- GCP residual threshold constants + survey/identity residual tests
- Preview scientific-compare tolerances (mass / depth / wet fraction)
- Splat AABB helper for extent `splat_bounds_enu`

## Honest gaps toward v1.0

- **Shared water/splat depth compositing** — 3D geo still uses camera-synced layered canvases; water does not correctly occlude individual Gaussians
- Full ANUGA analytical suite on real solver (lake-at-rest, dam-break, rainfall-on-slope) against published benchmarks
- Three drone datasets with RTK/GCP survey truth
- Site / city / regional performance benchmarks
- Windows installer upgrade + accessibility audit pass
- Uncertainty ensembles and large-scene 3D Tiles at production scale
