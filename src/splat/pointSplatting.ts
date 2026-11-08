/**
 * Gaussian Point Splatting — reference implementation of the sorting-free rasteriser from
 * `JorisAR/gaussian-point-splatting` (SIGGRAPH 2026), for InstaSplatter's large geospatial scenes.
 *
 * Status: **reference / CPU validation path. Not the shipped renderer.**
 * The shipped viewport is the WebGL2 EWA rasteriser in `renderer.ts`. This module exists to
 * (a) keep the sorting-free algorithm executable and testable, and (b) hold the WGSL for a future
 * WebGPU backend. The decision record is `docs/POINT-SPLATTING-VERDICT.md`.
 *
 * Idea
 * ----
 * Alpha-blended 3DGS needs a global back-to-front order of every Gaussian each frame. At 10^8
 * Gaussians that sort is the bottleneck, and order changes show up as popping. Gaussian Point
 * Splatting removes the sort: each Gaussian is decomposed into a handful of **opaque points**
 * sampled from its density, `N_i` proportional to opacity and projected area, and each point
 * writes into a depth-resolved visibility buffer. Transparency comes from the *sampling*, not
 * from blending, so the result is order-independent by construction.
 *
 * Corrections applied to the first version of this file (see the verdict doc for evidence)
 * ---------------------------------------------------------------------------------------
 * 1. **Box–Muller z component.** It returned `r2*cos(theta2)`; the third normal component must be
 *    `r2*sin(theta2)`. With cosine in both places the x and z coordinates were perfectly
 *    correlated, so "Gaussian samples" formed a plane-like sheet rather than an isotropic cloud.
 * 2. **Single-sample noise.** One point per pixel is a Bernoulli sample of coverage: a 30%
 *    opaque splat is either fully opaque or absent, so the image is binary-noisy, not 30%
 *    transparent. The resolve now averages `samplesPerPixel^2` jittered samples, which is what
 *    makes the sampling an actual estimate of opacity.
 * 3. **The WGSL "64-bit atomic visibility buffer" could not run.** WGSL has no `u64` type, and
 *    `atomicMin(depth)` followed by a separate colour write is a race: a farther point that wins
 *    the min early can overwrite a nearer point's colour written later. The shader below is a
 *    two-pass design (depth pass, then colour-resolve pass that compares depth) over `u32` depth,
 *    and the header states the required adapter feature.
 */

import type { Vec3 } from "./camera";

export interface GaussianPrimitive3D {
  /** Center position in local ENU or model space (metres). */
  center: Vec3;
  /** 3D scale along principal axes (metres, positive). */
  scale: Vec3;
  /** Unit quaternion [w, x, y, z]. */
  rotation: [number, number, number, number];
  /** Opacity in [0, 1]. */
  opacity: number;
  /** sRGB color in [0, 255]. */
  colorRgba: [number, number, number, number];
}

export interface GeospatialSplatChunk {
  chunkId: string;
  minEnu: Vec3;
  maxEnu: Vec3;
  splats: GaussianPrimitive3D[];
}

export interface PointSplattingConfig {
  width: number;
  height: number;
  focalX: number;
  focalY: number;
  nearClipM: number;
  farClipM: number;
  /** Base point density multiplier per pixel of projected Gaussian radius. */
  pointBudgetScale: number;
  /** Hard cap on discrete points emitted by a single Gaussian per frame. */
  maxPointsPerGaussian: number;
  /** Minimum opacity required for a Gaussian to emit points. */
  minOpacityCutoff: number;
  /**
   * Jittered samples per pixel axis. 2 means 4 samples: the per-pixel coverage is the mean of the
   * samples, which is what converts "0 of 4 samples hit" into 0%, 25%, 50% transparency instead of
   * the binary hit/miss a single sample produces.
   */
  samplesPerPixel: number;
}

export const DEFAULT_POINT_SPLATTING_CONFIG: PointSplattingConfig = {
  width: 1280,
  height: 720,
  focalX: 960,
  focalY: 960,
  nearClipM: 0.25,
  farClipM: 50_000,
  pointBudgetScale: 1.5,
  maxPointsPerGaussian: 32,
  minOpacityCutoff: 0.05,
  samplesPerPixel: 2,
};

/**
 * Pack a positive camera-space depth (float32) and RGBA8 color into a single 64-bit unsigned integer
 * (`BigInt`) such that integer `<` (`atomicMin`) strictly orders by nearest depth first.
 */
export function packDepthColor64(
  depthM: number,
  r: number,
  g: number,
  b: number,
  a = 255,
): bigint {
  const clampedDepth = Math.max(1e-5, Math.min(1e9, depthM));
  const f32 = new Float32Array([clampedDepth]);
  const u32 = new Uint32Array(f32.buffer);
  const depthBits = BigInt(u32[0] >>> 0);
  const rgbaBits = BigInt(
    ((r & 0xff) | ((g & 0xff) << 8) | ((b & 0xff) << 16) | ((a & 0xff) << 24)) >>> 0,
  );
  return (depthBits << 32n) | rgbaBits;
}

/**
 * Unpack a 64-bit atomic visibility word back into `{ depthM, rgba }`.
 */
export function unpackDepthColor64(packed: bigint): {
  depthM: number;
  rgba: [number, number, number, number];
} {
  if (packed === 0xffffffffffffffffn) {
    return { depthM: Number.POSITIVE_INFINITY, rgba: [0, 0, 0, 0] };
  }
  const depthU32 = Number((packed >> 32n) & 0xffffffffn) >>> 0;
  const rgbaU32 = Number(packed & 0xffffffffn) >>> 0;
  const u32 = new Uint32Array([depthU32]);
  const f32 = new Float32Array(u32.buffer);
  return {
    depthM: f32[0],
    rgba: [
      rgbaU32 & 0xff,
      (rgbaU32 >>> 8) & 0xff,
      (rgbaU32 >>> 16) & 0xff,
      (rgbaU32 >>> 24) & 0xff,
    ],
  };
}

/**
 * Deterministic Van der Corput / Halton radical inverse for quasi-Monte Carlo Gaussian sampling.
 */
export function haltonRadicalInverse(index: number, base: number): number {
  let result = 0;
  let f = 1 / base;
  let i = Math.max(0, Math.floor(index));
  while (i > 0) {
    result += f * (i % base);
    i = Math.floor(i / base);
    f /= base;
  }
  return result;
}

/**
 * Map uniform [0,1)^3 Halton sample to 3D standard normal N(0, I) via Box-Muller transform.
 */
export function sampleStandardNormal3D(sampleIndex: number, seedOffset = 0): Vec3 {
  const idx = sampleIndex + 1 + seedOffset * 31;
  const u1 = Math.max(1e-6, Math.min(1 - 1e-6, haltonRadicalInverse(idx, 2)));
  const u2 = haltonRadicalInverse(idx, 3);
  const u3 = Math.max(1e-6, Math.min(1 - 1e-6, haltonRadicalInverse(idx, 5)));
  const u4 = haltonRadicalInverse(idx, 7);

  const r1 = Math.sqrt(-2.0 * Math.log(u1));
  const theta1 = 2.0 * Math.PI * u2;
  const r2 = Math.sqrt(-2.0 * Math.log(u3));
  const theta2 = 2.0 * Math.PI * u4;

  // Two independent standard normals per Box-Muller pair: (r1 cos t1, r1 sin t1) and
  // (r2 cos t2, r2 sin t2). The third component must use sin, not a second cos.
  return [r1 * Math.cos(theta1), r1 * Math.sin(theta1), r2 * Math.sin(theta2)];
}

/**
 * Rotate a 3D vector by a unit quaternion [w, x, y, z].
 */
export function rotateByQuat(v: Vec3, q: [number, number, number, number]): Vec3 {
  const [w, x, y, z] = q;
  const tx = 2 * (y * v[2] - z * v[1]);
  const ty = 2 * (z * v[0] - x * v[2]);
  const tz = 2 * (x * v[1] - y * v[0]);
  return [
    v[0] + w * tx + (y * tz - z * ty),
    v[1] + w * ty + (z * tx - x * tz),
    v[2] + w * tz + (x * ty - y * tx),
  ];
}

/**
 * Compute the adaptive number of discrete points N_i to emit for a Gaussian at depth `camZ`.
 * Follows SIGGRAPH '26 Gaussian Point Splatting: N_i scales with opacity and projected screen radius.
 */
export function computeAdaptivePointCount(
  splat: GaussianPrimitive3D,
  camZ: number,
  config: PointSplattingConfig = DEFAULT_POINT_SPLATTING_CONFIG,
): number {
  if (splat.opacity < config.minOpacityCutoff || camZ <= config.nearClipM || camZ > config.farClipM) {
    return 0;
  }
  const maxScale = Math.max(splat.scale[0], splat.scale[1], splat.scale[2], 1e-4);
  const projectedRadiusPx = (maxScale * 0.5 * (config.focalX + config.focalY)) / camZ;
  const rawCount = Math.ceil(
    config.pointBudgetScale * splat.opacity * Math.max(1.0, Math.min(64.0, projectedRadiusPx)),
  );
  return Math.max(1, Math.min(config.maxPointsPerGaussian, rawCount));
}

/**
 * Hierarchical frustum/distance culling over geospatial splat chunks before point splatting.
 */
export function cullGeospatialSplatChunks(
  chunks: readonly GeospatialSplatChunk[],
  cameraEyeEnu: Vec3,
  cameraForwardEnu: Vec3,
  maxDistanceM: number,
  cosHalfFov = 0.25,
): GeospatialSplatChunk[] {
  const visible: GeospatialSplatChunk[] = [];
  const fLen = Math.hypot(cameraForwardEnu[0], cameraForwardEnu[1], cameraForwardEnu[2]) || 1;
  const fx = cameraForwardEnu[0] / fLen;
  const fy = cameraForwardEnu[1] / fLen;
  const fz = cameraForwardEnu[2] / fLen;

  for (const chunk of chunks) {
    const cx = 0.5 * (chunk.minEnu[0] + chunk.maxEnu[0]);
    const cy = 0.5 * (chunk.minEnu[1] + chunk.maxEnu[1]);
    const cz = 0.5 * (chunk.minEnu[2] + chunk.maxEnu[2]);
    const rx = 0.5 * Math.abs(chunk.maxEnu[0] - chunk.minEnu[0]);
    const ry = 0.5 * Math.abs(chunk.maxEnu[1] - chunk.minEnu[1]);
    const rz = 0.5 * Math.abs(chunk.maxEnu[2] - chunk.minEnu[2]);
    const boundingRadius = Math.hypot(rx, ry, rz);

    const dx = cx - cameraEyeEnu[0];
    const dy = cy - cameraEyeEnu[1];
    const dz = cz - cameraEyeEnu[2];
    const dist = Math.hypot(dx, dy, dz);

    if (dist - boundingRadius > maxDistanceM) {
      continue;
    }
    if (dist > boundingRadius * 1.5) {
      const cosAngle = (dx * fx + dy * fy + dz * fz) / Math.max(dist, 1e-6);
      const angularSlack = boundingRadius / Math.max(dist, 1e-6);
      if (cosAngle + angularSlack < cosHalfFov) {
        continue;
      }
    }
    visible.push(chunk);
  }
  return visible;
}

/**
 * Execute sorting-free stochastic Gaussian Point Splatting into a 64-bit visibility word per
 * sample, then resolve the samples into an RGBA image.
 *
 * The visibility word is `(depthBits << 32) | rgba8`, packed from the raw IEEE-754 bits of a
 * **positive** float. Positive floats compare the same as their bit patterns, so a plain integer
 * `min` orders by depth — that is the whole trick, and it needs no sort.
 *
 * Order-independence: the input array can be permuted arbitrarily and the output is identical,
 * because `min` over the same set of values is commutative. The smoke test asserts this.
 *
 * Coverage/transparency: `samplesPerPixel^2` jittered sub-pixel samples per pixel are resolved into
 * a coverage-weighted colour with alpha = hits/samples. A single sample per pixel would render a
 * 30%-opaque splat as either fully opaque or absent.
 */
export function rasterizeGaussianPointSplatting(
  splats: readonly GaussianPrimitive3D[],
  viewMatrixColMajor: ArrayLike<number>,
  config: PointSplattingConfig = DEFAULT_POINT_SPLATTING_CONFIG,
): {
  rgbaBuffer: Uint8ClampedArray;
  depthBufferM: Float32Array;
  emittedPoints: number;
  /** Fraction of pixels that received at least one sample. */
  coverageFraction: number;
  samplesPerPixel: number;
} {
  const { width, height, focalX, focalY, nearClipM, farClipM } = config;
  const sppAxis = Math.max(1, Math.min(4, Math.floor(config.samplesPerPixel)));
  const samples = sppAxis * sppAxis;
  const pixelCount = width * height;
  const cx = width * 0.5;
  const cy = height * 0.5;

  // Accumulate premultiplied colour and hit count per pixel across samples.
  const accumR = new Float64Array(pixelCount);
  const accumG = new Float64Array(pixelCount);
  const accumB = new Float64Array(pixelCount);
  const hits = new Uint16Array(pixelCount);
  const depthBufferM = new Float32Array(pixelCount);
  depthBufferM.fill(Number.POSITIVE_INFINITY);

  // Sub-pixel jitter offsets (Halton base 2 and 3, centred on the pixel).
  const offsetsX = new Float64Array(samples);
  const offsetsY = new Float64Array(samples);
  for (let k = 0; k < samples; k++) {
    offsetsX[k] = (haltonRadicalInverse(k + 1, 2) - 0.5) / sppAxis;
    offsetsY[k] = (haltonRadicalInverse(k + 1, 3) - 0.5) / sppAxis;
  }
  const visibility64 = new BigUint64Array(pixelCount);
  let emittedPoints = 0;

  for (let sample = 0; sample < samples; sample++) {
    visibility64.fill(0xffffffffffffffffn);
    const ox = offsetsX[sample];
    const oy = offsetsY[sample];

    for (let sIdx = 0; sIdx < splats.length; sIdx++) {
      const splat = splats[sIdx];
      if (!(splat.opacity >= config.minOpacityCutoff)) continue;

      const mx = splat.center[0];
      const my = splat.center[1];
      const mz = splat.center[2];
      const camZ =
        viewMatrixColMajor[2] * mx +
        viewMatrixColMajor[6] * my +
        viewMatrixColMajor[10] * mz +
        viewMatrixColMajor[14];
      if (camZ <= nearClipM || camZ > farClipM) continue;

      const nPoints = computeAdaptivePointCount(splat, camZ, config);
      if (nPoints === 0) continue;
      const [r, g, b, a] = splat.colorRgba;

      // Deterministic per-Gaussian seed so the same splat set renders identically each frame.
      const seed =
        Math.abs(
          Math.imul(Math.round(mx * 100), 73856093) ^
            Math.imul(Math.round(my * 100), 19349663) ^
            Math.imul(Math.round(mz * 100), 83492791),
        ) & 0xffff;

      for (let p = 0; p < nPoints; p++) {
        // Stochastic opacity gate: a point survives with probability = opacity.
        const xiAlpha = haltonRadicalInverse(p + 1 + seed + sample * 977, 11);
        if (xiAlpha > splat.opacity) continue;

        const localNormal = sampleStandardNormal3D(p + sample * 131, seed);
        const scaled: Vec3 = [
          localNormal[0] * splat.scale[0],
          localNormal[1] * splat.scale[1],
          localNormal[2] * splat.scale[2],
        ];
        const offsetWorld = rotateByQuat(scaled, splat.rotation);
        const pxWorld = mx + offsetWorld[0];
        const pyWorld = my + offsetWorld[1];
        const pzWorld = mz + offsetWorld[2];

        const xCam =
          viewMatrixColMajor[0] * pxWorld +
          viewMatrixColMajor[4] * pyWorld +
          viewMatrixColMajor[8] * pzWorld +
          viewMatrixColMajor[12];
        const yCam =
          viewMatrixColMajor[1] * pxWorld +
          viewMatrixColMajor[5] * pyWorld +
          viewMatrixColMajor[9] * pzWorld +
          viewMatrixColMajor[13];
        const zCam =
          viewMatrixColMajor[2] * pxWorld +
          viewMatrixColMajor[6] * pyWorld +
          viewMatrixColMajor[10] * pzWorld +
          viewMatrixColMajor[14];
        if (zCam <= nearClipM || zCam > farClipM) continue;

        const u = Math.round(cx + (xCam * focalX) / zCam + ox);
        const v = Math.round(cy + (yCam * focalY) / zCam + oy);
        if (u < 0 || u >= width || v < 0 || v >= height) continue;

        const pixIdx = v * width + u;
        const packed = packDepthColor64(zCam, r, g, b, a);
        if (packed < visibility64[pixIdx]) visibility64[pixIdx] = packed;
        emittedPoints++;
      }
    }

    for (let i = 0; i < pixelCount; i++) {
      const word = visibility64[i];
      if (word === 0xffffffffffffffffn) continue;
      const { depthM, rgba } = unpackDepthColor64(word);
      accumR[i] += rgba[0];
      accumG[i] += rgba[1];
      accumB[i] += rgba[2];
      hits[i]++;
      if (depthM < depthBufferM[i]) depthBufferM[i] = depthM;
    }
  }

  const rgbaBuffer = new Uint8ClampedArray(pixelCount * 4);
  let covered = 0;
  for (let i = 0; i < pixelCount; i++) {
    const n = hits[i];
    if (n === 0) continue;
    covered++;
    const base = i * 4;
    rgbaBuffer[base] = accumR[i] / n;
    rgbaBuffer[base + 1] = accumG[i] / n;
    rgbaBuffer[base + 2] = accumB[i] / n;
    // Coverage (not 255): the fraction of jittered samples that saw this surface.
    rgbaBuffer[base + 3] = Math.round((n / samples) * 255);
  }

  return {
    rgbaBuffer,
    depthBufferM,
    emittedPoints,
    coverageFraction: covered / pixelCount,
    samplesPerPixel: samples,
  };
}

/**
 * WebGPU WGSL for the sorting-free visibility pass.
 *
 * Recorded from a corrected design, not the original text. Two facts drive the shape of it:
 *
 * 1. **WGSL has no `u64`.** There is no 64-bit integer type and browsers do not expose
 *    `shader-int64`; the optional `atomic-vec2u-min-max` feature gives `atomic<vec2<u32>>` with
 *    `atomicStoreMin`, but it is adapter-dependent and NOT in the default WebGPU feature set. So the
 *    "64-bit atomic depth+color word" the CPU path uses is replaced here by a **32-bit depth pass**
 *    followed by a **colour resolve pass**, and the adapter feature is requested only as an
 *    optimisation.
 * 2. **`atomicMin(depth)` followed by a separate colour write is a race.** A point that wins the
 *    min early can have its colour overwritten by a farther point that writes later. Two passes
 *    remove the race: the depth pass is the only atomic step, and the colour pass writes only where
 *    the stored depth equals the point's depth.
 *
 * Dispatch: one workgroup of 64 per point, `splatCount / 64` workgroups, repeated once per
 * sub-pixel jitter sample with `jitter` updated between dispatches. A final fullscreen pass reads
 * `visibility` and averages the accumulated colour by the hit count.
 */
export const WGSL_GAUSSIAN_POINT_SPLATTING_SHADER = /* wgsl */ `
struct PointSplatUniforms {
  viewMatrix : mat4x4<f32>,
  width      : u32,
  height     : u32,
  focalX     : f32,
  focalY     : f32,
  nearClipM  : f32,
  farClipM   : f32,
  splatCount : u32,
  jitterX    : f32,
  jitterY    : f32,
  pass       : u32,   // 0 = depth pass, 1 = colour resolve pass
  pad0       : u32,
  pad1       : u32,
};

@group(0) @binding(0) var<uniform> uniforms : PointSplatUniforms;
@group(0) @binding(1) var<storage, read> points : array<vec4<f32>>;       // xyz + packed rgba8
@group(0) @binding(2) var<storage, read_write> depthBits : array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> colorOut  : array<u32>;

fn project(p : vec3<f32>) -> vec3<f32> {
  let cam = uniforms.viewMatrix * vec4<f32>(p, 1.0);
  return cam.xyz;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid : vec3<u32>) {
  let idx = gid.x;
  if (idx >= uniforms.splatCount) { return; }
  let pt = points[idx];
  let cam = project(pt.xyz);
  if (cam.z <= uniforms.nearClipM || cam.z > uniforms.farClipM) { return; }

  let u = i32(round(f32(uniforms.width) * 0.5 + (cam.x * uniforms.focalX) / cam.z + uniforms.jitterX));
  let v = i32(round(f32(uniforms.height) * 0.5 + (cam.y * uniforms.focalY) / cam.z + uniforms.jitterY));
  if (u < 0 || u >= i32(uniforms.width) || v < 0 || v >= i32(uniforms.height)) { return; }
  let pix = u32(v) * uniforms.width + u32(u);

  // Positive floats order the same as their bit patterns, so bitcast depth works with atomicMin.
  let bits = bitcast<u32>(cam.z);
  if (uniforms.pass == 0u) {
    atomicMin(&depthBits[pix], bits);
  } else {
    // Second pass: only the point that produced the winning depth may write the colour, so the
    // depth/colour pair is consistent without a 64-bit atomic word.
    if (depthBits[pix] == bits) {
      colorOut[pix] = u32(pt.w);
    }
  }
}
`;
