/**
 * Flood-simulation gate.
 *
 * Run: npx tsx src/geospatial/hazards/floodValidation.smoke.ts
 *
 * Prints the measured numbers and exits non-zero if any acceptance threshold fails. The point of
 * this file is that "the flood model is validated" is a command with an exit code, not a sentence
 * in a README. Every threshold below is a published-benchmark expectation, not a number chosen to
 * make the current implementation pass:
 *
 * - lake at rest: the exact solution is constant eta to machine precision (1e-6 m is generous).
 * - Ritter dry bed: a first-order scheme should be within a few percent L1 and ~1-2 cells of the
 *   analytic front; the front is the strict test, since it is where first-order diffusion shows.
 * - convergence: observed order must be positive and the error must fall under refinement.
 * - mass: closed domain, so the identity is exact; 1e-9 relative is loose for f64.
 */

import {
  GRAVITY_MPS2,
  createSweState,
  depthAveragedVelocity,
  totalVolumeM3,
} from "./swe";
import {
  damBreakConvergence,
  determinismCheck,
  lakeAtRestBumpy,
  lakeAtRestFlat,
  lakeAtRestShoreline,
  massConservation,
  ritterDamBreak,
} from "./floodValidation";
import {
  buildDemDomain,
  fillDepressionsPriorityFlood,
  rasterizeDomainForSolver,
  wetConnectivity,
} from "./demDomain";
import { domainToAsciiGrid } from "./floodSim";
import type { GaussianPrimitive3D } from "../../splat/pointSplatting";

interface Check {
  name: string;
  measured: string;
  threshold: string;
  pass: boolean;
}

const checks: Check[] = [];
function check(name: string, measured: number | string | boolean, threshold: string, pass: boolean) {
  checks.push({
    name,
    measured: typeof measured === "number" ? Number(measured.toPrecision(6)).toString() : String(measured),
    threshold,
    pass,
  });
}

// ---------------------------------------------------------------- solver benchmarks
const flat = lakeAtRestFlat();
check("lake at rest / flat bed: max |eta - eta0| (m)", flat.maxEtaDeviationM, "<= 1e-6", flat.maxEtaDeviationM <= 1e-6);
check("lake at rest / flat bed: max |u| (m/s)", flat.maxVelocityMps, "<= 1e-6", flat.maxVelocityMps <= 1e-6);

const bumpy = lakeAtRestBumpy();
check("lake at rest / bumpy bed: max |eta - eta0| (m)", bumpy.maxEtaDeviationM, "<= 1e-6", bumpy.maxEtaDeviationM <= 1e-6);
check("lake at rest / bumpy bed: max |u| (m/s)", bumpy.maxVelocityMps, "<= 1e-6", bumpy.maxVelocityMps <= 1e-6);

const shore = lakeAtRestShoreline();
check("lake at rest / dry shoreline: max |eta - eta0| (m)", shore.maxEtaDeviationM, "<= 1e-6", shore.maxEtaDeviationM <= 1e-6);
check("lake at rest / dry shoreline: max |u| (m/s)", shore.maxVelocityMps, "<= 1e-6", shore.maxVelocityMps <= 1e-6);
check("lake at rest: no positivity repairs", shore.positivityRepairs + flat.positivityRepairs + bumpy.positivityRepairs, "== 0", shore.positivityRepairs + flat.positivityRepairs + bumpy.positivityRepairs === 0);

const ritter = ritterDamBreak(10, 12);
check("Ritter dam break: relative L1 depth error", ritter.depthL1Rel, "<= 0.02", ritter.depthL1Rel <= 0.02);
check("Ritter dam break: front error (cells)", Math.abs(ritter.frontErrorCells), "<= 3", Math.abs(ritter.frontErrorCells) <= 3);
check("Ritter dam break: analytic front (m)", ritter.frontAnalyticM, "reference", true);
check("Ritter dam break: numeric front (m)", ritter.frontNumericM, "reference", true);
check("Ritter dam break: realised max Courant", ritter.maxCourant, "<= 0.45", ritter.maxCourant <= 0.4501);

const conv = damBreakConvergence([20, 10, 5], 8);
const monotone = conv.depthL1Rel.every((v, i) => i === 0 || v < conv.depthL1Rel[i - 1]);
check("Convergence: L1 falls under refinement", conv.depthL1Rel.join(" > "), "strictly decreasing", monotone);
check("Convergence: min observed order", Math.min(...conv.observedOrder), "> 0.25", Math.min(...conv.observedOrder) > 0.25);

const mass = massConservation();
check("Mass conservation: relative error", mass.relError, "<= 1e-9", mass.relError <= 1e-9);
check("Mass conservation: positivity repairs", mass.positivityRepairs, "== 0", mass.positivityRepairs === 0);

const det = determinismCheck();
check("Determinism: bitwise identical repeat", det.identical, "== true", det.identical);

// ---------------------------------------------------------------- domain contract
const cols = 16;
const rows = 16;
const dxM = 2;
const splats: GaussianPrimitive3D[] = [];
for (let r = 0; r < rows; r++) {
  for (let c = 0; c < cols; c++) {
    // Leave a 4x4 hole so the nodata path is exercised.
    if (c >= 6 && c < 10 && r >= 6 && r < 10) continue;
    const x = (c + 0.5) * dxM;
    const y = (r + 0.5) * dxM;
    splats.push({
      center: [x, y, 10 + 0.05 * x] as [number, number, number],
      scale: [0.4, 0.4, 0.3] as [number, number, number],
      rotation: [1, 0, 0, 0] as [number, number, number, number],
      opacity: 0.9,
      colorRgba: [120, 140, 100, 255] as [number, number, number, number],
    });
  }
}
const domain = buildDemDomain(splats, { cols, rows, dxM });
check("DEM: nodata cells preserved (not invented)", domain.provenance.nodataCellCount, "== 16", domain.provenance.nodataCellCount === 16);
check("DEM: coverage fraction reported", domain.provenance.coverageFraction, "== 0.9375", Math.abs(domain.provenance.coverageFraction - 0.9375) < 1e-9);
check("DEM: vertical datum stated", domain.provenance.verticalDatum, "!= unknown", domain.provenance.verticalDatum !== "unknown");

const { bed, unobservedCells } = rasterizeDomainForSolver(domain);
let wallAbove = true;
for (let i = 0; i < bed.length; i++) {
  if (domain.nodataMask[i] && bed[i] < 100) wallAbove = false;
}
check("DEM: nodata becomes a wall for the solver", wallAbove && unobservedCells === 16, "unobserved=16", wallAbove && unobservedCells === 16);

// A single-cell pit must be removed by priority-flood filling.
const pitted = {
  ...domain,
  elevationM: Float64Array.from(domain.elevationM),
  nodataMask: new Uint8Array(domain.nodataMask),
};
const pitIdx = 3 * cols + 3;
pitted.elevationM[pitIdx] -= 2.0;
const fill = fillDepressionsPriorityFlood(pitted);
check("DEM: priority-flood removes a synthetic pit", fill.report.filledCellCount >= 1, ">= 1", fill.report.filledCellCount >= 1);
check("DEM: filled pit depth (m)", fill.report.maxFillDepthM, ">= 1.5", fill.report.maxFillDepthM >= 1.5);

const wetH = new Float64Array(cols * rows);
wetH[0] = 1;
wetH[1] = 1;
wetH[cols * rows - 1] = 1;
const conn = wetConnectivity(wetH, cols, rows, 0.01);
check("Connectivity: isolated pond detected", conn.componentCount, "== 2", conn.componentCount === 2);
check("Connectivity: singleComponent flag", conn.singleComponent, "== false", conn.singleComponent === false);

const ascii = domainToAsciiGrid(domain);
check("Export: ESRI ASCII header present", ascii.includes("NODATA_value -9999"), "true", ascii.includes("NODATA_value -9999"));
check("Export: provenance carried in header", ascii.includes("verticalDatum="), "true", ascii.includes("verticalDatum="));

// ---------------------------------------------------------------- report
const width = Math.max(...checks.map((c) => c.name.length));
console.log("\nFlood / simulation-domain validation\n");
for (const c of checks) {
  console.log(
    `${c.pass ? "PASS" : "FAIL"}  ${c.name.padEnd(width)}  measured=${c.measured}  threshold=${c.threshold}`,
  );
}
const failed = checks.filter((c) => !c.pass);
console.log(
  `\n${checks.length - failed.length}/${checks.length} checks passed` +
    (failed.length > 0 ? ` — FAILED: ${failed.map((f) => f.name).join("; ")}` : ""),
);
if (failed.length > 0) {
  throw new Error(`flood validation failed: ${failed.length} check(s)`);
}

// Reference values used by the report, printed so a reader can see the physics constants used.
console.log(
  `\nreference: g = ${GRAVITY_MPS2} m/s^2, Ritter c0 = ${Math.sqrt(GRAVITY_MPS2 * 10).toFixed(3)} m/s`,
);
void createSweState;
void depthAveragedVelocity;
void totalVolumeM3;
