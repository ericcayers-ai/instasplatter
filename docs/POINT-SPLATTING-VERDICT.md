# Verdict: is Gaussian Point Splatting the right geospatial renderer here?

Decision record for the SIGGRAPH '26 sorting-free renderer (`JorisAR/gaussian-point-splatting`,
BSD-3-Clause, 218★ at sweep time) versus the alternatives found in the 2026-11-03 sweep
(`docs/research/A-rendering.md`).

## What was in the repo before this pass

`src/splat/pointSplatting.ts` (385 lines) described itself as an "integrated" implementation of the
paper, and `docs/PAPER-SWEEP-2024+.md` entry #61 recorded a "64-Bit Atomic Visibility Buffer" as
integrated. Three things were true and one was not:

| Claim | Reality found |
|---|---|
| CPU reference rasteriser exists | True, and order-independent under permutation (smoke tested) |
| "64-bit atomic visibility buffer" in WGSL | **Not runnable.** WGSL has no `u64`; the shader also had a depth/colour race (see below) |
| Integrated into the renderer | **False.** Nothing imported it; the viewport has only ever run the WebGL2 EWA path |
| Splat sampling is isotropic Gaussian | **False.** Box–Muller had `cos` where `sin` belongs, correlating x and z |

## Bugs found and fixed

1. **Box–Muller third component.** `sampleStandardNormal3D` returned
   `[r1cosθ1, r1sinθ1, r2cosθ2]`. The third normal must be `r2sinθ2`. With two cosines the x and z
   coordinates are perfectly correlated (z = r2·cos is a function of u3,u4 only, and its sign is
   tied to θ1), so sampled points formed a curved sheet instead of a 3-D Gaussian cloud. Fixed.
2. **Binary transparency.** One point per pixel is a Bernoulli draw of coverage: a 30%-opaque splat
   renders as fully opaque or absent, with no intermediate value. The resolve now averages
   `samplesPerPixel²` jittered samples and writes alpha = hits/samples, which is what makes the
   sampling an estimate of opacity rather than noise. Measured on a 30%-opacity Gaussian covering
   ~1.4% of a 64x64 view: with `samplesPerPixel: 1` every covered pixel had alpha 255 (binary); with
   2 (4 samples) the alpha histogram was 64:106, 128:34, 191:14, 255:2; with 4 (16 samples) it stepped
   in sixteenths (16:234, 32:125, 48:87, 64:25, …). Coverage also rose with sample count (1.4% ->
   3.8% -> 11.7%), which is the honest limit of this renderer: coverage is bounded by emitted points,
   so the per-Gaussian point budget is the quality knob, not the sample count alone.
3. **The WGSL shader.** (a) `atomicMin(depth)` followed by a separate colour store is a race: a
   farther point that loses the min can still overwrite a nearer point's colour. (b) There is no
   `atomic<u64>` in WGSL — `shader-int64` is not a browser feature, and the optional
   `atomic-vec2u-min-max` feature (`atomicStoreMin` on `atomic<vec2<u32>>`) is adapter-dependent and
   absent from the default feature set. The shader is now a **two-pass** design over 32-bit depth
   (depth pass with `atomicMin`, then a colour pass that writes only where the stored depth equals
   the point's depth), with the feature requirement stated in the file.

## Comparison on the criteria that matter for InstaSplat

| Criterion | Gaussian Point Splatting | StopThePop (SIGGRAPH'24) | Hierarchical-3DGS / Octree-GS LOD | Chunked streaming (SOG / 3D Tiles) |
|---|---|---|---|---|
| Removes the global sort | **Yes** (stochastic opaque points) | **Yes** (per-pixel sorted lists) | No — reduces what must be sorted | No — reduces what must be resident |
| Popping under rotation | Eliminated by construction | Eliminated | Reduced by screen-space-error LOD | N/A (loading, not ordering) |
| Runs on WebGL2 (this repo's path) | **No** — compute + atomics | **No** — per-pixel lists need image load/store | **Yes** — CPU-side chunk selection | Yes (format + loader work) |
| Cost driver | points emitted ∝ screen area × opacity | per-pixel list memory (K × pixels) | visible chunk count | bytes downloaded |
| Quality | stochastic noise, needs ≥4 spp to converge | exact per-pixel order, no noise | unchanged within a chunk | unchanged |
| Licence | BSD-3-Clause (clean) | `NOASSERTION` — read LICENSE before reuse | Inria NC / NOASSERTION — concept only | MIT / Apache-2.0 |
| Effort to ship in this repo | ~8–12 eng-days (WebGPU backend + resolve + adapter fallback) | high (needs a compute rasteriser too) | **done** (chunks + frustum culling landed, `docs/PERF.md`) | 2–3 days for export, more for a loader |

## Decision

1. **Do not replace the WebGL2 renderer with the point-splatting path.** The remaining ceilings that
   hurt this app today are "how much of the scene is on screen" and "how much of the scene is
   resident", and both are addressed without a rasteriser rewrite: chunk culling + a point budget
   gave a **92.5% frame-time reduction** at 300k splats in the inside-camera case and **8.3 s → 42 ms**
   at 1M splats in the wide case (`docs/PERF.md`). Replacing a working, tested WebGL2 path with a
   WebGPU path that has no adapter fallback is the higher-risk change for no measured gain on
   hardware that can already draw these scenes.
2. **Keep `pointSplatting.ts` as a validated reference implementation**, not as a claimed shipped
   feature. The CPU rasteriser is fast enough to be a correctness oracle (order-independence,
   coverage behaviour) and the corrected WGSL is the starting point for a future WebGPU backend.
3. **Take the LOD idea, not the rasteriser.** The measured wins came from chunk-level frustum
   culling, which is the Hierarchical-3DGS/Octree-GS principle applied at a granularity this renderer
   can act on. Screen-space-error LOD *within* a chunk is the next step and is not implemented
   (recorded as a gap in `docs/PERF.md`).
4. **Ship streaming before shipping sorting-free.** For 100M+ splats the binding constraint is
   residency (a 128 MiB storage-buffer binding holds ~4.2M splats at 32 bytes), not ordering. Delivery
   format work (`splat-transform` Streamed SOG, SPZ, 3D Tiles 2.0 + `KHR_gaussian_splatting`) is MIT
   and days of work, and it is the only item in the sweep that scales an order of magnitude.

## What would change this decision

- A WebGPU baseline in the shipping webview (Tauri on Windows uses WebView2, which does expose
  WebGPU behind a flag/vendor rollout) **plus** scenes above ~20M splats in one view. At that point
  the sort and the draw both need the compute path, and the two-pass shader in
  `src/splat/pointSplatting.ts` is the thing to bring up.
- Evidence that the stochastic noise is visible at the delivery sample count (4 spp) on real captures;
  if it is, StopThePop-style ordering (or 8–16 spp) becomes the better trade.
