import type { GeoWaterStyle } from "../types";

/** Backend used for the live flood preview physics / raster path. */
export type PreviewBackend = "webgpu" | "webgl" | "cpu";

/** Validation against a scientific (ANUGA) checkpoint when available. */
export type PreviewValidationState = "live" | "comparing" | "validated" | "diverged";

export interface PreviewDomain {
  /** West, south, east, north in WGS84 degrees. */
  bounds: [number, number, number, number];
  /** Fine-tile cell size (metres). */
  dxM: number;
  cols: number;
  rows: number;
  /** Coarse surround tile size relative to fine (e.g. 2 = half resolution). */
  coarseFactor: number;
}

export interface PreviewForcing {
  /** Uniform rainfall (mm/h) applied this step. */
  rainfallMmHr: number;
  /** North-boundary inflow discharge (m³/s), distributed across wet edge cells. */
  inflowCms: number;
  /** Manning n (dimensionless). */
  manningN: number;
  /** Infiltration rate (mm/h) when wet. */
  infiltrationMmHr: number;
}

export interface GridState {
  cols: number;
  rows: number;
  dx: number;
  /** Bed elevation (m). */
  z: Float32Array;
  /** Water depth (m). */
  h: Float32Array;
  /** Depth-averaged velocity east (m/s). */
  u: Float32Array;
  /** Depth-averaged velocity north (m/s). */
  v: Float32Array;
  /** Simulation time (scenario seconds). */
  timeS: number;
}

export interface PreviewStats {
  timeS: number;
  maxDepthM: number;
  meanDepthM: number;
  wetFraction: number;
  massM3: number;
  maxSpeedMs: number;
  hazardClass: number;
  cfl: number;
  dtS: number;
}

export interface ScientificCheckpoint {
  /** Scenario time (seconds from start). */
  timeS: number;
  maxDepthM: number;
  wetFraction: number;
  massM3: number;
  /** Optional downsampled depth field (row-major, same domain). */
  depthSample?: Float32Array;
  sampleCols?: number;
  sampleRows?: number;
}

export interface PreviewCompareReport {
  depthRmseM: number | null;
  extentIou: number | null;
  massRelError: number | null;
  wetFractionDelta: number;
  maxDepthDeltaM: number;
  withinTolerance: boolean;
  checkpointTimeS: number;
}

export interface PreviewDisplayFrame {
  /** Interpolated depth for display (m). */
  h: Float32Array;
  u: Float32Array;
  v: Float32Array;
  cols: number;
  rows: number;
  alpha: number;
  stats: PreviewStats;
  backend: PreviewBackend;
  validation: PreviewValidationState;
  compare: PreviewCompareReport | null;
}

export interface PreviewEngineOptions {
  domain?: Partial<PreviewDomain>;
  durationHours?: number;
  cfl?: number;
  /** Prefer low-power: coarser grid, skip particles, longer physics strides. */
  lowPower?: boolean;
  /** Disable particle motion / continuous play animation aids. */
  reducedMotion?: boolean;
  waterStyle?: GeoWaterStyle;
  /**
   * Soft-solver bed: DEM samples (row-major metres). When absent, synthetic
   * undulation is used and the UI should keep Demo / Live preview labels.
   */
  demBed?: {
    z: Float32Array | number[];
    cols: number;
    rows: number;
    bedSource?: "real" | "synthetic" | "proxy";
  } | null;
  /**
   * When true, blend HAND rapid inundation into the depth field (Live preview /
   * non-authoritative until ANUGA compare gates pass).
   */
  useHand?: boolean;
  /** Peak HAND stage (m) for rapid inundation path. */
  handPeakStageM?: number;
}

export interface PreviewCapabilities {
  webgpu: boolean;
  webgl2: boolean;
  reducedMotion: boolean;
  saveData: boolean;
  /** Effective backend after capability + low-power policy. */
  preferredBackend: PreviewBackend;
}
