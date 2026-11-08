/**
 * Splat renderer benchmark.
 *
 * Runs the real `SplatRenderer` (same WebGL2 shaders, same worker sort) against a synthetic
 * geospatial-density cloud, and reports frame timing plus the hardware-independent counters
 * (candidate splats, drawn splats, visible chunks).
 *
 * Driven by `tools/bench/run-bench.mjs` (Playwright + Chromium); results are recorded in
 * docs/PERF.md.
 *
 * Two deliberate choices:
 * - `manageLoop: false` and a manual frame pump. Headless Chromium throttles
 *   `requestAnimationFrame` (measured ~1 s per callback here), so an rAF-driven loop measures
 *   the throttler, not the renderer. Each frame ends with `gl.finish()` so the timer includes
 *   GPU work rather than command submission.
 * - Synthetic input: no capture fixture is committed, and the work a frame does is a function of
 *   splat count and spatial extent, not of photographic content.
 */

import { SplatRenderer } from "../../src/splat/renderer";
import { orbitBasis } from "../../src/splat/camera";

interface Measurement {
  label: string;
  frames: number;
  avgFrameMs: number;
  p50FrameMs: number;
  p95FrameMs: number;
  avgFps: number;
  drawnSplats: number;
  candidateSplats: number;
  visibleChunks: number;
  chunkCount: number;
  budgetStride: number;
}

const logEl = document.getElementById("log") as HTMLPreElement;
const lines: string[] = [];
function log(line: string) {
  lines.push(line);
  logEl.textContent = lines.join("\n");
}

/**
 * Build a 3DGS-style binary PLY in memory: a 400 m x 400 m site with a ground sheet plus
 * vertical structure, which is the shape that makes chunk culling meaningful.
 */
function makeSyntheticPly(count: number, seed = 12345): ArrayBuffer {
  const props = [
    "x", "y", "z",
    "f_dc_0", "f_dc_1", "f_dc_2",
    "opacity",
    "scale_0", "scale_1", "scale_2",
    "rot_0", "rot_1", "rot_2", "rot_3",
  ];
  // The vertex block must start on a 4-byte boundary to be viewed as a Float32Array, so the
  // padding has to sit INSIDE the header (a comment line). Appending it after `end_header`
  // shifts every float by 1-3 bytes: the parser then reads 1e37 coordinates (measured).
  const head = [
    "ply",
    "format binary_little_endian 1.0",
    "element vertex " + count,
    ...props.map((p) => `property float ${p}`),
  ].join("\n");
  const tail = "end_header\n";
  // Pick a filler comment whose length makes the whole header a multiple of 4 bytes. Any line
  // that is not `element`/`property` is ignored by the parser.
  let header = head + "\n" + tail;
  for (let padLen = 1; padLen <= 8 && header.length % 4 !== 0; padLen++) {
    header = head + "\n" + "#".repeat(padLen) + "\n" + tail;
  }
  if (header.length % 4 !== 0) throw new Error("PLY header alignment failed");
  const headerBytes = new TextEncoder().encode(header);
  const dataBytes = count * props.length * 4;
  const buffer = new ArrayBuffer(headerBytes.length + dataBytes);
  new Uint8Array(buffer).set(headerBytes, 0);
  const view = new Float32Array(buffer, headerBytes.length, count * props.length);

  let s = seed >>> 0;
  const rand = () => {
    // xorshift32: deterministic across runs and engines.
    s ^= s << 13;
    s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5;
    s >>>= 0;
    return s / 4294967296;
  };

  const stride = props.length;
  for (let i = 0; i < count; i++) {
    const o = i * stride;
    const onGround = rand() < 0.72;
    const x = (rand() - 0.5) * 400;
    const y = (rand() - 0.5) * 400;
    const height = 14 * rand() * rand();
    const z = onGround ? 0.4 * rand() : height;
    view[o] = x;
    view[o + 1] = y;
    view[o + 2] = z;
    const shade = 0.35 + 0.5 * rand() + 0.15 * (z / 14);
    view[o + 3] = (shade - 0.5) / 0.28209479177387814;
    view[o + 4] = (shade * 0.92 - 0.5) / 0.28209479177387814;
    view[o + 5] = (shade * 0.8 - 0.5) / 0.28209479177387814;
    view[o + 6] = 2.2; // sigmoid -> ~0.9 opacity
    const sc = onGround ? -3.4 : -2.8;
    view[o + 7] = sc;
    view[o + 8] = sc;
    view[o + 9] = sc;
    view[o + 10] = 1;
    view[o + 11] = 0;
    view[o + 12] = 0;
    view[o + 13] = 0;
  }
  return buffer;
}

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.min(sorted.length - 1, Math.floor(sorted.length * p));
  return sorted[idx];
}

let frameClock = 0;
const syncPixel = new Uint8Array(4);

/**
 * Block until the frame has actually executed.
 * `gl.finish()` is not sufficient through ANGLE/Vulkan (it returned in 0.01 ms for 250k
 * splats), so read one pixel back: a read must wait for the pipeline.
 */
function syncFrame(gl: WebGL2RenderingContext) {
  gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, syncPixel);
}

/** Draw `frames` frames synchronously and time each one, GPU work included. */
function measure(
  renderer: SplatRenderer,
  gl: WebGL2RenderingContext,
  label: string,
  frames = 30,
): Measurement {
  for (let i = 0; i < 5; i++) {
    renderer.renderFrame(frameClock);
    frameClock += 16;
    syncFrame(gl);
  }
  const times: number[] = [];
  const t0 = performance.now();
  for (let i = 0; i < frames; i++) {
    const a = performance.now();
    renderer.renderFrame(frameClock);
    frameClock += 16;
    syncFrame(gl);
    times.push(performance.now() - a);
  }
  const elapsed = performance.now() - t0;
  const sorted = [...times].sort((a, b) => a - b);
  const avg = times.reduce((a, b) => a + b, 0) / Math.max(1, times.length);
  const stats = renderer.frameStats;
  return {
    label,
    frames,
    avgFrameMs: avg,
    p50FrameMs: percentile(sorted, 0.5),
    p95FrameMs: percentile(sorted, 0.95),
    avgFps: elapsed > 0 ? (frames * 1000) / elapsed : 0,
    drawnSplats: stats.drawnSplats,
    candidateSplats: stats.candidateSplats,
    visibleChunks: stats.visibleChunks,
    chunkCount: stats.chunkCount,
    budgetStride: stats.budgetStride,
  };
}

/**
 * Fraction of pixels with any splat coverage. A shader regression (wrong matrix, broken RTC) shows
 * up here as a collapse to ~0 even when "drawnSplats" looks healthy.
 */
function measureCoverage(gl: WebGL2RenderingContext, w: number, h: number): number {
  const px = new Uint8Array(w * h * 4);
  gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
  let covered = 0;
  for (let i = 3; i < px.length; i += 4) {
    if (px[i] > 8) covered++;
  }
  return covered / (w * h);
}

/** Pump frames (with pauses so the worker can reply) until the pipeline settles. */
async function settle(
  renderer: SplatRenderer,
  gl: WebGL2RenderingContext,
  ms = 1500,
) {
  const start = performance.now();
  while (performance.now() - start < ms) {
    renderer.renderFrame(frameClock);
    frameClock += 16;
    syncFrame(gl);
    await new Promise((r) => setTimeout(r, 25));
  }
}

async function main() {
  const params = new URLSearchParams(location.search);
  const count = Number(params.get("count") ?? "250000");
  const budget = Number(params.get("budget") ?? "4000000");
  const frames = Number(params.get("frames") ?? "30");
  const vw = Number(params.get("w") ?? "960");
  const vh = Number(params.get("h") ?? "540");

  const canvas = document.getElementById("c") as HTMLCanvasElement;
  canvas.width = vw;
  canvas.height = vh;

  log(`splat count: ${count.toLocaleString()}`);
  const renderer = new SplatRenderer(canvas, { manageLoop: false });
  renderer.pointBudget = budget;
  const gl = canvas.getContext("webgl2") as WebGL2RenderingContext;
  const adapter = (() => {
    const ext = gl.getExtension("WEBGL_debug_renderer_info");
    return ext ? String(gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : "unknown adapter";
  })();

  let statsSeen = 0;
  renderer.onStats = () => {
    statsSeen++;
  };
  renderer.loadPly(makeSyntheticPly(count), true);

  const t0 = performance.now();
  while (statsSeen === 0 && performance.now() - t0 < 180_000) {
    renderer.renderFrame(frameClock);
    frameClock += 16;
    syncFrame(gl);
    await new Promise((r) => setTimeout(r, 60));
  }
  if (statsSeen === 0) throw new Error("worker never reported a parsed cloud");

  // Default: look at a corner of the site from inside it, so most of the cloud is outside the
  // frustum — the case a ground-level or oblique capture actually produces. `wide=1` keeps the
  // framed whole-scene camera instead, which is the honest "nothing to cull" control.
  const wide = params.get("wide") === "1";
  if (!wide) {
    renderer.camera.target = [-150, -150, 4];
    renderer.camera.distance = 40;
    renderer.camera.pitch = -0.35;
  }
  await settle(renderer, gl, 1200);

  // Floor: same scene with the splat pass disabled. Separates per-frame fixed cost from
  // per-splat draw cost.
  renderer.showSplat = false;
  const noDraw = measure(renderer, gl, "draw disabled", 20);
  renderer.showSplat = true;

  // Culling sanity sweep: orbit the camera in 90 degree steps. If the visible chunk count never
  // falls below the chunk count the frustum test is broken and the numbers below mean nothing.
  renderer.cullingEnabled = true;
  const facingYaw = renderer.camera.yaw;
  const yawSweep: {
    turnDeg: number;
    visibleChunks: number;
    candidates: number;
    sortRequests: number;
  }[] = [];
  for (const step of [0, 1, 2, 3]) {
    renderer.camera.yaw = facingYaw + (step * Math.PI) / 2;
    await settle(renderer, gl, 500);
    yawSweep.push({
      turnDeg: step * 90,
      visibleChunks: renderer.frameStats.visibleChunks,
      candidates: renderer.frameStats.candidateSplats,
      sortRequests: renderer.frameStats.sortRequests,
      eye: orbitBasis(renderer.camera).eye.map((v) => Math.round(v)),
    });
  }
  const awayStats = { ...renderer.frameStats };
  renderer.camera.yaw = facingYaw;
  await settle(renderer, gl, 500);

  renderer.cullingEnabled = true;
  renderer.pointBudget = budget;
  await settle(renderer, gl, 700);
  const withCulling = measure(renderer, gl, "culling on", frames);
  const coverageWithCulling = measureCoverage(gl, canvas.width, canvas.height);

  renderer.cullingEnabled = false;
  await settle(renderer, gl, 700);
  const withoutCulling = measure(renderer, gl, "culling off", frames);
  const coverageWithoutCulling = measureCoverage(gl, canvas.width, canvas.height);

  // Budget test: culling on, with a hard cap well below the visible count.
  renderer.cullingEnabled = true;
  renderer.pointBudget = Math.min(5_000, Math.max(1_000, Math.floor(count / 20)));
  await settle(renderer, gl, 700);
  const budgeted = measure(renderer, gl, "culling on + budget", frames);
  renderer.pointBudget = budget;

  const cullingSanityOk = awayStats.visibleChunks < withCulling.chunkCount;
  const chunkDebug = renderer.debugChunks(3);
  const sortPending = (renderer as unknown as { sortPending: boolean }).sortPending;

  const result = {
    adapter,
    cameraMode: wide ? "wide (whole scene)" : "inside (corner view)",
    sortPendingAtEnd: sortPending,
    chunkDebug,
    cullingSanity: {
      visibleWhenFacingAway: awayStats.visibleChunks,
      chunkCount: withCulling.chunkCount,
      yawSweep,
      ok: cullingSanityOk,
    },
    splatCount: count,
    viewport: { width: canvas.width, height: canvas.height },
    noDraw,
    coverageWithCulling,
    coverageWithoutCulling,
    withCulling,
    withoutCulling,
    budgeted,
    drawCostPerSplatUs:
      withCulling.drawnSplats > 0
        ? ((withCulling.avgFrameMs - noDraw.avgFrameMs) * 1000) / withCulling.drawnSplats
        : 0,
    sortReduction:
      withoutCulling.candidateSplats > 0
        ? 1 - withCulling.candidateSplats / withoutCulling.candidateSplats
        : 0,
    frameReduction:
      withoutCulling.avgFrameMs > 0
        ? 1 - withCulling.avgFrameMs / withoutCulling.avgFrameMs
        : 0,
  };

  log(
    [
      "",
      `adapter: ${adapter}`,
      `chunks: ${withCulling.chunkCount}`,
      `fixed floor   : ${noDraw.avgFrameMs.toFixed(2)} ms`,
      `culling on    : ${withCulling.avgFrameMs.toFixed(2)} ms  drawn ${withCulling.drawnSplats.toLocaleString()} / ${withCulling.candidateSplats.toLocaleString()} candidates  chunks ${withCulling.visibleChunks}/${withCulling.chunkCount}`,
      `culling off   : ${withoutCulling.avgFrameMs.toFixed(2)} ms  drawn ${withoutCulling.drawnSplats.toLocaleString()}`,
      `with budget   : ${budgeted.avgFrameMs.toFixed(2)} ms  stride ${budgeted.budgetStride}  drawn ${budgeted.drawnSplats.toLocaleString()}`,
      `coverage: ${(coverageWithCulling * 100).toFixed(1)}% of pixels (culling on), ${(coverageWithoutCulling * 100).toFixed(1)}% (culling off)`,
      `sort work reduction: ${(result.sortReduction * 100).toFixed(1)}%`,
      `frame reduction    : ${(result.frameReduction * 100).toFixed(1)}%`,
    ].join("\n"),
  );

  (window as unknown as { __BENCH_RESULT__: unknown }).__BENCH_RESULT__ = result;
}

main().catch((err) => {
  log(`ERROR ${String(err)}`);
  (window as unknown as { __BENCH_ERROR__: string }).__BENCH_ERROR__ = String(err);
});
