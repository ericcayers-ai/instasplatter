<div align="center">

# InstaSplatter

### Drag in a video or a folder of photos. Watch a 3D Gaussian Splat build itself, live.

**Zero-config by default. Infinitely configurable underneath.**

![Status](https://img.shields.io/badge/status-pre--alpha%20(in%20development)-orange)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)
![GPU](https://img.shields.io/badge/GPU-NVIDIA%20%7C%20AMD%20%7C%20Intel-76B900)
![License](https://img.shields.io/badge/license-TBD-lightgrey)

</div>

---

> **Project status: pre-alpha.** InstaSplatter is in active development and is not yet buildable or installable. This README describes the product being built; the engineering plan lives in **[ROADMAP.md](ROADMAP.md)**. Sections marked _(planned)_ are not implemented yet.

---

## What it is

InstaSplatter turns ordinary captures into photorealistic **3D Gaussian Splats** — no command line, no config files, no PhD in photogrammetry.

Drop an `.mp4` or a folder of images onto the window and the scene starts materializing in the viewport within seconds, sharpening in real time as it trains. Everything — resolution, frame count, iteration budget, quality/speed trade-offs — is **automatically tuned to your PC**. Power users can open Preferences and override every knob; everyone else never has to.

## Why it's different

- ⚡ **Live, not batch.** You watch the splat form and refine in real time — never a frozen progress bar.
- 🎛️ **Auto-optimized for your hardware.** InstaSplatter profiles your GPU/CPU/RAM and picks the optimal engine and settings. NVIDIA gets the CUDA fast path; AMD and Intel get a fully-working portable path.
- 🧹 **Clean output.** Floating blobs, moving people/cars, and per-photo exposure & white-balance drift are actively suppressed by the pipeline — not left for you to scrub out by hand.
- 🖱️ **Truly drag-and-drop.** Zero required configuration to get a result.
- 🔧 **Every setting, exposed.** A deep Preferences panel where every option defaults to **Auto**, with one-click quality and scene-type presets.
- 🔒 **Local & private.** All processing runs on your machine. No cloud dependency, no upload.

## Features

| | |
|---|---|
| **Input** | Video (`.mp4`, `.mov`, …) or an image folder (`.jpg`, `.png`, …) |
| **Smart frame selection** _(planned)_ | Auto blur rejection, duplicate culling, parallax-aware sampling |
| **Camera solving** _(planned)_ | Instant pose-free init + robust global Structure-from-Motion refinement |
| **Live reconstruction** _(planned)_ | Progressive Gaussian training streamed to an interactive viewport |
| **Clean-up built in** _(planned)_ | Floater removal, moving-object masking, appearance/exposure harmonization |
| **Auto-tuning** _(planned)_ | Hardware profiling → optimal engine + preset, with honest ETA |
| **Export** _(planned)_ | `.ply`, `.spz`, `.splat`, and compact PNG-packed formats |
| **Preferences** _(planned)_ | Full settings tree, all defaulting to Auto, with presets & custom profiles |

## How it works

```
Video / Image folder
        │
        ▼
  Frame selection  ──►  blur + duplicate + parallax gating
        │
        ▼
  Camera solving   ──►  instant pose-free init, then global SfM refine
        │
        ▼
  Live training    ──►  Gaussian Splatting streamed to the viewport
        │                (anti-aliased, bounded densification, few floaters)
        ▼
  Clean-up         ──►  floater pruning · moving-object masking · exposure harmonize
        │
        ▼
  Export           ──►  .ply / .spz / .splat / packed
```

Under the hood InstaSplatter uses a **two-engine** design: a CUDA path for maximum quality/speed on NVIDIA GPUs, and a cross-vendor portable path (Vulkan/WebGPU) so AMD and Intel machines work too. The right one is chosen for you automatically. See **[ROADMAP.md](ROADMAP.md)** for the full technical breakdown and the state-of-the-art methods behind each stage.

## Handling messy, real-world captures

Real captures are never perfect. InstaSplatter is built to tolerate them _(all planned)_:

- **Floating blobs** → bounded (MCMC) densification, visibility-based pruning, depth regularization.
- **Aliasing / zoom shimmer** → mip-based anti-aliasing.
- **People, cars, pets walking through** → automatic transient masking + a robust loss that discounts inconsistent pixels.
- **Auto-exposure / white-balance / lighting changes** → per-image appearance modeling + color harmonization.
- **Mixed cameras, focal lengths, resolutions** → per-image intrinsics.

A single **Clean ↔ Detailed** slider lets you bias toward a spotless result or maximum fine detail.

## Requirements

| | Minimum _(portable path)_ | Recommended _(CUDA path)_ |
|---|---|---|
| **OS** | Windows 10/11 (64-bit) | Windows 11 (64-bit) |
| **GPU** | Any Vulkan/DX12-capable AMD/Intel/NVIDIA GPU | NVIDIA RTX (≥ 6 GB VRAM) |
| **RAM** | 16 GB | 32 GB |
| **Disk** | A few GB free for cache & model weights | SSD recommended |

_A capable GPU is strongly recommended. On low-end hardware, InstaSplatter falls back to a lower-quality preview mode and tells you honestly what to expect._

## Installation

> ⚠️ Not yet available — InstaSplatter is pre-alpha. Signed installers will be published here once the MVP is ready. Track progress in **[ROADMAP.md](ROADMAP.md)**.

_Planned:_ download the signed Windows installer, run it, and launch. Heavy model weights are fetched once on first run (with a progress bar and checksum verification); after that the app works fully offline.

## Usage _(planned)_

1. **Launch** InstaSplatter — it detects your hardware and picks a preset on first run.
2. **Drag** a video file or an image folder onto the window.
3. **Watch** the scene form live. Orbit, pan, and zoom while it trains.
4. _(Optional)_ Pick a preset (Draft / Balanced / High / Max) or open **Preferences** to fine-tune anything.
5. **Export** to your format of choice.

That's it. No settings required.

## Roadmap

The full phased plan — architecture, pipeline stages, state-of-the-art methods, milestones, and open decisions — is in **[ROADMAP.md](ROADMAP.md)**.

**Milestones at a glance:**
- **Phase 0** — App skeleton, hardware profiler, drag-and-drop
- **Phase 1** — MVP: video → live splat → `.ply` export (NVIDIA)
- **Phase 2** — Clean results: floater/anti-alias/transient handling
- **Phase 3** — Instant preview + portable (AMD/Intel) engine
- **Phase 4** — In-the-wild robustness (appearance/exposure)
- **Phase 5** — Polish, full Preferences, installer & release
- **Phase 6** _(stretch)_ — Live webcam capture, editing, mesh export

## Built with

Standing on the shoulders of the open Gaussian-Splatting ecosystem — including projects and research such as **gsplat**, **Brush**, **COLMAP/GLOMAP**, feed-forward geometry models, mip-based anti-aliasing, MCMC densification, and modern transient/appearance handling. Full credits and references are in **[ROADMAP.md](ROADMAP.md)**. Component licenses are being audited to ensure a redistributable build.

## Contributing

The project is in early development. Issues, ideas, and capture samples that break the pipeline are welcome — hard real-world cases make the clean-up better.

## License

To be determined. InstaSplatter deliberately favors permissively licensed (Apache/MIT) components so the final app can be freely distributed; see the licensing notes in **[ROADMAP.md](ROADMAP.md)**.

---

<div align="center">
<sub>InstaSplatter — from capture to splat, instantly.</sub>
</div>
