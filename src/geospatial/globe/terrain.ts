/**
 * Globe terrain providers.
 *
 * Standard: Ellipsoid, or local DEM quantized-mesh / heightmap via
 * CesiumTerrainProvider.fromUrl when the DEM pipeline stages tiles under
 * `geo/derived/terrain/layer.json`.
 *
 * When only a GeoTIFF is staged (typical after USGS 3DEP / Copernicus fetch):
 * keep ellipsoid + flood overlay; soft-solver / HAND sample the GeoTIFF via
 * `sample_geo_dem` instead of Cesium terrain. Never MapLibre canvas-drape → Cesium.
 * Never enable ion World Terrain on the Standard path.
 */

import {
  CesiumTerrainProvider,
  createWorldTerrainAsync,
  EllipsoidTerrainProvider,
  type TerrainProvider,
} from "cesium";
import { applyCesiumIonPolicy, blankCesiumIon } from "./ion";

/** In-memory URL for AOI DEM terrain tiles (file:// via convertFileSrc or http). */
let localDemTerrainUrl: string | null = null;
const terrainListeners = new Set<() => void>();

/**
 * Register a local quantized-mesh or heightmap terrain root (directory with
 * `layer.json`, or CTB output URL). Pass null to clear back to ellipsoid.
 *
 * DEM staging (Rust / CTB) should call this when tiles are ready for the AOI.
 */
export function setLocalDemTerrainUrl(url: string | null): void {
  localDemTerrainUrl = url?.trim() || null;
  for (const cb of terrainListeners) cb();
}

export function getLocalDemTerrainUrl(): string | null {
  return localDemTerrainUrl;
}

export function subscribeLocalDemTerrainUrl(cb: () => void): () => void {
  terrainListeners.add(cb);
  return () => terrainListeners.delete(cb);
}

export type GlobeTerrainResult = {
  provider: TerrainProvider;
  source: "ellipsoid" | "local-dem" | "ion-world";
  detail: string;
};

/**
 * Resolve terrain for the Globe view.
 * @param experimental — TitleBar Experimental mode (required for ion World Terrain)
 * @param terrainUrlOverride — optional one-shot URL (else uses registry)
 */
export async function resolveGlobeTerrain(opts: {
  experimental: boolean;
  terrainUrlOverride?: string | null;
}): Promise<GlobeTerrainResult> {
  const url = (opts.terrainUrlOverride ?? localDemTerrainUrl)?.trim() || null;

  if (url) {
    blankCesiumIon();
    try {
      const provider = await CesiumTerrainProvider.fromUrl(url, {
        requestVertexNormals: true,
      });
      return {
        provider,
        source: "local-dem",
        detail: `Local DEM terrain · ${url}`,
      };
    } catch (err) {
      console.warn("[globe] local DEM terrain failed; falling back", err);
    }
  }

  const ion = applyCesiumIonPolicy(opts.experimental);
  if (ion.ionEnabled && ion.token) {
    try {
      const provider = await createWorldTerrainAsync();
      return {
        provider,
        source: "ion-world",
        detail: "Experimental · Cesium ion World Terrain",
      };
    } catch (err) {
      console.warn("[globe] ion World Terrain failed; using ellipsoid", err);
      blankCesiumIon();
    }
  } else {
    blankCesiumIon();
  }

  return {
    provider: new EllipsoidTerrainProvider(),
    source: "ellipsoid",
    detail: url
      ? "Ellipsoid (local DEM failed)"
      : "Ellipsoid · stage DEM tiles for relief",
  };
}
