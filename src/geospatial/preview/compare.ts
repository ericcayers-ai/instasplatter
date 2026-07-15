import type { PreviewCompareReport, ScientificCheckpoint } from "./types";
import { H_DRY } from "./softSolver";

/** Default tolerances before calling a preview “validated” against ANUGA. */
export const DEFAULT_COMPARE_TOLERANCE = {
  /** Relative mass error. */
  massRel: 0.15,
  /** Absolute max-depth difference (m). */
  maxDepthM: 0.35,
  /** Wet-fraction absolute delta. */
  wetFraction: 0.12,
  /** Depth RMSE when samples align (m). */
  depthRmseM: 0.4,
  /** Minimum intersection-over-union of wet extents. */
  extentIou: 0.7,
};

export function compareAgainstCheckpoint(
  preview: {
    maxDepthM: number;
    wetFraction: number;
    massM3: number;
    h?: Float32Array;
    cols?: number;
    rows?: number;
  },
  checkpoint: ScientificCheckpoint,
  tol = DEFAULT_COMPARE_TOLERANCE,
): PreviewCompareReport {
  const massRelError =
    checkpoint.massM3 > 1e-3
      ? Math.abs(preview.massM3 - checkpoint.massM3) / checkpoint.massM3
      : preview.massM3 < 1e-3
        ? 0
        : 1;
  const maxDepthDeltaM = Math.abs(preview.maxDepthM - checkpoint.maxDepthM);
  const wetFractionDelta = Math.abs(preview.wetFraction - checkpoint.wetFraction);

  let depthRmseM: number | null = null;
  let extentIou: number | null = null;

  if (
    preview.h &&
    checkpoint.depthSample &&
    preview.cols &&
    preview.rows &&
    checkpoint.sampleCols &&
    checkpoint.sampleRows &&
    preview.cols === checkpoint.sampleCols &&
    preview.rows === checkpoint.sampleRows
  ) {
    const n = preview.cols * preview.rows;
    let sse = 0;
    let inter = 0;
    let uni = 0;
    for (let i = 0; i < n; i++) {
      const a = preview.h[i];
      const b = checkpoint.depthSample[i];
      const d = a - b;
      sse += d * d;
      const aw = a > H_DRY;
      const bw = b > H_DRY;
      if (aw && bw) inter++;
      if (aw || bw) uni++;
    }
    depthRmseM = Math.sqrt(sse / n);
    extentIou = uni > 0 ? inter / uni : 1;
  }

  const withinTolerance =
    massRelError <= tol.massRel &&
    maxDepthDeltaM <= tol.maxDepthM &&
    wetFractionDelta <= tol.wetFraction &&
    (depthRmseM === null || depthRmseM <= tol.depthRmseM) &&
    (extentIou === null || extentIou >= tol.extentIou);

  return {
    depthRmseM,
    extentIou,
    massRelError,
    wetFractionDelta,
    maxDepthDeltaM,
    withinTolerance,
    checkpointTimeS: checkpoint.timeS,
  };
}

/** Pick nearest scientific checkpoint by scenario time. */
export function nearestCheckpoint(
  checkpoints: ScientificCheckpoint[],
  timeS: number,
): ScientificCheckpoint | null {
  if (!checkpoints.length) return null;
  let best = checkpoints[0];
  let bestD = Math.abs(best.timeS - timeS);
  for (let i = 1; i < checkpoints.length; i++) {
    const d = Math.abs(checkpoints[i].timeS - timeS);
    if (d < bestD) {
      best = checkpoints[i];
      bestD = d;
    }
  }
  return best;
}
