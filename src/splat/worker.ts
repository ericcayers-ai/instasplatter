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
//   { type: "parse", buffer: ArrayBuffer }
//   { type: "sort", viewProj: Float32Array }
// Messages out:
//   { type: "parsed", texdata, centers?, count, sceneCenter, sceneRadius }
//   { type: "sorted", depthIndex, count }

const SH_C0 = 0.28209479177387814;

let centers: Float32Array | null = null;
let splatCount = 0;

function sigmoid(x: number): number {
  return 1 / (1 + Math.exp(-x));
}

// IEEE 754 half-float conversion (fast path, no denormal care needed here)
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

interface PlyProp {
  name: string;
  offset: number;
  size: number;
  reader: (dv: DataView, off: number) => number;
}

function parsePly(buffer: ArrayBuffer) {
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

  const need = ["x", "y", "z"];
  for (const n of need) if (!p[n]) throw new Error(`ply missing ${n}`);
  const hasSplat = !!(p["scale_0"] && p["rot_0"] && p["opacity"] && p["f_dc_0"]);

  const texdata = new Uint32Array(vertexCount * 8);
  const texF = new Float32Array(texdata.buffer);
  const ctrs = new Float32Array(vertexCount * 3);

  let cx = 0, cy = 0, cz = 0;

  for (let i = 0; i < vertexCount; i++) {
    const base = i * stride;
    const x = p["x"].reader(dv, base + p["x"].offset);
    const y = p["y"].reader(dv, base + p["y"].offset);
    const z = p["z"].reader(dv, base + p["z"].offset);
    ctrs[i * 3] = x;
    ctrs[i * 3 + 1] = y;
    ctrs[i * 3 + 2] = z;
    cx += x; cy += y; cz += z;
    texF[i * 8] = x;
    texF[i * 8 + 1] = y;
    texF[i * 8 + 2] = z;

    let r = 128, g = 128, b = 128, a = 255;
    if (hasSplat) {
      r = Math.max(0, Math.min(255, (0.5 + SH_C0 * p["f_dc_0"].reader(dv, base + p["f_dc_0"].offset)) * 255));
      g = Math.max(0, Math.min(255, (0.5 + SH_C0 * p["f_dc_1"].reader(dv, base + p["f_dc_1"].offset)) * 255));
      b = Math.max(0, Math.min(255, (0.5 + SH_C0 * p["f_dc_2"].reader(dv, base + p["f_dc_2"].offset)) * 255));
      a = Math.max(0, Math.min(255, sigmoid(p["opacity"].reader(dv, base + p["opacity"].offset)) * 255));

      // covariance = R S Sᵀ Rᵀ
      const sx = Math.exp(p["scale_0"].reader(dv, base + p["scale_0"].offset));
      const sy = Math.exp(p["scale_1"].reader(dv, base + p["scale_1"].offset));
      const sz = Math.exp(p["scale_2"].reader(dv, base + p["scale_2"].offset));
      let qw = p["rot_0"].reader(dv, base + p["rot_0"].offset);
      let qx = p["rot_1"].reader(dv, base + p["rot_1"].offset);
      let qy = p["rot_2"].reader(dv, base + p["rot_2"].offset);
      let qz = p["rot_3"].reader(dv, base + p["rot_3"].offset);
      const qn = Math.hypot(qw, qx, qy, qz) || 1;
      qw /= qn; qx /= qn; qy /= qn; qz /= qn;

      const R = [
        1 - 2 * (qy * qy + qz * qz), 2 * (qx * qy - qw * qz), 2 * (qx * qz + qw * qy),
        2 * (qx * qy + qw * qz), 1 - 2 * (qx * qx + qz * qz), 2 * (qy * qz - qw * qx),
        2 * (qx * qz - qw * qy), 2 * (qy * qz + qw * qx), 1 - 2 * (qx * qx + qy * qy),
      ];
      // M = R * S
      const M = [
        R[0] * sx, R[1] * sy, R[2] * sz,
        R[3] * sx, R[4] * sy, R[5] * sz,
        R[6] * sx, R[7] * sy, R[8] * sz,
      ];
      const c00 = M[0] * M[0] + M[1] * M[1] + M[2] * M[2];
      const c01 = M[0] * M[3] + M[1] * M[4] + M[2] * M[5];
      const c02 = M[0] * M[6] + M[1] * M[7] + M[2] * M[8];
      const c11 = M[3] * M[3] + M[4] * M[4] + M[5] * M[5];
      const c12 = M[3] * M[6] + M[4] * M[7] + M[5] * M[8];
      const c22 = M[6] * M[6] + M[7] * M[7] + M[8] * M[8];
      texdata[i * 8 + 4] = packHalf2(c00, c01);
      texdata[i * 8 + 5] = packHalf2(c02, c11);
      texdata[i * 8 + 6] = packHalf2(c12, c22);
    } else {
      // Plain point cloud: render as small constant blobs.
      if (p["red"]) {
        r = p["red"].reader(dv, base + p["red"].offset);
        g = p["green"].reader(dv, base + p["green"].offset);
        b = p["blue"].reader(dv, base + p["blue"].offset);
      }
      const s = 0.005;
      texdata[i * 8 + 4] = packHalf2(s * s, 0);
      texdata[i * 8 + 5] = packHalf2(0, s * s);
      texdata[i * 8 + 6] = packHalf2(0, s * s);
    }
    texdata[i * 8 + 3] = (r | (g << 8) | (b << 16) | (a << 24)) >>> 0;
  }

  // Robust scene framing: centroid + 90th percentile radius (ignores floaters)
  cx /= vertexCount; cy /= vertexCount; cz /= vertexCount;
  const sampleN = Math.min(vertexCount, 20000);
  const step = Math.max(1, Math.floor(vertexCount / sampleN));
  const dists: number[] = [];
  for (let i = 0; i < vertexCount; i += step) {
    const dx = ctrs[i * 3] - cx, dy = ctrs[i * 3 + 1] - cy, dz = ctrs[i * 3 + 2] - cz;
    dists.push(Math.sqrt(dx * dx + dy * dy + dz * dz));
  }
  dists.sort((m, n) => m - n);
  const radius = dists[Math.floor(dists.length * 0.9)] || 1;

  return { texdata, centers: ctrs, count: vertexCount, center: [cx, cy, cz], radius };
}

function sortSplats(viewProj: Float32Array): Uint32Array {
  const n = splatCount;
  const out = new Uint32Array(n);
  if (!centers || n === 0) return out;
  // 16-bit counting sort on quantized view-space depth (back-to-front)
  let minD = Infinity, maxD = -Infinity;
  const depths = new Int32Array(n);
  const vp2 = viewProj[2], vp6 = viewProj[6], vp10 = viewProj[10];
  for (let i = 0; i < n; i++) {
    const d = (vp2 * centers[i * 3] + vp6 * centers[i * 3 + 1] + vp10 * centers[i * 3 + 2]) * 4096;
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
  // Back-to-front: larger depth (further along +view axis)… splat convention
  // sorts descending depth first.
  let acc = 0;
  for (let i = buckets - 1; i >= 0; i--) {
    starts[i] = acc;
    acc += counts[i];
  }
  for (let i = 0; i < n; i++) out[starts[depths[i]]++] = i;
  return out;
}

self.onmessage = (e: MessageEvent) => {
  const msg = e.data;
  if (msg.type === "parse") {
    try {
      const res = parsePly(msg.buffer);
      centers = res.centers;
      splatCount = res.count;
      (self as unknown as Worker).postMessage(
        {
          type: "parsed",
          texdata: res.texdata,
          count: res.count,
          sceneCenter: res.center,
          sceneRadius: res.radius,
        },
        [res.texdata.buffer],
      );
    } catch (err) {
      (self as unknown as Worker).postMessage({ type: "error", message: String(err) });
    }
  } else if (msg.type === "sort") {
    const order = sortSplats(msg.viewProj);
    (self as unknown as Worker).postMessage({ type: "sorted", depthIndex: order, count: splatCount }, [
      order.buffer,
    ]);
  }
};
