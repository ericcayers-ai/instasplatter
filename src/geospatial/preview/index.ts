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
export { H_DRY, DEFAULT_DOMAIN, downsampleDepth } from "./softSolver";
