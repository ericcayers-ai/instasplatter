# Splat renderer performance

Measured with `node tools/bench/run-bench.mjs` — the real `SplatRenderer` (same WebGL2 shaders,
same worker radix sort), driven frame-by-frame through `renderer.renderFrame()` with a 1-pixel
`readPixels` after each frame so the timer includes GPU execution.

## Method, and what these numbers are not

- **Adapter:** `ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device (Subzero)))` — Chromium's software
  rasteriser. These are **not desktop FPS claims**: an RTX 4060 is one to two orders of magnitude
  faster per splat. They are valid for **A/B comparison** on identical hardware, and the
  counter-based results (candidate splats, drawn instances, chunks visible) are
  hardware-independent.
- **Why `renderFrame` instead of `requestAnimationFrame`:** headless Chromium throttles rAF
  (measured ~1 s per callback at any scene size), so an rAF loop measures the throttler. The bench
  also uses `gl.finish()`-replacement (`readPixels`): `gl.finish()` returned in 0.01 ms for 250k
  splats through ANGLE/Vulkan and measured nothing.
- **Input:** a synthetic 3DGS PLY, a 400 m x 400 m site with a ground sheet plus vertical
  structure, deterministic xorshift seed. No capture fixture is committed to this repo.
- **Camera:** `inside` places the eye 40 m from a corner, looking at it — the case a ground-level
  or oblique capture produces. `wide` keeps the framed whole-scene camera: the control where
  culling cannot help.

Reproduce:

```powershell
node tools/bench/run-bench.mjs --count 300000 --frames 20          # inside view
node tools/bench/run-bench.mjs --count 1000000 --frames 10 --wide  # whole scene in view
```

## What changed in the renderer

Before: every frame sorted **all** splats on the worker and drew **every** splat as an instance.
Frame cost and sort cost were proportional to the size of the scene, not to what is on screen — the
ceiling `mkkellogg/GaussianSplats3D` documents at roughly 8–16M splats.

After (`src/splat/worker.ts`, `src/splat/renderer.ts`, `src/splat/frustum.ts`):

1. **Spatial chunks.** The worker partitions splats into a uniform 3-D grid with a **50 m target
   chunk edge** (capped at 16^3 = 4096 chunks) and keeps a permutation that makes each chunk a
   contiguous range.
2. **Frustum culling** (`frustumPlanes` / `visibleChunkIndices`, unit-tested in
   `src/splat/frustum.smoke.ts`). Six clip-space planes are extracted from
   `proj * view * model` once per frame, then each chunk's world-space bounding sphere is tested.
   Conservative: a chunk is dropped only when it is fully outside a plane.
3. **Visible-only sorting.** The sort request carries the visible chunk list; the worker gathers
   those contiguous ranges, applies the budget stride, and sorts only the result.
4. **Point budget.** `renderer.pointBudget` (default 4,000,000) caps the work per frame with a
   uniform stride, so a huge visible set degrades in quality instead of collapsing the frame rate.
5. **Counters.** `renderer.frameStats` / `onSplatStats` expose total/visible/candidate/drawn splats,
   the stride, and the sort-request count — the numbers below come from there.

## Results

### 300,000 splats, inside view (the culling case)

| Configuration | Frame | p95 | Drawn splats | Candidate splats | Chunks visible |
|---|---|---|---|---|---|
| draw disabled (fixed floor) | 0.2 ms | 0.3 ms | — | — | 32 / 512 |
| **culling on** | **147 ms** | 148 ms | 19,013 | 19,013 | 32 / 512 |
| culling off | 2,255 ms | 2,085 ms | 300,000 | 300,000 | 512 / 512 |
| culling on + budget 5,000 | 37.5 ms | 37.2 ms | 4,753 | 4,753 | 32 / 512 (stride 4) |

- **Sort work reduced 93.7%** (19,013 vs 300,000 splats submitted to the radix sort).
- **Frame time reduced 93.5%** (2,255 ms → 147 ms).
- Cost tracks drawn splats at ~7.7 µs per splat on this software rasteriser
  (`(147.5 - 0.2) ms / 19,013`), and 2255 ms / 300,000 = 7.5 µs — i.e. the win is
  "draw fewer splats", which transfers to any fill/vertex-bound adapter.
- **The frame is not empty:** 1.7% of pixels carry splat coverage after the run (the synthetic
  splats are ~3 cm at 40 m, so they are sub-pixel at this camera). A shader regression that broke the
  transform would drop this to 0.0% while `drawnSplats` still looked healthy, which is why the bench
  reports it.

Yaw sweep from the same run (chunks visible / splats drawn), showing the culling set follows the
camera instead of the scene size:

| Camera turn | Chunks visible | Splats drawn |
|---|---|---|
| 0° | 32 / 512 | 19,013 |
| 90° | 72 / 512 | 42,568 |
| 180° | 168 / 512 | 98,578 |
| 270° | 38 / 512 | 23,619 |

### 1,000,000 splats, wide view (the honest control)

| Configuration | Frame | Drawn splats | Chunks visible | Stride |
|---|---|---|---|---|
| culling on | 8,342 ms | 1,000,000 | 512 / 512 | 1 |
| culling off | 8,283 ms | 1,000,000 | 512 / 512 | 1 |
| culling on + budget 5,000 | **42 ms** | 5,000 | 512 / 512 | 200 |

With the whole scene in view there is nothing to cull (0% reduction, and the 0.7% difference is
noise). This is the case the **point budget** exists for: 8.3 s → 42 ms by drawing 0.5% of the
cloud at full spatial coverage.

## What this does not fix

- **The 100M+ regime still needs a delivery format, not a culling tweak.** Even with culling, the
  whole cloud must be decoded and resident. Streamed SOG / 3D Tiles 2.0 (`KHR_gaussian_splatting`)
  is the path; see `docs/REPO-INVENTORY-2026-11-03.md`.
- **Sorting is still global per visible set.** A per-tile or per-pixel sort (StopThePop,
  Gaussian Point Splatting) removes the visible-set sort entirely but needs compute/atomics that
  WebGL2 does not expose. `src/splat/pointSplatting.ts` holds the reference implementation and
  `docs/POINT-SPLATTING-VERDICT.md` records the decision.
- **No LOD within a chunk.** A distant chunk keeps all its splats; only whole chunks are culled.
  Screen-space-error LOD (Hierarchical-3DGS, Octree-GS) is the next step and is not implemented.

## Precision change made in the same pass

The splat vertex shader is now camera-relative: it subtracts the eye from the world position and
uses a translation-free view matrix (`u_view_rot * (center - u_eye)`), which is algebraically
identical to `u_view * center` but spends float32 precision on the distance to the camera instead of
the distance to the ENU origin. Measured (`src/geospatial/precision.smoke.ts`): absolute ENU error is
0.03 mm at 1 km, 0.49 mm at 10 km, 0.98 mm at 20 km, and crosses 1 mm at 32.8 km; camera-relative
error at 21 km from the origin was **512x smaller**. The two forms are verified to agree to 2e-5 m,
so this is a conditioning change, not a behavioural one.
