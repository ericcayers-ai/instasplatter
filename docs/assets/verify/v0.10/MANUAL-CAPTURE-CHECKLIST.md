# Manual GUI capture checklist — v0.10

Use a local GPU build (`npm run tauri dev` or installed debug build). Save PNGs into this folder using the filenames below. Replace the matching `*.note.md` when done.

## Preconditions

- [ ] Standard Mode (Experimental off) unless a step says otherwise
- [ ] No Cesium ion token in localStorage (`instasplatter.cesiumIonToken` unset)
- [ ] Optional: DevTools → Network filter `cesium.com` while on Globe (expect **zero** hits on Standard)

## Captures

| # | File | Steps | Pass criteria |
| --- | --- | --- | --- |
| 1 | `01-home-geo.png` | Open app → TitleBar **Geospatial** | Geo home / empty ENU or map chrome visible |
| 2 | `02-aoi-draw.png` | View **2D** → Draw AOI → finish polygon | AOI bound badge or polygon on map |
| 3 | `03-globe.png` | View **Globe** with AOI (DEM optional) | Cesium globe; attribution visible; no ion error toast |
| 4 | `04-enu-flood.png` | View **3D** → run soft/HAND preview | Water visible in ENU workspace |
| 5 | `05-2d-overlay.png` | View **2D** during wet preview | Flood depth/hazard overlay on satellite |
| 6 | `06-badge-demo.png` | Flood with synthetic/missing DEM or Demo mode | Badge reads **Demo** |
| 7 | `07-badge-scientific.png` | *(optional)* ANUGA installed + scientific run | Badge reads **Scientific** |
| 8 | `08-connector-progress.png` | Layer tree → fetch DTM / NFHL / gauges | Progress or ready status on connector |
| 9 | `09-recon-splat-geo.png` | Reconstruct sample MP4 → switch Geo → ENU splat on | Splat gizmo / survey layer visible |
| 10 | `10-flood-idle.png` | DEM-only project, preview at t≈0 | Dry / idle waterline |
| 11 | `11-flood-wet.png` | Scrub mid hydrograph | Partial inundation |
| 12 | `12-flood-peak.png` | Scrub near peak | Peak extent |
| 13 | `13-flood-export.png` | Export flood products | Export dialog or success + files under project |

## Network / Standard ion check

| Check | How | Pass |
| --- | --- | --- |
| No `api.cesium.com` | Globe open, Standard, no token, Network tab 30s | Zero requests |
| Ion blank | Console: Cesium `Ion.defaultAccessToken` empty string | Empty |

## Sample media (recon path)

Clips are **not** in-repo. Use local paths from [`docs/SMOKE-TEST.md`](../../../SMOKE-TEST.md):

- `DJI_20250623163523_0013_D.MP4`
- `20250820_212300.mp4`
- `VID_20220123_205403.MP4`

If none available, mark `09-recon-splat-geo.png` as **skipped — no sample MP4** and keep Path B (DEM-only) as the primary flood E2E.

## Status for this verify session

| Capture | Status |
| --- | --- |
| Automated unit/build logs | Done |
| PNG screenshots 01–13 | **Pending operator** |
| Headless WebDriver | Not run (unavailable in agent env) |
