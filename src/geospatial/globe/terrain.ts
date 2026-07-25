/**
 * Globe terrain providers.
 *
 * Standard path priority:
 * 1. Optional local `layer.json` tiles (CTB / quantized-mesh) via CesiumTerrainProvider
 * 2. In-app CustomHeightmapTerrainProvider from `sample_geo_dem` grid (no Docker CTB)
 * 3. Ellipsoid fallback
 *
 * Experimental: ion World Terrain only when a user token is stored.
 * Never MapLibre canvas-drape → Cesium. Never enable ion World Terrain on Standard.
 */

import {
  CesiumTerrainProvider,
  createWorldTerrainAsync,
  CustomHeightmapTerrainProvider,
  EllipsoidTerrainProvider,
  GeographicTilingScheme,
  Math as CesiumMath,
  type TerrainProvider,
} from "cesium";
import { applyCesiumIonPolicy, blankCesiumIon } from "./ion";

/** Sampled DEM grid for CustomHeightmapTerrainProvider (row-major, j=0 south). */
export type DemHeightmapGrid = {
  cols: number;
  rows: number;
  /** West, south, east, north (WGS84 degrees). */
  bounds: [number, number, number, number];
  z: Float32Array | number[];
};

const HEIGHTMAP_TILE = 64;

/** In-memory URL for AOI DEM terrain tiles (file:// via convertFileSrc or http). */
let localDemTerrainUrl: string | null = null;
/** In-app heightmap from sample_geo_dem (preferred default; no CTB). */
let demHeightmap: DemHeightmapGrid | null = null;
const terrainListeners = new Set<() => void>();

function notifyTerrainListeners(): void {
  for (const cb of terrainListeners) cb();
}

/**
 * Register a local quantized-mesh or heightmap terrain root (directory with
 * `layer.json`, or CTB output URL). Pass null to clear.
 */
export function setLocalDemTerrainUrl(url: string | null): void {
  localDemTerrainUrl = url?.trim() || null;
  notifyTerrainListeners();
}

export function getLocalDemTerrainUrl(): string | null {
  return localDemTerrainUrl;
}

/**
 * Register an in-app DEM heightmap for Cesium CustomHeightmapTerrainProvider.
 * Pass null to clear. Prefer capped sample grids (≤192×144).
 */
export function setDemHeightmap(grid: DemHeightmapGrid | null): void {
  if (!grid || grid.cols < 2 || grid.rows < 2 || !grid.z?.length) {
    demHeightmap = null;
  } else {
    const z =
      grid.z instanceof Float32Array ? grid.z : Float32Array.from(grid.z);
    demHeightmap = {
      cols: grid.cols,
      rows: grid.rows,
      bounds: grid.bounds,
      z,
    };
  }
  notifyTerrainListeners();
}

export function getDemHeightmap(): DemHeightmapGrid | null {
  return demHeightmap;
}

export function subscribeLocalDemTerrainUrl(cb: () => void): () => void {
  terrainListeners.add(cb);
  return () => terrainListeners.delete(cb);
}

/** Alias — heightmap + URL share the same listener set. */
export const subscribeDemTerrain = subscribeLocalDemTerrainUrl;

export type GlobeTerrainResult = {
  provider: TerrainProvider;
  source: "ellipsoid" | "local-dem" | "heightmap" | "ion-world";
  detail: string;
};

function sampleDemHeight(grid: DemHeightmapGrid, lonDeg: number, latDeg: number): number {
  const [west, south, east, north] = grid.bounds;
  if (lonDeg < west || lonDeg > east || latDeg < south || latDeg > north) {
    return 0;
  }
  const cols = grid.cols;
  const rows = grid.rows;
  const fx = ((lonDeg - west) / (east - west)) * (cols - 1);
  const fy = ((latDeg - south) / (north - south)) * (rows - 1);
  const x0 = Math.max(0, Math.min(cols - 2, Math.floor(fx)));
  const y0 = Math.max(0, Math.min(rows - 2, Math.floor(fy)));
  const tx = fx - x0;
  const ty = fy - y0;
  const z = grid.z;
  const z00 = Number(z[y0 * cols + x0]) || 0;
  const z10 = Number(z[y0 * cols + x0 + 1]) || 0;
  const z01 = Number(z[(y0 + 1) * cols + x0]) || 0;
  const z11 = Number(z[(y0 + 1) * cols + x0 + 1]) || 0;
  return (
    z00 * (1 - tx) * (1 - ty) +
    z10 * tx * (1 - ty) +
    z01 * (1 - tx) * ty +
    z11 * tx * ty
  );
}

function rectanglesOverlapDegrees(
  a: [number, number, number, number],
  west: number,
  south: number,
  east: number,
  north: number,
): boolean {
  return !(east < a[0] || west > a[2] || north < a[1] || south > a[3]);
}

/**
 * Build a CustomHeightmapTerrainProvider that bilinear-samples the DEM grid
 * for tiles intersecting the AOI; elsewhere returns ellipsoid (0).
 */
export function createDemHeightmapTerrainProvider(
  grid: DemHeightmapGrid,
): CustomHeightmapTerrainProvider {
  const tilingScheme = new GeographicTilingScheme();
  const width = HEIGHTMAP_TILE;
  const height = HEIGHTMAP_TILE;
  const aoi = grid.bounds;

  return new CustomHeightmapTerrainProvider({
    width,
    height,
    tilingScheme,
    credit: "Local DEM heightmap",
    callback: (x, y, level) => {
      const rect = tilingScheme.tileXYToRectangle(x, y, level);
      const west = CesiumMath.toDegrees(rect.west);
      const south = CesiumMath.toDegrees(rect.south);
      const east = CesiumMath.toDegrees(rect.east);
      const north = CesiumMath.toDegrees(rect.north);

      const buffer = new Float32Array(width * height);
      if (!rectanglesOverlapDegrees(aoi, west, south, east, north)) {
        return buffer;
      }

      for (let row = 0; row < height; row++) {
        // Cesium heightmap: row 0 = north edge of the tile.
        const lat = north + ((south - north) * row) / (height - 1);
        for (let col = 0; col < width; col++) {
          const lon = west + ((east - west) * col) / (width - 1);
          buffer[row * width + col] = sampleDemHeight(grid, lon, lat);
        }
      }
      return buffer;
    },
  });
}

/**
 * Resolve terrain for the Globe view.
 * @param experimental — TitleBar Experimental mode (required for ion World Terrain)
 * @param terrainUrlOverride — optional one-shot URL (else uses registry)
 * @param heightmapOverride — optional one-shot heightmap (else uses registry)
 */
export async function resolveGlobeTerrain(opts: {
  experimental: boolean;
  terrainUrlOverride?: string | null;
  heightmapOverride?: DemHeightmapGrid | null;
}): Promise<GlobeTerrainResult> {
  const url = (opts.terrainUrlOverride ?? localDemTerrainUrl)?.trim() || null;
  const heightmap = opts.heightmapOverride ?? demHeightmap;

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
      console.warn("[globe] local DEM terrain failed; trying heightmap", err);
    }
  }

  if (heightmap && heightmap.cols >= 2 && heightmap.rows >= 2) {
    blankCesiumIon();
    try {
      const provider = createDemHeightmapTerrainProvider(heightmap);
      return {
        provider,
        source: "heightmap",
        detail: `DEM heightmap · ${heightmap.cols}×${heightmap.rows}`,
      };
    } catch (err) {
      console.warn("[globe] DEM heightmap failed; falling back", err);
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
    detail: heightmap
      ? "Ellipsoid (heightmap failed)"
      : url
        ? "Ellipsoid (local DEM failed)"
        : "Ellipsoid · stage DEM for relief",
  };
}
