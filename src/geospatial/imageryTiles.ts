/**
 * Stitch Esri World Imagery tiles into a canvas texture for the ENU geo-plane.
 * Falls back to a solid tint when offline / low-power / CORS fails.
 */

import {
  ESRI_WORLD_IMAGERY_TILES,
  ESRI_WORLD_IMAGERY_ATTRIBUTION,
} from "./defaults";
import type { AoiWgs84 } from "./aoi";
import { aoiIsValid, normalizeAoi } from "./aoi";

export { ESRI_WORLD_IMAGERY_ATTRIBUTION };

function lon2tile(lon: number, z: number): number {
  return Math.floor(((lon + 180) / 360) * 2 ** z);
}

function lat2tile(lat: number, z: number): number {
  const rad = (lat * Math.PI) / 180;
  return Math.floor(
    ((1 - Math.log(Math.tan(rad) + 1 / Math.cos(rad)) / Math.PI) / 2) * 2 ** z,
  );
}

function tileUrl(z: number, x: number, y: number): string {
  return ESRI_WORLD_IMAGERY_TILES.replace("{z}", String(z))
    .replace("{x}", String(x))
    .replace("{y}", String(y));
}

function pickZoom(aoi: AoiWgs84, maxTiles: number): number {
  const [west, south, east, north] = normalizeAoi(aoi);
  for (let z = 16; z >= 4; z--) {
    const x0 = lon2tile(west, z);
    const x1 = lon2tile(east, z);
    const y0 = lat2tile(north, z);
    const y1 = lat2tile(south, z);
    const n = (x1 - x0 + 1) * (y1 - y0 + 1);
    if (n <= maxTiles) return z;
  }
  return 4;
}

function solidCanvas(w: number, h: number, rgb: [number, number, number]): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = `rgb(${rgb[0]},${rgb[1]},${rgb[2]})`;
  ctx.fillRect(0, 0, w, h);
  // Subtle grid so the plane reads as ground without imagery.
  ctx.strokeStyle = "rgba(255,255,255,0.08)";
  ctx.lineWidth = 1;
  for (let i = 0; i <= 8; i++) {
    const x = (i / 8) * w;
    const y = (i / 8) * h;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, h);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(w, y);
    ctx.stroke();
  }
  return c;
}

/**
 * Build a canvas covering the AOI with satellite tiles (or a placeholder).
 * `lowPower` caps tile count and prefers a tinted plane.
 */
export async function buildSatelliteCanvas(
  aoi: AoiWgs84 | null | undefined,
  opts?: { lowPower?: boolean; signal?: AbortSignal },
): Promise<{ canvas: HTMLCanvasElement; attribution: string; fromTiles: boolean }> {
  if (!aoiIsValid(aoi) || opts?.lowPower) {
    return {
      canvas: solidCanvas(512, 512, opts?.lowPower ? [42, 58, 48] : [36, 52, 44]),
      attribution: opts?.lowPower ? "Low-power terrain (no live tiles)" : "Draw an AOI for satellite terrain",
      fromTiles: false,
    };
  }

  const box = normalizeAoi(aoi);
  const maxTiles = opts?.lowPower ? 4 : 16;
  const z = pickZoom(box, maxTiles);
  const x0 = lon2tile(box[0], z);
  const x1 = lon2tile(box[2], z);
  const y0 = lat2tile(box[3], z);
  const y1 = lat2tile(box[1], z);
  const cols = x1 - x0 + 1;
  const rows = y1 - y0 + 1;
  const tileSize = 256;
  const canvas = document.createElement("canvas");
  canvas.width = cols * tileSize;
  canvas.height = rows * tileSize;
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = "#24342c";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  let loaded = 0;
  const jobs: Promise<void>[] = [];
  for (let ty = y0; ty <= y1; ty++) {
    for (let tx = x0; tx <= x1; tx++) {
      const url = tileUrl(z, tx, ty);
      jobs.push(
        (async () => {
          try {
            const resp = await fetch(url, { signal: opts?.signal, mode: "cors" });
            if (!resp.ok) return;
            const blob = await resp.blob();
            if (opts?.signal?.aborted) return;
            const bmp = await createImageBitmap(blob);
            ctx.drawImage(bmp, (tx - x0) * tileSize, (ty - y0) * tileSize);
            bmp.close();
            loaded++;
          } catch {
            // leave placeholder cell
          }
        })(),
      );
    }
  }
  await Promise.all(jobs);

  if (loaded === 0) {
    return {
      canvas: solidCanvas(512, 512, [36, 52, 44]),
      attribution: "Satellite tiles unavailable — placeholder terrain",
      fromTiles: false,
    };
  }

  return {
    canvas,
    attribution: ESRI_WORLD_IMAGERY_ATTRIBUTION.replace(/<[^>]+>/g, ""),
    fromTiles: true,
  };
}
