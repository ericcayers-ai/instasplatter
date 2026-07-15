import type { GeoFloodSnapshot, GeoWaterStyle, HydrographSample } from "./types";
import { PLACEHOLDER_HYDROGRAPH, PLACEHOLDER_SCENARIO } from "./defaults";

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/** Interpolate the placeholder hydrograph at a normalised scrub 0–1. */
export function sampleHydrograph(
  t01: number,
  series: HydrographSample[] = PLACEHOLDER_HYDROGRAPH,
): { hours: number; stageM: number; dischargeCms: number } {
  const duration = PLACEHOLDER_SCENARIO.durationHours;
  const hours = Math.max(0, Math.min(1, t01)) * duration;
  if (series.length === 0) return { hours, stageM: 0, dischargeCms: 0 };
  if (hours <= series[0].hours) {
    return { hours, stageM: series[0].stageM, dischargeCms: series[0].dischargeCms };
  }
  const last = series[series.length - 1];
  if (hours >= last.hours) {
    return { hours, stageM: last.stageM, dischargeCms: last.dischargeCms };
  }
  for (let i = 0; i < series.length - 1; i++) {
    const a = series[i];
    const b = series[i + 1];
    if (hours >= a.hours && hours <= b.hours) {
      const u = (hours - a.hours) / (b.hours - a.hours || 1);
      return {
        hours,
        stageM: lerp(a.stageM, b.stageM, u),
        dischargeCms: lerp(a.dischargeCms, b.dischargeCms, u),
      };
    }
  }
  return { hours, stageM: last.stageM, dischargeCms: last.dischargeCms };
}

/** Derive flood stats from scrub; merges live-preview engine stats when provided. */
export function floodSnapshotFromTime(
  t01: number,
  preview?: {
    maxDepthM: number;
    wetFraction: number;
    hazardClass: number;
    massM3?: number;
    maxSpeedMs?: number;
    backend?: GeoFloodSnapshot["backend"];
    validation?: GeoFloodSnapshot["validation"];
  } | null,
): GeoFloodSnapshot {
  const { hours, stageM, dischargeCms } = sampleHydrograph(t01);
  const peakStage = Math.max(...PLACEHOLDER_HYDROGRAPH.map((s) => s.stageM));
  const intensity = peakStage > 0 ? Math.min(1, stageM / peakStage) : 0;

  let maxDepthM = Math.max(0, stageM - 0.3);
  let wetFraction = Math.min(0.92, 0.08 + intensity * 0.72);
  let hazardClass = 0;
  if (maxDepthM >= 1.6 || dischargeCms >= 70) hazardClass = 3;
  else if (maxDepthM >= 1.0 || dischargeCms >= 45) hazardClass = 2;
  else if (maxDepthM >= 0.4 || dischargeCms >= 20) hazardClass = 1;

  if (preview) {
    maxDepthM = preview.maxDepthM;
    wetFraction = preview.wetFraction;
    hazardClass = preview.hazardClass;
  }

  const statusLabel =
    intensity < 0.15
      ? "Dry / low water"
      : intensity < 0.45
        ? "Rising"
        : intensity < 0.75
          ? "Near peak"
          : intensity < 0.92
            ? "Peak / high water"
            : "Receding";

  return {
    hours,
    stageM,
    dischargeCms,
    maxDepthM,
    wetFraction,
    hazardClass,
    statusLabel,
    massM3: preview?.massM3,
    maxSpeedMs: preview?.maxSpeedMs,
    backend: preview?.backend,
    validation: preview?.validation,
  };
}

export function hazardClassLabel(c: number): string {
  switch (c) {
    case 0:
      return "H0 Low";
    case 1:
      return "H1 Transition";
    case 2:
      return "H2 Moderate";
    default:
      return "H3 High";
  }
}

export function waterStyleLabel(style: GeoWaterStyle): string {
  switch (style) {
    case "depth":
      return "Depth";
    case "hazard":
      return "Hazard";
    case "contour":
      return "Contours";
  }
}

/**
 * Demo flood-extent ring polygons (WGS84) scaled by wet fraction.
 * Used when no scientific checkpoint GeoJSON is mapped to WGS84 yet —
 * scientific demo still drives wetFraction via the scrubber/time link.
 */
export function placeholderFloodPolygon(wetFraction: number): GeoJSON.Feature<GeoJSON.Polygon> {
  const [lon0, lat0] = [174.779, -41.2865];
  const rLon = 0.012 + wetFraction * 0.028;
  const rLat = 0.008 + wetFraction * 0.018;
  const steps = 32;
  const ring: [number, number][] = [];
  for (let i = 0; i <= steps; i++) {
    const a = (i / steps) * Math.PI * 2;
    // Slightly irregular shoreline so it does not look like a perfect ellipse.
    const wobble = 1 + 0.08 * Math.sin(a * 3) + 0.04 * Math.cos(a * 5);
    ring.push([lon0 + Math.cos(a) * rLon * wobble, lat0 + Math.sin(a) * rLat * wobble]);
  }
  return {
    type: "Feature",
    properties: { wetFraction },
    geometry: { type: "Polygon", coordinates: [ring] },
  };
}

export function placeholderWaterways(): GeoJSON.FeatureCollection {
  return {
    type: "FeatureCollection",
    features: [
      {
        type: "Feature",
        properties: { name: "Harbour edge" },
        geometry: {
          type: "LineString",
          coordinates: [
            [174.75, -41.29],
            [174.76, -41.288],
            [174.775, -41.285],
            [174.79, -41.283],
            [174.805, -41.282],
          ],
        },
      },
      {
        type: "Feature",
        properties: { name: "Drain stub" },
        geometry: {
          type: "LineString",
          coordinates: [
            [174.772, -41.292],
            [174.776, -41.289],
            [174.78, -41.287],
          ],
        },
      },
    ],
  };
}

export function placeholderGauges(): GeoJSON.FeatureCollection {
  return {
    type: "FeatureCollection",
    features: [
      {
        type: "Feature",
        properties: { name: "Site gauge A", kind: "stage" },
        geometry: { type: "Point", coordinates: [174.776, -41.288] },
      },
      {
        type: "Feature",
        properties: { name: "Inflow B", kind: "discharge" },
        geometry: { type: "Point", coordinates: [174.785, -41.284] },
      },
    ],
  };
}
