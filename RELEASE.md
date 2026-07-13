# Running InstaSplatter

## Install and run

Download the latest installer from [GitHub Releases](https://github.com/ericcayers-ai/instasplatter/releases), or build locally:

```
src-tauri/target/release/bundle/nsis/InstaSplatter_0.2.0_x64-setup.exe
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

168 Rust unit tests cover the linear algebra, the COLMAP and PLY readers and writers, the SPZ encoder, the incremental camera solver, and the mesh extractor. The camera-control maths in `src/splat/camera.ts` has its own numeric checks.

## What is new in v0.2.0

**Interface.** Distinctive InstaSplatter identity: Syne display type, IBM Plex body and mono, a restrained splatter motif on the home screen, light and dark themes, and keyboard focus rings.

**Viewport.** Orbit, pan and zoom were rewritten. Rotation no longer flips near the poles, panning tracks the cursor at any zoom distance, zoom moves toward the cursor rather than the screen centre, and scenes load the right way up. You can turn the model itself, snap its up axis to the nearest world axis, or stand it on a ground plane the app finds for you.

**Saving.** Every run writes a project file as it goes. If training is interrupted, the run resumes from its last checkpoint. Recent projects appear on the home screen with Resume and Delete actions.

**Export.** Splats save as PLY, `.splat` or SPZ. The viewport orientation is baked into whatever you export. Optional mesh export writes glb, OBJ or PLY with per-vertex colour.

**Live camera tracking (off by default).** Turn it on in Settings under Cameras. Cameras then solve one at a time and appear in the viewport as frustums, with a running count and confidence, instead of waiting for a single blocking COLMAP pass. If it loses confidence it tells you and switches back to COLMAP.

**Reliability.** Exhaustive error messages at every stage, a one-click diagnostics export, and a preflight free-disk check before jobs start.

## Settings that default to off

Three settings are off until they have been measured against an end-to-end baseline:

- **Live camera tracking.** Camera intrinsics are guessed from the image size and are not refined, so poses carry whatever error that guess introduces. COLMAP remains the accurate path.
- **Progressive resolution.** Training restarts at each resolution step, which resets the optimiser's moment estimates.
- **Mip-Splatting filter.** Applied between stages and baked into the result, rather than acting as a training-time regularizer.

Each is a single toggle in Settings.

## Manual end-to-end gate

Before trusting a release on your machine, run one full reconstruction (video or image folder) with engines installed. This is the acceptance check for ROADMAP-V2 item 1.1.
