/**
 * Local East-North-Up helpers shared by the geo 3D workspace (and safe for
 * reconstruction viewport callers). Prefer importing from here rather than
 * duplicating WGS84↔ENU math inside Viewport.
 */

import type { AoiWgs84 } from "./aoi";
import { aoiCenter, aoiIsValid, metresPerDegree, normalizeAoi } from "./aoi";

export type Vec3 = [number, number, number];

/** Lon/lat/height (degrees, degrees, metres) → local ENU metres about `origin`. */
export function wgs84ToEnu(
  lon: number,
  lat: number,
  h: number,
  origin: [number, number, number],
): Vec3 {
  const m = metresPerDegree(origin[1]);
  return [(lon - origin[0]) * m.lon, (lat - origin[1]) * m.lat, h - (origin[2] ?? 0)];
}

/** Local ENU metres → lon/lat/height about `origin`. */
export function enuToWgs84(
  east: number,
  north: number,
  up: number,
  origin: [number, number, number],
): [number, number, number] {
  const m = metresPerDegree(origin[1]);
  return [origin[0] + east / m.lon, origin[1] + north / m.lat, (origin[2] ?? 0) + up];
}

/** ENU origin + axis-aligned ground bounds for an AOI (flat ellipsoid approx). */
export function aoiEnuFrame(aoi: AoiWgs84): {
  origin: [number, number, number];
  minE: number;
  minN: number;
  maxE: number;
  maxN: number;
  widthM: number;
  heightM: number;
} {
  const box = normalizeAoi(aoi);
  const [lon0, lat0] = aoiCenter(box);
  const origin: [number, number, number] = [lon0, lat0, 0];
  const sw = wgs84ToEnu(box[0], box[1], 0, origin);
  const ne = wgs84ToEnu(box[2], box[3], 0, origin);
  return {
    origin,
    minE: sw[0],
    minN: sw[1],
    maxE: ne[0],
    maxN: ne[1],
    widthM: Math.abs(ne[0] - sw[0]),
    heightM: Math.abs(ne[1] - sw[1]),
  };
}

/** Default framing box when no AOI is set (local metres about origin). */
export function defaultEnuBounds(): {
  origin: [number, number, number];
  minE: number;
  minN: number;
  maxE: number;
  maxN: number;
  widthM: number;
  heightM: number;
} {
  return {
    origin: [0, 20, 0],
    minE: -200,
    minN: -150,
    maxE: 200,
    maxN: 150,
    widthM: 400,
    heightM: 300,
  };
}

export function frameForAoi(aoi: AoiWgs84 | null | undefined) {
  return aoiIsValid(aoi) ? aoiEnuFrame(aoi) : defaultEnuBounds();
}

/** Ray from NDC through an orbit camera (ENU Z-up, splat-style y-down projection). */
export function rayFromNdc(
  ndcX: number,
  ndcY: number,
  eye: Vec3,
  right: Vec3,
  down: Vec3,
  forward: Vec3,
  fovY: number,
  aspect: number,
): { origin: Vec3; dir: Vec3 } {
  const tanHalf = Math.tan(fovY / 2);
  const dir: Vec3 = [
    forward[0] + right[0] * ndcX * tanHalf * aspect + down[0] * ndcY * tanHalf,
    forward[1] + right[1] * ndcX * tanHalf * aspect + down[1] * ndcY * tanHalf,
    forward[2] + right[2] * ndcX * tanHalf * aspect + down[2] * ndcY * tanHalf,
  ];
  const len = Math.hypot(dir[0], dir[1], dir[2]) || 1;
  return {
    origin: eye,
    dir: [dir[0] / len, dir[1] / len, dir[2] / len],
  };
}

/** Intersect ray with horizontal plane z = planeZ. */
export function intersectPlaneZ(
  origin: Vec3,
  dir: Vec3,
  planeZ = 0,
): Vec3 | null {
  if (Math.abs(dir[2]) < 1e-8) return null;
  const t = (planeZ - origin[2]) / dir[2];
  if (t < 0) return null;
  return [origin[0] + dir[0] * t, origin[1] + dir[1] * t, planeZ];
}
