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
  | "nfhl"
  | "hydrosheds"
  | "forcing"
  | "flood_depth"
  | "flood_velocity"
  | "flood_hazard"
  | "flood_uncertainty"
  /** Experimental multi-hazard stubs — feed/STAC cards only, never physics. */
  | "hazard_quake"
  | "hazard_fire"
  | "hazard_landslide"
  | "hazard_tsunami";

export type GeoLayerGroup =
  | "basemap"
  | "terrain"
  | "survey"
  | "network"
  | "flood"
  | "hazards";

export type GeoViewMode = "2d" | "3d" | "globe";

export type GeoWaterStyle = "depth" | "hazard" | "contour";

export type GeoTool = "pan" | "measure" | "inspect" | "profile" | "drawAoi";

/** Basemap: Esri World Imagery (default) or low-bandwidth Carto/OSM. */
export type GeoBasemapMode = "satellite" | "lowBandwidth";

/** WGS84 AOI: west, south, east, north (degrees). */
export type AoiWgs84 = [number, number, number, number];

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
  /** Max depth across domain (m). */
  maxDepthM: number;
  /** Wet fraction 0–1. */
  wetFraction: number;
  /** Peak hazard class 0–3. */
  hazardClass: number;
  statusLabel: string;
  /** Present when driven by the live preview engine. */
  massM3?: number;
  maxSpeedMs?: number;
  backend?: "webgpu" | "webgl" | "cpu";
  validation?: "live" | "comparing" | "validated" | "diverged";
}

export type GeoPreviewBackend = "webgpu" | "webgl" | "cpu";
export type GeoPreviewValidation = "live" | "comparing" | "validated" | "diverged";

export interface GeoPreviewRuntime {
  backend: GeoPreviewBackend;
  validation: GeoPreviewValidation;
  maxDepthM: number;
  wetFraction: number;
  massM3: number;
  maxSpeedMs: number;
  hazardClass: number;
  cfl: number;
}

export interface GeoScenarioMeta {
  id: string;
  name: string;
  durationHours: number;
  engineLabel: string;
  note: string;
  /** Committed AOI when set (mirrors FloodScenario.aoiWgs84). */
  aoiWgs84?: AoiWgs84 | null;
  /** Flood lab rainfall template id. */
  rainfallTemplate?: string;
  /** Manning n preset id. */
  manningPreset?: string;
  /** Manning n value. */
  manningN?: number;
  /** Outlet BC summary for inspector. */
  outletBc?: string;
  /** Authority label: draft | live-preview | demo | scientific. */
  authority?: string;
}

/** Scientific / demo flood run feed for the inspector and map. */
export interface GeoScientificRun {
  runId: string;
  state: string;
  progress: number;
  detail: string;
  mode?: string | null;
  label?: string | null;
  massBalance?: number | null;
}
