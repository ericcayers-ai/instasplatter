InstaSplatter
Copyright 2026 Eric Ayers

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.

You may obtain a copy of the License at:
    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

---

## Third-party Component Notices

### Brush (ArthurBrussee/brush)
- License: Apache-2.0
- Source: https://github.com/ArthurBrussee/brush

### COLMAP
- License: BSD-3-Clause
- Source: https://github.com/colmap/colmap

### CesiumJS
- License: Apache-2.0
- Source: https://github.com/CesiumGS/cesium

### MapLibre GL JS
- License: BSD-3-Clause
- Source: https://github.com/maplibre/maplibre-gl-js

### Zustand (pmndrs)
- License: MIT
- Source: https://github.com/pmndrs/zustand

### React (Meta Platforms)
- License: MIT
- Source: https://github.com/facebook/react

### Tauri
- License: Apache-2.0 / MIT
- Source: https://github.com/tauri-apps/tauri

### Tailwind CSS
- License: MIT
- Source: https://github.com/tailwindlabs/tailwindcss

### Vite
- License: MIT
- Source: https://github.com/vitejs/vite

### Esri World Imagery
- Licensed for use with Esri products and services
- Attribution required in map views
- Source: https://www.arcgis.com/home/item.html?id=10df2279f9684e4a9f6a7f08febac2a9

### Cesium ion / CesiumJS runtime
- `@cesium/engine`, `@cesium/widgets` — Apache-2.0 (CesiumGS) includes `dompurify`
  (Apache-2.0 OR MPL-2.0, used under Apache-2.0) and `pako`, `tslib`, `mersenne-twister`
  (MIT). Notices are carried in `node_modules/**/LICENSE.md` at build time; the assembled
  bundle (Apache-2.0 application code + these MIT/Apache dependencies) carries no
  file-level copyleft obligation.

### MapLibre GL JS runtime dependencies
- `maplibre-gl` (BSD-3-Clause) bundles `@maplibre/mlt`, `geojson-vt`, `supercluster`,
  `vt-pbf`, `earcut`, `quickselect` and `gl-matrix` — ISC / MIT / BSD-3-Clause.

### MPL-2.0 components compiled into the Windows binary

The following crates are pulled in by `tauri`/`dirs` and ship inside `instasplatter.exe`.
They are **unmodified**. MPL-2.0 §3.2 permits combining them with Apache-2.0 code; the
obligation is attribution plus source availability, both satisfied by the crates' own
published sources.

| Crate | Version | License | Pulled in by |
|---|---|---|---|
| `cssparser` | 0.36.x | MPL-2.0 | `tauri-utils` (via `dom_query`) |
| `cssparser-macros` | 0.6.x | MPL-2.0 | `cssparser` |
| `dtoa-short` | 0.3.x | MPL-2.0 | `cssparser` |
| `selectors` | 0.36.x | MPL-2.0 | `tauri-utils` (via `dom_query`) |
| `option-ext` | 0.2.x | MPL-2.0 | `dirs-sys` → `dirs` |

Sources: https://crates.io/crates/cssparser , https://crates.io/crates/selectors ,
https://crates.io/crates/cssparser-macros , https://crates.io/crates/dtoa-short ,
https://crates.io/crates/option-ext

### Development-only tools (not distributed)

`lightningcss` (MPL-2.0, via Tailwind v4/Vite) and `playwright` (Apache-2.0) are build and
verification tooling. `grep -ril lightningcss dist` returns nothing, so no MPL-2.0 file is
part of the shipped bundle.

### Editor skill content

`.cursor/skills/impeccable` is the Impeccable design skill (Apache-2.0, stated in its own
`SKILL.md` front matter, © the Impeccable authors). It is developer tooling and is not part
of the application build. `.cursor/skills/bencium-controlled-ux-designer` was removed from
the repository because the upstream marketplace copy ships no license file in-tree.

### OpenStreetMap
- License: ODbL
- Attribution: OpenStreetMap contributors
- Source: https://www.openstreetmap.org/copyright

## Research Sidecar Licenses

Sidecars shipped as installable adapters. None are bundled in the base installer.

| Sidecar | License | Compliance |
|---------|---------|------------|
| ANUGA | Apache-2.0 | External, user-installed; never bundled. Upstream `LICENSE.txt` (GeoscienceAustralia/anuga_core) is Apache-2.0 — an earlier note in this file said GPL-2.0 and was wrong |
| SWMM | MIT-like (EPA) | External; install-sidecar command |
| VGGT-1B-Commercial | Application-gated | Opt-in; ACCEPTED marker required |
| VGGT-Research (NC) | CC BY-NC 4.0 | Experimental Mode only; NC ack required |
| LightGlue | Apache-2.0 | Standard installable adapter |
| RoMa v2 | MIT | Standard installable adapter |
| Depth Anything V3 | Apache-2.0 | Standard installable adapter |
| Depth Anything V2 | Apache-2.0 | Standard installable adapter |
| MapAnything | Apache-2.0 | Standard installable adapter |
| gsplat | Apache-2.0 | Standard installable adapter |
| Fixer | NVIDIA Open Model License | Standard installable adapter |

## Data Sources

| Source | License/Terms | Attribution |
|--------|---------------|-------------|
| USGS 3DEP | Public Domain | U.S. Geological Survey |
| Copernicus GLO-30 | Free and open | Contains modified Copernicus data |
| HydroSHEDS | Free for non-commercial; commercial contact WWF | World Wildlife Fund |
| OpenTopography | Various per-dataset | OpenTopography |
| FEMA NFHL | Public Domain | Federal Emergency Management Agency |
| USGS NWIS | Public Domain | U.S. Geological Survey |
| OSM Waterways | ODbL | OpenStreetMap contributors |
| Earth Search STAC | Various per-collection | Element 84 / NASA |
