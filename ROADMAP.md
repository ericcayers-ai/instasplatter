# InstaSplatter — Implementation Roadmap

> **Status (2026-07-03): v0.1 shipped.** Phase 0 and Phase 1 are complete and verified end-to-end on Windows 11 + RTX 4060 (video → frames → COLMAP 4.1 SfM → Brush training → live WebGL2 viewport → `.ply` export). Delivered beyond Phase 1: full Auto-defaulting Preferences panel, hardware profiler + auto presets, blur gating, strictness slider, first-run engine download, `.ply` live hot-swap viewport. Implementation choices vs. plan: portable Brush (wgpu) engine is the primary trainer for all vendors in v0.1 (the CUDA/gsplat path from §2 is deferred); SfM uses COLMAP binary (GLOMAP/VGGT instant-init deferred). Next up: Phase 2 cleanliness stack, then Phase 3 instant init.

> **Audience:** Claude Fable 5 (implementing agent).
> **Goal:** Ship a sleek, official-looking Windows desktop app where a user drags in an **image folder** or a **video** and watches a **Gaussian‑Splat reconstruction build live**, fully auto‑tuned to their hardware, with a deep preferences panel for power users.
> **Design ethos:** Zero-config by default, infinitely configurable underneath. "It just works" for 95% of users; "it exposes everything" for the other 5%.

---

## 0. TL;DR / North Star

**One sentence:** Drag a video or photo folder onto InstaSplatter → within seconds a 3D scene starts materializing in the viewport and keeps sharpening → export a clean `.ply`/`.spz` splat, with floaters and moving-object ghosts handled automatically.

**Three non-negotiables:**
1. **Automatic everything.** The app profiles the machine and picks the optimal engine, resolution, iteration budget, and densification strategy. The user never *has* to touch a setting.
2. **Live, not batch.** Reconstruction is progressive — the user sees the splat appear and refine in real time, with a live quality/ETA readout. Never a frozen "processing…" spinner.
3. **Clean output.** Floaters, background blobs, transient/moving objects, and per-image exposure/white-balance drift are actively suppressed by the pipeline, not left for the user to clean up.

---

## 1. Product Definition

### 1.1 Target users
- **Prosumers / 3D hobbyists** capturing scenes with a phone or camera.
- **VFX / game / archviz artists** who want fast previz splats.
- **Researchers / educators** who want a no-CLI reference pipeline.
- **Real-estate / e-commerce / heritage** capture operators.

### 1.2 Core user stories
- [ ] As a user, I drag a folder of JPEGs onto the window and a splat starts forming without me configuring anything.
- [ ] As a user, I drag in an `.mp4` and the app extracts the best frames automatically.
- [ ] As a user, I watch the reconstruction sharpen live and can orbit the scene while it trains.
- [ ] As a user, I get a clean result even though 3 photos had a person walking through and the lighting changed.
- [ ] As a power user, I open Preferences and override the SfM backend, densification cap, and export format.
- [ ] As a user, I export to `.ply` / `.spz` / `.splat` and open it anywhere.
- [ ] As a user on a laptop iGPU, it still runs (slower, lower preset) and tells me honestly what to expect.

### 1.3 Explicit non-goals (v1)
- Not a full splat *editor* (integrate/hand-off to SuperSplat for surgical cleanup instead — see §11.4).
- Not a live-webcam SLAM capture tool (that's a v2 stretch — see §14, Phase 6).
- Not a mesh/photogrammetry tool (no textured-mesh export in v1; optional later).
- Not cross-platform in v1 (Windows-first; architecture must not *preclude* macOS/Linux).

---

## 2. High-Level Architecture

```
┌───────────────────────────────────────────────────────────────┐
│  UI Shell (Tauri 2 + React/Svelte, WebView2)                    │
│  - Drag&drop, live 3D viewport (WebGL/WebGPU splat renderer)    │
│  - Preferences panel, progress/telemetry HUD                    │
└───────────────▲───────────────────────────┬───────────────────┘
                │ IPC (typed events/commands) │ shared-memory / file
┌───────────────┴───────────────────────────▼───────────────────┐
│  Orchestrator (Rust core)                                       │
│  - Job graph, hardware profiler, engine selection, presets      │
│  - Streams progress + intermediate splat chunks to UI           │
└───────┬───────────────────────────────────────────┬───────────┘
        │ spawn / manage                              │ spawn / manage
┌───────▼─────────────────┐            ┌──────────────▼────────────┐
│  Engine A: CUDA path     │            │  Engine B: Portable path   │
│  (NVIDIA, max quality)   │            │  (AMD/Intel/fallback)      │
│  Python sidecar:         │            │  Rust/wgpu (Brush-style):  │
│  gsplat + COLMAP/GLOMAP  │            │  cross-vendor 3DGS train   │
│  + VGGT/MASt3R + SAM2    │            │  + pose-free init          │
└──────────────────────────┘            └───────────────────────────┘
```

**Why this split:**
- **Tauri 2** → sleek modern UI, tiny footprint, native Windows WebView2, Rust backend for real integration (not Electron bloat). Fluent-inspired theming gives the "official Windows app" feel.
- **Rust orchestrator** → robust process management, hardware detection, and low-latency streaming of partial results to the viewport.
- **Two engines** → the honest answer to "auto-optimize for any PC." NVIDIA users get the CUDA SOTA stack (gsplat); everyone else gets a genuinely working portable path (wgpu). Users never see this; the profiler picks it.

> **Decision to confirm with product owner:** Tauri 2 (recommended) vs **WinUI 3 / .NET** (most "native Windows official" but heavier, C#, harder ML integration). Tauri wins on sleekness + Rust cohesion; WinUI wins on OS-native polish. Defaulting to Tauri. See §12 open questions.

---

## 3. The Reconstruction Pipeline (the heart)

Each stage lists the **SOTA choice**, **fallbacks**, and **why**. The orchestrator runs these as a streamed graph so the viewport updates continuously.

### Stage 0 — Ingestion
- [ ] **Image folder:** enumerate, validate (decode, EXIF), sort. Read focal length / sensor / exposure / ISO / white-balance from EXIF to seed intrinsics and appearance handling.
- [ ] **Video:** demux + decode (bundled **FFmpeg**). Adaptive frame extraction — *not* fixed FPS.
- [ ] Detect input scale (indoor object vs. room vs. large outdoor) to seed presets.

### Stage 1 — Frame selection & quality gating *(critical for clean results)*
- [ ] **Blur/sharpness filter:** variance-of-Laplacian + no-reference IQA; drop the blurriest frames.
- [ ] **Redundancy control:** keep frames with sufficient parallax/baseline; drop near-duplicates (optical-flow / feature-overlap based). Target an even angular distribution, not temporal.
- [ ] **Exposure/rolling-shutter flags:** tag frames with heavy motion blur or extreme exposure for down-weighting later.
- [ ] Adaptive target frame count based on §6 hardware profile (e.g., 80–120 on an iGPU, 300–600 on a 4090).

### Stage 2 — Camera pose / Structure-from-Motion
Two selectable tracks; profiler picks by scene size + hardware + "speed vs. quality" preset.

| Track | Method | When | Notes |
|---|---|---|---|
| **Instant (default for "live")** | **VGGT** (feed-forward geometry) or **MASt3R-SfM** / **InstantSplat** init | Fast preview, unordered/sparse, pose-free | Poses + dense point cloud in seconds → live splat starts *immediately* |
| **Robust/quality** | **GLOMAP** (global SfM, much faster than incremental COLMAP) or **COLMAP** | Final refine, hard scenes | Bundle-adjusted metric poses |
| **Matching front-end** | **LightGlue** (+SuperPoint/ALIKED/DISK) or **RoMa** dense; **MASt3R** matches for wide baselines | — | Robust to viewpoint/appearance change |

- [ ] Implement **VGGT/MASt3R fast init** to get a live splat on screen within seconds, then **refine poses with GLOMAP in the background** and hot-swap when ready ("progressive pose refinement").
- [ ] Robust intrinsics: use EXIF focal as prior; solve for shared/​per-camera intrinsics; support unknown focal.
- [ ] Handle **mixed cameras / mixed focal lengths / mixed resolutions** in one dataset (per-image intrinsics).

### Stage 3 — Initialization
- [ ] Seed Gaussians from the SfM/dense point cloud (VGGT/MASt3R dense points give a *much* denser, floater-resistant start than sparse COLMAP).
- [ ] Optional **monocular depth prior** (Depth Anything V2 / metric depth) to regularize geometry and kill background floaters early.
- [ ] Estimate scene bounds, up-vector, and a sensible initial camera for the viewport.

### Stage 4 — Gaussian training (live)
- **Core engine:** **gsplat** (Nerfstudio, CUDA) on NVIDIA; **Brush**-style Rust/wgpu on other GPUs.
- **Densification strategy (this is where floaters live or die):**
  - [ ] **MCMC densification** (*3DGS as Markov Chain Monte Carlo*) as the default — gives a hard cap on Gaussian count and markedly fewer floaters than vanilla adaptive-density control.
  - [ ] **AbsGrad / Taming-3DGS steerable densification** as an alternative preset for speed.
  - [ ] Opacity/scale regularization + periodic opacity reset; prune low-opacity and oversized Gaussians.
- **Anti-aliasing / quality:**
  - [ ] **Mip-Splatting** (3D smoothing filter + 2D mip filter) to remove aliasing and the "spiky" artifacts at varying zoom/resolution.
  - [ ] Optional **2DGS / GOF** surface mode for flatter, cleaner geometry (great for objects/rooms; fewer volumetric blobs).
- **Live streaming:**
  - [ ] Emit the current Gaussian set to the viewport every N iterations (delta updates / chunked) so the user watches it sharpen.
  - [ ] Progressive resolution: train at low res first (fast convergence, instant feedback), then upscale.

### Stage 5 — Handling inconsistencies *(dedicated — see full detail in §5)*
- [ ] Transient/moving-object suppression (people, cars, pets).
- [ ] Per-image appearance/exposure/white-balance compensation.
- [ ] Robust loss / outlier down-weighting.

### Stage 6 — Post-process & cleanup
- [ ] **Floater removal pass:** opacity thresholding + statistical outlier removal + visibility-based pruning (remove Gaussians seen by too few cameras / outside the convex hull of observations).
- [ ] Optional **auto-crop** to the region of interest (foreground vs. unbounded background) using the observed frustum overlap.
- [ ] **Sky/background handling:** separate far-field/sky Gaussians or an environment sphere so the background doesn't spray floaters into the scene.

### Stage 7 — Export
- [ ] Formats: **`.ply`** (standard 3DGS), **`.spz`** (Niantic compressed, small), **`.splat`/`.ksplat`** (web viewers), **SOG / Self-Organizing Gaussians** (PNG-packed, tiny).
- [ ] Compression presets (lossless ↔ aggressive) with size/quality readout.
- [ ] Metadata sidecar: capture settings, pose file, license/attribution, InstaSplatter version.
- [ ] Optional mesh/point-cloud export (v2).

---

## 4. Auto-Optimization / Hardware Profiler

The feature the user explicitly asked for: *"All settings automatically optimized for the PC that it is running on."*

- [ ] **Detect on launch (cached):**
  - GPU vendor/model, VRAM, compute capability / tensor cores, driver version (NVML for NVIDIA, DXGI/Vulkan for others).
  - CPU cores, RAM, free disk, display resolution/DPI, power source (laptop on battery → eco preset).
- [ ] **Micro-benchmark** (first run, ~5–10 s): a tiny train loop to measure real throughput → calibrates iteration budget & ETA.
- [ ] **Engine selection:** NVIDIA + sufficient VRAM → CUDA/gsplat; else → wgpu portable engine; no capable GPU → CPU-limited "preview only" mode with a clear warning.
- [ ] **Derive a preset** (see §7.2) mapping hardware → {input resolution, max frames, max Gaussian count, iterations, densification cadence, batch size, mixed-precision}.
- [ ] **Thermal/OOM guardrails:** dynamic down-scaling if VRAM pressure or thermal throttling is detected; never crash — degrade gracefully and tell the user.
- [ ] **Honest ETA & quality forecast** shown before and during the run ("~4 min, High preset on your RTX 4070").

---

## 5. Handling Floaters & Cross-Image Inconsistencies *(explicit requirement)*

This is a first-class subsystem, not an afterthought. Failure modes and the SOTA countermeasure for each:

| Problem | Symptom | Countermeasure (SOTA) |
|---|---|---|
| **Floating blobs / "floaters"** | Semi-transparent puffs in empty space | **MCMC densification** (bounded, fewer floaters) + opacity/scale regularization + **visibility-based pruning** + monocular-depth regularization |
| **Aliasing / zoom artifacts** | Spikes/shimmer when zooming | **Mip-Splatting** (3D+2D filters) |
| **Moving objects / people / cars** | Ghosts, smears, duplicated geometry | **Transient masking**: **SAM 2 / Grounded-SAM** to segment likely-dynamic classes + robust masking à la **SpotLessSplats** (down-weight inconsistent pixels) |
| **In-the-wild appearance change** (clouds, auto-exposure, WB) | Flicker, color blotches, seams | **Per-image appearance embeddings** (NeRF-W style) / **WildGaussians** + **bilateral-grid color correction** (bilagrid) |
| **Exposure/ISO/WB differences** | Bright/dark patches per view | Exposure compensation from EXIF + learned per-image tone; robust photometric loss |
| **Rolling shutter / motion blur** | Warped edges, smear | Frame gating (§1) + down-weight flagged frames; optional deblur pre-pass |
| **Bad poses / drift** | Doubled walls, misalignment | GLOMAP global SfM + **progressive pose refinement**; robust bundle adjustment |
| **Reflections / transparency** | Blobs behind glass/mirrors | Specular-aware down-weighting; optional exclude-mask; document as known-hard |
| **Sky / unbounded background** | Floater spray at infinity | Far-field/sky separation or environment sphere; scene-contraction for unbounded scenes |

- [ ] Build a **robust loss** (e.g., trimmed/robust photometric + SSIM + optional LPIPS) that automatically discounts pixels inconsistent across views — the single biggest lever for "in-the-wild" cleanliness.
- [ ] **Auto-mask pipeline:** run segmentation once, cache masks, expose an on/off toggle + class list in Preferences ("ignore people/vehicles").
- [ ] Provide a **"strictness" slider** (Clean ↔ Detailed): Clean biases toward fewer Gaussians / stronger pruning / stronger transient rejection; Detailed preserves fine structure.

---

## 6. Live 3D Viewport / Renderer

- [ ] **Real-time splat renderer** in the WebView: WebGPU (preferred) with WebGL2 fallback; sorted alpha-blended Gaussian rasterization. Reuse a proven renderer core (gsplat-web / antimatter15-style / Babylon or Three GS) rather than writing from scratch.
- [ ] **Orbit/pan/zoom** while training; smooth camera; grid/gizmo; "frame scene" button.
- [ ] **Progressive display:** render partial Gaussian sets with delta updates from the trainer.
- [ ] **Overlays:** input camera frustums, point cloud, bounding box, floater-heatmap debug view.
- [ ] **Compare view:** slider between "raw" and "cleaned" result.
- [ ] Handle very large splats (LOD / chunked streaming / culling) so a 5M-Gaussian scene stays interactive.

---

## 7. Preferences System *(every setting, optional, with presets + Auto)*

### 7.1 Design rules
- Every setting has an **`Auto`** value (the default) — Auto is always safe and hardware-aware.
- Settings are grouped, searchable, and show **live tooltips** explaining trade-offs.
- A **"Reset to Auto"** per-section and global.
- Changing a setting shows its **impact estimate** (time/VRAM/quality).
- Presets are one click; any preset can be forked into a custom profile.

### 7.2 Presets (quality tiers)
- [ ] **Auto (default)** — profiler-chosen.
- [ ] **Draft / Live-Preview** — fastest, lowest iterations, instant feedback.
- [ ] **Balanced** — good default quality/time.
- [ ] **High** — more frames, more iterations, Mip-Splatting on.
- [ ] **Max / Archival** — max frames, MCMC high cap, full anti-aliasing + appearance modeling.
- [ ] **Eco / Laptop** — battery/thermal friendly.
- [ ] **Object / Turntable**, **Room / Indoor**, **Outdoor / Large-scale** — scene-type presets that tune bounds, background handling, densification.

### 7.3 Full settings tree (all exposed, all default to Auto)
- **Input**
  - [ ] Video frame extraction: Auto / fixed FPS / target frame count / adaptive-parallax
  - [ ] Max frames, min sharpness threshold, duplicate rejection strength
  - [ ] Image downscale / max resolution
- **Camera / SfM**
  - [ ] Backend: Auto / VGGT / MASt3R-SfM / GLOMAP / COLMAP
  - [ ] Matcher: Auto / LightGlue / RoMa / exhaustive / sequential / vocab-tree
  - [ ] Intrinsics: shared / per-image / from-EXIF / solve-focal
  - [ ] Pose refinement on/off; progressive refine on/off
- **Training**
  - [ ] Engine: Auto / CUDA (gsplat) / Portable (wgpu)
  - [ ] Iterations (or "quality target" auto-stop)
  - [ ] Densification: Auto / MCMC / AbsGrad / Taming / Default-ADC
  - [ ] Max Gaussian count / VRAM cap
  - [ ] Spherical-harmonics degree (color detail)
  - [ ] Mixed precision, batch size, learning-rate schedule (advanced/expert-collapsed)
  - [ ] Anti-aliasing: Mip-Splatting on/off
  - [ ] Surface mode: Off / 2DGS / GOF
- **Cleanliness / Robustness**
  - [ ] Floater pruning strength
  - [ ] Transient masking: Auto / off / classes (people, vehicles, animals, custom)
  - [ ] Appearance modeling: Auto / per-image embeddings / bilateral-grid / off
  - [ ] Robust-loss strictness (Clean ↔ Detailed slider)
  - [ ] Depth regularization on/off
  - [ ] Sky/background: Auto / environment-sphere / crop / keep
- **Output**
  - [ ] Format: `.ply` / `.spz` / `.splat` / SOG
  - [ ] Compression level; SH quantization
  - [ ] Auto-crop, coordinate up-axis, scale/units
  - [ ] Output folder, naming template, keep-intermediates toggle
- **App / System**
  - [ ] GPU selection (multi-GPU), VRAM ceiling, CPU thread cap
  - [ ] Cache location & size, temp cleanup policy
  - [ ] Theme (System / Light / Dark / high-contrast), accent color, DPI scaling
  - [ ] Telemetry (opt-in, off by default), crash reports
  - [ ] Auto-update channel (Stable / Beta)

- [ ] Persist as JSON profiles; import/export; per-project overrides remembered.

---

## 8. UI / UX Design ("sleek, official-looking, presentable")

- [ ] **Design language:** Fluent-inspired (rounded 8–12px, Mica/acrylic background, subtle depth), consistent 4/8px spacing grid, tasteful motion (150–250ms ease), no clutter.
- [ ] **Main window:** big drop zone with animated hint → transforms into the live viewport once a job starts. Left rail: input thumbnails/frames. Bottom: progress + telemetry HUD (iteration, Gaussians, PSNR/quality proxy, VRAM, ETA). Top: preset selector + Export.
- [ ] **First-run experience:** one-screen welcome, hardware detected + preset chosen shown proudly, "Drop a video or folder to begin."
- [ ] **Empty/loading/error states** all designed (never a raw stack trace — friendly, actionable messages).
- [ ] **Accessibility:** keyboard nav, focus rings, screen-reader labels, reduced-motion mode, WCAG-AA contrast, full DPI scaling.
- [ ] **Micro-delight:** the "scene forming" animation is the hero moment — make it feel magical but stay performant.
- [ ] **Branding:** app icon, wordmark, splash, installer art; consistent palette. Keep it credible/professional (not gamer-RGB).
- [ ] **Localization-ready** (string tables), even if v1 ships English-only.

---

## 9. Non-Functional Requirements

- [ ] **Robustness:** never lose a job to a crash — checkpoint training; resume on relaunch.
- [ ] **Cancel/pause/resume** any stage; safe cleanup of temp files.
- [ ] **Performance targets:** first splat visible < ~15 s on a mid-range NVIDIA GPU for a 30 s clip; interactive viewport ≥ 30 fps up to a few million Gaussians.
- [ ] **Footprint:** installer reasonable despite bundled CUDA/Python; lazy-download heavy model weights on first use with a progress UI and checksum verification.
- [ ] **Offline-first:** works with no internet after initial model download; no cloud dependency for core function.
- [ ] **Security/privacy:** all processing local; telemetry opt-in; sign the binary; verify downloaded weights.
- [ ] **Logging:** structured logs + a "Copy diagnostics" button for support.

---

## 10. Tech Stack (proposed, confirm in §12)

| Layer | Choice | Alt |
|---|---|---|
| UI shell | **Tauri 2** + React/Svelte + Tailwind + shadcn/Fluent theme | WinUI 3 / .NET; Electron |
| Core/orchestrator | **Rust** | C# |
| Splat viewer | **WebGPU** splat renderer (WebGL2 fallback) | native wgpu view |
| CUDA engine | **gsplat** + PyTorch, **COLMAP/GLOMAP**, **VGGT/MASt3R**, **SAM 2**, **Depth Anything V2** | Nerfstudio / original 3DGS |
| Portable engine | **Brush** (Rust + Burn + wgpu) | — |
| Video/IO | **FFmpeg** (bundled) | — |
| Python env | embedded via **uv** / bundled miniforge, isolated | PyInstaller-frozen sidecar |
| Packaging | **MSI/NSIS** via Tauri bundler, code-signed, auto-update | MSIX |

---

## 11. Milestones (phased, gated)

Each phase ends with a **demoable build** and a go/no-go gate.

### Phase 0 — Foundations (skeleton)
- [ ] Repo, CI, code signing, crash reporting, logging.
- [ ] Tauri shell + drag-and-drop + empty viewport + Preferences scaffold.
- [ ] Hardware profiler (detect + cache) and preset resolver.
- [ ] Process/IPC plumbing between Rust core and a stub engine.
- **Gate:** drop a folder → app enumerates it, shows detected hardware + chosen preset.

### Phase 1 — MVP happy path (NVIDIA/CUDA)
- [ ] FFmpeg ingestion + basic frame gating.
- [ ] COLMAP/GLOMAP SfM → gsplat training → live viewport streaming.
- [ ] `.ply` export.
- [ ] Real progress/ETA HUD.
- **Gate:** drag a video on an RTX GPU → watch a splat form → export `.ply`. **This is the "wow" demo.**

### Phase 2 — Clean results
- [ ] MCMC densification + floater pruning + Mip-Splatting.
- [ ] Depth regularization; visibility-based pruning; auto-crop.
- [ ] Robust loss + basic transient masking (SAM 2).
- **Gate:** a dataset with a walking person and lighting change reconstructs cleanly.

### Phase 3 — Instant + Portable
- [ ] VGGT/MASt3R fast init → splat on screen in seconds; progressive pose refine.
- [ ] Brush/wgpu portable engine for AMD/Intel; engine auto-selection.
- [ ] Micro-benchmark calibration + graceful VRAM/thermal degradation.
- **Gate:** runs end-to-end on an AMD GPU and an Intel iGPU laptop; instant preview on NVIDIA.

### Phase 4 — In-the-wild robustness + appearance
- [ ] Per-image appearance embeddings + bilateral-grid color correction.
- [ ] Exposure/WB compensation from EXIF; sky/background handling.
- [ ] Strictness slider; scene-type presets.
- **Gate:** outdoor, mixed-camera, mixed-exposure dataset looks coherent.

### Phase 5 — Polish & release
- [ ] Full Preferences tree with Auto everywhere; profiles import/export.
- [ ] All export formats (`.spz`/`.splat`/SOG) + compression.
- [ ] Design pass, accessibility, first-run, error states, docs/tutorial.
- [ ] Installer, signing, auto-update, telemetry (opt-in).
- **Gate:** ship v1.0.

### Phase 6 — Stretch (v2)
- [ ] Live-webcam SLAM capture (MonoGS/SplaTAM-style).
- [ ] Splat editing (embed/hand-off SuperSplat), mesh export, relighting.
- [ ] Cloud/offload option; macOS/Linux builds; batch/CLI mode.

---

## 12. Open Questions / Decisions to Lock

- [ ] **UI framework:** Tauri 2 (recommended) vs WinUI 3. → affects §2, §10.
- [ ] **Bundling heavy deps:** ship CUDA/Python in installer (big) vs first-run download (slimmer, needs internet once). → recommend first-run download w/ checksum.
- [ ] **Minimum spec** to advertise (e.g., "GTX 1060 6GB / 16GB RAM" for CUDA path; portable path for the rest).
- [ ] **Model weight licensing** (VGGT, MASt3R, SAM 2, Depth Anything V2) — confirm redistribution/commercial terms per model before bundling.
- [ ] **3DGS/gsplat licensing** — the original 3DGS is research/non-commercial; **gsplat (Apache-2.0)** and **Brush** avoid that trap. Prefer Apache/MIT-licensed components throughout for a shippable product; audit every dependency's license.
- [ ] **Metric scale:** do we need real-world units (from EXIF/known object)? Default: relative scale, optional metric.
- [ ] **Telemetry policy** and privacy copy.

> Any item here that blocks implementation should be raised early; where unblocked, proceed with the recommended default and note the assumption.

---

## 13. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| COLMAP/SfM fails on hard scenes | VGGT/MASt3R pose-free fallback; clear "couldn't solve cameras" guidance + capture tips |
| VRAM OOM on big scenes | MCMC hard cap, dynamic downscale, VRAM ceiling setting |
| Portable (wgpu) path slower/lower quality | Set expectations in UI; keep it functional, not aspirational |
| Licensing landmines (non-commercial 3DGS/weights) | License audit in Phase 0; prefer Apache/MIT stack (gsplat/Brush) |
| Bundling bloat / install friction | First-run weighted download w/ progress + checksum; slim installer |
| "Live" feels laggy on weak GPUs | Progressive low-res-first; Draft preset; honest ETA |
| Floaters still slip through | Layer defenses (MCMC + pruning + robust loss + masking); expose strictness slider; offer SuperSplat hand-off |

---

## 14. Definition of Done (v1.0)

- [ ] Drag video **or** image folder → live splat with **zero** required configuration.
- [ ] Auto hardware profiling picks engine + preset; runs on NVIDIA (CUDA) **and** AMD/Intel (portable).
- [ ] Floaters, moving objects, and exposure drift are visibly handled on a standard "hard" test set.
- [ ] Full Preferences panel: every setting present, all defaulting to **Auto**, with presets.
- [ ] Export `.ply` + at least one compressed format; result opens in third-party viewers.
- [ ] Signed installer, auto-update, first-run experience, accessible sleek UI.
- [ ] Crash-resilient (checkpoint/resume), offline-capable, local-only processing.

---

## 15. Glossary / Key References (for the implementer to pull SOTA from)

- **3DGS** — 3D Gaussian Splatting (Kerbl et al., 2023).
- **gsplat** — Nerfstudio's Apache-2.0 CUDA splatting library (preferred training core).
- **Brush** — Rust/Burn/wgpu cross-vendor 3DGS trainer+viewer (portable engine).
- **MCMC densification** — *3D Gaussian Splatting as Markov Chain Monte Carlo* (bounded count, fewer floaters).
- **Mip-Splatting** — anti-aliasing via 3D smoothing + 2D mip filters.
- **2DGS / GOF / RaDe-GS** — surface-oriented splatting (flatter, cleaner geometry).
- **VGGT** — Visual Geometry Grounded Transformer (feed-forward geometry/poses; instant init).
- **MASt3R / MASt3R-SfM / DUSt3R** — pose-free dense matching & reconstruction.
- **InstantSplat** — fast pose-free 3DGS from sparse views.
- **GLOMAP** — global SfM, much faster than incremental COLMAP.
- **LightGlue / SuperPoint / ALIKED / DISK / RoMa** — feature matching front-ends.
- **SpotLessSplats / WildGaussians / NeRF-W** — transient & in-the-wild appearance handling.
- **SAM 2 / Grounded-SAM** — segmentation for transient masking.
- **Depth Anything V2** — monocular depth prior for regularization.
- **Bilateral grid (bilagrid)** — per-image color/exposure correction.
- **SuperSplat** — web-based splat editor (cleanup / hand-off target).
- **.spz** — Niantic compressed splat format; **SOG / Self-Organizing Gaussians** — PNG-packed compact format.

> **Implementation note for Fable 5:** these method names reflect the state of the art as of the roadmap's writing. **Before building any stage, do a fresh capability check** — the splatting field moves monthly; prefer the newest well-licensed, well-maintained implementation over the specific paper named here if a better one exists. Always verify a library's license permits commercial distribution before bundling.
