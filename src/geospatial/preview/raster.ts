import type { GeoWaterStyle } from "../types";
import { H_DRY } from "./softSolver";

/** Hydro cyan → deeper absorption colormap (RGBA). */
export function depthToRgba(
  depthM: number,
  maxDepthRef: number,
  style: GeoWaterStyle,
  hazardClassHint: number,
  out: Uint8ClampedArray,
  offset: number,
): void {
  if (depthM <= H_DRY) {
    out[offset] = 0;
    out[offset + 1] = 0;
    out[offset + 2] = 0;
    out[offset + 3] = 0;
    return;
  }

  if (style === "hazard") {
    const h = hazardClassHint >= 0
      ? hazardClassHint
      : depthM >= 1.6
        ? 3
        : depthM >= 1.0
          ? 2
          : depthM >= 0.35
            ? 1
            : 0;
    const palette = [
      [37, 198, 217, 90],
      [241, 184, 75, 140],
      [229, 93, 120, 170],
      [229, 93, 120, 210],
    ];
    const c = palette[Math.min(3, h)];
    out[offset] = c[0];
    out[offset + 1] = c[1];
    out[offset + 2] = c[2];
    out[offset + 3] = c[3];
    return;
  }

  const t = Math.max(0, Math.min(1, depthM / Math.max(0.15, maxDepthRef)));
  if (style === "contour") {
    // Soft fill; shoreline drawn as a separate GeoJSON stroke.
    out[offset] = 37;
    out[offset + 1] = 198;
    out[offset + 2] = 217;
    out[offset + 3] = Math.round(40 + t * 50);
    return;
  }

  // Depth absorption: pale → Hydro Cyan → Basin Ink tint.
  const r = Math.round(37 + (8 - 37) * t);
  const g = Math.round(198 + (18 - 198) * t * 0.55);
  const b = Math.round(217 + (29 - 217) * t * 0.35);
  const a = Math.round(55 + t * 160);
  out[offset] = r;
  out[offset + 1] = g;
  out[offset + 2] = b;
  out[offset + 3] = a;
}

export function rasterizeDepth(
  h: Float32Array,
  cols: number,
  rows: number,
  style: GeoWaterStyle,
  maxDepthRef: number,
  image?: ImageData,
): ImageData {
  const img =
    image && image.width === cols && image.height === rows
      ? image
      : new ImageData(cols, rows);
  const data = img.data;
  for (let j = 0; j < rows; j++) {
    // ImageData origin is top-left; grid j=0 is south → flip vertically.
    const srcRow = rows - 1 - j;
    for (let i = 0; i < cols; i++) {
      const depth = h[srcRow * cols + i];
      const o = (j * cols + i) * 4;
      depthToRgba(depth, maxDepthRef, style, -1, data, o);
    }
  }
  return img;
}

/**
 * Marching-squares-ish shoreline rings in normalised cell coords [0,1]²
 * (x east, y north). Converted to WGS84 by caller via domain bounds.
 */
export function extractShorelineRings(
  h: Float32Array,
  cols: number,
  rows: number,
  threshold = H_DRY * 4,
): [number, number][][] {
  const rings: [number, number][][] = [];
  // Trace outer boundary of wet cells (cheap contour).
  const visited = new Uint8Array(cols * rows);
  const dirs: [number, number][] = [
    [1, 0],
    [0, 1],
    [-1, 0],
    [0, -1],
  ];

  const wet = (i: number, j: number) =>
    i >= 0 && j >= 0 && i < cols && j < rows && h[j * cols + i] > threshold;

  for (let j = 0; j < rows; j++) {
    for (let i = 0; i < cols; i++) {
      const k = j * cols + i;
      if (!wet(i, j) || visited[k]) continue;
      // Only start on shoreline (has dry neighbour).
      const shoreline =
        !wet(i + 1, j) || !wet(i - 1, j) || !wet(i, j + 1) || !wet(i, j - 1);
      if (!shoreline) continue;

      const ring: [number, number][] = [];
      let ci = i;
      let cj = j;
      let guard = cols * rows;
      while (guard-- > 0) {
        const ck = cj * cols + ci;
        if (visited[ck]) break;
        visited[ck] = 1;
        ring.push([(ci + 0.5) / cols, (cj + 0.5) / rows]);
        let moved = false;
        for (const [di, dj] of dirs) {
          const ni = ci + di;
          const nj = cj + dj;
          if (!wet(ni, nj)) continue;
          const nk = nj * cols + ni;
          if (visited[nk]) continue;
          const edge =
            !wet(ni + 1, nj) ||
            !wet(ni - 1, nj) ||
            !wet(ni, nj + 1) ||
            !wet(ni, nj - 1);
          if (!edge) continue;
          ci = ni;
          cj = nj;
          moved = true;
          break;
        }
        if (!moved) break;
      }
      if (ring.length >= 8) {
        ring.push(ring[0]);
        rings.push(ring);
      }
    }
  }
  return rings.slice(0, 6);
}

export function ringsToLngLat(
  rings: [number, number][][],
  bounds: [number, number, number, number],
): GeoJSON.FeatureCollection {
  const [west, south, east, north] = bounds;
  const features: GeoJSON.Feature[] = rings.map((ring, idx) => ({
    type: "Feature",
    properties: { id: idx },
    geometry: {
      type: "LineString",
      coordinates: ring.map(([xn, yn]) => [
        west + xn * (east - west),
        south + yn * (north - south),
      ]),
    },
  }));
  return { type: "FeatureCollection", features };
}

export interface Particle {
  x: number; // normalised 0–1 east
  y: number; // normalised 0–1 north
  life: number;
}

export function seedParticles(
  h: Float32Array,
  u: Float32Array,
  v: Float32Array,
  cols: number,
  rows: number,
  count: number,
  existing: Particle[] = [],
): Particle[] {
  const out = existing.filter((p) => p.life > 0);
  const need = Math.max(0, count - out.length);
  let attempts = need * 8;
  while (out.length < count && attempts-- > 0) {
    const i = Math.floor(Math.random() * cols);
    const j = Math.floor(Math.random() * rows);
    const k = j * cols + i;
    if (h[k] <= H_DRY) continue;
    if (Math.hypot(u[k], v[k]) < 0.05) continue;
    out.push({
      x: (i + Math.random()) / cols,
      y: (j + Math.random()) / rows,
      life: 0.6 + Math.random() * 1.4,
    });
  }
  return out;
}

export function advanceParticles(
  particles: Particle[],
  h: Float32Array,
  u: Float32Array,
  v: Float32Array,
  cols: number,
  rows: number,
  dtS: number,
  dx: number,
): Particle[] {
  const invDx = 1 / Math.max(1e-6, dx);
  const scale = dtS * invDx;
  for (const p of particles) {
    const i = Math.max(0, Math.min(cols - 1, Math.floor(p.x * cols)));
    const j = Math.max(0, Math.min(rows - 1, Math.floor(p.y * rows)));
    const k = j * cols + i;
    if (h[k] <= H_DRY) {
      p.life = 0;
      continue;
    }
    p.x += (u[k] * scale) / cols;
    p.y += (v[k] * scale) / rows;
    p.life -= dtS / 3600; // scenario-hour life drain when using scenario dt
    if (p.x < 0 || p.x > 1 || p.y < 0 || p.y > 1) p.life = 0;
  }
  return particles.filter((p) => p.life > 0);
}

export function particlesToGeoJson(
  particles: Particle[],
  bounds: [number, number, number, number],
): GeoJSON.FeatureCollection {
  const [west, south, east, north] = bounds;
  return {
    type: "FeatureCollection",
    features: particles.map((p, id) => ({
      type: "Feature",
      properties: { id, life: p.life },
      geometry: {
        type: "Point",
        coordinates: [west + p.x * (east - west), south + p.y * (north - south)],
      },
    })),
  };
}
