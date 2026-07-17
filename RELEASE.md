# Running InstaSplatter

## Install and run

Download the latest installer from [GitHub Releases](https://github.com/ericcayers-ai/instasplatter/releases), or build locally:

```
src-tauri/target/release/bundle/nsis/InstaSplatter_0.9.1_x64-setup.exe
```

Run it, then launch InstaSplatter from the Start menu. To skip the installer:

```
src-tauri/target/release/instasplatter.exe
```

Drop a video file or a folder of images onto the window. On the first run the app downloads COLMAP and Brush, checks each against a pinned SHA-256, and installs them under your app data directory. Both downloads together are about 200 MB. Video input also needs FFmpeg on `PATH`:

```
winget install ffmpeg
```

Nothing else is required. There is no CUDA and no Python runtime; the trainer runs on any GPU that supports Vulkan, Metal or DirectX 12.

## Rebuilding

```
npm install
npm run tauri build
```

The installer lands in `src-tauri/target/release/bundle/nsis/`. For a development run with hot reload:

```
npm run tauri dev
```

## Tests

```
cd src-tauri && cargo test
npx tsc --noEmit
node --experimental-strip-types src/splat/camera.ts
```

Rust unit tests cover linear algebra, COLMAP and PLY readers and writers, the SPZ encoder, the incremental camera solver, the mesh extractor, and the Experimental Minecraft schematic (Sponge v2) writer. The camera-control maths in `src/splat/camera.ts` has its own numeric checks.

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

Before trusting a release on your machine, run one full reconstruction (video or image folder) with engines installed. This is the acceptance check for ROADMAP-V2 item 1.1. With Experimental Mode on, also try **Export schematic** and confirm a `.schem` opens in your preferred paste tool.
