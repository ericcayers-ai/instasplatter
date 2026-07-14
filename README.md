<div align="center">

# InstaSplatter

### Drag in a video or a folder of photos. Watch a 3D Gaussian splat build itself, live.

**Zero-config by default. Every setting exposed underneath.**

![Status](https://img.shields.io/badge/status-v0.5.0-green)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)
![GPU](https://img.shields.io/badge/GPU-cross--vendor%20wgpu-38B7A6)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
[![Contributing](https://img.shields.io/badge/contributing-guide-informational)](CONTRIBUTING.md)
[![Code of Conduct](https://img.shields.io/badge/code%20of%20conduct-Contributor%20Covenant-blueviolet)](CODE_OF_CONDUCT.md)

</div>

---

> **v0.5.0** is the dual-mode quality overhaul: **Standard Mode** is VGGT-Commercial-first with RoMa v2 densify (Lichtfeld-style recipe, not the GPL plugin) composed with DAV2/MVS; **Experimental Mode** is a TitleBar master toggle (NC license ack) unlocking Ω → MASt3R → DUSt3R, merge-all densify, Difix+Fixer, and Max floors. Research and license notes live in **[docs/RESEARCH-STACK.md](docs/RESEARCH-STACK.md)**.

---

## What it is

InstaSplatter turns ordinary captures into photorealistic **3D Gaussian splats** with no command line and no config files.

Drop an `.mp4`, a folder of images, or several at once onto the window and the scene materializes in the viewport while it trains. Resolution, frame count, iteration budget, and quality trade-offs are **automatically tuned to your PC**. Power users can open Settings and override every knob; everyone else never has to.

## Dual mode (Standard vs Experimental)

| | Standard (default) | Experimental (opt-in) |
|---|---|---|
| Cameras | VGGT-Commercial → COLMAP | Ω → MASt3R → DUSt3R → VGGT-C → COLMAP |
| Dense init | RoMa v2 ∧ DAV2 ∧ MVS ∧ sparse | Merge **all** densifiers |
| Polish | Fixer | Difix then Fixer |
| License | Commercial-safe defaults | NC research after one-time ack |
| UI | Quiet | Red/violet banner + solver chips |

NC weights are never shipped in the installer. See [tools/sidecars/README.md](tools/sidecars/README.md).

## Why it is different

- **Live, not batch.** You watch the splat form and refine in real time, with smooth interpolation between training checkpoints.
- **Dense by default.** RoMa + COLMAP patch-match MVS (and optional Depth Anything V2 / VGGT sidecars) seed training so needle/floater clouds are far less common.
- **One cross-vendor binary.** Brush on wgpu runs on NVIDIA, AMD, and Intel. No CUDA or Python runtime required for the base install.
- **Batch queue.** Enqueue multiple videos or folders; GPU training stays serialized.
- **Clean output controls.** A Clean vs. detailed slider maps to floater-suppression losses in the trainer.
- **Truly drag-and-drop.** Zero required configuration to get a result.
- **Local and private.** All processing runs on your machine.

## Features

| | |
|---|---|
| **Input** | Video (`.mp4`, `.mov`, …), an image folder, or a batch of either |
| **Smart frame selection** | Adaptive video extraction, mild blur rejection, even temporal subsampling |
| **Camera solving** | VGGT-Commercial primary (Standard), COLMAP fallback, or Experimental NC chain |
| **Dense init** | RoMa v2 ∧ neural ∧ patch-match MVS ∧ sparse → `init.ply` |
| **Live reconstruction** | Brush (wgpu) or gsplat (CUDA) streamed into a WebGL2 splat viewport |
| **Quality defaults** | Progressive resolution, Mip-Splatting filter, raised densify budget |
| **Experimental Mode** | TitleBar toggle + NC license modal + Max floors |
| **Export** | `.ply`, `.splat`, `.spz` splats; optional textured mesh as glb, OBJ, or PLY |
| **Settings** | Full panel, every value defaulting to Auto, quality presets |
| **Resume** | Project bundles with checkpoint resume after interruption |
| **Batch** | Queue, pause, cancel, and per-item progress in the UI |

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

### Building from source

Prereqs: **Rust** (stable, MSVC), **Node.js 20+**, **FFmpeg** on PATH, Windows 10/11.

```bash
npm install
npm run tauri dev      # development
npm run tauri build    # NSIS installer in src-tauri/target/release/bundle
```

## Usage

1. **Launch** InstaSplatter. It detects your hardware and picks a preset.
2. **Drag** a video file or an image folder onto the window.
3. **Watch** the scene form live. Orbit (drag), pan (right-drag), zoom (wheel) while it trains.
4. _(Optional)_ Open **Settings** for presets or any individual setting.
5. **Export** a splat (PLY, Splat, or SPZ) or an optional mesh when complete.

## Roadmap

- **[ROADMAP-V2.md](ROADMAP-V2.md)** — V2 phases 1–5 (current)
- **[ROADMAP.md](ROADMAP.md)** — Long-range product plan

## Contributing

See **[CONTRIBUTING.md](CONTRIBUTING.md)** for build setup, testing, and PR expectations. By participating, you agree to follow the **[Code of Conduct](CODE_OF_CONDUCT.md)**.

## License

InstaSplatter is licensed under the **[Apache License 2.0](LICENSE)** (Copyright 2026 Eric Ayers). The project prefers Apache/MIT redistributable components for default product paths; see licensing notes in the roadmaps and [docs/RESEARCH-STACK.md](docs/RESEARCH-STACK.md).

---

<div align="center">
<sub>InstaSplatter — from capture to splat.</sub>
</div>
