# UI workflow map and coherence notes — 2026-11-03

Written by reading the shell (`src/App.tsx`, `src/components/shell/*`, `src/geospatial/*`) rather than
from memory, and applying `frontend-superskill` §5 Workflow D (3-D/data-heavy surfaces) to the new
surfaces. This is an audit of the workflow, not a redesign.

## Structure as it stands

```
TitleBar          suite switch (Reconstruction | Geospatial) · story controls · About
IconRail          Capture · Scene · Settings · Log   (each sets a ShellFocus + opens a dock)
LeftDock          suite-dependent sources / layers / capture
Main             Reconstruction: DropZone → Processing → Viewport
                 Geospatial:     GeoViewport (3D ENU | 2D MapLibre | Cesium Globe)
RightDock        Geospatial: QuickStats · Scenario inspector · **Localise** · Log
                 Reconstruction: Visualization · Stats · … · Log
StatusBar         stage, fps, splat count, worker state
```

## Reconstruction journey
`DropZone` (drop files) → `Processing` (live stage list, frustums, sparse/dense cloud, splat viewport)
→ result actions (mesh / SPZ / schematic export). State lives in the zustand store (`screen`, `stages`,
`splatCount`, `resultPath`, `jobError`), which `StatusBar` and `RightDock → ReconStats` both read — one
source of truth.

## Geospatial journey
`GeoToolbar` (view mode: 3D/2D/Globe · basemap · tools: Pan/AOI/Inspect · AOI clear) → draw AOI →
DEM prepare (catalog or GeoTIFF) → scenario inspector (forcing, hydrograph) → preview/flood badge →
exports. `PreviewBadge` keeps the "Live preview ≠ authoritative" distinction visible at the surface.

## What changed in this pass
- **Localise** dock section (`src/geospatial/VpsPanel.tsx`): load submap bundle → load query bundle →
  Localise → pose, inliers, residual, σ → Save fix (IPC → `<workspace>/vps/localizations/`). Plus a
  labelled synthetic self-test so a user can distinguish "my bundle is wrong" from "the app is broken".
- Receipts: `docs/assets/verify/vps-panel/{1440,768,390}.png` with a mechanical zero-overflow check at
  all three widths, and a DOM assertion that the result grid renders with the expected rows.

## Coherence gaps (ranked)

1. **`Localise` is not on the ShellFocus map.** The icon rail drives focus (`capture`/`scene`/
   `settings`/`log`); the new section is reachable only by scrolling the right dock. Fix: add a
   `localise` focus and a rail affordance, or fold it under Settings for the geospatial suite.
2. **Flood provenance is computed and not shown.** `runSplatToMeshFloodSimulation` returns coverage,
   nodata count, depression-fill report, connectivity and mass error; no component renders them. The
   honest landing spot is next to `PreviewBadge`.
3. **Two "simulation" vocabularies.** ANUGA (`scaffold` mode, external) and the in-app solver both
   produce water. They need one shared badge vocabulary, not two independent labels.
4. **The globe cannot show a capture.** Splat → tileset export does not exist, so a user who georeferences
   a site cannot see it on the globe (documented in the audit, not a UI bug).
5. **No screenshot of the whole app could be taken in this environment.** The shell calls Tauri IPC on
   `init()`; a plain browser renders "Initialization failed". A full-shell visual receipt needs either
   the Tauri binary on screen or an IPC stub for the dev server. The panel receipt above is real; a
   shell-wide receipt is not.

## Rules applied from `frontend-superskill` §5
- **D1 counters before screenshots** — the renderer bench reports candidates/drawn/coverage; the panel
  receipt asserts DOM rows, not just pixels.
- **D3 adapter named with every number** — `docs/PERF.md` starts with the SwiftShader string.
- **D4 precision is an interface decision** — the 1 mm-at-32.8-km threshold is documented beside the
  code that enforces camera-relative rendering.
- **D5 labelled approximation** — the self-test says "synthetic self-test (not a real localisation)" in
  the result row itself, and σ carries "first-order bound, not a calibrated covariance".
- **D7 import flows are UI** — loaded-state text, field-naming errors, labelled self-test escape hatch.
- **D8 gate** — `npm run smoke:static`, `npm run bench`, `npm run bench:vps-ui`, plus `tsc`/`lint`/`build`.
