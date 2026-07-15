<div align="center">

# InstaSplatter

### Dual suite: live Gaussian reconstruction and local-first geospatial flood analysis.

**Zero-config by default. Every setting exposed underneath.**

![Status](https://img.shields.io/badge/status-v0.8.1-green)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)
![GPU](https://img.shields.io/badge/GPU-cross--vendor%20wgpu-38B7A6)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
[![Contributing](https://img.shields.io/badge/contributing-guide-informational)](CONTRIBUTING.md)
[![Code of Conduct](https://img.shields.io/badge/code%20of%20conduct-Contributor%20Covenant-blueviolet)](CODE_OF_CONDUCT.md)

</div>

---

> **v0.8.1** ships the **Reconstruction** and **Geospatial** suites side by side. Reconstruction keeps **Standard** (commercially redistributable) vs **Experimental** (NC ack) routing with scored pose fusion, sidecar schema v2, and SPZ v4. Standard densifiers (RoMa, DA3/DAV2, MapAnything, LightGlue, VGGT-Commercial, Fixer) are **installable adapters** that fail clearly without weights — not pretend-ready stubs. Geospatial adds MapLibre viewport, drone georegistration (ENU/ECEF), dual flood engines (ANUGA Domain.evolve when DEM/mesh allow, else labelled demo/scaffold — never authoritative until evolve + calibration), offline exports with explicit authority flags, and experimental hydro adapters behind promotion gates. Research and license notes: **[docs/RESEARCH-STACK.md](docs/RESEARCH-STACK.md)**.

---

## What it is

InstaSplatter turns ordinary captures into photorealistic **3D Gaussian splats**, then — when you switch suites — into a **metrically georeferenced** map with scientific and preview flood runs.

Drop an `.mp4`, a folder of images, or several at once onto the window and the scene materializes in the viewport while it trains. Resolution, frame count, iteration budget, and quality trade-offs are **automatically tuned to your PC**. Power users can open Settings and override every knob; everyone else never has to.

## Product suites

| Suite | Job |
|---|---|
| **Reconstruction** | Capture → cameras → dense evidence → live splat / mesh export |
| **Geospatial** | Georegistered scene → DEM/layers → flood scenarios → timed exports |

Switch suites from the shell. Projects are versioned (`v2`) and can carry either suite; reconstruction projects remain loadable.

## Dual mode (Standard vs Experimental)

Applies inside both suites where engines are gated:

| | Standard (default) | Experimental (opt-in) |
|---|---|---|
| Reconstruction cameras | Capture-aware commercial chain (VGGT-C, MapAnything, COLMAP) | Profile-matched NC research hypotheses, scored then fused |
| Dense / polish | RoMa v2 ∧ DA3 ∧ MVS; Fixer | Confidence-fuse densifiers; Difix then Fixer |
| Flood | ANUGA Domain.evolve when installed+DEM (+ SWMM network); labelled demo/scaffold otherwise | TRITON / Wflow / GeoClaw external; GPL engines plugin-only |
| Preview | WebGPU/CPU soft solver labelled **non-authoritative** | Same preview path; never promoted without gates |
| License | Commercial-safe defaults | NC research after one-time ack; GPL never bundled |

NC weights and GPL hydro binaries are never shipped in the installer. See [tools/sidecars/README.md](tools/sidecars/README.md).

## Why it is different

- **Two suites, one shell.** Reconstruction and geospatial share queue, engines, diagnostics, and Standard/Experimental policy.
- **Live, not batch.** Watch the splat form while training; scrub a hydrograph linked to the flood waterline.
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
| **Live reconstruction** | Brush (wgpu) or gsplat (CUDA) → WebGL2 splat viewport |
| **Geospatial viewport** | MapLibre layers, scenario inspector, hydrograph timeline |
| **Flood** | ANUGA/SWMM scientific path + WebGPU/CPU preview + demo fallback |
| **Exports** | Splat PLY/SPZ v4; flood COG/GeoPackage/Zarr metadata/manifests |
| **Modes** | Suite switch + Standard / Experimental toggles |
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
2. Pick a **suite**: Reconstruction or Geospatial.
3. **Reconstruction** — drag a video or image folder; watch the splat form; export PLY/SPZ/mesh.
4. **Geospatial** — open/create a geo project, ingest telemetry/GCP as needed, run flood scientific or preview, export products with manifests.
5. _(Optional)_ Settings for presets, Experimental Mode (NC ack), or individual knobs.

## Roadmap / release gates

- **v0.8** (this release): suites, georeg, viewport, dual flood engines, exports, experimental adapters.
- **v0.9–v1.0**: large-scene tiling, uncertainty ensembles, full ANUGA validation suite, multi-drone RTK/GCP truth sets, site/city benchmarks, accessibility + installer migration audit.

See also **[ROADMAP-V2.md](ROADMAP-V2.md)** and **[ROADMAP.md](ROADMAP.md)**.

## Contributing

See **[CONTRIBUTING.md](CONTRIBUTING.md)** for suite overview, build setup, testing (`cargo test` / `tsc`), and PR expectations. By participating, you agree to follow the **[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)**.
Report bugs and features with the [GitHub issue forms](https://github.com/ericcayers-ai/instasplatter/issues/new/choose).

## License

InstaSplatter is licensed under the **[Apache License 2.0](LICENSE)** (Copyright 2026 Eric Ayers). The project prefers Apache/MIT redistributable components for default product paths; see licensing notes in the roadmaps and [docs/RESEARCH-STACK.md](docs/RESEARCH-STACK.md).

---

<div align="center">
<sub>InstaSplatter — from capture to splat to flood scene.</sub>
</div>
