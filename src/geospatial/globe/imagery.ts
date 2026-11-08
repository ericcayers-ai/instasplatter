/**
 * Cesium XYZ imagery — reuse Esri URLs + attribution from defaults
 * (same sources as MapLibre / imageryTiles).
 */

import { Credit, UrlTemplateImageryProvider, WebMercatorTilingScheme } from "cesium";
import {
  ESRI_WORLD_IMAGERY_ATTRIBUTION,
  ESRI_WORLD_IMAGERY_TILES,
  ESRI_WORLD_STREET_MAP_ATTRIBUTION,
  ESRI_WORLD_STREET_MAP_TILES,
} from "../defaults";
import type { GeoBasemapMode } from "../types";

function stripHtml(html: string): string {
  return html.replace(/<[^>]+>/g, "").replace(/&copy;/g, "©").trim();
}

export function globeImageryAttribution(mode: GeoBasemapMode): string {
  return mode === "satellite"
    ? stripHtml(ESRI_WORLD_IMAGERY_ATTRIBUTION)
    : stripHtml(ESRI_WORLD_STREET_MAP_ATTRIBUTION);
}

/** Build a UrlTemplateImageryProvider for the current basemap mode. */
export function createGlobeImageryProvider(mode: GeoBasemapMode): UrlTemplateImageryProvider {
  if (mode === "satellite") {
    return new UrlTemplateImageryProvider({
      url: ESRI_WORLD_IMAGERY_TILES,
      tilingScheme: new WebMercatorTilingScheme(),
      maximumLevel: 19,
      credit: new Credit(ESRI_WORLD_IMAGERY_ATTRIBUTION, true),
    });
  }

  // World Street Map tiles are rendered interactively; no bulk or offline cache.
  return new UrlTemplateImageryProvider({
    url: ESRI_WORLD_STREET_MAP_TILES,
    tilingScheme: new WebMercatorTilingScheme(),
    maximumLevel: 20,
    credit: new Credit(ESRI_WORLD_STREET_MAP_ATTRIBUTION, true),
  });
}
