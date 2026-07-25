export type {
  GridState,
  PreviewBackend,
  PreviewCapabilities,
  PreviewCompareReport,
  PreviewDisplayFrame,
  PreviewDomain,
  PreviewEngineOptions,
  PreviewForcing,
  PreviewStats,
  PreviewValidationState,
  ScientificCheckpoint,
} from "./types";

export { detectPreviewCapabilities, probeWebGpu } from "./capabilities";
export { compareAgainstCheckpoint, nearestCheckpoint, DEFAULT_COMPARE_TOLERANCE } from "./compare";
export {
  FloodPreviewEngine,
  validationBadgeLabel,
  type PreviewRenderArtifacts,
} from "./engine";
export { createGpuAccelerator } from "./webgpuSolver";
export { H_DRY, DEFAULT_DOMAIN, downsampleDepth, buildBedFromDemSamples, buildSyntheticBed, resolveDomain, resolveDomainSmokeOk, MAX_PREVIEW_COLS, MAX_PREVIEW_ROWS, MAX_PREVIEW_CELLS } from "./softSolver";
export {
  handInundate,
  handStageFromTime01,
  handInundationSmokeOk,
  HAND_AUTHORITY_LABEL,
  MAX_HAND_SEARCH_RADIUS,
} from "./handInundation";
