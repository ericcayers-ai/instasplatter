/**
 * Sun-driven globe lighting (Photorealism merge Item 3).
 *
 * The Rust side (`src-tauri/src/geospatial/sun.rs`) is the source of truth
 * for astronomical sun position; this module asks it over IPC and converts
 * the returned ENU direction to the ECEF light vector Cesium's
 * DirectionalLight expects. When IPC is unavailable (plain `npm run dev`
 * browser preview), a local mirror of the same NOAA/Meeus low-precision
 * math runs in TypeScript so visual verification works without the shell.
 */

import { Cartesian3, DirectionalLight, type Viewer } from "cesium";

export interface SunFix {
  azimuthDeg: number;
  elevationDeg: number;
  /** Unit direction toward the sun in the observer's ENU frame. */
  east: number;
  north: number;
  up: number;
}

/** NOAA / Meeus low-precision mirror of geospatial::sun (fallback only). */
export function sunFixLocal(
  latDeg: number,
  lonDeg: number,
  date: Date,
): SunFix {
  const RAD = Math.PI / 180;
  const hourUtc =
    date.getUTCHours() +
    date.getUTCMinutes() / 60 +
    (date.getUTCSeconds() + date.getUTCMilliseconds() / 1000) / 3600;
  let y = date.getUTCFullYear();
  let m = date.getUTCMonth() + 1;
  const d =
    date.getUTCDate() - 0.5 + hourUtc / 24; // JD at midday-ish handled below
  if (m <= 2) {
    y -= 1;
    m += 12;
  }
  const a = Math.floor(y / 100);
  const b = 2 - a + Math.floor(a / 4);
  const jd =
    Math.floor(365.25 * (y + 4716)) +
    Math.floor(30.6001 * (m + 1)) +
    d +
    b -
    1524.5;
  void d;

  const t = (jd - 2451545.0) / 36525;
  const l0 = (280.46646 + t * (36000.76983 + 0.0003032 * t)) % 360;
  const mAnom = (357.52911 + t * (35999.05029 - 0.0001537 * t)) * RAD;
  const ecc = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
  const cDeg =
    (1.914602 - t * (0.004817 + 0.000014 * t)) * Math.sin(mAnom) +
    (0.019993 - 0.000101 * t) * Math.sin(2 * mAnom) +
    0.000289 * Math.sin(3 * mAnom);
  const omega = (125.04 - 1934.136 * t) * RAD;
  const lambda =
    ((l0 + cDeg - 0.00569 - 0.00478 * Math.sin(omega)) % 360) * RAD;
  const secs = 21.448 - t * (46.815 + 0.00059 * t) + t * t * 0.001813;
  const eps0 = (23 + (26 + secs / 60) / 60) * RAD;
  const eps = eps0 + 0.00256 * Math.cos(omega);

  const dec = Math.asin(Math.sin(eps) * Math.sin(lambda));
  const yArg = Math.tan(eps / 2) ** 2;
  const twoL0 = (2 * l0) * RAD;
  const eotMin =
    4 *
    RAD *
    (yArg * Math.sin(twoL0) -
      2 * ecc * Math.sin(mAnom) +
      4 * ecc * yArg * Math.sin(mAnom) * Math.cos(twoL0) -
      0.5 * yArg ** 2 * Math.sin(2 * twoL0) -
      1.25 * ecc ** 2 * Math.sin(2 * mAnom));

  const tst = hourUtc * 60 + eotMin + 4 * lonDeg;
  const ha = (tst / 4 - 180) * RAD;
  const phi = latDeg * RAD;

  const up = Math.sin(phi) * Math.sin(dec) + Math.cos(phi) * Math.cos(dec) * Math.cos(ha);
  const east = -Math.cos(dec) * Math.sin(ha);
  const north = Math.cos(phi) * Math.sin(dec) - Math.sin(phi) * Math.cos(dec) * Math.cos(ha);

  let az = (Math.atan2(east, north) / RAD + 360) % 360;
  az = Number.isFinite(az) ? az : 0;
  return {
    azimuthDeg: az,
    elevationDeg: Math.asin(Math.max(-1, Math.min(1, up))) / RAD,
    east,
    north,
    up,
  };
}

async function invokeSun(
  latDeg: number,
  lonDeg: number,
  date: Date,
): Promise<SunFix | null> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const res = await invoke<{
      azimuthDeg: number;
      elevationDeg: number;
      directionEnu: { east: number; north: number; up: number };
    }>("get_sun_position", {
      latDeg,
      lonDeg,
      year: date.getUTCFullYear(),
      month: date.getUTCMonth() + 1,
      day: date.getUTCDate(),
      hourUtc:
        date.getUTCHours() +
        date.getUTCMinutes() / 60 +
        date.getUTCSeconds() / 3600,
    });
    return {
      azimuthDeg: res.azimuthDeg,
      elevationDeg: res.elevationDeg,
      east: res.directionEnu.east,
      north: res.directionEnu.north,
      up: res.directionEnu.up,
    };
  } catch {
    return null; // not running inside Tauri (or command missing)
  }
}

/**
 * Resolve the sun fix for an observer location/instant, preferring the Rust
 * astronomy command and falling back to the in-page mirror.
 */
export async function resolveSunFix(
  latDeg: number,
  lonDeg: number,
  when: Date = new Date(),
): Promise<SunFix> {
  return (await invokeSun(latDeg, lonDeg, when)) ?? sunFixLocal(latDeg, lonDeg, when);
}

/**
 * Convert an ENU direction at (latDeg, lonDeg) to an ECEF unit vector using
 * the same basis convention as src-tauri/src/geospatial/transforms.rs.
 */
export function enuToEcefDirection(fix: SunFix, latDeg: number, lonDeg: number): [number, number, number] {
  const sinLon = Math.sin(lonDeg * (Math.PI / 180));
  const cosLon = Math.cos(lonDeg * (Math.PI / 180));
  const sinLat = Math.sin(latDeg * (Math.PI / 180));
  const cosLat = Math.cos(latDeg * (Math.PI / 180));
  // Columns of R are the ENU axes in ECEF.
  const east = [-sinLon, cosLon, 0];
  const north = [-sinLat * cosLon, -sinLat * sinLon, cosLat];
  const up = [cosLat * cosLon, cosLat * sinLon, sinLat];
  return [
    east[0] * fix.east + north[0] * fix.north + up[0] * fix.up,
    east[1] * fix.east + north[1] * fix.north + up[1] * fix.up,
    east[2] * fix.east + north[2] * fix.north + up[2] * fix.up,
  ];
}

/**
 * Point Cesium's directional light along the astronomically-correct sun
 * direction for (latDeg, lonDeg) at `when`, with atmosphere + fog enabled.
 */
export async function applySunDrivenLighting(
  viewer: Viewer,
  latDeg: number,
  lonDeg: number,
  when: Date = new Date(),
): Promise<void> {
  const scene = viewer.scene;
  scene.globe.enableLighting = true;
  scene.globe.dynamicAtmosphereLighting = true;
  scene.globe.dynamicAtmosphereLightingFromSun = false; // we drive the light
  if (scene.skyAtmosphere) scene.skyAtmosphere.show = true;
  scene.fog.enabled = true;
  scene.fog.density = 2.0e-4;

  const fix = await resolveSunFix(latDeg, lonDeg, when);
  if (viewer.isDestroyed()) return;
  const ecefDir = enuToEcefDirection(fix, latDeg, lonDeg);
  scene.light = new DirectionalLight({
    direction: new Cartesian3(ecefDir[0], ecefDir[1], ecefDir[2]),
    intensity: 2.0,
  });
  scene.requestRender();
}
