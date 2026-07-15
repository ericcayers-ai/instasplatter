/** Layer kinds for the geospatial layer tree and map hooks. */
export type GeoLayerKind =
  | "basemap"
  | "imagery"
  | "dtm"
  | "dsm"
  | "splat"
  | "mesh"
  | "buildings"
  | "waterways"
  | "gauges"
  | "forcing"
  | "flood_depth"
  | "flood_velocity"
  | "flood_hazard"
  | "flood_uncertainty";

export type GeoLayerGroup = "basemap" | "terrain" | "survey" | "network" | "flood";

export type GeoViewMode = "2d" | "3d";

export type GeoWaterStyle = "depth" | "hazard" | "contour";

export type GeoTool = "pan" | "measure" | "inspect" | "profile";

export interface GeoLayer {
  id: string;
  label: string;
  kind: GeoLayerKind;
  group: GeoLayerGroup;
  visible: boolean;
  opacity: number;
  /** True until real GIS/simulation data is wired. */
  placeholder: boolean;
  /** empty = no data yet; hook = renderer/export stub; ready = shows on map. */
  status: "empty" | "hook" | "ready";
}

export interface HydrographSample {
  /** Hours from scenario start. */
  hours: number;
  /** Stage / water-surface elevation proxy (m). */
  stageM: number;
  /** Inflow / gauge discharge proxy (m³/s). */
  dischargeCms: number;
}

export interface GeoFloodSnapshot {
  hours: number;
  stageM: number;
  dischargeCms: number;
  /** Placeholder max depth across domain (m). */
  maxDepthM: number;
  /** Placeholder wet fraction 0–1. */
  wetFraction: number;
  /** Placeholder peak hazard class 0–3. */
  hazardClass: number;
  statusLabel: string;
}

export interface GeoScenarioMeta {
  id: string;
  name: string;
  durationHours: number;
  engineLabel: string;
  note: string;
}
