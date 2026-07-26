/**
 * Focused smoke for Part B AOI-freeze caps.
 * Run: npx --yes tsx src/geospatial/preview/domainCap.smoke.ts
 */
import { coarsenDomainDxFromPlan, domainFromAoi } from "../aoi";
import {
  handInundationSmokeOk,
  MAX_HAND_SEARCH_RADIUS,
} from "./handInundation";
import {
  MAX_PREVIEW_CELLS,
  MAX_PREVIEW_COLS,
  MAX_PREVIEW_ROWS,
  resolveDomain,
  resolveDomainSmokeOk,
} from "./softSolver";

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(msg);
}

const hugeAoi: [number, number, number, number] = [-122.5, 37.0, -121.5, 38.0];
const domain = domainFromAoi(hugeAoi);
assert(domain.cols <= 192 && domain.rows <= 144, "domainFromAoi must cap cols/rows");

const coarsened = coarsenDomainDxFromPlan(domain, 50);
assert(coarsened.cols === domain.cols && coarsened.rows === domain.rows, "plan must not expand grid");
assert(coarsened.dxM >= domain.dxM, "plan may only coarsen dxM");

const clamped = resolveDomain({ cols: 5000, rows: 4000, dxM: 1, bounds: domain.bounds });
assert(clamped.cols <= MAX_PREVIEW_COLS, "resolveDomain cols cap");
assert(clamped.rows <= MAX_PREVIEW_ROWS, "resolveDomain rows cap");
assert(clamped.cols * clamped.rows <= MAX_PREVIEW_CELLS, "resolveDomain cell budget");

assert(resolveDomainSmokeOk(), "resolveDomainSmokeOk");
assert(handInundationSmokeOk(), "handInundationSmokeOk");
assert(MAX_HAND_SEARCH_RADIUS <= 8, "HAND radius must stay small");

console.log("domainCap.smoke: ok");
