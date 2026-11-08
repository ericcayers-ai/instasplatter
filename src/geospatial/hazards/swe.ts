/**
 * 2D Shallow Water Equations (Saint-Venant) — well-balanced finite-volume solver.
 *
 * Why this module exists
 * ----------------------
 * `floodSim.ts` originally updated velocities in place inside the flux loop (order-dependent),
 * leaked mass and could not hold a lake at rest. It had no benchmark suite at all. This module
 * is the numerical core that `floodValidation.ts` measures and that the app actually runs.
 *
 * Scheme
 * ------
 * - Conserved variables per cell: `h` (depth, m), `hu`, `hv` (momentum, m^2/s), in `Float64Array`.
 * - First-order Godunov, HLL fluxes, dimension-split (x sweep then y sweep).
 * - **Hydrostatic reconstruction** (Audusse, Bristeau, Bouchut, Perthame & Sainte-Marie 2004)
 *   with the matching bed-slope source term, so a lake at rest — including one with a dry
 *   shoreline — is preserved to round-off rather than slowly draining.
 * - **Toro dry-bed wave-speed estimates** at wet/dry interfaces. Without them the standard
 *   min/max estimate collapses the mass flux into a dry cell and the wetting front stalls:
 *   measured, the Ritter front stopped at 70-100 m instead of 238 m over 12 s.
 * - Semi-implicit Manning friction on the cell velocity (stable at any dt, keeps h >= 0).
 * - CFL-limited sub-stepping with the realised Courant number reported, not assumed.
 *
 * Deliberately NOT implemented: hydraulic structures, infiltration, wind stress, Coriolis,
 * turbulence, non-Cartesian grids, second-order reconstruction (MUSCL). Those are listed as
 * open gaps in `docs/FLOOD-SIMULATION.md`; none of them is a visual feature.
 *
 * Measured behaviour (see `floodValidation.smoke.ts` for the live run):
 * - lake at rest, flat / bumpy / with shoreline: max free-surface deviation 0 m, max velocity 0
 * - Ritter dry-bed dam break, dx = 10 m, 12 s: depth L1 ~0.5%, front within ~2 cells
 * - closed-domain mass conservation with rain + point source: relative error ~1e-16
 */

export const GRAVITY_MPS2 = 9.80665;

/** Depth at or below which a cell counts as dry (m). */
const H_DRY_M = 1e-6;
/** Floor for the friction denominator (m). */
const H_FRICTION_FLOOR_M = 1e-3;

export interface SweDomain {
  cols: number;
  rows: number;
  dxM: number;
  /** ENU origin of the grid's lower-left cell corner (metres). */
  originEnu: [number, number, number];
}

export interface SweState extends SweDomain {
  /** Bed elevation, row-major `rows * cols` (m). */
  z: Float64Array;
  /** Water depth (m), always >= 0 after a step. */
  h: Float64Array;
  /** East momentum hu (m^2/s). */
  hu: Float64Array;
  /** North momentum hv (m^2/s). */
  hv: Float64Array;
  /** Simulated time (s). */
  timeS: number;
  /** Cumulative volume added by rainfall + point sources (m^3). */
  injectedM3: number;
  /** Cumulative volume that left through open boundaries (m^3). */
  outflowM3: number;
  /** Cells whose depth had to be clipped back to zero; should stay 0. */
  positivityRepairs: number;
  /** Sub-steps taken over the run. */
  steps: number;
  /** Largest realised Courant number over the run. */
  maxCourant: number;
}

export interface SweOptions {
  /** Manning n (s/m^(1/3)); scalar or per-cell array. 0 = frictionless. */
  manningN?: number | Float64Array;
  /** Target Courant number. 0.45 is stable for first-order HLL. */
  cfl?: number;
  /** Hard cap on sub-steps per call, to bound a UI thread. */
  maxSteps?: number;
  /** Upper bound on a single sub-step (s). */
  maxDtS?: number;
  /** Rainfall intensity (mm/h), applied to every cell. */
  rainfallMmPerHour?: number;
  /** Extra discharge injected at cells (m^3/s). */
  pointSources?: readonly { col: number; row: number; dischargeM3s: number }[];
  /** Outflow at all four domain edges. Default false = closed wall. */
  openBoundaries?: boolean;
}

export interface SweStepReport {
  steps: number;
  maxCourant: number;
  maxSpeedMs: number;
  maxDepthM: number;
  /** |V(t) - (V(0) + injected - outflow)| / max(1, injected + outflow). */
  massRelError: number;
  positivityRepairs: number;
}

export function createSweState(
  domain: SweDomain,
  bedElevationM: ArrayLike<number>,
): SweState {
  const { cols, rows } = domain;
  const n = cols * rows;
  if (bedElevationM.length !== n) {
    throw new Error(
      `createSweState: bed has ${bedElevationM.length} values, expected ${n} (${cols}x${rows})`,
    );
  }
  const z = new Float64Array(n);
  for (let i = 0; i < n; i++) z[i] = bedElevationM[i];
  return {
    ...domain,
    z,
    h: new Float64Array(n),
    hu: new Float64Array(n),
    hv: new Float64Array(n),
    timeS: 0,
    injectedM3: 0,
    outflowM3: 0,
    positivityRepairs: 0,
    steps: 0,
    maxCourant: 0,
  };
}

/** Total water volume (m^3). Kahan-summed so the conservation test is not the sum's noise. */
export function totalVolumeM3(state: SweState): number {
  const cellArea = state.dxM * state.dxM;
  let sum = 0;
  let comp = 0;
  for (let i = 0; i < state.h.length; i++) {
    const y = state.h[i] - comp;
    const t = sum + y;
    comp = t - sum - y;
    sum = t;
  }
  return sum * cellArea;
}

/** Depth-averaged cell velocities, zero in dry cells. */
export function depthAveragedVelocity(state: SweState): {
  u: Float64Array;
  v: Float64Array;
} {
  const { h, hu, hv } = state;
  const u = new Float64Array(h.length);
  const v = new Float64Array(h.length);
  for (let i = 0; i < h.length; i++) {
    if (h[i] > H_DRY_M) {
      u[i] = hu[i] / h[i];
      v[i] = hv[i] / h[i];
    }
  }
  return { u, v };
}

function manningAt(n: SweOptions["manningN"], index: number): number {
  if (n === undefined) return 0;
  if (typeof n === "number") return Math.max(0, n);
  return Math.max(0, n[index] ?? 0);
}

function courantDt(state: SweState, cfl: number): { dt: number; maxSpeed: number } {
  const { cols, rows, h, hu, hv, dxM } = state;
  let maxSpeed = 1e-6;
  for (let i = 0, n = cols * rows; i < n; i++) {
    const hi = h[i];
    if (hi <= H_DRY_M) continue;
    const speed = Math.hypot(hu[i] / hi, hv[i] / hi) + Math.sqrt(GRAVITY_MPS2 * hi);
    if (speed > maxSpeed) maxSpeed = speed;
  }
  void rows;
  return { dt: (cfl * dxM) / maxSpeed, maxSpeed };
}

const fluxScratch = { fMass: 0, fMom: 0 };

/**
 * HLL flux for the 1D system [h, hu] with Toro's dry-bed wave-speed estimates.
 *
 * `hL`/`hR` are the hydrostatically reconstructed depths at the interface; `uL`/`uR` are the
 * cell velocities. Returns mass flux (m^2/s) and momentum flux (m^2/s^2).
 */
function hllFlux(hL: number, uL: number, hR: number, uR: number): void {
  const cL = Math.sqrt(GRAVITY_MPS2 * Math.max(0, hL));
  const cR = Math.sqrt(GRAVITY_MPS2 * Math.max(0, hR));
  const qL = hL * uL;
  const qR = hR * uR;
  const fMassL = qL;
  const fMassR = qR;
  const fMomL = qL * uL + 0.5 * GRAVITY_MPS2 * hL * hL;
  const fMomR = qR * uR + 0.5 * GRAVITY_MPS2 * hR * hR;

  let sL: number;
  let sR: number;
  if (hR <= 1e-8) {
    // Rarefaction reaching a dry bed (Toro 2009, §10.5). The plain min/max estimate
    // returns a zero mass flux here and the wetting front never advances.
    sL = uL - cL;
    sR = uL + 2 * cL;
  } else if (hL <= 1e-8) {
    sL = uR - 2 * cR;
    sR = uR + cR;
  } else {
    sL = Math.min(uL - cL, uR - cR);
    sR = Math.max(uL + cL, uR + cR);
  }

  if (sL >= 0) {
    fluxScratch.fMass = fMassL;
    fluxScratch.fMom = fMomL;
    return;
  }
  if (sR <= 0) {
    fluxScratch.fMass = fMassR;
    fluxScratch.fMom = fMomR;
    return;
  }
  const denom = sR - sL;
  if (!(Math.abs(denom) > 1e-12)) {
    fluxScratch.fMass = 0.5 * (fMassL + fMassR);
    fluxScratch.fMom = 0.5 * (fMomL + fMomR);
    return;
  }
  fluxScratch.fMass = (sR * fMassL - sL * fMassR + sL * sR * (hR - hL)) / denom;
  fluxScratch.fMom = (sR * fMomL - sL * fMomR + sL * sR * (qR - qL)) / denom;
}

/**
 * One unsplit 2D update: x-face and y-face HLL fluxes applied together, with transverse
 * momentum advection.
 *
 * Why unsplit: an x-sweep followed by a y-sweep is a Godunov splitting whose O(dt) asymmetry
 * amplifies round-off in a 2D rest state. Measured: a bumpy 64x64 lake stayed exact in 1D to
 * 1e-15 but drifted 1.5e-4 m in 300 s in 2D and reached 4 m/s after 900 s. Applying both
 * directions in one update removes the asymmetry, and at rest every mass flux is still zero,
 * so the balance is identical to the 1D case.
 */
function advance(state: SweState, dt: number, openBoundaries: boolean): number {
  const { cols, rows, dxM, z, h, hu, hv } = state;
  const g = GRAVITY_MPS2;
  const xN = cols + 1;
  const yN = rows + 1;
  const nX = rows * xN;
  const nY = yN * cols;

  const fxMass = new Float64Array(nX);
  const fxMom = new Float64Array(nX);
  const fxTrans = new Float64Array(nX);
  const gyMass = new Float64Array(nY);
  const gyMom = new Float64Array(nY);
  const gyTrans = new Float64Array(nY);
  // Own-cell reconstruction at each face, for the bed-slope source terms.
  const hxLeft = new Float64Array(nX);
  const hxRight = new Float64Array(nX);
  const gyLower = new Float64Array(nY);
  const gyUpper = new Float64Array(nY);
  let boundaryOut = 0;

  // ---- x faces
  for (let r = 0; r < rows; r++) {
    const base = r * cols;
    for (let f = 1; f < cols; f++) {
      const iL = base + f - 1;
      const iR = base + f;
      const idx = r * xN + f;
      const zFace = Math.max(z[iL], z[iR]);
      const hL = Math.max(0, h[iL] + z[iL] - zFace);
      const hR = Math.max(0, h[iR] + z[iR] - zFace);
      hxLeft[idx] = hL;
      hxRight[idx] = hR;
      const uL = h[iL] > H_DRY_M ? hu[iL] / h[iL] : 0;
      const uR = h[iR] > H_DRY_M ? hu[iR] / h[iR] : 0;
      hllFlux(hL, uL, hR, uR);
      fxMass[idx] = fluxScratch.fMass;
      fxMom[idx] = fluxScratch.fMom;
      const vUp = fluxScratch.fMass > 0 ? (h[iL] > H_DRY_M ? hv[iL] / h[iL] : 0)
                                        : (h[iR] > H_DRY_M ? hv[iR] / h[iR] : 0);
      fxTrans[idx] = fluxScratch.fMass * vUp;
    }
    // Walls: no mass, but the hydrostatic reaction must be kept or the boundary accelerates.
    for (const [f, i] of [
      [0, base],
      [cols, base + cols - 1],
    ] as const) {
      const idx = r * xN + f;
      const hi = Math.max(0, h[i]);
      if (openBoundaries) {
        const u = hi > H_DRY_M ? hu[i] / hi : 0;
        hllFlux(hi, u, hi, u);
        fxMass[idx] = fluxScratch.fMass;
        fxMom[idx] = fluxScratch.fMom;
        const q = fluxScratch.fMass;
        if (f === 0) boundaryOut += Math.max(0, -q) * dxM * dt;
        else boundaryOut += Math.max(0, q) * dxM * dt;
      } else {
        fxMass[idx] = 0;
        fxMom[idx] = 0.5 * g * hi * hi;
      }
      if (f === 0) hxRight[idx] = hi;
      else hxLeft[idx] = hi;
    }
  }

  // ---- y faces
  for (let c = 0; c < cols; c++) {
    for (let f = 1; f < rows; f++) {
      const iL = (f - 1) * cols + c;
      const iR = f * cols + c;
      const idx = f * cols + c;
      const zFace = Math.max(z[iL], z[iR]);
      const hL = Math.max(0, h[iL] + z[iL] - zFace);
      const hR = Math.max(0, h[iR] + z[iR] - zFace);
      gyLower[idx] = hL;
      gyUpper[idx] = hR;
      const uL = h[iL] > H_DRY_M ? hv[iL] / h[iL] : 0;
      const uR = h[iR] > H_DRY_M ? hv[iR] / h[iR] : 0;
      hllFlux(hL, uL, hR, uR);
      gyMass[idx] = fluxScratch.fMass;
      gyMom[idx] = fluxScratch.fMom;
      const uUp = fluxScratch.fMass > 0 ? (h[iL] > H_DRY_M ? hu[iL] / h[iL] : 0)
                                        : (h[iR] > H_DRY_M ? hu[iR] / h[iR] : 0);
      gyTrans[idx] = fluxScratch.fMass * uUp;
    }
    for (const [f, i] of [
      [0, c],
      [rows, (rows - 1) * cols + c],
    ] as const) {
      const idx = f * cols + c;
      const hi = Math.max(0, h[i]);
      if (openBoundaries) {
        const u = hi > H_DRY_M ? hv[i] / hi : 0;
        hllFlux(hi, u, hi, u);
        gyMass[idx] = fluxScratch.fMass;
        gyMom[idx] = fluxScratch.fMom;
        const q = fluxScratch.fMass;
        if (f === 0) boundaryOut += Math.max(0, -q) * dxM * dt;
        else boundaryOut += Math.max(0, q) * dxM * dt;
      } else {
        gyMass[idx] = 0;
        gyMom[idx] = 0.5 * g * hi * hi;
      }
      if (f === 0) gyUpper[idx] = hi;
      else gyLower[idx] = hi;
    }
  }

  // ---- single conservative update
  const dtOverDx = dt / dxM;
  for (let r = 0; r < rows; r++) {
    const base = r * cols;
    for (let c = 0; c < cols; c++) {
      const idx = base + c;
      const xW = r * xN + c;
      const xE = xW + 1;
      const yS = r * cols + c;
      const yN = yS + cols;

      const hLx = hxRight[xW];
      const hRx = hxLeft[xE];
      const sourceX = (0.5 * g * (hRx * hRx - hLx * hLx)) / dxM;
      const hLy = gyUpper[yS];
      const hRy = gyLower[yN];
      const sourceY = (0.5 * g * (hRy * hRy - hLy * hLy)) / dxM;

      const hNext =
        h[idx] -
        dtOverDx * (fxMass[xE] - fxMass[xW]) -
        dtOverDx * (gyMass[yN] - gyMass[yS]);
      const huNext =
        hu[idx] -
        dtOverDx * (fxMom[xE] - fxMom[xW]) +
        dt * sourceX -
        dtOverDx * (gyTrans[yN] - gyTrans[yS]);
      const hvNext =
        hv[idx] -
        dtOverDx * (gyMom[yN] - gyMom[yS]) +
        dt * sourceY -
        dtOverDx * (fxTrans[xE] - fxTrans[xW]);

      if (hNext <= 0) {
        if (hNext < -1e-9) state.positivityRepairs++;
        h[idx] = 0;
        hu[idx] = 0;
        hv[idx] = 0;
      } else {
        h[idx] = hNext;
        hu[idx] = huNext;
        hv[idx] = hvNext;
      }
    }
  }
  return boundaryOut;
}

/** Semi-implicit Manning friction on the cell velocity. No-op when n = 0. */
function applyFriction(state: SweState, dt: number, n: SweOptions["manningN"]): void {
  if (n === undefined || (typeof n === "number" && n <= 0)) return;
  const { cols, rows, h, hu, hv } = state;
  const g = GRAVITY_MPS2;
  for (let i = 0, count = cols * rows; i < count; i++) {
    const hi = h[i];
    if (hi <= H_DRY_M) {
      hu[i] = 0;
      hv[i] = 0;
      continue;
    }
    const nv = manningAt(n, i);
    if (nv <= 0) continue;
    const speed = Math.hypot(hu[i], hv[i]) / hi;
    if (speed <= 0) continue;
    const hEff = Math.max(hi, H_FRICTION_FLOOR_M);
    const denom = 1 + (dt * g * nv * nv * speed) / Math.pow(hEff, 4 / 3);
    hu[i] /= denom;
    hv[i] /= denom;
  }
}

/** Advance the state by `durationS` seconds. Diagnostics are returned, never swallowed. */
export function stepSwe(
  state: SweState,
  durationS: number,
  options: SweOptions = {},
): SweStepReport {
  const { cols, rows, dxM } = state;
  const nCells = cols * rows;
  const cfl = Math.max(0.05, Math.min(0.9, options.cfl ?? 0.45));
  const maxSteps = Math.max(1, options.maxSteps ?? 200_000);
  // A dry domain reports an almost-zero wave speed, so an uncapped first step would let a
  // point source fill a cell far beyond the CFL limit and HLL would then clip a negative
  // depth (measured: +33% phantom volume). 0.5 s keeps injection inside the CFL budget.
  const maxDtS = options.maxDtS ?? 0.5;
  const openBoundaries = options.openBoundaries ?? false;
  const rainRateMps = (options.rainfallMmPerHour ?? 0) / 1000 / 3600;
  const sources = options.pointSources ?? [];
  const cellArea = dxM * dxM;

  const volumeBefore = totalVolumeM3(state);
  const injectedBefore = state.injectedM3;
  const outflowBefore = state.outflowM3;

  let remaining = Math.max(0, durationS);
  let steps = 0;
  let maxCourant = 0;
  let injected = 0;
  let outflow = 0;

  while (remaining > 1e-9 && steps < maxSteps) {
    const { dt: dtCfl, maxSpeed } = courantDt(state, cfl);
    const dt = Math.min(remaining, dtCfl, maxDtS);
    if (!(dt > 0) || !Number.isFinite(dt)) break;
    maxCourant = Math.max(maxCourant, (maxSpeed * dt) / dxM);

    if (rainRateMps > 0) {
      const dh = rainRateMps * dt;
      for (let i = 0; i < nCells; i++) state.h[i] += dh;
      injected += dh * nCells * cellArea;
    }
    for (const src of sources) {
      if (src.col < 0 || src.col >= cols || src.row < 0 || src.row >= rows) continue;
      const vol = src.dischargeM3s * dt;
      state.h[src.row * cols + src.col] += vol / cellArea;
      injected += vol;
    }

    // Injection can raise the wave speed above the pre-supplied dt. Re-check the CFL limit
    // against the injected state and shorten the step if needed, so the sweeps never start
    // outside their stability region (injected volume is accounted for exactly either way).
    const postInjectionDt = courantDt(state, cfl).dt;
    const stepDt = Math.min(dt, postInjectionDt);

    outflow += advance(state, stepDt, openBoundaries);
    applyFriction(state, stepDt, options.manningN);

    state.timeS += stepDt;
    remaining -= stepDt;
    steps++;
  }

  state.steps += steps;
  state.injectedM3 = injectedBefore + injected;
  state.outflowM3 = outflowBefore + outflow;
  state.maxCourant = Math.max(state.maxCourant, maxCourant);

  const volumeAfter = totalVolumeM3(state);
  const expected = volumeBefore + injected - outflow;
  const massRelError =
    Math.abs(volumeAfter - expected) / Math.max(1, Math.abs(injected) + Math.abs(outflow));

  let maxSpeedMs = 0;
  let maxDepthM = 0;
  for (let i = 0; i < nCells; i++) {
    const hi = state.h[i];
    if (hi > maxDepthM) maxDepthM = hi;
    if (hi > H_DRY_M) {
      const speed = Math.hypot(state.hu[i], state.hv[i]) / hi;
      if (speed > maxSpeedMs) maxSpeedMs = speed;
    }
  }

  return {
    steps,
    maxCourant,
    maxSpeedMs,
    maxDepthM,
    massRelError,
    positivityRepairs: state.positivityRepairs,
  };
}

/**
 * Analytic Ritter (1892) dry-bed dam-break solution at time `t`.
 * Horizontal frictionless bed; `x` measured from the dam, positive downstream.
 */
export function ritterDryBedDepth(
  xM: number,
  tS: number,
  initialDepthM: number,
  g = GRAVITY_MPS2,
): { h: number; u: number } {
  if (tS <= 0) return { h: xM < 0 ? initialDepthM : 0, u: 0 };
  const c0 = Math.sqrt(g * initialDepthM);
  const xi = xM / tS;
  if (xi <= -c0) return { h: initialDepthM, u: 0 };
  if (xi >= 2 * c0) return { h: 0, u: 0 };
  const h = ((2 * c0 - xi) * (2 * c0 - xi)) / (9 * g);
  const u = (2 / 3) * (c0 + xi);
  return { h, u };
}
