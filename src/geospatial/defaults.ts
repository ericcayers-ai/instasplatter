import type { GeoLayer, GeoScenarioMeta, HydrographSample } from "./types";

export const DEFAULT_GEO_LAYERS: GeoLayer[] = [
  {
    id: "basemap",
    label: "Basemap",
    kind: "basemap",
    group: "basemap",
    visible: true,
    opacity: 1,
    placeholder: false,
    status: "ready",
  },
  {
    id: "imagery",
    label: "Imagery",
    kind: "imagery",
    group: "terrain",
    visible: false,
    opacity: 0.85,
    placeholder: true,
    status: "empty",
  },
  {
    id: "dtm",
    label: "DTM",
    kind: "dtm",
    group: "terrain",
    visible: false,
    opacity: 0.9,
    placeholder: true,
    status: "empty",
  },
  {
    id: "dsm",
    label: "DSM",
    kind: "dsm",
    group: "terrain",
    visible: false,
    opacity: 0.75,
    placeholder: true,
    status: "empty",
  },
  {
    id: "splat",
    label: "Gaussian splat",
    kind: "splat",
    group: "survey",
    visible: false,
    opacity: 1,
    placeholder: true,
    status: "hook",
  },
  {
    id: "mesh",
    label: "Mesh",
    kind: "mesh",
    group: "survey",
    visible: false,
    opacity: 1,
    placeholder: true,
    status: "hook",
  },
  {
    id: "buildings",
    label: "Buildings",
    kind: "buildings",
    group: "network",
    visible: false,
    opacity: 0.8,
    placeholder: true,
    status: "empty",
  },
  {
    id: "waterways",
    label: "OSM waterways",
    kind: "waterways",
    group: "network",
    visible: true,
    opacity: 0.9,
    placeholder: true,
    status: "ready",
  },
  {
    id: "gauges",
    label: "USGS gauges",
    kind: "gauges",
    group: "network",
    visible: true,
    opacity: 1,
    placeholder: true,
    status: "ready",
  },
  {
    id: "nfhl",
    label: "FEMA NFHL zones",
    kind: "nfhl",
    group: "network",
    visible: false,
    opacity: 0.45,
    placeholder: true,
    status: "ready",
  },
  {
    id: "hydrosheds",
    label: "HydroSHEDS basins",
    kind: "hydrosheds",
    group: "network",
    visible: false,
    opacity: 0.5,
    placeholder: true,
    status: "ready",
  },
  {
    id: "forcing",
    label: "Rain / forcing",
    kind: "forcing",
    group: "flood",
    visible: false,
    opacity: 0.6,
    placeholder: true,
    status: "empty",
  },
  {
    id: "flood_depth",
    label: "Flood depth",
    kind: "flood_depth",
    group: "flood",
    visible: true,
    opacity: 0.72,
    placeholder: false,
    status: "ready",
  },
  {
    id: "flood_velocity",
    label: "Flood velocity",
    kind: "flood_velocity",
    group: "flood",
    visible: false,
    opacity: 0.7,
    placeholder: false,
    status: "ready",
  },
  {
    id: "flood_hazard",
    label: "Flood hazard",
    kind: "flood_hazard",
    group: "flood",
    visible: false,
    opacity: 0.75,
    placeholder: true,
    status: "ready",
  },
  {
    id: "flood_uncertainty",
    label: "Uncertainty",
    kind: "flood_uncertainty",
    group: "flood",
    visible: false,
    opacity: 0.5,
    placeholder: true,
    status: "empty",
  },
  // Experimental multi-hazard stubs — appear as palette cards / hook rows only (no solvers).
  {
    id: "hazard_quake",
    label: "Earthquake (USGS / GDACS)",
    kind: "hazard_quake",
    group: "hazards",
    visible: false,
    opacity: 1,
    placeholder: true,
    status: "hook",
  },
  {
    id: "hazard_fire",
    label: "Wildfire (GDACS / STAC)",
    kind: "hazard_fire",
    group: "hazards",
    visible: false,
    opacity: 1,
    placeholder: true,
    status: "hook",
  },
  {
    id: "hazard_landslide",
    label: "Landslide (GDACS / USGS)",
    kind: "hazard_landslide",
    group: "hazards",
    visible: false,
    opacity: 1,
    placeholder: true,
    status: "hook",
  },
  {
    id: "hazard_tsunami",
    label: "Tsunami (GDACS / USGS)",
    kind: "hazard_tsunami",
    group: "hazards",
    visible: false,
    opacity: 1,
    placeholder: true,
    status: "hook",
  },
];

export const LAYER_GROUP_LABELS: Record<GeoLayer["group"], string> = {
  basemap: "Basemap",
  terrain: "Terrain",
  survey: "Survey products",
  network: "Network",
  flood: "Flood results",
  hazards: "Hazard stubs",
};

/** Draft Flood lab scenario — soft / HAND live preview + optional ANUGA. */
export const PLACEHOLDER_SCENARIO: GeoScenarioMeta = {
  id: "draft-site-rain",
  name: "Flood lab — site rain",
  durationHours: 12,
  engineLabel: "Soft + HAND preview / ANUGA when ready",
  note: "Flood lab defaults: Chicago-style hyetograph, south outlet stage BC, mixed-urban Manning. Soft/HAND stay Live preview (non-authoritative) until ANUGA compare gates pass.",
  aoiWgs84: null,
  rainfallTemplate: "chicago_6h",
  manningPreset: "mixed_urban",
  manningN: 0.035,
  outletBc: "south stage 0.4 m",
  authority: "draft",
};

/** Rainfall hyetograph templates (mm/h vs hours) for Flood lab. */
export const FLOOD_LAB_HYETOGRAPHS: {
  id: string;
  label: string;
  authority: string;
  samples: { hours: number; mmPerHour: number }[];
}[] = [
  {
    id: "chicago_6h",
    label: "Chicago-style 6 h",
    authority: "draft-template",
    samples: [
      { hours: 0, mmPerHour: 2 },
      { hours: 1, mmPerHour: 8 },
      { hours: 2, mmPerHour: 28 },
      { hours: 3, mmPerHour: 42 },
      { hours: 4, mmPerHour: 22 },
      { hours: 5, mmPerHour: 10 },
      { hours: 6, mmPerHour: 4 },
      { hours: 12, mmPerHour: 1 },
    ],
  },
  {
    id: "uniform_25",
    label: "Uniform 25 mm/h",
    authority: "draft-template",
    samples: [
      { hours: 0, mmPerHour: 25 },
      { hours: 12, mmPerHour: 25 },
    ],
  },
  {
    id: "flash_2h",
    label: "Flash 2 h peak",
    authority: "draft-template",
    samples: [
      { hours: 0, mmPerHour: 5 },
      { hours: 0.5, mmPerHour: 55 },
      { hours: 1, mmPerHour: 40 },
      { hours: 2, mmPerHour: 8 },
      { hours: 12, mmPerHour: 1 },
    ],
  },
];

/** Manning n presets for Flood lab (draft — not calibrated). */
export const FLOOD_LAB_MANNING: { id: string; label: string; n: number }[] = [
  { id: "channel", label: "Channel", n: 0.03 },
  { id: "mixed_urban", label: "Mixed urban", n: 0.035 },
  { id: "floodplain", label: "Floodplain", n: 0.045 },
  { id: "forest", label: "Forest", n: 0.08 },
];

/** Outlet boundary-condition presets. */
export const FLOOD_LAB_OUTLET_BC: { id: string; label: string; detail: string }[] = [
  { id: "south_stage", label: "South stage", detail: "Open outlet · stage 0.4 m on south edge" },
  { id: "free_outfall", label: "Free outfall", detail: "Critical-depth proxy on south edge" },
  { id: "closed", label: "Closed basin", detail: "Reflective walls — ponding only" },
];

/**
 * Synthetic hydrograph for the timeline shell.
 * Peak around hour 4–5, then recession — enough to scrub meaningfully.
 */
export const PLACEHOLDER_HYDROGRAPH: HydrographSample[] = [
  { hours: 0, stageM: 0.35, dischargeCms: 4 },
  { hours: 1, stageM: 0.55, dischargeCms: 12 },
  { hours: 2, stageM: 0.95, dischargeCms: 28 },
  { hours: 3, stageM: 1.55, dischargeCms: 52 },
  { hours: 4, stageM: 2.15, dischargeCms: 78 },
  { hours: 5, stageM: 2.35, dischargeCms: 85 },
  { hours: 6, stageM: 2.05, dischargeCms: 68 },
  { hours: 7, stageM: 1.55, dischargeCms: 44 },
  { hours: 8, stageM: 1.15, dischargeCms: 30 },
  { hours: 9, stageM: 0.85, dischargeCms: 20 },
  { hours: 10, stageM: 0.65, dischargeCms: 14 },
  { hours: 11, stageM: 0.5, dischargeCms: 10 },
  { hours: 12, stageM: 0.42, dischargeCms: 7 },
];

/** Cold-start map view before an AOI is drawn (worldwide, not site-locked). */
export const GEO_MAP_CENTER: [number, number] = [0, 20];
export const GEO_MAP_ZOOM = 1.6;

/** Esri World Imagery (Standard satellite). Attribution required. */
export const ESRI_WORLD_IMAGERY_TILES =
  "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}";
export const ESRI_WORLD_IMAGERY_ATTRIBUTION =
  'Tiles &copy; <a href="https://www.esri.com/">Esri</a> — Source: Esri, Maxar, Earthstar Geographics, and the GIS User Community';

export const CARTO_DARK_TILES = [
  "https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
  "https://b.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
  "https://c.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
] as const;
export const CARTO_ATTRIBUTION =
  '&copy; <a href="https://www.openstreetmap.org/copyright">OSM</a> &copy; <a href="https://carto.com/">CARTO</a>';
