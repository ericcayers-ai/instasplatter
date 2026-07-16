import type { GridState, PreviewDomain, PreviewForcing, PreviewStats } from "./types";

const G = 9.81;
export const H_DRY = 5e-4;

/**
 * Fallback preview domain used only when no AOI has been committed yet
 * (tests / cold start). Live UI rebinds via `domainFromAoi` after draw.
 */
export const DEFAULT_DOMAIN: PreviewDomain = {
  bounds: [174.762, -41.298, 174.796, -41.275],
  dxM: 12,
  cols: 96,
  rows: 72,
  coarseFactor: 2,
};

export function createEmptyState(cols: number, rows: number, dx: number, timeS = 0): GridState {
  const n = cols * rows;
  return {
    cols,
    rows,
    dx,
    z: new Float32Array(n),
    h: new Float32Array(n),
    u: new Float32Array(n),
    v: new Float32Array(n),
    timeS,
  };
}

export function cloneState(s: GridState): GridState {
  return {
    cols: s.cols,
    rows: s.rows,
    dx: s.dx,
    z: s.z.slice(),
    h: s.h.slice(),
    u: s.u.slice(),
    v: s.v.slice(),
    timeS: s.timeS,
  };
}

export function copyState(dst: GridState, src: GridState): void {
  dst.cols = src.cols;
  dst.rows = src.rows;
  dst.dx = src.dx;
  dst.timeS = src.timeS;
  dst.z.set(src.z);
  dst.h.set(src.h);
  dst.u.set(src.u);
  dst.v.set(src.v);
}

/** Synthetic DEM: gentle basin + channel + harbour shelf. */
export function buildSyntheticBed(cols: number, rows: number): Float32Array {
  const z = new Float32Array(cols * rows);
  for (let j = 0; j < rows; j++) {
    for (let i = 0; i < cols; i++) {
      const xn = i / (cols - 1);
      const yn = j / (rows - 1);
      // Base slope south → harbour (lower y in local northing is lower elevation).
      let elev = 4.2 - yn * 3.4 + (xn - 0.5) * (xn - 0.5) * 1.8;
      // Main channel along a slight diagonal.
      const channel = Math.exp(-((xn - 0.42 - yn * 0.18) ** 2) / 0.012);
      elev -= channel * 1.6;
      // Small depression (ponding).
      const basin = Math.exp(-((xn - 0.62) ** 2 + (yn - 0.55) ** 2) / 0.02);
      elev -= basin * 0.9;
      // Levee stub.
      if (Math.abs(xn - 0.55) < 0.03 && yn > 0.35 && yn < 0.7) elev += 1.1;
      z[j * cols + i] = elev;
    }
  }
  return z;
}

export function resolveDomain(partial?: Partial<PreviewDomain>, lowPower = false): PreviewDomain {
  // Prefer caller-supplied bounds/grid; DEFAULT_DOMAIN is only the cold-start fallback.
  const base: PreviewDomain = {
    bounds: partial?.bounds ?? DEFAULT_DOMAIN.bounds,
    dxM: partial?.dxM ?? DEFAULT_DOMAIN.dxM,
    cols: partial?.cols ?? DEFAULT_DOMAIN.cols,
    rows: partial?.rows ?? DEFAULT_DOMAIN.rows,
    coarseFactor: partial?.coarseFactor ?? DEFAULT_DOMAIN.coarseFactor,
  };
  if (lowPower) {
    return {
      ...base,
      cols: Math.max(32, Math.round(base.cols / 2)),
      rows: Math.max(24, Math.round(base.rows / 2)),
      dxM: base.dxM * 2,
      coarseFactor: Math.max(2, base.coarseFactor),
    };
  }
  return base;
}

/** Linear-blend display frames between two physics states. */
export function lerpStates(a: GridState, b: GridState, t: number, out: GridState): void {
  const u = Math.max(0, Math.min(1, t));
  const n = a.cols * a.rows;
  out.cols = a.cols;
  out.rows = a.rows;
  out.dx = a.dx;
  out.timeS = a.timeS + (b.timeS - a.timeS) * u;
  out.z.set(a.z);
  for (let i = 0; i < n; i++) {
    out.h[i] = a.h[i] + (b.h[i] - a.h[i]) * u;
    out.u[i] = a.u[i] + (b.u[i] - a.u[i]) * u;
    out.v[i] = a.v[i] + (b.v[i] - a.v[i]) * u;
  }
}

export function computeStats(state: GridState, dtS: number, cfl: number): PreviewStats {
  const { cols, rows, dx, h, u, v } = state;
  const n = cols * rows;
  let maxDepth = 0;
  let sumDepth = 0;
  let wet = 0;
  let mass = 0;
  let maxSpeed = 0;
  for (let i = 0; i < n; i++) {
    const d = h[i];
    if (d > H_DRY) {
      wet++;
      sumDepth += d;
      mass += d * dx * dx;
      maxDepth = Math.max(maxDepth, d);
      maxSpeed = Math.max(maxSpeed, Math.hypot(u[i], v[i]));
    }
  }
  const meanDepth = wet > 0 ? sumDepth / wet : 0;
  let hazardClass = 0;
  const hazardScore = maxDepth * Math.max(1, maxSpeed);
  if (hazardScore >= 1.8 || maxDepth >= 1.6) hazardClass = 3;
  else if (hazardScore >= 0.9 || maxDepth >= 1.0) hazardClass = 2;
  else if (hazardScore >= 0.25 || maxDepth >= 0.35) hazardClass = 1;

  return {
    timeS: state.timeS,
    maxDepthM: maxDepth,
    meanDepthM: meanDepth,
    wetFraction: wet / n,
    massM3: mass,
    maxSpeedMs: maxSpeed,
    hazardClass,
    cfl,
    dtS,
  };
}

/** Relative mass error for preview soft-solver conservation checks. */
export function massRelError(aM3: number, bM3: number): number {
  const denom = Math.max(Math.abs(bM3), 1e-6);
  return Math.abs(aM3 - bM3) / denom;
}

/**
 * Lake-at-rest smoke: flat bed, still water, no forcing — mass and peak depth
 * should stay within soft tolerances after `steps` (non-authoritative preview).
 */
export function lakeAtRestMassOk(
  cols = 32,
  rows = 24,
  depthM = 0.5,
  steps = 40,
  massTol = 0.05,
  depthTolM = 0.02,
): boolean {
  const dx = 10;
  const state = createEmptyState(cols, rows, dx);
  state.z.fill(0);
  state.h.fill(depthM);
  const forcing: PreviewForcing = {
    rainfallMmHr: 0,
    infiltrationMmHr: 0,
    manningN: 0.03,
    inflowCms: 0,
  };
  const t0 = computeStats(state, 0, 0);
  for (let i = 0; i < steps; i++) softStep(state, forcing, 0.45, 1.0);
  const t1 = computeStats(state, 0, 0);
  return (
    massRelError(t1.massM3, t0.massM3) <= massTol &&
    Math.abs(t1.maxDepthM - depthM) <= depthTolM
  );
}

/**
 * One CFL-aware shallow-sheet step (diffusive / Manning flux continuity).
 * Stable enough for live graphics; not a scientific SWE substitute.
 */
export function softStep(
  state: GridState,
  forcing: PreviewForcing,
  cflTarget: number,
  maxDtS: number,
): { dtS: number; cfl: number } {
  const { cols, rows, dx, z, h, u, v } = state;
  const n = cols * rows;
  const hn = new Float32Array(n);
  const un = new Float32Array(n);
  const vn = new Float32Array(n);

  let hMax = H_DRY;
  let velMax = 0;
  for (let i = 0; i < n; i++) {
    if (h[i] > hMax) hMax = h[i];
    const sp = Math.hypot(u[i], v[i]);
    if (sp > velMax) velMax = sp;
  }
  const wave = Math.sqrt(G * hMax) + velMax + 0.25;
  let dt = (cflTarget * dx) / wave;
  dt = Math.min(dt, maxDtS);
  const cfl = (wave * dt) / dx;

  const rainM = (forcing.rainfallMmHr / 1000) * (dt / 3600);
  const infilM = (forcing.infiltrationMmHr / 1000) * (dt / 3600);
  const nMan = Math.max(0.01, forcing.manningN);

  // Momentum from free-surface slope + Manning resistance.
  for (let j = 0; j < rows; j++) {
    for (let i = 0; i < cols; i++) {
      const k = j * cols + i;
      const depth = h[k];
      if (depth <= H_DRY) {
        un[k] = 0;
        vn[k] = 0;
        continue;
      }
      const il = i > 0 ? i - 1 : i;
      const ir = i < cols - 1 ? i + 1 : i;
      const jd = j > 0 ? j - 1 : j;
      const ju = j < rows - 1 ? j + 1 : j;
      const eta = z[k] + depth;
      const etaL = z[j * cols + il] + h[j * cols + il];
      const etaR = z[j * cols + ir] + h[j * cols + ir];
      const etaD = z[jd * cols + i] + h[jd * cols + i];
      const etaU = z[ju * cols + i] + h[ju * cols + i];
      const sx = (etaR - etaL) / ((ir - il || 1) * dx);
      const sy = (etaU - etaD) / ((ju - jd || 1) * dx);
      const slope = Math.hypot(sx, sy);
      const friction = (G * nMan * nMan) / Math.pow(Math.max(depth, H_DRY), 4 / 3);
      const damp = 1 / (1 + friction * dt);
      let uu = u[k] - G * sx * dt;
      let vv = v[k] - G * sy * dt;
      uu *= damp;
      vv *= damp;
      // Cap runaway velocities.
      const cap = 8 + 6 * Math.sqrt(Math.max(0, depth));
      const sp = Math.hypot(uu, vv);
      if (sp > cap) {
        uu = (uu / sp) * cap;
        vv = (vv / sp) * cap;
      }
      // Mild drain when nearly flat & shallow (helps recession).
      if (slope < 1e-5 && depth < 0.08) {
        uu *= 0.85;
        vv *= 0.85;
      }
      un[k] = uu;
      vn[k] = vv;
    }
  }

  // Continuity with donor-cell fluxes.
  for (let j = 0; j < rows; j++) {
    for (let i = 0; i < cols; i++) {
      const k = j * cols + i;
      let flux = 0;
      // East face
      if (i < cols - 1) {
        const ke = k + 1;
        const ue = 0.5 * (un[k] + un[ke]);
        const he = ue >= 0 ? h[k] : h[ke];
        flux += ue * Math.max(0, he);
      }
      // West face
      if (i > 0) {
        const kw = k - 1;
        const uw = 0.5 * (un[k] + un[kw]);
        const hw = uw >= 0 ? h[kw] : h[k];
        flux -= uw * Math.max(0, hw);
      }
      // North face
      if (j < rows - 1) {
        const kn = k + cols;
        const vnFace = 0.5 * (vn[k] + vn[kn]);
        const hnFace = vnFace >= 0 ? h[k] : h[kn];
        flux += vnFace * Math.max(0, hnFace);
      }
      // South face
      if (j > 0) {
        const ks = k - cols;
        const vs = 0.5 * (vn[k] + vn[ks]);
        const hs = vs >= 0 ? h[ks] : h[k];
        flux -= vs * Math.max(0, hs);
      }
      let hNew = h[k] - (dt / dx) * flux + rainM - (h[k] > H_DRY ? infilM : 0);
      if (hNew < H_DRY) hNew = 0;
      hn[k] = hNew;
    }
  }

  // North-edge inflow distributed along wettest third of the top row.
  if (forcing.inflowCms > 0) {
    const edge = Math.max(3, Math.floor(cols / 3));
    const i0 = Math.floor((cols - edge) / 2);
    const vol = forcing.inflowCms * dt;
    const perCell = vol / (edge * dx * dx);
    for (let i = i0; i < i0 + edge; i++) {
      const k = (rows - 1) * cols + i;
      hn[k] += perCell;
    }
  }

  // Reflective / no-flow boundaries: zero normal velocity.
  for (let j = 0; j < rows; j++) {
    un[j * cols] = 0;
    un[j * cols + cols - 1] = 0;
  }
  for (let i = 0; i < cols; i++) {
    vn[i] = 0;
    vn[(rows - 1) * cols + i] = 0;
  }

  // Clear velocity on dry cells.
  for (let k = 0; k < n; k++) {
    if (hn[k] <= H_DRY) {
      un[k] = 0;
      vn[k] = 0;
      hn[k] = 0;
    }
  }

  h.set(hn);
  u.set(un);
  v.set(vn);
  state.timeS += dt;
  return { dtS: dt, cfl };
}

/** Coarse averaging of a fine tile — multires surround hook. */
export function downsampleDepth(
  fine: Float32Array,
  cols: number,
  rows: number,
  factor: number,
): { h: Float32Array; cols: number; rows: number } {
  const f = Math.max(1, Math.floor(factor));
  const c = Math.max(1, Math.floor(cols / f));
  const r = Math.max(1, Math.floor(rows / f));
  const out = new Float32Array(c * r);
  for (let j = 0; j < r; j++) {
    for (let i = 0; i < c; i++) {
      let sum = 0;
      let count = 0;
      for (let jj = 0; jj < f; jj++) {
        for (let ii = 0; ii < f; ii++) {
          const fi = i * f + ii;
          const fj = j * f + jj;
          if (fi < cols && fj < rows) {
            sum += fine[fj * cols + fi];
            count++;
          }
        }
      }
      out[j * c + i] = count ? sum / count : 0;
    }
  }
  return { h: out, cols: c, rows: r };
}
