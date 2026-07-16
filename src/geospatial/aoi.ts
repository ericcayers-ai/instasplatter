import type { PreviewDomain } from "./preview/types";

/** WGS84 axis-aligned AOI: west, south, east, north (degrees). */
export type AoiWgs84 = [number, number, number, number];

const M_PER_DEG_LAT = 111_320;

export function normalizeAoi(raw: AoiWgs84): AoiWgs84 {
  const west = Math.min(raw[0], raw[2]);
  const east = Math.max(raw[0], raw[2]);
  const south = Math.min(raw[1], raw[3]);
  const north = Math.max(raw[1], raw[3]);
  return [west, south, east, north];
}

export function aoiIsValid(aoi: AoiWgs84 | null | undefined): aoi is AoiWgs84 {
  if (!aoi) return false;
  const [west, south, east, north] = aoi;
  return (
    Number.isFinite(west) &&
    Number.isFinite(south) &&
    Number.isFinite(east) &&
    Number.isFinite(north) &&
    east > west &&
    north > south &&
    east - west > 1e-5 &&
    north - south > 1e-5
  );
}

export function aoiCenter(aoi: AoiWgs84): [number, number] {
  return [0.5 * (aoi[0] + aoi[2]), 0.5 * (aoi[1] + aoi[3])];
}

/** Approximate metres-per-degree at AOI latitude. */
export function metresPerDegree(latDeg: number): { lon: number; lat: number } {
  return {
    lat: M_PER_DEG_LAT,
    lon: M_PER_DEG_LAT * Math.cos((latDeg * Math.PI) / 180),
  };
}

/** AOI centre origin + ENU box for `plan_geo_extent`. */
export function aoiToEnuBox(aoi: AoiWgs84): {
  origin: [number, number, number];
  demBoundsEnu: [number, number, number, number];
  widthM: number;
  heightM: number;
} {
  const [west, south, east, north] = normalizeAoi(aoi);
  const [lon0, lat0] = aoiCenter([west, south, east, north]);
  const m = metresPerDegree(lat0);
  const minE = (west - lon0) * m.lon;
  const maxE = (east - lon0) * m.lon;
  const minN = (south - lat0) * m.lat;
  const maxN = (north - lat0) * m.lat;
  return {
    origin: [lon0, lat0, 0],
    demBoundsEnu: [minE, minN, maxE, maxN],
    widthM: Math.abs(maxE - minE),
    heightM: Math.abs(maxN - minN),
  };
}

/**
 * Soft-solver domain from an AOI. Cell size follows extent length so worldwide
 * sites are not locked to the Wellington demo grid.
 */
export function domainFromAoi(aoi: AoiWgs84, lowPower = false): PreviewDomain {
  const bounds = normalizeAoi(aoi);
  const { widthM, heightM } = aoiToEnuBox(bounds);
  const longest = Math.max(widthM, heightM, 50);
  const targetCells = lowPower ? 48 : 96;
  const dxM = Math.max(4, longest / targetCells);
  const cols = Math.max(24, Math.min(192, Math.round(widthM / dxM) || targetCells));
  const rows = Math.max(16, Math.min(144, Math.round(heightM / dxM) || Math.round(targetCells * 0.75)));
  return {
    bounds,
    dxM,
    cols,
    rows,
    coarseFactor: 2,
  };
}

export function aoiPolygonGeoJson(aoi: AoiWgs84): GeoJSON.FeatureCollection {
  const [west, south, east, north] = normalizeAoi(aoi);
  return {
    type: "FeatureCollection",
    features: [
      {
        type: "Feature",
        properties: { kind: "aoi" },
        geometry: {
          type: "Polygon",
          coordinates: [
            [
              [west, south],
              [east, south],
              [east, north],
              [west, north],
              [west, south],
            ],
          ],
        },
      },
    ],
  };
}

/** Simple network stubs scaled into the AOI (not Wellington-locked). */
export function networkFeaturesForAoi(aoi: AoiWgs84): {
  waterways: GeoJSON.FeatureCollection;
  gauges: GeoJSON.FeatureCollection;
} {
  const [west, south, east, north] = normalizeAoi(aoi);
  const lon = (t: number) => west + t * (east - west);
  const lat = (t: number) => south + t * (north - south);
  return {
    waterways: {
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          properties: { name: "Main channel" },
          geometry: {
            type: "LineString",
            coordinates: [
              [lon(0.08), lat(0.35)],
              [lon(0.28), lat(0.42)],
              [lon(0.55), lat(0.55)],
              [lon(0.82), lat(0.68)],
            ],
          },
        },
        {
          type: "Feature",
          properties: { name: "Drain stub" },
          geometry: {
            type: "LineString",
            coordinates: [
              [lon(0.4), lat(0.22)],
              [lon(0.48), lat(0.38)],
              [lon(0.55), lat(0.52)],
            ],
          },
        },
      ],
    },
    gauges: {
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          properties: { name: "Site gauge A", kind: "stage" },
          geometry: { type: "Point", coordinates: [lon(0.32), lat(0.45)] },
        },
        {
          type: "Feature",
          properties: { name: "Inflow B", kind: "discharge" },
          geometry: { type: "Point", coordinates: [lon(0.7), lat(0.62)] },
        },
      ],
    },
  };
}
