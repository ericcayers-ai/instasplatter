# Geospatial natural-disaster + Cesium globe — design

**Status:** approved for phased implementation (v0.10)  
**Release target:** v0.10.0  
**Scope:** Geospatial suite — flood simulation realism + optional CesiumJS Globe view; docs/license gate first

## Problem

The geospatial suite today (v0.9.x) can draw an AOI, run a soft flood preview, and optionally call ANUGA/SWMM, but DEM/catalog connectors are stubs, DEM is often synthetic, and the only views are MapLibre 2D and ENU WebGL. Users need a more realistic **flood-without-splat** path (real DEM + rapid inundation aids + hazard data layers) and an optional **globe** for planetary context — without replacing the ENU workspace or shipping ion-locked terrain as a hard dependency.

## Goals (v0.10)

1. **Flood-only simulated physics** — inundation remains the sole authoritative simulation path (ANUGA scientific when installed; soft preview / HAND labelled non-authoritative until promotion gates pass).
2. **Real DEM + catalog** — USGS 3DEP / Copernicus GLO-30 (and related connectors) feed `dem` stage/condition for the AOI; no synthetic-only bed when a DEM is available.
3. **Optional Globe view** — CesiumJS as a third mode beside MapLibre and ENU (`2d | 3d | globe`), dual-viewer + shared store.
4. **Hazard data layers** — FEMA NFHL, HydroSHEDS, gauges, OSM waterways as overlays; earthquake / wildfire / landslide / tsunami = Experimental stubs only (no fake physics).
5. **License-clean Standard path** — CesiumJS Apache-2.0 self-hosted; blank ion token; no ion World Terrain / Bing / Google as hard deps; no GPL hydro in the base installer.

## Non-goals (out of scope for v0.10)

- Replacing ENU with Cesium for splat editing / gizmos  
- Bundling ANUGA, SWMM, or Cesium ion credentials in the installer  
- Authoritative wildfire / quake / tsunami / landslide solvers  
- GPL hydro engines on the Standard path  
- MapLibre canvas-drape → Cesium terrain (VRAM killer; forbidden)

## Research summary

### Cesium licensing (must follow)

| Item | Decision |
|---|---|
| Library | **CesiumJS** (`cesium` / `@cesium/engine`), **Apache-2.0**; self-host Workers / Assets / Widgets.css |
| React wrapper | **Resium** optional (Apache-2.0); prefer a thin Tauri-CSP-friendly wrapper if Resium fights asset paths |
| Ion default token | **Blank** on Standard; disable geocoder and ion default base layers; assert **zero** `api.cesium.com` traffic on Standard |
| User ion token | **Experimental only**; Commercial ion ToS if used; Community = non-commercial / eval only |
| Forbidden hard deps | ion World Terrain, Bing, Google Photorealistic / 3D Tiles as required Standard terrain or imagery |
| Terrain without ion | AOI DEM → GDAL → quantized-mesh ([tumgis/cesium-terrain-builder](https://github.com/tum-gis/cesium-terrain-builder-docker)) or heightmap → local `CesiumTerrainProvider` |
| Alt tiles | AWS Mapzen Terrain Tiles OK with per-source attribution |

### Flood P0 realism aids

| Source | Role | License / notes |
|---|---|---|
| USGS 3DEP / elevation APIs | DEM for US AOIs | Public domain / USGS |
| Copernicus DEM GLO-30 | Worldwide DEM | Cite Copernicus |
| OpenTopography | High-res DEM when API key present | API ToS |
| HAND stage→extent | Rapid inundation preview | Prefer [NOAA-OWP/inundation-mapping](https://github.com/NOAA-OWP/inundation-mapping) **Apache-2.0**; label Live preview / non-authoritative |
| FEMA NFHL (WMS/Feature) | Flood-zone overlay (not simulation) | US public |
| HydroSHEDS / HydroBASINS | Catchments / flow | Cite HydroSHEDS |
| NOAA / USGS gauges (NWIS) | Hydrograph forcing | Public |
| OSM waterways | Network overlay | **ODbL** — attribution + share-alike for derived OSM databases |
| Earth Search STAC | Optional flood-extent imagery | STAC / open |

### Hazard policy

| Hazard | v0.10 treatment |
|---|---|
| Flood | Simulated (soft preview, HAND, ANUGA when ready) + data layers |
| Earthquake / wildfire / landslide / tsunami | Experimental data stubs / feed cards only — **no** fake physics, **no** authoritative claims |

## Approaches considered

1. **Dual-viewer + shared store (MapLibre | ENU | Cesium)** *(recommended)*  
   Pros: keeps ENU as metric splat workspace; Globe is opt-in; one AOI / flood / DEM truth in the store. Cons: camera sync is approximate across CRS conventions.

2. **Replace ENU with Cesium**  
   Pros: one 3D stack. Cons: splat gizmos and ENU metric tools regress; out of scope for v0.10.

3. **MapLibre terrain → canvas drape into Cesium**  
   Pros: reuses 2D pipeline. Cons: VRAM blow-up; explicitly forbidden.

## Architecture

```
GeoViewport (viewMode: 2d | 3d | globe)
  ├─ MapLibre 2D     ← soft depth ImageSource / overlays
  ├─ ENU WebGL       ← terrain + depth water + splat gizmos (unchanged role)
  └─ CesiumJS Globe  ← DEM terrain provider + flood primitive/texture + attribution

Shared store (AOI, layers, dem path, preview depths, authority badge)
       │
Rust geospatial
  Catalog connectors → DEM stage/condition → soft preview / HAND / ANUGA
                                         → exports (DEM-only OK)
```

### Viewer roles

| Mode | Stack | Job |
|---|---|---|
| `2d` | MapLibre | Satellite + AOI + flood ImageSource |
| `3d` | ENU WebGL | Default metric workspace; splat gizmos; flood depth mesh |
| `globe` | CesiumJS | Optional planetary context; DEM terrain + flood overlay; splat only if cheap (3D Tiles / billboard) else keep splat in ENU |

Camera/AOI sync: bbox + look-at between MapLibre ↔ Cesium where practical; ENU remains the editing workspace.

### Terrain pipeline (Standard)

```
Catalog fetch (3DEP / GLO-30 / user GeoTIFF)
  → dem.rs stage + condition (nodata, AOI clip, ExtentPlan resolution)
  → GDAL → quantized-mesh (tumgis CTB) or heightmap tiles
  → CesiumTerrainProvider (local / file URL)
```

Never: MapLibre canvas-drape → Cesium.  
Experimental only: ion World Terrain when user supplies a token.

### Flood P0 data path

```
AOI → fetch DEM → soft preview bed from DEM samples
                → HAND rapid inundation (Live preview badge)
                → NFHL / HydroSHEDS / gauges / OSM waterways as layer-tree layers
                → ANUGA when sidecar ready (Scientific badge; Demo when missing)
                → exports: depth/hazard COG + scenario manifest for DEM-only workspaces
```

### Components (touchpoints)

| Unit | Responsibility |
|---|---|
| `src/geospatial/GeoViewport.tsx` | Host 2d / 3d / globe; share store-driven AOI + flood |
| `src/geospatial/GeoToolbar.tsx` | View-mode toggle including Globe |
| `src/geospatial/GeoLayerTree.tsx` + `defaults.ts` | NFHL / HydroSHEDS / gauges / waterways layers |
| `src/geospatial/types.ts` | Extend `GeoViewMode` → `"2d" \| "3d" \| "globe"` |
| `src/geospatial/globe/CesiumGlobe.tsx` (new) | Thin Cesium wrapper; blank ion; local terrain + flood |
| `src/geospatial/imageryTiles.ts` | Imagery XYZ + attribution (Esri where ToS allows; fallbacks) |
| `src/geospatial/preview/softSolver.ts` | Bed from real DEM samples |
| `src/state/store.ts` / `src/lib/ipc.ts` | Shared geo state + IPC for catalog/DEM/HAND |
| `src-tauri/.../catalog.rs` | Real `CatalogEntry` / `fetch_asset` for P0 connectors |
| `src-tauri/.../dem.rs` | Stage + condition AOI DEM; terrain-tile prep hook |
| `src-tauri/.../hydro.rs` | ANUGA/SWMM + promotion gates; HAND labelled non-authoritative |
| `src-tauri/.../exports.rs` | DEM-only workspace exports |
| About / `docs/RESEARCH-STACK.md` | Cesium + DEM/HAND/NFHL/HydroSHEDS license table |

### Standard vs Experimental (geo / Cesium)

| | Standard | Experimental |
|---|---|---|
| CesiumJS engine | ON (self-hosted, blank ion) | Same |
| Local DEM terrain | ON | ON |
| ion World Terrain / geocoder / ion basemaps | Off | Only if user ion token set + ack |
| HAND / soft preview | Live preview (non-authoritative) | Same |
| ANUGA | Scientific when installed | Same |
| Multi-hazard physics | Never | Never (stubs / feeds only) |
| GPL hydro | Never in installer | External plugin only |

### Error handling / honesty

- Missing DEM → clear connector error; soft preview may fall back to synthetic bed only with an honest badge  
- Missing ANUGA → Demo / Live preview labels; never claim Scientific  
- HAND extents → always non-authoritative until compare gates pass  
- Blank ion on Standard → no silent ion traffic; geocoder disabled  
- OSM-derived layers → ODbL attribution in chrome / About  

### Testing (later phases; not Phase A)

- Catalog/DEM unit tests for fetch + stage  
- Cesium-prep: quantized-mesh / heightmap smoke  
- Assert Standard path makes no `api.cesium.com` requests  
- UI: Globe toggle + flood overlay screenshots under `docs/assets/verify/v0.10/`  
- E2E: DEM-only AOI → preview → export without splat  

## Success criteria (v0.10 release)

1. Design + RESEARCH-STACK license notes landed (this Phase A).  
2. Real DEM for AOI drives soft preview / HAND / optional ANUGA.  
3. Globe view shows local DEM terrain + flood overlay with correct attribution and blank ion on Standard.  
4. NFHL / HydroSHEDS / gauges / OSM waterways appear as layers; other hazards remain Experimental stubs.  
5. Verification matrix green before tag; multi-platform installers only after gates pass.  

## Phasing reminder

| Phase | Content |
|---|---|
| **A** | This spec + RESEARCH-STACK (docs only) |
| B | Cesium globe viewer + `GeoViewMode` |
| C | DEM connectors, HAND, flood layers, DEM-only export |
| D | Multi-hazard Experimental stubs |
| E | Verification matrix |
| F | Multi-platform CI / 0.10.0 release |
