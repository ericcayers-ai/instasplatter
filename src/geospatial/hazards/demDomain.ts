/**
 * Simulation-domain construction from a Gaussian splat cloud.
 *
 * The research sweep (docs/research/D-mesh-flood.md) is blunt about the gap this module closes:
 * turning a splat into a *shape* is not the same as producing a *simulation domain*. Every
 * raster flood model — this repo's solver, SFINCS, LISFLOOD-FP, BG_Flood, HiPIMS, GeoClaw —
 * requires four things a raw splat DEM does not have:
 *
 *   1. metres, positive up, and a stated vertical datum;
 *   2. an explicit **nodata mask** (silently inventing terrain by IDW-filling holes creates and
 *      destroys flow paths — the previous implementation did exactly that);
 *   3. **depression conditioning**: splat noise leaves thousands of one-cell pits that trap water
 *      and destroy hydraulic connectivity (RichDEM/WhiteboxTools treat fill as mandatory);
 *   4. a coverage/quality record, so a result can be refused rather than quietly trusted.
 *
 * The ground-extraction rule is deliberate: per cell, take the `groundQuantile` of the
 * opacity-weighted splash heights, using the *bottom* of each Gaussian's vertical extent. That
 * strips canopy and flying floaters, which would otherwise become fake dams. It is a heuristic
 * and it is labelled as one — it is not a classified ground surface.
 */

import type { GaussianPrimitive3D } from "../../splat/pointSplatting";

export type VerticalDatum =
  | "ellipsoidal-wgs84"
  | "geoid-egm96"
  | "geoid-nzvd2016"
  | "local-arbitrary"
  | "unknown";

export interface DemProvenance {
  /** How the ground surface was derived. */
  method: "splat-ground-quantile" | "imported-raster" | "dem-catalog";
  groundQuantile: number;
  minOpacity: number;
  smoothPasses: number;
  /** Cells with no usable splat support — terrain there is NOT observed. */
  nodataCellCount: number;
  /** total cells */
  cellCount: number;
  /** Fraction of cells with observed ground (1 - nodata/cells). */
  coverageFraction: number;
  /** Per-cell height spread before quantile selection (metres); a quality signal. */
  medianVerticalSpreadM: number;
  verticalDatum: VerticalDatum;
  /** EPSG code when the ENU frame has a known georeference, else null. */
  horizontalCrsEpsg: number | null;
  /** Free-text note shown in the UI next to the domain, e.g. "not survey-grade". */
  note: string;
}

export interface DemDomain {
  cols: number;
  rows: number;
  dxM: number;
  originEnu: [number, number, number];
  /** Bed elevation, row-major (m). Values at nodata cells are 0 and MUST NOT be used. */
  elevationM: Float64Array;
  /** 1 = nodata (no observation), 0 = observed. */
  nodataMask: Uint8Array;
  provenance: DemProvenance;
}

export interface BuildDomainOptions {
  cols: number;
  rows: number;
  dxM: number;
  originEnu?: [number, number, number];
  /** Splats below this opacity are ignored (floaters, haze). */
  minOpacity?: number;
  /** Quantile of per-cell heights taken as ground (0.1 = 10th percentile). */
  groundQuantile?: number;
  /** Edge-preserving Laplacian smoothing passes over observed cells. */
  smoothPasses?: number;
  verticalDatum?: VerticalDatum;
  horizontalCrsEpsg?: number | null;
  note?: string;
}

const DEFAULT_NOTE =
  "Derived from a splat-derived ground envelope. Not survey-grade; no GCP/RTK control applied.";

/** Build a DEM domain from a splat cloud. Observed cells only; no invented terrain. */
export function buildDemDomain(
  splats: readonly GaussianPrimitive3D[],
  options: BuildDomainOptions,
): DemDomain {
  const cols = Math.max(2, Math.floor(options.cols));
  const rows = Math.max(2, Math.floor(options.rows));
  const dxM = Math.max(0.05, options.dxM);
  const originEnu: [number, number, number] = options.originEnu ?? [0, 0, 0];
  const minOpacity = options.minOpacity ?? 0.55;
  const groundQuantile = Math.max(0.02, Math.min(0.5, options.groundQuantile ?? 0.15));
  const smoothPasses = Math.max(0, Math.min(6, options.smoothPasses ?? 1));

  const nCells = cols * rows;
  const buckets: number[][] = Array.from({ length: nCells }, () => []);
  for (const s of splats) {
    if (!(s.opacity >= minOpacity)) continue;
    if (!Number.isFinite(s.center[0]) || !Number.isFinite(s.center[1])) continue;
    const relX = s.center[0] - originEnu[0];
    const relY = s.center[1] - originEnu[1];
    const col = Math.floor(relX / dxM);
    const row = Math.floor(relY / dxM);
    if (col < 0 || col >= cols || row < 0 || row >= rows) continue;
    // Bottom of the Gaussian's vertical extent: the canopy-free estimate.
    const groundEst = s.center[2] - 0.5 * Math.abs(s.scale[2]);
    buckets[row * cols + col].push(groundEst);
  }

  const elevationM = new Float64Array(nCells);
  const nodataMask = new Uint8Array(nCells);
  const spreads: number[] = [];
  let observed = 0;

  for (let i = 0; i < nCells; i++) {
    const bucket = buckets[i];
    if (bucket.length === 0) {
      nodataMask[i] = 1;
      continue;
    }
    bucket.sort((a, b) => a - b);
    const qIdx = Math.min(bucket.length - 1, Math.floor(bucket.length * groundQuantile));
    elevationM[i] = bucket[qIdx];
    spreads.push(bucket[bucket.length - 1] - bucket[0]);
    observed++;
  }

  spreads.sort((a, b) => a - b);
  const medianSpread = spreads.length > 0 ? spreads[Math.floor(spreads.length / 2)] : Number.NaN;

  // Smooth only across observed cells; nodata must stay nodata so the solver never sees
  // interpolated terrain as if it had been measured.
  if (smoothPasses > 0) {
    const scratch = new Float64Array(nCells);
    for (let pass = 0; pass < smoothPasses; pass++) {
      scratch.set(elevationM);
      for (let r = 0; r < rows; r++) {
        for (let c = 0; c < cols; c++) {
          const idx = r * cols + c;
          if (nodataMask[idx]) continue;
          let sum = scratch[idx] * 2;
          let weight = 2;
          if (c > 0 && !nodataMask[idx - 1]) {
            sum += scratch[idx - 1];
            weight += 1;
          }
          if (c + 1 < cols && !nodataMask[idx + 1]) {
            sum += scratch[idx + 1];
            weight += 1;
          }
          if (r > 0 && !nodataMask[idx - cols]) {
            sum += scratch[idx - cols];
            weight += 1;
          }
          if (r + 1 < rows && !nodataMask[idx + cols]) {
            sum += scratch[idx + cols];
            weight += 1;
          }
          elevationM[idx] = sum / weight;
        }
      }
    }
  }

  return {
    cols,
    rows,
    dxM,
    originEnu,
    elevationM,
    nodataMask,
    provenance: {
      method: "splat-ground-quantile",
      groundQuantile,
      minOpacity,
      smoothPasses,
      nodataCellCount: nCells - observed,
      cellCount: nCells,
      coverageFraction: nCells > 0 ? observed / nCells : 0,
      medianVerticalSpreadM: medianSpread,
      verticalDatum: options.verticalDatum ?? "local-arbitrary",
      horizontalCrsEpsg: options.horizontalCrsEpsg ?? null,
      note: options.note ?? DEFAULT_NOTE,
    },
  };
}

export interface DepressionFillReport {
  filledCellCount: number;
  maxFillDepthM: number;
  meanFillDepthM: number;
  totalFillVolumeM3: number;
}

/**
 * Priority-flood depression filling (Barnes, Lehman & Mulla 2014, "Priority-Flood: An optimal
 * depression-filling and watershed-labeling algorithm", Comput. Geosci. 62).
 *
 * Splat-derived ground contains spurious pits; every raster hydraulic solver needs them removed
 * or water is trapped in noise. Nodata cells are treated as a wall — we fill toward them but do
 * not modify across them, so an unobserved gap cannot become a drain.
 */
export function fillDepressionsPriorityFlood(dem: DemDomain): {
  dem: DemDomain;
  report: DepressionFillReport;
} {
  const { cols, rows, dxM } = dem;
  const nCells = cols * rows;
  const z = Float64Array.from(dem.elevationM);
  const closed = new Uint8Array(nCells);

  const heap = new MinHeap();
  const push = (idx: number) => {
    if (closed[idx]) return;
    closed[idx] = 1;
    heap.push(z[idx], idx);
  };

  for (let c = 0; c < cols; c++) {
    if (!dem.nodataMask[c]) push(c);
    if (!dem.nodataMask[(rows - 1) * cols + c]) push((rows - 1) * cols + c);
  }
  for (let r = 0; r < rows; r++) {
    if (!dem.nodataMask[r * cols]) push(r * cols);
    if (!dem.nodataMask[r * cols + cols - 1]) push(r * cols + cols - 1);
  }

  let filledCellCount = 0;
  let maxFill = 0;
  let totalFill = 0;

  while (heap.size > 0) {
    const { priority: zc, value: idx } = heap.pop();
    const c = idx % cols;
    const r = (idx - c) / cols;
    const neighbours: number[] = [];
    if (c > 0) neighbours.push(idx - 1);
    if (c + 1 < cols) neighbours.push(idx + 1);
    if (r > 0) neighbours.push(idx - cols);
    if (r + 1 < rows) neighbours.push(idx + cols);
    for (const nb of neighbours) {
      if (closed[nb]) continue;
      if (dem.nodataMask[nb]) {
        closed[nb] = 1;
        continue;
      }
      const zn = z[nb];
      if (zn < zc) {
        const delta = zc - zn;
        z[nb] = zc;
        filledCellCount++;
        totalFill += delta;
        if (delta > maxFill) maxFill = delta;
      }
      closed[nb] = 1;
      heap.push(z[nb], nb);
    }
  }

  const filled: DemDomain = {
    ...dem,
    elevationM: z,
    provenance: {
      ...dem.provenance,
      note:
        `${dem.provenance.note} Depressions priority-filled: ${filledCellCount} cell(s), ` +
        `max ${maxFill.toFixed(3)} m.`,
    },
  };
  const observedCells = Math.max(1, nCells - dem.provenance.nodataCellCount);
  return {
    dem: filled,
    report: {
      filledCellCount,
      maxFillDepthM: maxFill,
      meanFillDepthM: totalFill / observedCells,
      totalFillVolumeM3: totalFill * dxM * dxM,
    },
  };
}

/**
 * O(n) spatially coherent import of a splat-derived bed into a solver grid.
 *
 * Returns the elevation array the solver uses. Nodata cells are set to
 * `fallbackAboveNeighbour` so water cannot enter them, and the caller is expected to surface
 * `unobservedCells` in the UI rather than treat the domain as complete.
 */
export function rasterizeDomainForSolver(dem: DemDomain): {
  bed: Float64Array;
  solvableCellCount: number;
  unobservedCells: number;
} {
  const n = dem.cols * dem.rows;
  const bed = new Float64Array(n);
  let solvable = 0;
  let unobserved = 0;
  // Nodata is raised above every observed neighbour: water runs around an unobserved gap
  // instead of draining through invented terrain.
  let maxObserved = -Infinity;
  for (let i = 0; i < n; i++) {
    if (!dem.nodataMask[i] && dem.elevationM[i] > maxObserved) maxObserved = dem.elevationM[i];
  }
  if (!Number.isFinite(maxObserved)) maxObserved = 0;
  const wallElevation = maxObserved + 1e3;
  for (let i = 0; i < n; i++) {
    if (dem.nodataMask[i]) {
      bed[i] = wallElevation;
      unobserved++;
    } else {
      bed[i] = dem.elevationM[i];
      solvable++;
    }
  }
  return { bed, solvableCellCount: solvable, unobservedCells: unobserved };
}

export interface WetConnectivityReport {
  componentCount: number;
  sizes: number[];
  /** True when every wet cell is reachable from the largest component. */
  singleComponent: boolean;
}

/**
 * Connected-component audit of the wet mask. An isolated pond is either a real depression or a
 * numerical artefact; either way the UI has to be able to say so instead of drawing it.
 */
export function wetConnectivity(
  h: Float64Array,
  cols: number,
  rows: number,
  minDepthM = 0.01,
): WetConnectivityReport {
  const n = cols * rows;
  const seen = new Uint8Array(n);
  const sizes: number[] = [];
  const stack: number[] = [];
  for (let start = 0; start < n; start++) {
    if (seen[start] || !(h[start] >= minDepthM)) continue;
    let size = 0;
    stack.length = 0;
    stack.push(start);
    seen[start] = 1;
    while (stack.length > 0) {
      const idx = stack.pop() as number;
      size++;
      const c = idx % cols;
      const r = (idx - c) / cols;
      if (c > 0 && !seen[idx - 1] && h[idx - 1] >= minDepthM) {
        seen[idx - 1] = 1;
        stack.push(idx - 1);
      }
      if (c + 1 < cols && !seen[idx + 1] && h[idx + 1] >= minDepthM) {
        seen[idx + 1] = 1;
        stack.push(idx + 1);
      }
      if (r > 0 && !seen[idx - cols] && h[idx - cols] >= minDepthM) {
        seen[idx - cols] = 1;
        stack.push(idx - cols);
      }
      if (r + 1 < rows && !seen[idx + cols] && h[idx + cols] >= minDepthM) {
        seen[idx + cols] = 1;
        stack.push(idx + cols);
      }
    }
    sizes.push(size);
  }
  sizes.sort((a, b) => b - a);
  const total = sizes.reduce((a, b) => a + b, 0);
  return {
    componentCount: sizes.length,
    sizes,
    singleComponent: sizes.length <= 1 || (sizes[0] ?? 0) === total,
  };
}

/** Minimal binary min-heap keyed by a float priority. Deterministic for equal priorities. */
class MinHeap {
  private pri: number[] = [];
  private val: number[] = [];
  private seq: number[] = [];
  private counter = 0;

  get size(): number {
    return this.pri.length;
  }

  push(priority: number, value: number): void {
    this.pri.push(priority);
    this.val.push(value);
    this.seq.push(this.counter++);
    let i = this.pri.length - 1;
    while (i > 0) {
      const parent = (i - 1) >> 1;
      if (this.less(i, parent)) {
        this.swap(i, parent);
        i = parent;
      } else break;
    }
  }

  pop(): { priority: number; value: number } {
    const priority = this.pri[0];
    const value = this.val[0];
    const last = this.pri.length - 1;
    if (last > 0) {
      this.pri[0] = this.pri[last];
      this.val[0] = this.val[last];
      this.seq[0] = this.seq[last];
    }
    this.pri.pop();
    this.val.pop();
    this.seq.pop();
    let i = 0;
    const n = this.pri.length;
    for (;;) {
      const l = i * 2 + 1;
      const r = l + 1;
      let smallest = i;
      if (l < n && this.less(l, smallest)) smallest = l;
      if (r < n && this.less(r, smallest)) smallest = r;
      if (smallest === i) break;
      this.swap(i, smallest);
      i = smallest;
    }
    return { priority, value };
  }

  private less(a: number, b: number): boolean {
    if (this.pri[a] !== this.pri[b]) return this.pri[a] < this.pri[b];
    return this.seq[a] < this.seq[b];
  }

  private swap(a: number, b: number): void {
    const p = this.pri[a];
    this.pri[a] = this.pri[b];
    this.pri[b] = p;
    const v = this.val[a];
    this.val[a] = this.val[b];
    this.val[b] = v;
    const s = this.seq[a];
    this.seq[a] = this.seq[b];
    this.seq[b] = s;
  }
}
