/**
 * Flood depth overlay on the globe — SingleTile imagery draped over the AOI
 * (not MapLibre canvas-drape→terrain).
 */

import {
  ImageryLayer,
  Rectangle,
  SingleTileImageryProvider,
  type Viewer,
} from "cesium";
import type { PreviewRenderArtifacts } from "../preview";

type FloodTagged = ImageryLayer & { __isFlood?: boolean };

function syncCanvas(canvas: HTMLCanvasElement, image: ImageData): void {
  if (canvas.width !== image.width || canvas.height !== image.height) {
    canvas.width = image.width;
    canvas.height = image.height;
  }
  const ctx = canvas.getContext("2d");
  if (ctx) ctx.putImageData(image, 0, 0);
}

function removeFloodLayer(viewer: Viewer): void {
  const layers = viewer.imageryLayers;
  for (let i = layers.length - 1; i >= 0; i--) {
    const layer = layers.get(i) as FloodTagged;
    if (layer.__isFlood) {
      layers.remove(layer, true);
    }
  }
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
    removeFloodLayer(viewer);
    viewer.scene.requestRender();
    return;
  }

  syncCanvas(canvas, artifacts.image);
  const [west, south, east, north] = artifacts.bounds;
  const rectangle = Rectangle.fromDegrees(west, south, east, north);
  const dataUrl = canvas.toDataURL("image/png");

  removeFloodLayer(viewer);

  try {
    const provider = await SingleTileImageryProvider.fromUrl(dataUrl, { rectangle });
    const layer = viewer.imageryLayers.addImageryProvider(provider) as FloodTagged;
    layer.__isFlood = true;
    layer.alpha = Math.max(0, Math.min(1, opacity));
  } catch (err) {
    console.warn("[globe] flood overlay failed", err);
  }
  viewer.scene.requestRender();
}

export function clearFloodOverlay(viewer: Viewer): void {
  removeFloodLayer(viewer);
  viewer.scene.requestRender();
}
