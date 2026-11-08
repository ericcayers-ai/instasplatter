/**
 * AOI SingleTile imagery drapes on the globe — flood depth + DEM relief
 * (elevation tint / hillshade). Not MapLibre canvas-drape→terrain.
 *
 * Prefer capped sample grids / soft-preview canvases. Heavy work is scheduled
 * via requestIdleCallback (or setTimeout fallback) so AOI commit stays responsive.
 */

import {
  ImageryLayer,
  Rectangle,
  SingleTileImageryProvider,
  type Viewer,
} from "cesium";
import type { PreviewRenderArtifacts } from "../preview";
import type { DemHeightmapGrid } from "./terrain";

type TaggedLayer = ImageryLayer & {
  __isFlood?: boolean;
  __isDemRelief?: boolean;
};

function syncCanvas(canvas: HTMLCanvasElement, image: ImageData): void {
  if (canvas.width !== image.width || canvas.height !== image.height) {
    canvas.width = image.width;
    canvas.height = image.height;
  }
  const ctx = canvas.getContext("2d");
  if (ctx) ctx.putImageData(image, 0, 0);
}

function removeTaggedLayers(
  viewer: Viewer,
  tag: "__isFlood" | "__isDemRelief",
): void {
  const layers = viewer.imageryLayers;
  for (let i = layers.length - 1; i >= 0; i--) {
    const layer = layers.get(i) as TaggedLayer;
    if (layer[tag]) {
      layers.remove(layer, true);
    }
  }
}

/** Schedule work when the browser is idle (AOI/DEM drape must not block UI). */
export function scheduleIdleWork(fn: () => void, timeoutMs = 120): () => void {
  const ric = (
    window as Window & {
      requestIdleCallback?: (
        cb: IdleRequestCallback,
        opts?: IdleRequestOptions,
      ) => number;
      cancelIdleCallback?: (id: number) => void;
    }
  ).requestIdleCallback;
  const cic = (
    window as Window & { cancelIdleCallback?: (id: number) => void }
  ).cancelIdleCallback;

  if (typeof ric === "function") {
    const id = ric(() => fn(), { timeout: timeoutMs });
    return () => {
      if (typeof cic === "function") cic(id);
    };
  }
  const t = window.setTimeout(fn, Math.min(32, timeoutMs));
  return () => window.clearTimeout(t);
}

async function addSingleTileOverlay(
  viewer: Viewer,
  dataUrl: string,
  bounds: [number, number, number, number],
  tag: "__isFlood" | "__isDemRelief",
  opacity: number,
): Promise<void> {
  const [west, south, east, north] = bounds;
  const rectangle = Rectangle.fromDegrees(west, south, east, north);
  removeTaggedLayers(viewer, tag);
  try {
    const provider = await SingleTileImageryProvider.fromUrl(dataUrl, {
      rectangle,
    });
    const layer = viewer.imageryLayers.addImageryProvider(provider) as TaggedLayer;
    layer[tag] = true;
    layer.alpha = Math.max(0, Math.min(1, opacity));
  } catch (err) {
    console.warn(`[globe] ${tag} overlay failed`, err);
  }
  viewer.scene.requestRender();
}

/**
 * Update (or create) a translucent flood raster over the preview domain bounds.
 * Uses a data-URL snapshot of the soft-solver canvas — throttled by the caller.
 */
export async function applyFloodOverlay(
  viewer: Viewer,
  canvas: HTMLCanvasElement,
  artifacts: PreviewRenderArtifacts | null,
  visible: boolean,
  opacity: number,
): Promise<void> {
  if (!artifacts || !visible) {
    removeTaggedLayers(viewer, "__isFlood");
    viewer.scene.requestRender();
    return;
  }

  syncCanvas(canvas, artifacts.image);
  const dataUrl = canvas.toDataURL("image/png");
  await addSingleTileOverlay(
    viewer,
    dataUrl,
    artifacts.bounds,
    "__isFlood",
    opacity,
  );
}

export function clearFloodOverlay(viewer: Viewer): void {
  removeTaggedLayers(viewer, "__isFlood");
  viewer.scene.requestRender();
}

/**
 * Rasterize elevation tint + NW hillshade from a capped DEM sample grid.
 * ImageData origin is top-left; z row-major with j=0 south → flip vertically.
 */
export function rasterizeDemRelief(
  grid: DemHeightmapGrid,
  image?: ImageData,
  hillshade?: ArrayLike<number>,
): ImageData {
  const cols = grid.cols;
  const rows = grid.rows;
  const z = grid.z;
  const img =
    image && image.width === cols && image.height === rows
      ? image
      : new ImageData(cols, rows);
  const data = img.data;

  let zMin = Infinity;
  let zMax = -Infinity;
  for (let i = 0; i < cols * rows; i++) {
    const v = Number(z[i]);
    if (!Number.isFinite(v)) continue;
    if (v < zMin) zMin = v;
    if (v > zMax) zMax = v;
  }
  if (!(zMax > zMin)) {
    zMin = 0;
    zMax = 1;
  }
  const zRange = zMax - zMin;

  // Light from northwest (azimuth ~315°, altitude ~45°).
  const az = (315 * Math.PI) / 180;
  const alt = (45 * Math.PI) / 180;
  const lx = Math.cos(alt) * Math.sin(az);
  const ly = Math.cos(alt) * Math.cos(az);
  const lz = Math.sin(alt);

  const elevAt = (i: number, j: number) => {
    const ii = Math.max(0, Math.min(cols - 1, i));
    const jj = Math.max(0, Math.min(rows - 1, j));
    return Number(z[jj * cols + ii]) || 0;
  };

  for (let j = 0; j < rows; j++) {
    const srcRow = rows - 1 - j;
    for (let i = 0; i < cols; i++) {
      const elev = elevAt(i, srcRow);
      const t = (elev - zMin) / zRange;

      // Elevation tint: basin ink → hydro teal → gauge brass (midnight palette).
      const r = Math.round(18 + (37 - 18) * t + (241 - 37) * t * t * 0.35);
      const g = Math.round(29 + (198 - 29) * t * 0.55 + (184 - 198) * t * t * 0.2);
      const b = Math.round(40 + (217 - 40) * t * 0.45);

      const dzdx = (elevAt(i + 1, srcRow) - elevAt(i - 1, srcRow)) * 0.5;
      const dzdy = (elevAt(i, srcRow + 1) - elevAt(i, srcRow - 1)) * 0.5;
      const nx = -dzdx;
      const ny = -dzdy;
      const nz = 1;
      const inv = 1 / Math.sqrt(nx * nx + ny * ny + nz * nz);
      const fallbackShade = Math.max(
        0.25,
        Math.min(1, (nx * inv * lx + ny * inv * ly + nz * inv * lz) * 0.85 + 0.15),
      );
      const sunShade = hillshade?.[srcRow * cols + i];
      const shade = Number.isFinite(sunShade)
        ? Math.max(0.25, Math.min(1, 0.25 + 0.75 * Number(sunShade)))
        : fallbackShade;

      const o = (j * cols + i) * 4;
      data[o] = Math.round(r * shade);
      data[o + 1] = Math.round(g * shade);
      data[o + 2] = Math.round(b * shade);
      data[o + 3] = Math.round(110 + t * 90);
    }
  }
  return img;
}

/**
 * Drape DEM elevation tint / hillshade as SingleTile over the AOI.
 * Builds the canvas from the capped sample grid (idle-friendly caller).
 */
export async function applyDemReliefOverlay(
  viewer: Viewer,
  canvas: HTMLCanvasElement,
  grid: DemHeightmapGrid | null,
  visible: boolean,
  opacity: number,
  hillshade?: ArrayLike<number>,
): Promise<void> {
  if (!grid || !visible || grid.cols < 2 || grid.rows < 2) {
    removeTaggedLayers(viewer, "__isDemRelief");
    viewer.scene.requestRender();
    return;
  }

  const image = rasterizeDemRelief(grid, undefined, hillshade);
  syncCanvas(canvas, image);
  const dataUrl = canvas.toDataURL("image/png");
  await addSingleTileOverlay(
    viewer,
    dataUrl,
    grid.bounds,
    "__isDemRelief",
    opacity,
  );
}

export function clearDemReliefOverlay(viewer: Viewer): void {
  removeTaggedLayers(viewer, "__isDemRelief");
  viewer.scene.requestRender();
}
