<div align="center">

# InstaSplatter

### Drag in a video or a folder of photos. Watch a 3D Gaussian splat build itself, live.

**Zero-config by default. Every setting exposed underneath.**

![Status](https://img.shields.io/badge/status-v0.2.0-green)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)
![GPU](https://img.shields.io/badge/GPU-cross--vendor%20wgpu-38B7A6)
![License](https://img.shields.io/badge/license-TBD-lightgrey)

</div>

---

> **v0.2.0** ships the V2 roadmap phases the codebase can own: professional reconstruction UI, live camera tracking (opt-in), splat and mesh export, checkpoint resume, and exhaustive error handling. Phase status and deferred items are in **[ROADMAP-V2.md](ROADMAP-V2.md)**. The original long-range plan is in **[ROADMAP.md](ROADMAP.md)**.

---

## What it is

InstaSplatter turns ordinary captures into photorealistic **3D Gaussian splats** with no command line and no config files.

Drop an `.mp4` or a folder of images onto the window and the scene materializes in the viewport while it trains. Resolution, frame count, iteration budget, and quality trade-offs are **automatically tuned to your PC**. Power users can open Settings and override every knob; everyone else never has to.

## Why it is different

- **Live, not batch.** You watch the splat form and refine in real time.
- **One cross-vendor binary.** Brush on wgpu runs on NVIDIA, AMD, and Intel. No CUDA or Python runtime required.
- **Clean output controls.** A Clean vs. detailed slider maps to floater-suppression losses in the trainer.
- **Truly drag-and-drop.** Zero required configuration to get a result.
- **Local and private.** All processing runs on your machine.

## Features

| | |
|---|---|
| **Input** | Video (`.mp4`, `.mov`, …) or an image folder (`.jpg`, `.png`, …) |
| **Smart frame selection** | Adaptive video extraction, blur rejection, even temporal subsampling |
| **Camera solving** | COLMAP 4.1 SfM (default) or opt-in live incremental tracking with COLMAP fallback |
| **Live reconstruction** | Brush (wgpu) training streamed into a WebGL2 splat viewport |
| **Clean-up** | Opacity/scale regularization and a Clean vs. detailed strictness slider |
| **Auto-tuning** | Hardware profiling → auto preset, live ETA |
| **Export** | `.ply`, `.splat`, `.spz` splats; optional textured mesh as glb, OBJ, or PLY |
| **Settings** | Full panel, every value defaulting to Auto, quality presets |
| **Resume** | Project bundles with checkpoint resume after interruption |

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

## License

To be determined. InstaSplatter favors permissively licensed (Apache/MIT) components. See licensing notes in the roadmaps.

---

<div align="center">
<sub>InstaSplatter — from capture to splat.</sub>
</div>
