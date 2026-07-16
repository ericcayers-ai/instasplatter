/** Parse binary / ASCII XYZRGB point PLYs written by the live recon preview. */

export interface XyzRgbCloud {
  positions: Float32Array;
  colors: Float32Array;
  count: number;
}

function headerEnd(bytes: Uint8Array): { end: number; header: string } | null {
  const probe = bytes.subarray(0, Math.min(bytes.length, 256 * 1024));
  const text = new TextDecoder("latin1").decode(probe);
  const idx = text.indexOf("end_header");
  if (idx < 0) return null;
  const nl = text.indexOf("\n", idx);
  if (nl < 0) return null;
  return { end: nl + 1, header: text.slice(0, idx) };
}

export function parseXyzRgbPly(buffer: ArrayBuffer): XyzRgbCloud {
  const bytes = new Uint8Array(buffer);
  const meta = headerEnd(bytes);
  if (!meta) throw new Error("Not a PLY file.");
  const { end, header } = meta;

  let vertexCount = 0;
  let binaryLe = false;
  let binaryBe = false;
  let ascii = false;
  const props: { name: string; type: string; offset: number }[] = [];
  let offset = 0;
  let inVertex = false;

  const sizeOf = (t: string) => {
    switch (t) {
      case "float":
      case "float32":
      case "uint":
      case "int":
      case "uint32":
      case "int32":
        return 4;
      case "double":
      case "float64":
        return 8;
      case "uchar":
      case "uint8":
      case "char":
      case "int8":
        return 1;
      case "ushort":
      case "uint16":
      case "short":
      case "int16":
        return 2;
      default:
        return 4;
    }
  };

  for (const line of header.split(/\r?\n/)) {
    const parts = line.trim().split(/\s+/);
    if (parts[0] === "format") {
      if (parts[1] === "binary_little_endian") binaryLe = true;
      else if (parts[1] === "binary_big_endian") binaryBe = true;
      else if (parts[1] === "ascii") ascii = true;
    } else if (parts[0] === "element") {
      inVertex = parts[1] === "vertex";
      if (inVertex) vertexCount = parseInt(parts[2] ?? "0", 10) || 0;
    } else if (parts[0] === "property" && inVertex && parts.length >= 3) {
      const type = parts[1];
      const name = parts[2];
      props.push({ name, type, offset });
      offset += sizeOf(type);
    }
  }

  const stride = offset;
  if (!vertexCount || !stride) throw new Error("Empty point cloud.");

  const find = (n: string) => props.find((p) => p.name === n)?.offset;
  const ox = find("x");
  const oy = find("y");
  const oz = find("z");
  if (ox === undefined || oy === undefined || oz === undefined) {
    throw new Error("PLY missing xyz.");
  }
  const or = find("red") ?? find("r");
  const og = find("green") ?? find("g");
  const ob = find("blue") ?? find("b");

  const positions = new Float32Array(vertexCount * 3);
  const colors = new Float32Array(vertexCount * 4);
  const view = new DataView(buffer, end);

  if (ascii) {
    const text = new TextDecoder().decode(bytes.subarray(end));
    const lines = text.trim().split(/\r?\n/);
    for (let i = 0; i < vertexCount && i < lines.length; i++) {
      const f = lines[i].trim().split(/\s+/).map(Number);
      positions[i * 3] = f[0] ?? 0;
      positions[i * 3 + 1] = f[1] ?? 0;
      positions[i * 3 + 2] = f[2] ?? 0;
      colors[i * 4] = (f[3] ?? 200) / 255;
      colors[i * 4 + 1] = (f[4] ?? 200) / 255;
      colors[i * 4 + 2] = (f[5] ?? 200) / 255;
      colors[i * 4 + 3] = 0.9;
    }
    return { positions, colors, count: vertexCount };
  }

  if (binaryBe) throw new Error("Big-endian PLY is not supported.");
  if (!binaryLe) throw new Error("Unknown PLY format.");

  const readF32 = (base: number, off: number) => view.getFloat32(base + off, true);
  const readU8 = (base: number, off: number) => view.getUint8(base + off);

  for (let i = 0; i < vertexCount; i++) {
    const base = i * stride;
    if (base + stride > view.byteLength) break;
    positions[i * 3] = readF32(base, ox);
    positions[i * 3 + 1] = readF32(base, oy);
    positions[i * 3 + 2] = readF32(base, oz);
    const r = or !== undefined ? readU8(base, or) : 200;
    const g = og !== undefined ? readU8(base, og) : 200;
    const b = ob !== undefined ? readU8(base, ob) : 200;
    colors[i * 4] = r / 255;
    colors[i * 4 + 1] = g / 255;
    colors[i * 4 + 2] = b / 255;
    colors[i * 4 + 3] = 0.85;
  }

  return { positions, colors, count: vertexCount };
}

/** Extract triangle edge wireframe from a simple mesh PLY (x/y/z + list uchar int vertex_indices). */
export function parseMeshWirePly(buffer: ArrayBuffer): Float32Array | null {
  const bytes = new Uint8Array(buffer);
  const meta = headerEnd(bytes);
  if (!meta) return null;
  const { end, header } = meta;
  if (!header.includes("element face") || !header.includes("binary_little_endian")) {
    return null;
  }

  let vertexCount = 0;
  let faceCount = 0;
  let inVertex = false;
  let inFace = false;
  let vertexStride = 0;
  let faceHasList = false;

  for (const line of header.split(/\r?\n/)) {
    const parts = line.trim().split(/\s+/);
    if (parts[0] === "element") {
      inVertex = parts[1] === "vertex";
      inFace = parts[1] === "face";
      if (inVertex) vertexCount = parseInt(parts[2] ?? "0", 10) || 0;
      if (inFace) faceCount = parseInt(parts[2] ?? "0", 10) || 0;
    } else if (parts[0] === "property" && inVertex) {
      const t = parts[1];
      vertexStride += t === "double" || t === "float64" ? 8 : t.startsWith("u") || t === "float" || t === "float32" || t === "int" ? (t.includes("char") || t.includes("8") ? 1 : t.includes("short") || t.includes("16") ? 2 : 4) : 4;
    } else if (parts[0] === "property" && inFace && parts[1] === "list") {
      faceHasList = true;
    }
  }

  if (!vertexCount || !faceCount || !faceHasList || vertexStride < 12) return null;

  const view = new DataView(buffer, end);
  const positions = new Float32Array(vertexCount * 3);
  for (let i = 0; i < vertexCount; i++) {
    const base = i * vertexStride;
    positions[i * 3] = view.getFloat32(base, true);
    positions[i * 3 + 1] = view.getFloat32(base + 4, true);
    positions[i * 3 + 2] = view.getFloat32(base + 8, true);
  }

  let off = vertexCount * vertexStride;
  const edges: number[] = [];
  for (let f = 0; f < faceCount; f++) {
    if (off >= view.byteLength) break;
    const n = view.getUint8(off);
    off += 1;
    const idx: number[] = [];
    for (let k = 0; k < n; k++) {
      idx.push(view.getInt32(off, true));
      off += 4;
    }
    for (let k = 0; k < idx.length; k++) {
      const a = idx[k];
      const b = idx[(k + 1) % idx.length];
      edges.push(
        positions[a * 3],
        positions[a * 3 + 1],
        positions[a * 3 + 2],
        positions[b * 3],
        positions[b * 3 + 1],
        positions[b * 3 + 2],
      );
    }
  }
  return edges.length ? new Float32Array(edges) : null;
}
