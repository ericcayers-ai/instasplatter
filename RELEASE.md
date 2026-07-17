# Running InstaSplatter

## Install and run

Download the latest installer from [GitHub Releases](https://github.com/ericcayers-ai/instasplatter/releases), or build locally.

**Windows (primary):**

```
src-tauri/target/release/bundle/nsis/InstaSplatter_0.10.0_x64-setup.exe
```

**Linux (best-effort):** AppImage and `.deb` under `src-tauri/target/release/bundle/{appimage,deb}/`.

**macOS (best-effort):** `.dmg` under `src-tauri/target/release/bundle/dmg/`. For v0.10, CI produces an **unsigned / ad-hoc** dmg (no Apple notarization secrets). Gatekeeper may require right-click → Open on first launch.

Run the Windows installer, then launch InstaSplatter from the Start menu. To skip the installer:

```
src-tauri/target/release/instasplatter.exe
```

Drop a video file or a folder of images onto the window. On the first run the app downloads COLMAP and Brush, checks each against a pinned SHA-256, and installs them under your app data directory. Both downloads together are about 200 MB. Video input also needs FFmpeg on `PATH`:

```
winget install ffmpeg
```

Nothing else is required for reconstruction. There is no CUDA and no Python runtime; the trainer runs on any GPU that supports Vulkan, Metal or DirectX 12. Scientific flood (ANUGA/SWMM) is optional and not bundled in the installer.

## Multi-platform CI builds

Installers for Windows / Linux / macOS are produced by [`.github/workflows/release.yml`](.github/workflows/release.yml):

| Trigger | What happens |
|---|---|
| **Actions → release → Run workflow** (`workflow_dispatch`) | Matrix build + upload artifacts (no GitHub Release) |
| **Push tag** `v*` (e.g. `v0.10.0`) | Same builds + attach installers to a **draft** GitHub Release |

Local CI on one OS does **not** produce the other platforms’ installers — wait for the matrix job (or run `npm run tauri build` on each host).

**Publish gate:** do **not** convert the draft to a published release (and do not rely on the tag alone) until MANUAL GUI screenshots in [`docs/assets/verify/v0.10/`](docs/assets/verify/v0.10/) are filled or explicitly waived — see [`VERIFY-SUMMARY.md`](docs/assets/verify/v0.10/VERIFY-SUMMARY.md) and [`docs/E2E-GEO-V010.md`](docs/E2E-GEO-V010.md).

## Rebuilding

```
npm install
npm run tauri build
```

Bundles land under `src-tauri/target/release/bundle/` (`nsis`, `appimage`, `deb`, and/or `dmg` depending on host OS). For a development run with hot reload:

```
npm run tauri dev
```

### Linux build dependencies

On Debian/Ubuntu-class hosts (and GitHub `ubuntu-latest`), install WebKitGTK 4.1 and related packages before `tauri build`:

```
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  libssl-dev \
  libgtk-3-dev \
  xdg-utils
```

If AppImage bundling fails on newer glibc with a `linuxdeploy` / `.relr.dyn` strip error, set `NO_STRIP=true` for the build (the release workflow already does this on Linux).

### macOS signing (v0.10)

Notarization and Developer ID signing secrets are **not** required for v0.10. CI ships an unsigned / ad-hoc `.dmg`. That is intentional for this release; signed/notarized builds can come later when secrets are available.

## Tests

```
cd src-tauri && cargo test
npx tsc --noEmit
node --experimental-strip-types src/splat/camera.ts
```

Rust unit tests cover linear algebra, COLMAP and PLY readers and writers, the SPZ encoder, the incremental camera solver, the mesh extractor, geospatial catalog/DEM/hydro paths, and the Experimental Minecraft schematic (Sponge v2) writer. The camera-control maths in `src/splat/camera.ts` has its own numeric checks.

## What is new in v0.10.0

**Geospatial + Cesium + disaster overlays.** CesiumJS **Globe** view mode beside MapLibre 2D and ENU 3D; real DEM staging/conditioning for AOI; catalog connectors (USGS 3DEP, Copernicus, HydroSHEDS, NFHL, gauges, STAC); stronger flood realism (DEM-backed soft preview / HAND path) with honest Live preview / Demo / Scientific badges; Experimental multi-hazard **data** stubs only (no fake physics). Multi-platform installer CI: Windows NSIS (primary) + Linux AppImage/deb + macOS dmg (best-effort, unsigned).

Verification matrix and evidence: [`docs/E2E-GEO-V010.md`](docs/E2E-GEO-V010.md), [`docs/assets/verify/v0.10/`](docs/assets/verify/v0.10/). Design: [`docs/superpowers/specs/2026-07-17-geo-disaster-cesium-design.md`](docs/superpowers/specs/2026-07-17-geo-disaster-cesium-design.md).

## What is new in v0.9.2

**Shell QOL + design skills.** Clearer TitleBar, StatusBar, panels, and drop/processing surfaces; design tokens and copy tightened for scanability. Ships Impeccable and Bencium Controlled UX Designer as app-wide Cursor skills (`.agents` / `.cursor`) plus PRODUCT/DESIGN notes.

## What is new in v0.9.1

**Experimental Minecraft schematic.** With Experimental Mode on and a finished reconstruction, **Export schematic** writes a Sponge Schematic v2 `.schem` (Gzip NBT) that WorldEdit-class tools can paste. The splat is voxelized with a robust AABB, opacity filter, and vanilla concrete colour palette. Standard Mode is unchanged.

## What is new in v0.9.0

Worldwide AOI geospatial suite, 3D ENU workspace, live reconstruction stages, Settings/About cleanup. See the [v0.9.0 release](https://github.com/ericcayers-ai/instasplatter/releases/tag/v0.9.0).

## Settings that default to off

- **Live camera tracking.** Camera intrinsics are guessed from the image size and are not refined, so poses carry whatever error that guess introduces. COLMAP remains the accurate path.
- **Research sidecars.** NC weights (VGGT-Ω, Difix research). Off until you accept the license risk.
- **Minecraft schematic export.** Experimental Mode only.

Progressive resolution, Mip-Splatting, dense init, neural densifiers (when installed), and post polish (when Fixer is installed) default **ON**.

## Manual end-to-end gate

Before trusting a release on your machine, run one full reconstruction (video or image folder) with engines installed. This is the acceptance check for ROADMAP-V2 item 1.1. With Experimental Mode on, also try **Export schematic** and confirm a `.schem` opens in your preferred paste tool. For geospatial v0.10, complete or waive the MANUAL GUI checklist under [`docs/assets/verify/v0.10/MANUAL-CAPTURE-CHECKLIST.md`](docs/assets/verify/v0.10/MANUAL-CAPTURE-CHECKLIST.md) before publishing.
