/**
 * Splat → simulation domain → hydraulic simulation.
 *
 * Pipeline (each stage has a named, inspectable contract):
 *
 *   GaussianPrimitive3D[]
 *     --buildDemDomain-->          DemDomain  (metres, nodata mask, datum, provenance)
 *     --fillDepressionsPriorityFlood-->      conditioned bed (no spurious pits)
 *     --createSweState-->          SweState   (well-balanced HLL shallow-water core, swe.ts)
 *     --stepSwe-->                 simulated depth/momentum over time
 *     --extractFloodWaterSurfaceMesh-->      3D water surface for the viewport
 *     --domainToAsciiGrid-->       ESRI ASCII grid, the interchange format SFINCS,
 *                                  LISFLOOD-FP, Itzï and BG_Flood accept
 *
 * What changed from the previous version of this module, and why:
 * - The solver was replaced by the validated core in `swe.ts` (this file used to contain an
 *   order-dependent velocity predictor that leaked 33% of injected volume).
 * - Empty DEM cells are no longer silently invent-filled by inverse-distance weighting. They are
 *   marked nodata and reported; the solver treats them as walls. Inventing terrain invents flow
 *   paths (docs/research/D-mesh-flood.md).
 * - Depression filling, wet-connectivity auditing and a coverage/provenance record are now part
 *   of the pipeline, because a raster solver without them is not usable for a real answer.
 *
 * What it is still NOT: an authoritative inundation model. The core is first-order, has no
 * structures, no infiltration, no calibrated roughness map and no survey-grade datum. Numbers it
 * produces are "interactive preview / engineering estimate" until the promotion gates in
 * `docs/FLOOD-SIMULATION.md` are met with a validated external solver comparison.
 */

import type { GaussianPrimitive3D } from "../../splat/pointSplatting";
import {
  buildDemDomain,
  fillDepressionsPriorityFlood,
  rasterizeDomainForSolver,
  wetConnectivity,
  type DemDomain,
  type BuildDomainOptions,
  type DepressionFillReport,
  type WetConnectivityReport,
} from "./demDomain";
import {
  createSweState,
  depthAveragedVelocity,
  stepSwe,
  totalVolumeM3,
  type SweOptions,
  type SweState,
  type SweStepReport,
} from "./swe";

export type { SweState as ShallowWaterState };
export type { SweOptions as FloodSimulationOptions };
export type { DemDomain };

export interface SplatTerrainMesh {
  cols: number;
  rows: number;
  dxM: number;
  originEnu: [number, number, number];
  /** Ground elevation (m) in row-major order. Nodata cells hold 0 — check `nodataMask`. */
  elevationM: Float32Array;
  /** 1 = unobserved cell. */
  nodataMask: Uint8Array;
  /** Packed XYZ vertex positions (3 * rows * cols). */
  vertices: Float32Array;
  /** Packed XYZ vertex unit normals (3 * rows * cols). */
  normals: Float32Array;
  /** Triangle indices, two triangles per cell quad. */
  indices: Uint32Array;
}

export interface FloodWaterSurfaceMesh {
  cols: number;
  rows: number;
  dxM: number;
  originEnu: [number, number, number];
  vertices: Float32Array;
  normals: Float32Array;
  depths: Float32Array;
  speeds: Float32Array;
  indices: Uint32Array;
  wetVertexCount: number;
  wetTriangleCount: number;
}

export interface FloodPointSource {
  col: number;
  row: number;
  dischargeM3s: number;
}

export interface FloodSimulationResult {
  domain: DemDomain;
  terrainMesh: SplatTerrainMesh;
  waterState: SweState;
  waterSurfaceMesh: FloodWaterSurfaceMesh;
  stepReport: SweStepReport;
  depressionFill: DepressionFillReport;
  connectivity: WetConnectivityReport;
  /** Total simulated volume at the end of the run (m^3). */
  totalVolumeM3: number;
  /** Volume injected by rainfall + point sources over the run (m^3). */
  injectedM3: number;
  /** Unobserved cells inside the requested grid. */
  unobservedCells: number;
}

/**
 * Build the bed for a simulation and the viewport mesh that shows it.
 * Nodata cells are excluded from the mesh triangles so the UI cannot draw invented terrain.
 */
export function buildSimulationDomain(
  splats: readonly GaussianPrimitive3D[],
  options: BuildDomainOptions & { fillDepressions?: boolean },
): {
  domain: DemDomain;
  terrainMesh: SplatTerrainMesh;
  depressionFill: DepressionFillReport;
  unobservedCells: number;
} {
  const raw = buildDemDomain(splats, options);
  const filled = options.fillDepressions === false
    ? { dem: raw, report: { filledCellCount: 0, maxFillDepthM: 0, meanFillDepthM: 0, totalFillVolumeM3: 0 } }
    : fillDepressionsPriorityFlood(raw);
  const domain = filled.dem;

  const elevationF32 = new Float32Array(domain.elevationM.length);
  for (let i = 0; i < elevationF32.length; i++) {
    elevationF32[i] = domain.nodataMask[i] ? 0 : domain.elevationM[i];
  }
  const mesh = buildMeshFromElevationGrid(
    elevationF32,
    domain.cols,
    domain.rows,
    domain.dxM,
    domain.originEnu,
    domain.nodataMask,
  );
  return {
    domain,
    terrainMesh: mesh,
    depressionFill: filled.report,
    unobservedCells: domain.provenance.nodataCellCount,
  };
}

/**
 * Construct a 3D indexed terrain triangle mesh with finite-difference surface normals.
 * With a nodata mask, quads touching an unobserved cell are skipped rather than drawn flat.
 */
export function buildMeshFromElevationGrid(
  elevationM: Float32Array,
  cols: number,
  rows: number,
  dxM: number,
  originEnu: [number, number, number] = [0, 0, 0],
  nodataMask?: Uint8Array,
): SplatTerrainMesh {
  const nVerts = cols * rows;
  const vertices = new Float32Array(nVerts * 3);
  const normals = new Float32Array(nVerts * 3);

  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const idx = r * cols + c;
      const vBase = idx * 3;
      vertices[vBase] = originEnu[0] + (c + 0.5) * dxM;
      vertices[vBase + 1] = originEnu[1] + (r + 0.5) * dxM;
      vertices[vBase + 2] = elevationM[idx];

      const zL = elevationM[r * cols + Math.max(0, c - 1)];
      const zR = elevationM[r * cols + Math.min(cols - 1, c + 1)];
      const zD = elevationM[Math.max(0, r - 1) * cols + c];
      const zU = elevationM[Math.min(rows - 1, r + 1) * cols + c];

      const dzdx = (zR - zL) / (2.0 * dxM);
      const dzdy = (zU - zD) / (2.0 * dxM);
      const nx = -dzdx;
      const ny = -dzdy;
      const nz = 1.0;
      const len = Math.hypot(nx, ny, nz) || 1.0;
      normals[vBase] = nx / len;
      normals[vBase + 1] = ny / len;
      normals[vBase + 2] = nz / len;
    }
  }

  const indexList: number[] = [];
  for (let r = 0; r < rows - 1; r++) {
    for (let c = 0; c < cols - 1; c++) {
      const v00 = r * cols + c;
      const v10 = v00 + 1;
      const v01 = (r + 1) * cols + c;
      const v11 = v01 + 1;
      if (nodataMask && (nodataMask[v00] || nodataMask[v10] || nodataMask[v01] || nodataMask[v11])) {
        continue;
      }
      indexList.push(v00, v10, v11, v00, v11, v01);
    }
  }

  return {
    cols,
    rows,
    dxM,
    originEnu,
    elevationM,
    nodataMask: nodataMask ?? new Uint8Array(cols * rows),
    vertices,
    normals,
    indices: Uint32Array.from(indexList),
  };
}

/** Total water volume of a shallow-water state (m^3). */
export function computeWaterVolumeM3(state: SweState): number {
  return totalVolumeM3(state);
}

/**
 * End-to-end splat → domain → simulation → water surface.
 * `durationS` is simulated time, not wall-clock.
 */
export function runSplatToMeshFloodSimulation(
  splats: readonly GaussianPrimitive3D[],
  gridSpec: BuildDomainOptions,
  durationS: number,
  options: SweOptions = {},
): FloodSimulationResult {
  const { domain, terrainMesh, depressionFill, unobservedCells } = buildSimulationDomain(
    splats,
    gridSpec,
  );
  const { bed } = rasterizeDomainForSolver(domain);
  const waterState = createSweState(
    {
      cols: domain.cols,
      rows: domain.rows,
      dxM: domain.dxM,
      originEnu: domain.originEnu,
    },
    bed,
  );

  const stepReport = stepSwe(waterState, durationS, options);
  const waterSurfaceMesh = extractFloodWaterSurfaceMesh(waterState);
  const connectivity = wetConnectivity(
    waterState.h,
    waterState.cols,
    waterState.rows,
    0.01,
  );

  return {
    domain,
    terrainMesh,
    waterState,
    waterSurfaceMesh,
    stepReport,
    depressionFill,
    connectivity,
    totalVolumeM3: totalVolumeM3(waterState),
    injectedM3: waterState.injectedM3,
    unobservedCells,
  };
}

/**
 * Extract the free-surface mesh eta = z + h over wet cells.
 * Normals are the true surface normals of eta, not a constant up vector.
 */
export function extractFloodWaterSurfaceMesh(
  state: SweState,
  minWetDepthM = 0.01,
): FloodWaterSurfaceMesh {
  const { cols, rows, dxM, originEnu, z, h } = state;
  const nVerts = cols * rows;
  const vertices = new Float32Array(nVerts * 3);
  const normals = new Float32Array(nVerts * 3);
  const depths = new Float32Array(nVerts);
  const speeds = new Float32Array(nVerts);
  const eta = new Float64Array(nVerts);

  for (let i = 0; i < nVerts; i++) {
    eta[i] = z[i] + h[i];
    depths[i] = h[i];
  }
  const vel = depthAveragedVelocity(state);
  let wetVertexCount = 0;
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const idx = r * cols + c;
      const vBase = idx * 3;
      vertices[vBase] = originEnu[0] + (c + 0.5) * dxM;
      vertices[vBase + 1] = originEnu[1] + (r + 0.5) * dxM;
      vertices[vBase + 2] = eta[idx];
      if (h[idx] >= minWetDepthM) {
        wetVertexCount++;
        speeds[idx] = Math.hypot(vel.u[idx], vel.v[idx]);
      }
      const eL = eta[r * cols + Math.max(0, c - 1)];
      const eR = eta[r * cols + Math.min(cols - 1, c + 1)];
      const eD = eta[Math.max(0, r - 1) * cols + c];
      const eU = eta[Math.min(rows - 1, r + 1) * cols + c];
      const nx = -(eR - eL) / (2 * dxM);
      const ny = -(eU - eD) / (2 * dxM);
      const len = Math.hypot(nx, ny, 1) || 1;
      normals[vBase] = nx / len;
      normals[vBase + 1] = ny / len;
      normals[vBase + 2] = 1 / len;
    }
  }

  const indexList: number[] = [];
  for (let r = 0; r < rows - 1; r++) {
    for (let c = 0; c < cols - 1; c++) {
      const v00 = r * cols + c;
      const v10 = v00 + 1;
      const v01 = (r + 1) * cols + c;
      const v11 = v01 + 1;
      if (
        h[v00] >= minWetDepthM ||
        h[v10] >= minWetDepthM ||
        h[v01] >= minWetDepthM ||
        h[v11] >= minWetDepthM
      ) {
        indexList.push(v00, v10, v11, v00, v11, v01);
      }
    }
  }

  return {
    cols,
    rows,
    dxM,
    originEnu,
    vertices,
    normals,
    depths,
    speeds,
    indices: Uint32Array.from(indexList),
    wetVertexCount,
    wetTriangleCount: indexList.length / 3,
  };
}

/**
 * ESRI ASCII grid export — the interchange format every raster flood model accepts
 * (SFINCS `.dep`, LISFLOOD-FP `.asc`, Itzï, BG_Flood). Elevation in metres, nodata = -9999,
 * plus a `#` comment header carrying the provenance the format cannot express itself.
 *
 * Note the row order: ESRI ASCII writes north-first, so rows are emitted in reverse.
 */
export function domainToAsciiGrid(domain: DemDomain, nodataValue = -9999): string {
  const lines: string[] = [];
  const p = domain.provenance;
  lines.push(`# instasplatter simulation domain`);
  lines.push(`# method=${p.method} groundQuantile=${p.groundQuantile} minOpacity=${p.minOpacity}`);
  lines.push(`# verticalDatum=${p.verticalDatum} horizontalCrsEpsg=${p.horizontalCrsEpsg ?? "none"}`);
  lines.push(`# coverage=${(p.coverageFraction * 100).toFixed(1)}% nodataCells=${p.nodataCellCount}`);
  lines.push(`# originEnu=${domain.originEnu.join(",")}`);
  lines.push(`# ${p.note.replace(/\s+/g, " ")}`);
  lines.push(`ncols ${domain.cols}`);
  lines.push(`nrows ${domain.rows}`);
  lines.push(`xllcorner ${domain.originEnu[0]}`);
  lines.push(`yllcorner ${domain.originEnu[1]}`);
  lines.push(`cellsize ${domain.dxM}`);
  lines.push(`NODATA_value ${nodataValue}`);
  for (let r = domain.rows - 1; r >= 0; r--) {
    const row: string[] = [];
    for (let c = 0; c < domain.cols; c++) {
      const idx = r * domain.cols + c;
      row.push(
        domain.nodataMask[idx] ? String(nodataValue) : domain.elevationM[idx].toFixed(4),
      );
    }
    lines.push(row.join(" "));
  }
  return lines.join("\n") + "\n";
}

/**
 * Deprecated compatibility wrappers. They keep the old names working while pointing at the
 * validated core; new code should call `buildSimulationDomain` / `stepSwe` directly.
 */
export function splatCloudToTerrainMeshAndDem(
  splats: readonly GaussianPrimitive3D[],
  options: BuildDomainOptions,
): SplatTerrainMesh {
  return buildSimulationDomain(splats, options).terrainMesh;
}

/** @deprecated use `stepSwe` from `./swe`. */
export function stepShallowWaterSaintVenant(
  state: SweState,
  durationS: number,
  options: SweOptions = {},
): SweStepReport {
  return stepSwe(state, durationS, options);
}
