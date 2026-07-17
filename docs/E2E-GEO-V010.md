# Geospatial E2E verification matrix — v0.10 (Phase E)

**Date:** 2026-07-17  
**Scope:** Phase B–D gates from the geo-disaster + Cesium plan.  
**Evidence root:** [`docs/assets/verify/v0.10/`](./assets/verify/v0.10/)  
**Status legend:** PASS · FAIL · CONDITIONAL (criteria stated) · MANUAL (capture pending)

---

## Automated unit gates (this run)

| Gate | Command | Result | Evidence |
| --- | --- | --- | --- |
| Rust unit suite | `cd src-tauri && cargo test` | **PASS** — 263 passed, 0 failed | [`cargo-test.log`](./assets/verify/v0.10/cargo-test.log) |
| Geospatial subset | same log (`test geospatial::…`) | **PASS** — 55 geospatial tests ok | same |
| Typecheck | `npx tsc --noEmit` | **PASS** — exit 0 | [`tsc-noEmit.log`](./assets/verify/v0.10/tsc-noEmit.log) |
| Frontend build | `npm run build` | **PASS** — vite build ok | [`npm-build.log`](./assets/verify/v0.10/npm-build.log) |

No unit regressions found; no code fixes required for this verify pass.

---

## Standard path — Cesium ion / `api.cesium.com`

| Assertion | How verified | Status |
| --- | --- | --- |
| Blank ion default token before Viewer | Code: `blankCesiumIon()` sets `Ion.defaultAccessToken = ""` in [`src/geospatial/globe/ion.ts`](../src/geospatial/globe/ion.ts); called from `CesiumGlobe` mount and terrain loader | **PASS** (static) |
| Standard never enables ion | `applyCesiumIonPolicy(false)` returns `{ ionEnabled: false }` and leaves token blank | **PASS** (static) |
| Experimental ion only with stored user token | Token read from `localStorage` key `instasplatter.cesiumIonToken` only when Experimental Mode is on | **PASS** (static) |
| Zero `api.cesium.com` on Standard | DevTools Network filter while Globe open in Standard Mode with no stored token — see manual checklist | **MANUAL** (criteria: zero requests to `api.cesium.com`) |

---

## Phase B — Cesium globe viewer

| # | Item | How verified | Status |
| --- | --- | --- | --- |
| B1 | `cesium` dependency + Vite asset/Workers copy | `package.json` has `cesium`; `vite.config.ts` uses `vite-plugin-cesium`; `npm run build` succeeds | **PASS** |
| B2 | `CesiumGlobe.tsx` thin React wrapper | Module present under `src/geospatial/globe/`; mounted from `GeoViewport` when `viewMode === "globe"` | **PASS** |
| B3 | `GeoViewMode` includes `"globe"` | `types.ts`: `"2d" \| "3d" \| "globe"`; toolbar Globe toggle | **PASS** |
| B4 | Globe toggle in toolbar / viewport | `GeoToolbar.tsx` + `GeoScenarioInspector` view mode buttons | **PASS** (code); screenshot **MANUAL** |
| B5 | Standard terrain = local DEM / ellipsoid (no ion World Terrain) | `terrain.ts` blanks ion; local `CesiumTerrainProvider.fromUrl` or ellipsoid fallback | **PASS** (static) |
| B6 | Experimental ion World Terrain only with token | `terrain.ts` + `applyCesiumIonPolicy(experimental)` | **PASS** (static); live ion **CONDITIONAL** (needs Exp + user token) |
| B7 | Imagery XYZ + attribution (Esri / Carto) | `globe/imagery.ts` + attribution strip in `CesiumGlobe` | **PASS** (code); UI capture **MANUAL** |
| B8 | Flood overlay on Globe | `globe/floodOverlay.ts` + soft/HAND bed via `sample_geo_dem` | **PASS** (code); UI **MANUAL** |
| B9 | Camera/AOI sync MapLibre ↔ Cesium | `globe/cameraSync.ts` present and wired | **PASS** (code); interactive **MANUAL** |
| B10 | Splat stays in ENU (not required on Globe) | Toolbar copy: splat stays in 3D; Globe shows flood+DEM first | **PASS** |

---

## Phase C — Flood realism (DEM-only capable)

| # | Item | How verified | Status |
| --- | --- | --- | --- |
| C1 | Real DEM download / stage / condition | `catalog.rs` USGS/Copernicus/OpenTopo fetchers; `dem.rs` stage+condition tests | **PASS** unit; live USGS fetch **CONDITIONAL** (`usgs_3dep_fetch_smoke_or_skip`) |
| C2 | Soft preview bed from DEM samples | `softSolver.buildBedFromDemSamples`; `PreviewEngine.setDemBed`; store wires `sample_geo_dem` | **PASS** |
| C3 | HAND rapid inundation labelled Live preview | `handInundation.ts` authority label; blended in `preview/engine.ts` | **PASS** |
| C4 | Scientific path when ANUGA ready; Demo badge honest | `GeoViewport` / `CesiumGlobe` authority badge; hydro registry + GPL refuse tests; `manifest_marks_demo_non_authoritative` | **PASS** unit; ANUGA live run **CONDITIONAL** (sidecar installed) |
| C5 | Flood lab scenario defaults | `PLACEHOLDER_SCENARIO` / hydrograph templates in `defaults.ts` | **PASS** |
| C6 | NFHL / HydroSHEDS / gauges / OSM waterways layers | `DEFAULT_GEO_LAYERS` + `GeoLayerTree` FETCHABLE set + MapLibre sources | **PASS** (code); live connector fetch **CONDITIONAL** (network) |
| C7 | DEM-only exports (no splat) | `exports::export_dem_only_workspace_without_splat_or_prior_run` + offline flood bundle tests | **PASS** |
| C8 | Soft + HAND without ANUGA = non-authoritative | Authority badge Demo / Live preview when not calibrated ANUGA | **PASS** (code); UI **MANUAL** |

---

## Phase D — Multi-hazard data stubs

| # | Item | How verified | Status |
| --- | --- | --- | --- |
| D1 | Hazard palette: Flood (simulate) vs Quake/Fire/Landslide/Tsunami stubs | `HazardPalette.tsx` + `HAZARD_STUBS` | **PASS** |
| D2 | Experimental stubs = feeds/STAC links only (no fake physics) | `stubs.ts` blurbs + layer `status: "hook"`; About panel non-claims | **PASS** |
| D3 | Docs / About non-claims | README hazard table + `AboutPanel.tsx` flood-only physics wording | **PASS** |

---

## Path A — Recon + splat → Geo

| Step | Expected | How verified | Status |
| --- | --- | --- | --- |
| A1 | Sample MP4 available | Documented in [`SMOKE-TEST.md`](./SMOKE-TEST.md) (`DJI_…`, `20250820_212300.mp4`, `VID_…`) — **not committed** to repo | **CONDITIONAL** — use local smoke clip when present |
| A2 | Reconstruct to splat checkpoint | Existing recon pipeline + [`E2E-VERIFICATION.md`](./E2E-VERIFICATION.md) / `tools/smoke-local.ps1` | **MANUAL** / GPU lab |
| A3 | Switch suite to Geospatial | TitleBar suite switch | **MANUAL** |
| A4 | Splat gizmo visible in ENU (3D) | `GeoWorkspace3D` survey splat layer | **MANUAL** — capture `01-recon-geo-splat-enu.png` |
| A5 | Flood with registered splat optional | Soft/HAND or Demo still runs with splat layer on | **MANUAL** |

---

## Path B — Flood without splat (DEM-only)

| Step | Expected | How verified | Status |
| --- | --- | --- | --- |
| B1 | New geo project | `geospatial_project_creates_geo_dirs` unit | **PASS** unit; UI **MANUAL** |
| B2 | Draw AOI | MapLibre draw tool / toolbar | **MANUAL** — `03-aoi-draw.png` |
| B3 | Fetch / stage DEM | Catalog connectors + dem condition; or user GeoTIFF | Unit **PASS**; live fetch **CONDITIONAL** |
| B4 | Preview flood idle | Soft/HAND engine with DEM bed | **MANUAL** — `10-flood-idle.png` |
| B5 | Wet / peak | Scrub hydrograph timeline | **MANUAL** — `11-flood-wet.png`, `12-flood-peak.png` |
| B6 | Export depth/hazard + manifest | DEM-only export unit test | **PASS** unit; UI **MANUAL** — `13-flood-export.png` |
| B7 | ANUGA scientific (optional) | When launcher found under `tools/` | **CONDITIONAL** — Scientific badge only if calibrated ANUGA |

---

## UI screenshot matrix

Full capture instructions: [`assets/verify/v0.10/MANUAL-CAPTURE-CHECKLIST.md`](./assets/verify/v0.10/MANUAL-CAPTURE-CHECKLIST.md).

| Asset | Scene | Status |
| --- | --- | --- |
| `01-home-geo.png` | Geospatial home / empty workspace | **MANUAL** — placeholder note present |
| `02-aoi-draw.png` | AOI draw on 2D | **MANUAL** |
| `03-globe.png` | Globe view with DEM/flood | **MANUAL** |
| `04-enu-flood.png` | ENU 3D flood | **MANUAL** |
| `05-2d-overlay.png` | MapLibre flood overlay | **MANUAL** |
| `06-badge-demo.png` | Demo authority badge | **MANUAL** |
| `07-badge-scientific.png` | Scientific badge (ANUGA) | **CONDITIONAL** / MANUAL |
| `08-connector-progress.png` | Catalog fetch progress | **CONDITIONAL** / MANUAL |
| `09-recon-splat-geo.png` | Recon splat in Geo ENU | **MANUAL** / CONDITIONAL on sample MP4 |
| `10-flood-idle.png` … `13-flood-export.png` | DEM-only flood sequence | **MANUAL** |

Headless Tauri WebDriver screenshots were **not** captured in this verify session (GPU/WebView automation not available in agent environment). Automated evidence is unit/build logs + code-static assertions above.

---

## Conditional / open items (honest)

| Item | Classification | Criteria to close |
| --- | --- | --- |
| Live USGS 3DEP / FEMA / NWIS / STAC fetches | CONDITIONAL | Network available; connector returns asset or clear skip message |
| OpenTopography without API key | CONDITIONAL (expected) | Clear error without key — covered by unit `opentopo_without_key_errors_clearly` |
| ANUGA Scientific badge | CONDITIONAL | ANUGA launcher installed + calibrated run |
| Cesium ion World Terrain | CONDITIONAL / Experimental | Exp Mode + user token; never required for Standard |
| GUI screenshots | MANUAL pending | Operator runs checklist; drop PNGs into `docs/assets/verify/v0.10/` |
| Recon+splat E2E | MANUAL / CONDITIONAL | Local smoke MP4 from `SMOKE-TEST.md` + GPU train |
| Installers (NSIS / AppImage / dmg) | Out of Phase E | Owned by **multiplatform-release** todo — not gated here |

**Automatable checklist items:** zero open FAILs.  
**Remaining work before tagging release:** complete MANUAL screenshot captures (recommended) and Phase F release engineering.

---

## Go / no-go for multiplatform-release

See recommendation at end of verify session notes in [`assets/verify/v0.10/VERIFY-SUMMARY.md`](./assets/verify/v0.10/VERIFY-SUMMARY.md).

**Feature / unit gate:** **GO** to start multiplatform-release engineering (CI matrix, version bump, tag) from an automation standpoint.

**Release publish / tag:** **NO-GO until** MANUAL UI captures are filled (or explicitly waived by release owner) and Phase F CI artifacts are green — do not tag `v0.10.0` on unit gates alone per plan Phase E note (“evidence must land in the verify folder before tag”).
