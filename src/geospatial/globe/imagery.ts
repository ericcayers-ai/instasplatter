/**
 * Cesium XYZ imagery — reuse Esri / Carto URLs + attribution from defaults
 * (same sources as MapLibre / imageryTiles).
 */

import { Credit, UrlTemplateImageryProvider, WebMercatorTilingScheme } from "cesium";
import {
  CARTO_ATTRIBUTION,
  CARTO_DARK_TILES,
  ESRI_WORLD_IMAGERY_ATTRIBUTION,
  ESRI_WORLD_IMAGERY_TILES,
} from "../defaults";
import type { GeoBasemapMode } from "../types";

function stripHtml(html: string): string {
  return html.replace(/<[^>]+>/g, "").replace(/&copy;/g, "©").trim();
}

export function globeImageryAttribution(mode: GeoBasemapMode): string {
  return mode === "satellite"
    ? stripHtml(ESRI_WORLD_IMAGERY_ATTRIBUTION)
    : stripHtml(CARTO_ATTRIBUTION);
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

  // Cesium picks one URL; rotate via first Carto mirror (same pattern as MapLibre list).
  return new UrlTemplateImageryProvider({
    url: CARTO_DARK_TILES[0],
    tilingScheme: new WebMercatorTilingScheme(),
    maximumLevel: 20,
    credit: new Credit(CARTO_ATTRIBUTION, true),
  });
}
