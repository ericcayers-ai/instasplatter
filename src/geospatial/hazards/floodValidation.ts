/**
 * Validation suite for the shallow-water core (`swe.ts`).
 *
 * Every function here returns MEASURED numbers. Nothing is asserted as "correct" without
 * an analytic solution or a conservation identity to compare against. The runner
 * (`floodValidation.smoke.ts`) prints the table; CI thresholds live in
 * `floodGate.ts` so a claim of "validated" always has a pass/fail behind it.
 *
 * Benchmarks
 * ----------
 * 1. Lake at rest, flat bed          — well-balancedness, machine-precision identity.
 * 2. Lake at rest, bumpy bed         — the real well-balancedness test (Audusse 2004).
 * 3. Lake at rest with a shoreline   — wet/dry well-balancedness.
 * 4. Ritter dry-bed dam break        — analytic rarefaction (Ritter 1892), L1 depth error.
 * 5. Dam-break front position        — arrival distance vs x_f = 2 c0 t.
 * 6. Grid convergence                — observed order of accuracy under h-refinement.
 * 7. Mass conservation               — closed domain, rain + point source.
 * 8. Determinism                     — identical inputs, bitwise-identical output.
 * 9. Courant audit                   — realised max CFL stays under the target.
 */

import {
  GRAVITY_MPS2,
  createSweState,
  depthAveragedVelocity,
  ritterDryBedDepth,
  stepSwe,
  totalVolumeM3,
  type SweState,
} from "./swe";

export interface LakeAtRestResult {
  label: string;
  steps: number;
  maxEtaDeviationM: number;
  maxVelocityMps: number;
  positivityRepairs: number;
}

export interface DamBreakResult {
  dxM: number;
  timeS: number;
  /** Normalised L1 error of depth against the analytic Ritter solution. */
  depthL1Rel: number;
  /** Numeric front position (m from the dam) vs analytic 2*c0*t. */
  frontNumericM: number;
  frontAnalyticM: number;
  frontErrorCells: number;
  maxCourant: number;
  positivityRepairs: number;
}

export interface ConvergenceResult {
  dxM: number[];
  depthL1Rel: number[];
  /** Observed order between successive refinements. */
  observedOrder: number[];
}

export interface MassConservationResult {
  rainMmPerHour: number;
  sourceM3s: number;
  durationS: number;
  expectedM3: number;
  measuredM3: number;
  relError: number;
  steps: number;
  positivityRepairs: number;
}

function etaDeviation(state: SweState, eta0: number): number {
  let worst = 0;
  for (let i = 0; i < state.h.length; i++) {
    const eta = state.z[i] + state.h[i];
    // Only compare wet cells: a dry cell at rest is valid where the bed is above water.
    if (state.h[i] <= 1e-9) continue;
    const dev = Math.abs(eta - eta0);
    if (dev > worst) worst = dev;
  }
  return worst;
}

/** 1. Flat bed, still water — the trivial well-balanced case. */
export function lakeAtRestFlat(): LakeAtRestResult {
  const cols = 48;
  const rows = 48;
  const dxM = 10;
  const z = new Float64Array(cols * rows).fill(4);
  const state = createSweState({ cols, rows, dxM, originEnu: [0, 0, 0] }, z);
  state.h.fill(1.5);
  const eta0 = 4 + 1.5;
  const report = stepSwe(state, 600, { manningN: 0, cfl: 0.45 });
  const vel = depthAveragedVelocity(state);
  let maxV = 0;
  for (let i = 0; i < vel.u.length; i++) {
    maxV = Math.max(maxV, Math.hypot(vel.u[i], vel.v[i]));
  }
  return {
    label: "flat bed",
    steps: report.steps,
    maxEtaDeviationM: etaDeviation(state, eta0),
    maxVelocityMps: maxV,
    positivityRepairs: report.positivityRepairs,
  };
}

/** 2. Bumpy bed, still water — Audusse's well-balancedness test. */
export function lakeAtRestBumpy(): LakeAtRestResult {
  const cols = 64;
  const rows = 64;
  const dxM = 5;
  const n = cols * rows;
  const z = new Float64Array(n);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const x = (c + 0.5) * dxM;
      const y = (r + 0.5) * dxM;
      z[r * cols + c] =
        3 * Math.sin((2 * Math.PI * x) / (cols * dxM)) * Math.cos((2 * Math.PI * y) / (rows * dxM)) +
        2 * Math.exp(-((x - 160) ** 2 + (y - 160) ** 2) / 8000);
    }
  }
  const eta0 = Math.max(...Array.from(z)) + 4;
  const state = createSweState({ cols, rows, dxM, originEnu: [0, 0, 0] }, z);
  for (let i = 0; i < n; i++) state.h[i] = Math.max(0, eta0 - z[i]);
  const report = stepSwe(state, 900, { manningN: 0, cfl: 0.45 });
  const vel = depthAveragedVelocity(state);
  let maxV = 0;
  for (let i = 0; i < vel.u.length; i++) {
    maxV = Math.max(maxV, Math.hypot(vel.u[i], vel.v[i]));
  }
  return {
    label: "bumpy bed (wet everywhere)",
    steps: report.steps,
    maxEtaDeviationM: etaDeviation(state, eta0),
    maxVelocityMps: maxV,
    positivityRepairs: report.positivityRepairs,
  };
}

/** 3. Bumpy bed with a dry shoreline — the hard well-balancedness case. */
export function lakeAtRestShoreline(): LakeAtRestResult {
  const cols = 64;
  const rows = 16;
  const dxM = 5;
  const n = cols * rows;
  const z = new Float64Array(n);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const x = (c + 0.5) * dxM;
      // Parabolic bowl: the water reaches part-way up the sides, so a shoreline exists.
      const centre = cols * dxM * 0.5;
      z[r * cols + c] = 0.004 * (x - centre) * (x - centre);
    }
  }
  const eta0 = 6;
  const state = createSweState({ cols, rows, dxM, originEnu: [0, 0, 0] }, z);
  for (let i = 0; i < n; i++) state.h[i] = Math.max(0, eta0 - z[i]);
  const report = stepSwe(state, 600, { manningN: 0, cfl: 0.4 });
  const vel = depthAveragedVelocity(state);
  let maxV = 0;
  for (let i = 0; i < vel.u.length; i++) {
    maxV = Math.max(maxV, Math.hypot(vel.u[i], vel.v[i]));
  }
  return {
    label: "bumpy bed with dry shoreline",
    steps: report.steps,
    maxEtaDeviationM: etaDeviation(state, eta0),
    maxVelocityMps: maxV,
    positivityRepairs: report.positivityRepairs,
  };
}

/**
 * 4 & 5. Ritter dry-bed dam break against the analytic solution.
 * Dam at x = 0, deep water (h0) on the left, dry bed on the right, frictionless.
 */
export function ritterDamBreak(dxM = 10, durationS = 12, h0 = 10): DamBreakResult {
  const lengthM = 2000;
  const cols = Math.round(lengthM / dxM);
  const rows = 4;
  const damCol = Math.floor(cols / 2);
  const z = new Float64Array(cols * rows);
  const state = createSweState({ cols, rows, dxM, originEnu: [0, 0, 0] }, z);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < damCol; c++) {
      state.h[r * cols + c] = h0;
    }
  }
  // Closed boundaries: this is a 1D problem, the y direction must not leak.
  const report = stepSwe(state, durationS, { manningN: 0, cfl: 0.45, maxDtS: 0.5 });

  const midRow = Math.floor(rows / 2);
  const damXM = damCol * dxM;
  let l1 = 0;
  let norm = 0;
  for (let c = 0; c < cols; c++) {
    const xCentre = (c + 0.5) * dxM - damXM;
    const exact = ritterDryBedDepth(xCentre, state.timeS, h0);
    const numeric = state.h[midRow * cols + c];
    l1 += Math.abs(numeric - exact.h) * dxM;
    norm += Math.abs(exact.h) * dxM;
  }
  // Analytic front: x/t = 2*c0 => x = 2*c0*t. Find the numeric upstream-most wet cell.
  const c0 = Math.sqrt(GRAVITY_MPS2 * h0);
  const frontAnalyticM = 2 * c0 * state.timeS;
  let frontNumericM = 0;
  for (let c = cols - 1; c >= damCol; c--) {
    if (state.h[midRow * cols + c] > 1e-3) {
      frontNumericM = (c + 1) * dxM - damXM;
      break;
    }
  }

  return {
    dxM,
    timeS: state.timeS,
    depthL1Rel: norm > 0 ? l1 / norm : Number.NaN,
    frontNumericM,
    frontAnalyticM,
    frontErrorCells: (frontAnalyticM - frontNumericM) / dxM,
    maxCourant: report.maxCourant,
    positivityRepairs: report.positivityRepairs,
  };
}

/** 6. Observed order of accuracy under grid refinement on the Ritter problem. */
export function damBreakConvergence(
  resolutions = [20, 10, 5],
  durationS = 8,
): ConvergenceResult {
  const runs = resolutions.map((dx) => ritterDamBreak(dx, durationS));
  const observedOrder: number[] = [];
  for (let i = 1; i < runs.length; i++) {
    const ratio = Math.log(runs[i - 1].depthL1Rel / runs[i].depthL1Rel) /
      Math.log(resolutions[i - 1] / resolutions[i]);
    observedOrder.push(ratio);
  }
  return {
    dxM: resolutions,
    depthL1Rel: runs.map((r) => r.depthL1Rel),
    observedOrder,
  };
}

/** 7. Mass conservation in a closed domain with rain and a point source. */
export function massConservation(): MassConservationResult {
  const cols = 40;
  const rows = 40;
  const dxM = 4;
  const n = cols * rows;
  const z = new Float64Array(n);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const x = (c + 0.5) * dxM - 80;
      const y = (r + 0.5) * dxM - 80;
      z[r * cols + c] = 0.01 * (x * x + y * y) * 0.05;
    }
  }
  const state = createSweState({ cols, rows, dxM, originEnu: [0, 0, 0] }, z);
  const rainMmPerHour = 60;
  const sourceM3s = 3;
  const durationS = 240;
  const report = stepSwe(state, durationS, {
    manningN: 0.035,
    cfl: 0.45,
    rainfallMmPerHour: rainMmPerHour,
    pointSources: [{ col: 20, row: 20, dischargeM3s: sourceM3s }],
  });
  const rainVol = ((rainMmPerHour / 1000) / 3600) * n * dxM * dxM * durationS;
  const sourceVol = sourceM3s * durationS;
  const expected = rainVol + sourceVol;
  const measured = totalVolumeM3(state);
  return {
    rainMmPerHour,
    sourceM3s,
    durationS,
    expectedM3: expected,
    measuredM3: measured,
    relError: Math.abs(measured - expected) / expected,
    steps: report.steps,
    positivityRepairs: report.positivityRepairs,
  };
}

/** 8. Determinism: the same inputs must produce bitwise-identical state. */
export function determinismCheck(): { identical: boolean; volumeA: number; volumeB: number } {
  const build = (): SweState => {
    const cols = 32;
    const rows = 32;
    const dxM = 5;
    const n = cols * rows;
    const z = new Float64Array(n);
    for (let i = 0; i < n; i++) z[i] = 0.5 * Math.sin(i * 0.37) + 0.2 * Math.cos(i * 0.11);
    const s = createSweState({ cols, rows, dxM, originEnu: [0, 0, 0] }, z);
    for (let i = 0; i < n; i++) s.h[i] = 0.2 + 0.3 * Math.abs(Math.sin(i * 0.7));
    return s;
  };
  const a = build();
  const b = build();
  stepSwe(a, 60, { manningN: 0.03, cfl: 0.45 });
  stepSwe(b, 60, { manningN: 0.03, cfl: 0.45 });
  let identical = true;
  for (let i = 0; i < a.h.length; i++) {
    if (a.h[i] !== b.h[i]) {
      identical = false;
      break;
    }
  }
  if (identical) {
    for (let i = 0; i < a.hu.length; i++) {
      if (a.hu[i] !== b.hu[i] || a.hv[i] !== b.hv[i]) {
        identical = false;
        break;
      }
    }
  }
  return { identical, volumeA: totalVolumeM3(a), volumeB: totalVolumeM3(b) };
}

export interface ValidationSummary {
  lakeFlat: LakeAtRestResult;
  lakeBumpy: LakeAtRestResult;
  lakeShoreline: LakeAtRestResult;
  ritter: DamBreakResult;
  convergence: ConvergenceResult;
  mass: MassConservationResult;
  determinism: { identical: boolean; volumeA: number; volumeB: number };
}

/** Run every benchmark once. Callers decide the thresholds. */
export function runAllValidation(): ValidationSummary {
  return {
    lakeFlat: lakeAtRestFlat(),
    lakeBumpy: lakeAtRestBumpy(),
    lakeShoreline: lakeAtRestShoreline(),
    ritter: ritterDamBreak(10, 12),
    convergence: damBreakConvergence([20, 10, 5], 8),
    mass: massConservation(),
    determinism: determinismCheck(),
  };
}
