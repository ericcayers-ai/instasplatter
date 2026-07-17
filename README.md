<div align="center">

# InstaSplatter

### Dual suite: live Gaussian reconstruction and local-first geospatial flood analysis.

**Zero-config by default. Every setting exposed underneath.**

![Status](https://img.shields.io/badge/status-v0.9.2-green)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)
![GPU](https://img.shields.io/badge/GPU-cross--vendor%20wgpu-38B7A6)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
[![Contributing](https://img.shields.io/badge/contributing-guide-informational)](CONTRIBUTING.md)
[![Code of Conduct](https://img.shields.io/badge/code%20of%20conduct-Contributor%20Covenant-blueviolet)](CODE_OF_CONDUCT.md)

</div>

---

> **v0.9.2** revamps the **shell UX** for clearer navigation and installs **Impeccable** and **Bencium Controlled UX Designer** skills app-wide. **v0.9.1** adds an **Experimental Minecraft schematic** export (Sponge v2 `.schem` from finished splats). **v0.9.0** shipped a **worldwide AOI-driven Geospatial suite** with a primary **3D ENU workspace** (Esri World Imagery terrain, depth water, georegistered splat gizmos), optional **2D satellite** map with fixed flood overlays, **live reconstruction stages** (cameras → sparse → dense → splats), grouped Settings, discrete Experimental Mode, and an **About** implementations panel. Flood authority badges stay honest (Live preview / Demo / Scientific). Reconstruction keeps **Standard** vs **Experimental** routing. Research and license notes: **[docs/RESEARCH-STACK.md](docs/RESEARCH-STACK.md)**.

---

## What it is

InstaSplatter turns ordinary captures into photorealistic **3D Gaussian splats**, then — when you switch suites — into a **metrically georeferenced** map with scientific and preview flood runs.

Drop an `.mp4`, a folder of images, or several at once onto the window and the scene materializes in the viewport while it trains. Resolution, frame count, iteration budget, and quality trade-offs are **automatically tuned to your PC**. Power users can open Settings and override every knob; everyone else never has to.

## Product suites

| Suite | Job |
|---|---|
| **Reconstruction** | Capture → cameras → dense evidence → live splat / mesh export |
| **Geospatial** | Draw AOI anywhere → ENU 3D workspace / 2D satellite → flood scenarios → timed exports |

Switch suites from the TitleBar. Geospatial defaults to the **3D workspace**; toggle **2D satellite** to draw or edit an AOI. Projects are versioned (`v2`) and can carry either suite; reconstruction projects remain loadable.

## Dual mode (Standard vs Experimental)

Applies inside both suites where engines are gated:

| | Standard (default) | Experimental (opt-in) |
|---|---|---|
| Reconstruction cameras | Capture-aware commercial chain (VGGT-C, MapAnything, COLMAP) | Profile-matched NC research hypotheses, scored then fused |
| Dense / polish | RoMa v2 ∧ DA3 ∧ MVS; Fixer | Confidence-fuse densifiers; Difix then Fixer |
| Flood | ANUGA Domain.evolve when installed+DEM (+ SWMM network); labelled demo/scaffold otherwise | TRITON / Wflow / GeoClaw external; GPL engines plugin-only |
| Preview | WebGPU/CPU soft solver labelled **non-authoritative** | Same preview path; never promoted without gates |
| License | Commercial-safe defaults | NC research after one-time ack; GPL never bundled |

Experimental is a single TitleBar control (+ discrete banner). Open **About** for Standard vs Experimental stacks, geospatial engines, sidecars, and license/attribution (including Esri World Imagery). NC weights and GPL hydro binaries are never shipped in the installer. See [tools/sidecars/README.md](tools/sidecars/README.md).

## Why it is different

- **Two suites, one shell.** Reconstruction and geospatial share queue, engines, diagnostics, and Standard/Experimental policy.
- **Live, not batch.** Watch cameras → sparse → dense → splat stages in 3D; scrub a hydrograph linked to the flood waterline.
- **AOI anywhere.** Draw a flood domain worldwide; soft-solver and scientific extent rebind off the box (not Wellington-locked).
- **Metric when possible.** EXIF/DJI/GCP → ENU/ECEF; unscaled scenes stay clearly labelled.
- **Science vs graphics.** ANUGA/SWMM for authoritative runs after calibration; live preview stays a badge until within tolerances. Demo/uncalibrated exports never claim authority.
- **One cross-vendor binary.** Brush on wgpu runs on NVIDIA, AMD, and Intel. No CUDA or Python required for the base install.
- **Local and private.** All processing runs on your machine.

## Features

| | |
|---|---|
| **Input** | Video, image folders, batch queue; geospatial telemetry/GCP CSV |
| **Camera solving** | Scored capture-aware routing, COLMAP 4.1 pose priors / BA |
| **Dense init** | Schema v2 sidecars, Sim(3) fusion, gsplat `init.ply` |
| **Live reconstruction** | Sparse/dense clouds + frustums + Brush/gsplat PLY hot-swap in one 3D viewport |
| **Geospatial 3D** | ENU workspace: Esri imagery terrain, depth water, editable splat gizmos |
| **Geospatial 2D** | MapLibre satellite + AOI draw + flood depth overlay |
| **Flood** | ANUGA/SWMM scientific path + WebGPU/CPU preview + demo fallback |
| **Exports** | Splat PLY/SPZ v4; flood COG/GeoPackage/Zarr metadata/manifests; Experimental Minecraft `.schem` |
| **Modes** | Suite switch + Standard / Experimental + About implementations |
| **Resume** | Project bundles with checkpoint resume |

## Requirements

| | Minimum | Recommended |
|---|---|---|
| **OS** | Windows 10/11 (64-bit) | Windows 11 (64-bit) |
| **GPU** | Any Vulkan/DX12-capable GPU | Dedicated GPU with 6+ GB VRAM |
| **RAM** | 16 GB | 32 GB |
| **Disk** | A few GB free for cache | SSD recommended |

## Installation

Installers are published on [GitHub Releases](https://github.com/ericcayers-ai/instasplatter/releases). COLMAP and Brush download automatically on first run (~200 MB). Video input needs FFmpeg on `PATH`:

```
winget install ffmpeg
```

Optional scientific flood: install the ANUGA/SWMM workers under the engines path (see `tools/sidecars/anuga` and `tools/sidecars/swmm`). Without them, geospatial flood runs a **labelled demo** path — not scientifically authoritative.

### Building from source

Prereqs: **Rust** (stable, MSVC), **Node.js 20+**, **FFmpeg** on PATH, Windows 10/11.

```bash
npm install
npm run tauri dev      # development
npm run tauri build    # NSIS installer in src-tauri/target/release/bundle
```

## Usage

1. **Launch** InstaSplatter. It detects your hardware and picks a preset.
2. Pick a **suite**: Reconstruction or Geospatial (TitleBar).
3. **Reconstruction** — drag a video or image folder; watch live stages (cameras / sparse / dense / splat); export PLY/SPZ/mesh. With Experimental Mode on, export a Minecraft `.schem` schematic from the finished splat.
4. **Geospatial** — open/create a geo project, draw an AOI in 2D (or work in the default 3D ENU workspace), run flood scientific or preview, export products with manifests. Check the authority badge (Live preview / Demo / Scientific).
5. _(Optional)_ Settings groups, Experimental Mode (NC ack), or **About** for stacks and attribution.

## Roadmap / release gates

- **v0.8**: suites, georeg, viewport, dual flood engines, exports, experimental adapters.
- **v0.9** (this release): worldwide AOI, Esri imagery, 3D ENU workspace, live recon stages, Settings/About cleanup.
- **v1.0**: large-scene tiling, uncertainty ensembles, full ANUGA validation suite, multi-drone RTK/GCP truth sets, site/city benchmarks, accessibility + installer migration audit.

See also **[ROADMAP-V2.md](ROADMAP-V2.md)** and **[ROADMAP.md](ROADMAP.md)**.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

Apache-2.0. Third-party notices and research sidecar licenses are documented in [docs/RESEARCH-STACK.md](docs/RESEARCH-STACK.md) and [About](src/components/shell/AboutPanel.tsx) in-app.
