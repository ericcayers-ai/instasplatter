# Running InstaSplatter

## Install and run

The installer is built at:

```
src-tauri/target/release/bundle/nsis/InstaSplatter_0.1.0_x64-setup.exe
```

Run it, then launch InstaSplatter from the Start menu. To skip the installer, run the executable directly:

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
```

165 Rust unit tests cover the linear algebra, the COLMAP and PLY readers and writers, the SPZ encoder, the incremental camera solver, and the mesh extractor. The camera-control maths in `src/splat/camera.ts` has its own numeric checks.

## What is new since v0.1

**Viewport.** Orbit, pan and zoom were rewritten. Rotation no longer flips near the poles, panning tracks the cursor at any zoom distance, zoom moves toward the cursor rather than the screen centre, and scenes load the right way up. You can turn the model itself, snap its up axis to the nearest world axis, or stand it on a ground plane the app finds for you.

**Saving.** Every run writes a project file as it goes. If training is interrupted, the run resumes from its last checkpoint.

**Export.** Splats save as PLY, `.splat` or SPZ. The viewport orientation is baked into whatever you export.

**Mesh export.** After a reconstruction finishes, "Export mesh" renders depth from each solved camera, fuses it into a signed-distance volume, and extracts a coloured, watertight surface. It writes glb, OBJ or PLY.

**Live camera tracking (off by default).** Turn it on in Preferences under Cameras. Cameras then solve one at a time and appear in the viewport as frustums, with a running count and confidence, instead of waiting for a single blocking COLMAP pass. If it loses confidence it tells you and switches back to COLMAP.

## Settings that default to off

Three settings this release adds are off until they have been measured against an end-to-end baseline:

- **Live camera tracking.** The camera intrinsics are guessed from the image size and are not refined, so poses carry whatever error that guess introduces. COLMAP remains the accurate path.
- **Progressive resolution.** Training restarts at each resolution step, which resets the optimiser's moment estimates.
- **Mip-Splatting filter.** Applied between stages and baked into the result, rather than acting as a training-time regularizer.

Each is a single toggle in Preferences.
