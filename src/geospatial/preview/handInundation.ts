/**
 * HAND-inspired rapid inundation (Height Above Nearest Drainage).
 *
 * Local equivalent of NOAA-OWP stage→extent mapping: approximate drainage
 * elevation from the lowest cells in a neighbourhood, then wet cells where
 * (z − z_drain) < stage. Labelled **Live preview / non-authoritative** until
 * ANUGA compare gates pass — not a substitute for calibrated SWE.
 */

import { H_DRY } from "./softSolver";

export const HAND_AUTHORITY_LABEL = "Live preview · HAND (non-authoritative)";

export interface HandInundationOpts {
  /** Water-surface stage above local drainage (m). */
  stageM: number;
  /** Neighbourhood half-width (cells) for nearest-drainage search. */
  searchRadius?: number;
  /** Optional rainfall residual depth on dry cells (m). */
  residualDepthM?: number;
}

export interface HandInundationResult {
  /** Depth field (m), row-major. */
  h: Float32Array;
  /** Approximate HAND (m), row-major. */
  hand: Float32Array;
  wetFraction: number;
  maxDepthM: number;
  authority: typeof HAND_AUTHORITY_LABEL;
}

/**
 * Compute HAND and inundate where HAND < stage.
 * Drainage elevation ≈ min(z) in a local window (channel/thalweg proxy).
 */
export function handInundate(
  z: Float32Array,
  cols: number,
  rows: number,
  opts: HandInundationOpts,
): HandInundationResult {
  const stage = Math.max(0, opts.stageM);
  const radius = Math.max(1, Math.floor(opts.searchRadius ?? Math.max(2, Math.round(Math.min(cols, rows) / 12))));
  const residual = Math.max(0, opts.residualDepthM ?? 0);
  const n = cols * rows;
  const hand = new Float32Array(n);
  const h = new Float32Array(n);

  // Precompute rolling min drainage approx.
  for (let j = 0; j < rows; j++) {
    for (let i = 0; i < cols; i++) {
      const k = j * cols + i;
      let zDrain = z[k];
      const i0 = Math.max(0, i - radius);
      const i1 = Math.min(cols - 1, i + radius);
      const j0 = Math.max(0, j - radius);
      const j1 = Math.min(rows - 1, j + radius);
      for (let jj = j0; jj <= j1; jj++) {
        for (let ii = i0; ii <= i1; ii++) {
          const zz = z[jj * cols + ii];
          if (zz < zDrain) zDrain = zz;
        }
      }
      const handM = Math.max(0, z[k] - zDrain);
      hand[k] = handM;
      if (handM < stage) {
        h[k] = Math.max(residual, stage - handM);
      } else {
        h[k] = residual > H_DRY ? residual * 0.15 : 0;
      }
    }
  }

  let wet = 0;
  let maxDepth = 0;
  for (let i = 0; i < n; i++) {
    if (h[i] > H_DRY) {
      wet++;
      maxDepth = Math.max(maxDepth, h[i]);
    } else {
      h[i] = 0;
    }
  }

  return {
    h,
    hand,
    wetFraction: wet / n,
    maxDepthM: maxDepth,
    authority: HAND_AUTHORITY_LABEL,
  };
}

/** Stage (m) from normalised scenario time using a simple rise–peak–recede curve. */
export function handStageFromTime01(t01: number, peakStageM = 1.5): number {
  const t = Math.max(0, Math.min(1, t01));
  // Peak near t=0.4, similar to placeholder hydrograph.
  const rise = Math.sin(Math.min(1, t / 0.4) * (Math.PI / 2));
  const fall = t <= 0.4 ? 1 : Math.exp(-(t - 0.4) / 0.35);
  return peakStageM * rise * (t <= 0.4 ? 1 : fall);
}

/** Smoke: flat bed stays dry at stage 0; depressed cell wets when stage > HAND. */
export function handInundationSmokeOk(): boolean {
  const cols = 8;
  const rows = 6;
  const z = new Float32Array(cols * rows);
  z.fill(10);
  // Channel through middle row.
  for (let i = 0; i < cols; i++) z[3 * cols + i] = 8;
  const dry = handInundate(z, cols, rows, { stageM: 0 });
  if (dry.wetFraction > 0.05) return false;
  const wet = handInundate(z, cols, rows, { stageM: 1.5 });
  return wet.wetFraction > 0.1 && wet.maxDepthM > 0.5;
}
