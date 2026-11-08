/// <reference lib="webworker" />
// Splat worker: parses 3DGS .ply files into a packed GPU texture buffer and
// performs depth sorting for back-to-front alpha blending.
//
// Packed layout per splat (8 uint32 = 2 RGBA32UI texels):
//   [0..2] x,y,z         (float bits)
//   [3]    rgba8 color   (r | g<<8 | b<<16 | a<<24)
//   [4..6] cov 3D upper-triangle (6 half-floats packed pairwise)
//   [7]    unused
//
// Messages in:
//   { type: "parse", buffer: ArrayBuffer, interpolate?: boolean }
//   { type: "lerp", t: number }
//   { type: "sort", viewProj: Float32Array, visibleChunks?: number[] | null, budget?: number }
// Messages out:
//   { type: "parsed", texdata, count, sceneCenter, sceneRadius, canInterpolate, chunks }
//   { type: "lerped", texdata, count, chunks }
//   { type: "sorted", depthIndex, count, candidateCount, stride }

const SH_C0 = 0.28209479177387814;

let centers: Float32Array | null = null;
let splatCount = 0;

/** Unpacked attributes for live interpolation between Brush checkpoints. */
interface AttrCloud {
  count: number;
  pos: Float32Array;
  rgba: Float32Array;
  cov: Float32Array;
  center: [number, number, number];
  radius: number;
}

let prevAttrs: AttrCloud | null = null;
let nextAttrs: AttrCloud | null = null;

/**
 * One spatial chunk of the splat set, in model space.
 * `start`/`count` index into `chunkPerm`, which groups splats by chunk, so a visible chunk is a
 * contiguous range and a candidate list can be assembled without a per-splat scan.
 */
export interface SplatChunkMeta {
  minM: [number, number, number];
  maxM: [number, number, number];
  start: number;
  count: number;
}

let chunkPerm: Uint32Array | null = null;
let chunks: SplatChunkMeta[] = [];

/** Target chunk edge length (m). Smaller = finer culling, larger = cheaper index. */
const TARGET_CHUNK_EXTENT_M = 50;

/**
 * Partition splats into a uniform 3-D grid over the scene bounds.
 *
 * Target: a few hundred chunks regardless of scene size, so per-frame culling costs a few
 * thousand corner tests instead of a per-splat pass, and the per-frame sort only ever sees the
 * visible set (see docs/PERF.md for the measured effect).
 */
function buildChunks(attrs: AttrCloud): { perm: Uint32Array; chunks: SplatChunkMeta[] } {
  const n = attrs.count;
  const pos = attrs.pos;
  if (n === 0) return { perm: new Uint32Array(0), chunks: [] };

  // Chunk granularity is chosen metrically, not by splat count: culling resolution is what
  // matters, and two captures of the same site at different densities should partition the same
  // way. ~50 m chunks are a few seconds of flight and small enough that a ground-level or
  // oblique camera culls most of a multi-hundred-metre site; the 16^3 cap bounds the index.
  const perAxisFor = (extent: number) =>
    Math.min(16, Math.max(1, Math.round(extent / TARGET_CHUNK_EXTENT_M)));
  void n;

  let minX = Infinity, minY = Infinity, minZ = Infinity;
  let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity;
  for (let i = 0; i < n; i++) {
    const x = pos[i * 3], y = pos[i * 3 + 1], z = pos[i * 3 + 2];
    if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) continue;
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (z < minZ) minZ = z;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
    if (z > maxZ) maxZ = z;
  }
  if (!Number.isFinite(minX)) {
    minX = attrs.center[0] - attrs.radius;
    minY = attrs.center[1] - attrs.radius;
    minZ = attrs.center[2] - attrs.radius;
    maxX = attrs.center[0] + attrs.radius;
    maxY = attrs.center[1] + attrs.radius;
    maxZ = attrs.center[2] + attrs.radius;
  }
  const eps = 1e-6;
  const sx = (maxX - minX) || eps;
  const sy = (maxY - minY) || eps;
  const sz = (maxZ - minZ) || eps;
  // One axis count for the whole box, driven by the largest horizontal extent.
  const perAxis = perAxisFor(Math.max(sx, sy));
  const axisCells = perAxis * perAxis * perAxis;

  const cellOf = new Uint32Array(n);
  const cellCounts = new Uint32Array(axisCells);
  for (let i = 0; i < n; i++) {
    const ix = Math.min(perAxis - 1, Math.max(0, Math.floor(((pos[i * 3] - minX) / sx) * perAxis)));
    const iy = Math.min(perAxis - 1, Math.max(0, Math.floor(((pos[i * 3 + 1] - minY) / sy) * perAxis)));
    const iz = Math.min(perAxis - 1, Math.max(0, Math.floor(((pos[i * 3 + 2] - minZ) / sz) * perAxis)));
    const cell = ix + iy * perAxis + iz * perAxis * perAxis;
    cellOf[i] = cell;
    cellCounts[cell]++;
  }

  const startOf = new Uint32Array(axisCells);
  let acc = 0;
  for (let c = 0; c < axisCells; c++) {
    startOf[c] = acc;
    acc += cellCounts[c];
  }
  const cursor = Uint32Array.from(startOf);
  const perm = new Uint32Array(n);
  for (let i = 0; i < n; i++) perm[cursor[cellOf[i]]++] = i;

  const out: SplatChunkMeta[] = [];
  const cellSizeX = sx / perAxis;
  const cellSizeY = sy / perAxis;
  const cellSizeZ = sz / perAxis;
  for (let iz = 0; iz < perAxis; iz++) {
    for (let iy = 0; iy < perAxis; iy++) {
      for (let ix = 0; ix < perAxis; ix++) {
        const cell = ix + iy * perAxis + iz * perAxis * perAxis;
        if (cellCounts[cell] === 0) continue;
        out.push({
          minM: [minX + ix * cellSizeX, minY + iy * cellSizeY, minZ + iz * cellSizeZ],
          maxM: [
            minX + (ix + 1) * cellSizeX,
            minY + (iy + 1) * cellSizeY,
            minZ + (iz + 1) * cellSizeZ,
          ],
          start: startOf[cell],
          count: cellCounts[cell],
        });
      }
    }
  }
  return { perm, chunks: out };
}

function sigmoid(x: number): number {
  return 1 / (1 + Math.exp(-x));
}

const f32buf = new Float32Array(1);
const u32buf = new Uint32Array(f32buf.buffer);
function toHalf(v: number): number {
  f32buf[0] = v;
  const x = u32buf[0];
  const sign = (x >> 16) & 0x8000;
  let exp = ((x >> 23) & 0xff) - 127 + 15;
  let frac = (x >> 13) & 0x3ff;
  if (exp <= 0) return sign;
  if (exp >= 31) {
    exp = 31;
    frac = 0;
  }
  return sign | (exp << 10) | frac;
}
function packHalf2(a: number, b: number): number {
  return toHalf(a) | (toHalf(b) << 16);
}

function packCloud(attrs: AttrCloud): Uint32Array {
  const n = attrs.count;
  const texdata = new Uint32Array(n * 8);
  const texF = new Float32Array(texdata.buffer);
  for (let i = 0; i < n; i++) {
    const i3 = i * 3;
    const i4 = i * 4;
    const i6 = i * 6;
    texF[i * 8] = attrs.pos[i3];
    texF[i * 8 + 1] = attrs.pos[i3 + 1];
    texF[i * 8 + 2] = attrs.pos[i3 + 2];
    const r = Math.max(0, Math.min(255, attrs.rgba[i4] * 255));
    const g = Math.max(0, Math.min(255, attrs.rgba[i4 + 1] * 255));
    const b = Math.max(0, Math.min(255, attrs.rgba[i4 + 2] * 255));
    const a = Math.max(0, Math.min(255, attrs.rgba[i4 + 3] * 255));
    texdata[i * 8 + 3] = (r | (g << 8) | (b << 16) | (a << 24)) >>> 0;
    texdata[i * 8 + 4] = packHalf2(attrs.cov[i6], attrs.cov[i6 + 1]);
    texdata[i * 8 + 5] = packHalf2(attrs.cov[i6 + 2], attrs.cov[i6 + 3]);
    texdata[i * 8 + 6] = packHalf2(attrs.cov[i6 + 4], attrs.cov[i6 + 5]);
  }
  return texdata;
}

function lerpCloud(a: AttrCloud, b: AttrCloud, t: number): AttrCloud {
  const n = Math.min(a.count, b.count);
  const pos = new Float32Array(n * 3);
  const rgba = new Float32Array(n * 4);
  const cov = new Float32Array(n * 6);
  const u = 1 - t;
  for (let i = 0; i < n * 3; i++) pos[i] = a.pos[i] * u + b.pos[i] * t;
  for (let i = 0; i < n * 4; i++) rgba[i] = a.rgba[i] * u + b.rgba[i] * t;
  for (let i = 0; i < n * 6; i++) cov[i] = a.cov[i] * u + b.cov[i] * t;
  return {
    count: n,
    pos,
    rgba,
    cov,
    center: [
      a.center[0] * u + b.center[0] * t,
      a.center[1] * u + b.center[1] * t,
      a.center[2] * u + b.center[2] * t,
    ],
    radius: a.radius * u + b.radius * t,
  };
}

interface PlyProp {
  name: string;
  offset: number;
  size: number;
  reader: (dv: DataView, off: number) => number;
}

function parsePly(buffer: ArrayBuffer): AttrCloud {
  const headerMax = new Uint8Array(buffer, 0, Math.min(64 * 1024, buffer.byteLength));
  const headerText = new TextDecoder().decode(headerMax);
  const endIdx = headerText.indexOf("end_header");
  if (endIdx < 0) throw new Error("not a PLY file");
  const headerLen = headerText.indexOf("\n", endIdx) + 1;
  const header = headerText.slice(0, headerLen);

  let vertexCount = 0;
  const props: PlyProp[] = [];
  let offset = 0;
  const typeSizes: Record<string, number> = {
    float: 4, float32: 4, double: 8, float64: 8,
    uchar: 1, uint8: 1, char: 1, int8: 1,
    ushort: 2, uint16: 2, short: 2, int16: 2,
    uint: 4, uint32: 4, int: 4, int32: 4,
  };
  let inVertex = false;
  for (const line of header.split("\n")) {
    const parts = line.trim().split(/\s+/);
    if (parts[0] === "element") {
      inVertex = parts[1] === "vertex";
      if (inVertex) vertexCount = parseInt(parts[2]);
    } else if (parts[0] === "property" && inVertex) {
      const t = parts[1];
      const size = typeSizes[t] ?? 4;
      const rd =
        t === "float" || t === "float32"
          ? (dv: DataView, off: number) => dv.getFloat32(off, true)
          : t === "double" || t === "float64"
            ? (dv: DataView, off: number) => dv.getFloat64(off, true)
            : t === "uchar" || t === "uint8"
              ? (dv: DataView, off: number) => dv.getUint8(off)
              : (dv: DataView, off: number) => dv.getInt32(off, true);
      props.push({ name: parts[2], offset, size, reader: rd });
      offset += size;
    }
  }
  const stride = offset;
  const dv = new DataView(buffer, headerLen);
  const p: Record<string, PlyProp> = {};
  for (const pr of props) p[pr.name] = pr;

  for (const n of ["x", "y", "z"]) if (!p[n]) throw new Error(`ply missing ${n}`);
  const hasSplat = !!(p["scale_0"] && p["rot_0"] && p["opacity"] && p["f_dc_0"]);

  const pos = new Float32Array(vertexCount * 3);
  const rgba = new Float32Array(vertexCount * 4);
  const cov = new Float32Array(vertexCount * 6);
  let cx = 0,
    cy = 0,
    cz = 0;

  for (let i = 0; i < vertexCount; i++) {
    const base = i * stride;
    const x = p["x"].reader(dv, base + p["x"].offset);
    const y = p["y"].reader(dv, base + p["y"].offset);
    const z = p["z"].reader(dv, base + p["z"].offset);
    pos[i * 3] = x;
    pos[i * 3 + 1] = y;
    pos[i * 3 + 2] = z;
    cx += x;
    cy += y;
    cz += z;

    let r = 0.5,
      g = 0.5,
      b = 0.5,
      a = 1;
    if (hasSplat) {
      r = Math.max(0, Math.min(1, 0.5 + SH_C0 * p["f_dc_0"].reader(dv, base + p["f_dc_0"].offset)));
      g = Math.max(0, Math.min(1, 0.5 + SH_C0 * p["f_dc_1"].reader(dv, base + p["f_dc_1"].offset)));
      b = Math.max(0, Math.min(1, 0.5 + SH_C0 * p["f_dc_2"].reader(dv, base + p["f_dc_2"].offset)));
      a = Math.max(0, Math.min(1, sigmoid(p["opacity"].reader(dv, base + p["opacity"].offset))));

      const sx = Math.exp(p["scale_0"].reader(dv, base + p["scale_0"].offset));
      const sy = Math.exp(p["scale_1"].reader(dv, base + p["scale_1"].offset));
      const sz = Math.exp(p["scale_2"].reader(dv, base + p["scale_2"].offset));
      let qw = p["rot_0"].reader(dv, base + p["rot_0"].offset);
      let qx = p["rot_1"].reader(dv, base + p["rot_1"].offset);
      let qy = p["rot_2"].reader(dv, base + p["rot_2"].offset);
      let qz = p["rot_3"].reader(dv, base + p["rot_3"].offset);
      const qn = Math.hypot(qw, qx, qy, qz) || 1;
      qw /= qn;
      qx /= qn;
      qy /= qn;
      qz /= qn;

      const R = [
        1 - 2 * (qy * qy + qz * qz),
        2 * (qx * qy - qw * qz),
        2 * (qx * qz + qw * qy),
        2 * (qx * qy + qw * qz),
        1 - 2 * (qx * qx + qz * qz),
        2 * (qy * qz - qw * qx),
        2 * (qx * qz - qw * qy),
        2 * (qy * qz + qw * qx),
        1 - 2 * (qx * qx + qy * qy),
      ];
      const M = [
        R[0] * sx,
        R[1] * sy,
        R[2] * sz,
        R[3] * sx,
        R[4] * sy,
        R[5] * sz,
        R[6] * sx,
        R[7] * sy,
        R[8] * sz,
      ];
      cov[i * 6] = M[0] * M[0] + M[1] * M[1] + M[2] * M[2];
      cov[i * 6 + 1] = M[0] * M[3] + M[1] * M[4] + M[2] * M[5];
      cov[i * 6 + 2] = M[0] * M[6] + M[1] * M[7] + M[2] * M[8];
      cov[i * 6 + 3] = M[3] * M[3] + M[4] * M[4] + M[5] * M[5];
      cov[i * 6 + 4] = M[3] * M[6] + M[4] * M[7] + M[5] * M[8];
      cov[i * 6 + 5] = M[6] * M[6] + M[7] * M[7] + M[8] * M[8];
    } else {
      if (p["red"]) {
        r = p["red"].reader(dv, base + p["red"].offset) / 255;
        g = p["green"].reader(dv, base + p["green"].offset) / 255;
        b = p["blue"].reader(dv, base + p["blue"].offset) / 255;
      }
      const s = 0.005;
      cov[i * 6] = s * s;
      cov[i * 6 + 3] = s * s;
      cov[i * 6 + 5] = s * s;
    }
    rgba[i * 4] = r;
    rgba[i * 4 + 1] = g;
    rgba[i * 4 + 2] = b;
    rgba[i * 4 + 3] = a;
  }

  cx /= vertexCount;
  cy /= vertexCount;
  cz /= vertexCount;
  const sampleN = Math.min(vertexCount, 20000);
  const step = Math.max(1, Math.floor(vertexCount / sampleN));
  const dists: number[] = [];
  for (let i = 0; i < vertexCount; i += step) {
    const dx = pos[i * 3] - cx,
      dy = pos[i * 3 + 1] - cy,
      dz = pos[i * 3 + 2] - cz;
    dists.push(Math.sqrt(dx * dx + dy * dy + dz * dz));
  }
  dists.sort((m, n) => m - n);
  const radius = dists[Math.floor(dists.length * 0.9)] || 1;

  return {
    count: vertexCount,
    pos,
    rgba,
    cov,
    center: [cx, cy, cz],
    radius,
  };
}

/**
 * Back-to-front radix sort over a candidate list (all splats, or only the visible ones).
 * `candidates` holds splat indices; `candidateCount` is how many of them to sort.
 */
function sortSplats(
  viewProj: Float32Array,
  candidates: Uint32Array,
  candidateCount: number,
): Uint32Array {
  const n = candidateCount;
  const out = new Uint32Array(n);
  if (!centers || n === 0) return out;
  let minD = Infinity,
    maxD = -Infinity;
  const depths = new Int32Array(n);
  const vp2 = viewProj[2],
    vp6 = viewProj[6],
    vp10 = viewProj[10];
  for (let i = 0; i < n; i++) {
    const sidx = candidates[i];
    const d =
      (vp2 * centers[sidx * 3] + vp6 * centers[sidx * 3 + 1] + vp10 * centers[sidx * 3 + 2]) * 4096;
    depths[i] = d | 0;
    if (d < minD) minD = d;
    if (d > maxD) maxD = d;
  }
  const buckets = 256 * 256;
  const scale = (buckets - 1) / (maxD - minD || 1);
  const counts = new Uint32Array(buckets);
  for (let i = 0; i < n; i++) {
    depths[i] = ((depths[i] - minD) * scale) | 0;
    counts[depths[i]]++;
  }
  const starts = new Uint32Array(buckets);
  let acc = 0;
  for (let i = buckets - 1; i >= 0; i--) {
    starts[i] = acc;
    acc += counts[i];
  }
  for (let i = 0; i < n; i++) out[starts[depths[i]]++] = candidates[i];
  return out;
}

/**
 * Gather the candidate list for the visible chunks, applying the point budget as a uniform
 * stride. A stride is a controlled quality degradation: the frame keeps its full spatial extent
 * instead of collapsing when the visible count exceeds the budget.
 */
function gatherCandidates(
  visibleChunks: number[] | null,
  budget: number,
): { candidates: Uint32Array; count: number; stride: number } {
  const perm = chunkPerm;
  const all = splatCount;
  if (!perm || !visibleChunks || visibleChunks.length === 0) {
    const stride = budget > 0 && all > budget ? Math.ceil(all / budget) : 1;
    const count = Math.floor(all / stride);
    const candidates = new Uint32Array(count);
    for (let i = 0; i < count; i++) candidates[i] = i * stride;
    return { candidates, count, stride };
  }

  let total = 0;
  for (const ci of visibleChunks) {
    const ch = chunks[ci];
    if (ch) total += ch.count;
  }
  if (total === 0) return { candidates: new Uint32Array(0), count: 0, stride: 1 };

  const stride = budget > 0 && total > budget ? Math.ceil(total / budget) : 1;
  const count = Math.floor(total / stride);
  const candidates = new Uint32Array(count);
  let w = 0;
  // Chunks are contiguous ranges of `perm`, so this is a handful of strided range reads.
  for (const ci of visibleChunks) {
    const ch = chunks[ci];
    if (!ch) continue;
    for (let k = 0; k < ch.count; k += stride) {
      if (w >= count) break;
      candidates[w++] = perm[ch.start + k];
    }
    if (w >= count) break;
  }
  return { candidates, count: w, stride };
}

function adopt(attrs: AttrCloud) {
  centers = attrs.pos;
  splatCount = attrs.count;
}

self.onmessage = (e: MessageEvent) => {
  const msg = e.data;
  if (msg.type === "parse") {
    try {
      const parsed = parsePly(msg.buffer);
      const wantLerp = !!msg.interpolate && !!prevAttrs && prevAttrs.count === parsed.count;
      let display: AttrCloud;
      if (wantLerp) {
        nextAttrs = parsed;
        display = prevAttrs!;
      } else {
        prevAttrs = parsed;
        nextAttrs = null;
        display = parsed;
      }
      adopt(display);
      const built = buildChunks(display);
      chunkPerm = built.perm;
      chunks = built.chunks;
      const texdata = packCloud(display);
      (self as unknown as Worker).postMessage(
        {
          type: "parsed",
          texdata,
          count: display.count,
          sceneCenter: parsed.center,
          sceneRadius: parsed.radius,
          canInterpolate: !!wantLerp,
          chunks: built.chunks,
        },
        [texdata.buffer],
      );
    } catch (err) {
      (self as unknown as Worker).postMessage({ type: "error", message: String(err) });
    }
  } else if (msg.type === "lerp") {
    if (!prevAttrs || !nextAttrs) return;
    const t = Math.max(0, Math.min(1, msg.t ?? 1));
    const blended = lerpCloud(prevAttrs, nextAttrs, t);
    adopt(blended);
    // Positions move during a lerp, so the spatial partition is rebuilt here. This runs at
    // checkpoint cadence (a few times per capture), not per frame.
    const built = buildChunks(blended);
    chunkPerm = built.perm;
    chunks = built.chunks;
    const texdata = packCloud(blended);
    if (t >= 1) {
      prevAttrs = nextAttrs;
      nextAttrs = null;
    }
    (self as unknown as Worker).postMessage(
      { type: "lerped", texdata, count: blended.count, chunks: built.chunks },
      [texdata.buffer],
    );
  } else if (msg.type === "sort") {
    try {
      const { candidates, count, stride } = gatherCandidates(
        msg.visibleChunks ?? null,
        typeof msg.budget === "number" ? msg.budget : 0,
      );
      const order = sortSplats(msg.viewProj, candidates, count);
      (self as unknown as Worker).postMessage(
        { type: "sorted", depthIndex: order, count: order.length, candidateCount: count, stride },
        [order.buffer],
      );
    } catch (err) {
      // A sort that never answers leaves the renderer's sortPending stuck, which silently
      // freezes the frame at the previous camera. Always answer.
      (self as unknown as Worker).postMessage({ type: "error", message: `sort: ${String(err)}` });
    }
  }
};
