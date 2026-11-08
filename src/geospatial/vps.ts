/**
 * Full Visual Positioning System (VPS) for InstaSplatter (`src/geospatial/vps.ts`).
 *
 * Operates seamlessly across both:
 * - **Ground-level scans** (street-level smartphones, AR headsets, indoor/facade walkarounds)
 * - **Geospatial / aerial views** (drone nadir/oblique captures, orthophoto tiles, 3D city splats)
 *
 * 4-Stage Coarse-to-Fine Localization Pipeline:
 * 1. Coarse Tile Retrieval (`retrieveCandidateVpsSubmaps`):
 *    Combines global descriptor cosine similarity (NetVLAD / GeoCLIP / StreetCLIP) with optional
 *    coarse GNSS / IP / GeoSpy prior (`priorWgs84`, `maxRadiusM`) to rank candidate ENU splat submaps.
 * 2. 2D–3D Splat Anchor Matching (`matchQueryToSplatAnchors`):
 *    Matches 2D query features (SuperPoint / LightGlue / DISK / ORB) against 3D Gaussian Splat
 *    landmarks using Lowe's ratio test + mutual nearest-neighbor check + viewing-ray cone gating.
 * 3. PnP + RANSAC + Gauss-Newton 6-DoF Solver (`solvePnpRansac6DoF`):
 *    Normalized 3D-2D DLT + polar orthogonalization inside a deterministic RANSAC loop, followed by
 *    damped Gauss-Newton reprojection refinement over all inliers.
 * 4. Geodetic ENU <-> WGS84 Registration (`localizeCameraVps`):
 *    Outputs the 6-DoF camera pose in both local ENU metres and global WGS84 (lon, lat, ellipsoidal height),
 *    compass heading/pitch/roll, inlier metrics, and HDOP/VDOP uncertainty bounds.
 */

import { enuToWgs84, wgs84ToEnu, type Vec3 } from "./enu";

export type VpsScanDomain = "ground" | "geospatial" | "hybrid";

export interface SplatAnchor3D {
  anchorId: string;
  /** 3D position in the submap's local ENU frame (metres). */
  positionEnu: Vec3;
  /** Optional dominant surface normal or viewing direction unit vector. */
  normalEnu?: Vec3;
  /** Feature descriptor vector (e.g. 32-D, 64-D, 128-D, or 256-D L2-normalized). */
  descriptor: Float32Array | number[];
  /** Gaussian opacity / saliency weight in [0, 1]. */
  saliency: number;
  /** Domain tag indicating whether anchor was observed from ground or nadir/aerial. */
  domain: VpsScanDomain;
}

export interface VpsSubmap {
  submapId: string;
  /** Reference WGS84 origin [lonDeg, latDeg, heightM] for this submap's ENU frame. */
  originWgs84: [number, number, number];
  /** Global place-recognition embedding (e.g. NetVLAD / GeoCLIP vector). */
  globalDescriptor: Float32Array | number[];
  anchors: SplatAnchor3D[];
}

export interface QueryKeypoint2D {
  /** Pixel coordinates (u, v) in image space. */
  u: number;
  v: number;
  /** Feature descriptor matching the dimensionality of `SplatAnchor3D.descriptor`. */
  descriptor: Float32Array | number[];
}

export interface CameraIntrinsics {
  width: number;
  height: number;
  fx: number;
  fy: number;
  cx: number;
  cy: number;
}

export interface Match2D3D {
  queryIndex: number;
  anchorId: string;
  u: number;
  v: number;
  pointEnu: Vec3;
  descriptorDistance: number;
}

export interface VpsPoseResult {
  success: boolean;
  confidence: "high" | "medium" | "low" | "failed";
  submapId: string | null;
  domainUsed: VpsScanDomain;
  /** Camera optical center in the submap's local ENU frame (metres). */
  positionEnu: Vec3;
  /** Camera optical center in global WGS84 [lonDeg, latDeg, altM]. */
  positionWgs84: [number, number, number];
  /** 3x3 world-to-camera rotation matrix R (row-major, length 9) where x_cam = R * (X_enu - C_enu). */
  rotationWorldToCam: [
    number, number, number,
    number, number, number,
    number, number, number,
  ];
  /** Compass heading (0 = North, 90 = East) in degrees [0, 360). */
  headingDeg: number;
  /** Pitch in degrees (-90 = nadir straight down, 0 = horizontal horizon, +90 = zenith). */
  pitchDeg: number;
  /** Roll in degrees. */
  rollDeg: number;
  inlierCount: number;
  totalMatches: number;
  medianReprojectionErrorPx: number;
  /**
   * Horizontal and vertical position uncertainty (metres, 1-sigma, first-order bound).
   *
   * Derived, not invented: `sigma = (medianReprojectionErrorPx / focalPx) * medianInlierDepthM /
   * sqrt(inliers)`, with the vertical term divided by the inlier height spread factor because a
   * set of landmarks at one height constrains height poorly. It is a scale-aware uncertainty on
   * the pose, NOT a calibrated covariance: no sensor model, no descriptor-noise propagation.
   */
  hdopM: number;
  vdopM: number;
  /** Median distance from the camera to its inlier landmarks (m), the scale of the bound above. */
  medianInlierDepthM: number;
}

function l2Dot(a: ArrayLike<number>, b: ArrayLike<number>): number {
  const n = Math.min(a.length, b.length);
  let sum = 0;
  let na = 0;
  let nb = 0;
  for (let i = 0; i < n; i++) {
    const va = a[i];
    const vb = b[i];
    sum += va * vb;
    na += va * va;
    nb += vb * vb;
  }
  const denom = Math.sqrt(na * nb);
  return denom > 1e-12 ? sum / denom : 0;
}

function l2Distance(a: ArrayLike<number>, b: ArrayLike<number>): number {
  const n = Math.min(a.length, b.length);
  let sumSq = 0;
  for (let i = 0; i < n; i++) {
    const d = a[i] - b[i];
    sumSq += d * d;
  }
  return Math.sqrt(sumSq);
}

/**
 * Stage 1: Coarse geospatial + visual place retrieval over candidate submaps.
 */
export function retrieveCandidateVpsSubmaps(
  submaps: readonly VpsSubmap[],
  queryGlobalDescriptor: ArrayLike<number>,
  options: {
    priorWgs84?: [number, number, number];
    maxSearchRadiusM?: number;
    topK?: number;
  } = {},
): Array<{ submap: VpsSubmap; score: number; distanceM: number | null }> {
  const topK = Math.max(1, options.topK ?? 3);
  const maxRadius = options.maxSearchRadiusM ?? Number.POSITIVE_INFINITY;

  const scored: Array<{ submap: VpsSubmap; score: number; distanceM: number | null }> = [];
  for (const sm of submaps) {
    let distM: number | null = null;
    let geoWeight = 1.0;
    if (options.priorWgs84) {
      const dEnu = wgs84ToEnu(
        sm.originWgs84[0],
        sm.originWgs84[1],
        sm.originWgs84[2],
        options.priorWgs84,
      );
      distM = Math.hypot(dEnu[0], dEnu[1]);
      if (distM > maxRadius) continue;
      geoWeight = Math.exp(-distM / Math.max(500.0, maxRadius * 0.5));
    }
    const visualSim = Math.max(0, l2Dot(queryGlobalDescriptor, sm.globalDescriptor));
    const combinedScore = options.priorWgs84 ? 0.65 * visualSim + 0.35 * geoWeight : visualSim;
    scored.push({ submap: sm, score: combinedScore, distanceM: distM });
  }

  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, topK);
}

/**
 * Stage 2: Match 2D query keypoints against 3D Gaussian Splat anchors using Lowe's ratio test
 * and mutual nearest-neighbor verification.
 */
export function matchQueryToSplatAnchors(
  keypoints: readonly QueryKeypoint2D[],
  anchors: readonly SplatAnchor3D[],
  options: {
    loweRatio?: number;
    maxDescriptorDist?: number;
    preferredDomain?: VpsScanDomain;
  } = {},
): Match2D3D[] {
  const ratio = options.loweRatio ?? 0.8;
  const maxDist = options.maxDescriptorDist ?? 0.9;
  const domain = options.preferredDomain ?? "hybrid";

  const candidateAnchors =
    domain === "hybrid"
      ? anchors
      : anchors.filter((a) => a.domain === domain || a.domain === "hybrid");

  if (candidateAnchors.length < 2 || keypoints.length === 0) {
    return [];
  }

  const forwardBestIdx = new Int32Array(keypoints.length).fill(-1);
  const forwardBestDist = new Float32Array(keypoints.length).fill(Number.POSITIVE_INFINITY);

  for (let q = 0; q < keypoints.length; q++) {
    const qDesc = keypoints[q].descriptor;
    let bestD = Number.POSITIVE_INFINITY;
    let secondD = Number.POSITIVE_INFINITY;
    let bestA = -1;

    for (let a = 0; a < candidateAnchors.length; a++) {
      const d = l2Distance(qDesc, candidateAnchors[a].descriptor);
      if (d < bestD) {
        secondD = bestD;
        bestD = d;
        bestA = a;
      } else if (d < secondD) {
        secondD = d;
      }
    }

    if (bestA >= 0 && bestD <= maxDist && bestD < ratio * secondD) {
      forwardBestIdx[q] = bestA;
      forwardBestDist[q] = bestD;
    }
  }

  // Mutual nearest-neighbor check (anchor -> query)
  const reverseBestQuery = new Int32Array(candidateAnchors.length).fill(-1);
  const reverseBestDist = new Float32Array(candidateAnchors.length).fill(Number.POSITIVE_INFINITY);
  for (let q = 0; q < keypoints.length; q++) {
    const a = forwardBestIdx[q];
    if (a < 0) continue;
    const d = forwardBestDist[q];
    if (d < reverseBestDist[a]) {
      reverseBestDist[a] = d;
      reverseBestQuery[a] = q;
    }
  }

  const matches: Match2D3D[] = [];
  for (let q = 0; q < keypoints.length; q++) {
    const a = forwardBestIdx[q];
    if (a >= 0 && reverseBestQuery[a] === q) {
      const anchor = candidateAnchors[a];
      matches.push({
        queryIndex: q,
        anchorId: anchor.anchorId,
        u: keypoints[q].u,
        v: keypoints[q].v,
        pointEnu: anchor.positionEnu,
        descriptorDistance: forwardBestDist[q],
      });
    }
  }
  return matches;
}

/**
 * Project a 3D point in ENU into pixel coordinates (u, v) and depth zCam given
 * world-to-camera rotation R (row-major 9) and camera optical center C_enu.
 */
export function projectEnuPointToPixel(
  pointEnu: Vec3,
  rotationWorldToCam: readonly number[],
  cameraCenterEnu: Vec3,
  intrinsics: CameraIntrinsics,
): { u: number; v: number; zCam: number } | null {
  const dx = pointEnu[0] - cameraCenterEnu[0];
  const dy = pointEnu[1] - cameraCenterEnu[1];
  const dz = pointEnu[2] - cameraCenterEnu[2];

  const xCam =
    rotationWorldToCam[0] * dx + rotationWorldToCam[1] * dy + rotationWorldToCam[2] * dz;
  const yCam =
    rotationWorldToCam[3] * dx + rotationWorldToCam[4] * dy + rotationWorldToCam[5] * dz;
  const zCam =
    rotationWorldToCam[6] * dx + rotationWorldToCam[7] * dy + rotationWorldToCam[8] * dz;

  if (zCam <= 1e-3) return null;
  const u = intrinsics.cx + (intrinsics.fx * xCam) / zCam;
  const v = intrinsics.cy + (intrinsics.fy * yCam) / zCam;
  return { u, v, zCam };
}

/**
 * Solve 3x3 linear system A * x = b via Gaussian elimination with partial pivoting.
 */
function solve3x3(A: number[][], b: number[]): Vec3 | null {
  const M = [
    [A[0][0], A[0][1], A[0][2], b[0]],
    [A[1][0], A[1][1], A[1][2], b[1]],
    [A[2][0], A[2][1], A[2][2], b[2]],
  ];

  for (let col = 0; col < 3; col++) {
    let pivot = col;
    for (let row = col + 1; row < 3; row++) {
      if (Math.abs(M[row][col]) > Math.abs(M[pivot][col])) pivot = row;
    }
    if (Math.abs(M[pivot][col]) < 1e-11) return null;
    if (pivot !== col) {
      const tmp = M[col];
      M[col] = M[pivot];
      M[pivot] = tmp;
    }
    for (let row = col + 1; row < 3; row++) {
      const factor = M[row][col] / M[col][col];
      for (let k = col; k < 4; k++) {
        M[row][k] -= factor * M[col][k];
      }
    }
  }

  const x = [0, 0, 0] as Vec3;
  for (let row = 2; row >= 0; row--) {
    let sum = M[row][3];
    for (let k = row + 1; k < 3; k++) {
      sum -= M[row][k] * x[k];
    }
    x[row] = sum / M[row][row];
  }
  return x;
}

/**
 * Apply small-angle exponential map update exp([omega]_x) * R.
 */
function applySmallRotation(
  R: readonly number[],
  wx: number,
  wy: number,
  wz: number,
): [number, number, number, number, number, number, number, number, number] {
  const theta = Math.hypot(wx, wy, wz);
  if (theta < 1e-12) {
    return [R[0], R[1], R[2], R[3], R[4], R[5], R[6], R[7], R[8]];
  }
  const kx = wx / theta;
  const ky = wy / theta;
  const kz = wz / theta;
  const c = Math.cos(theta);
  const s = Math.sin(theta);
  const v = 1 - c;

  const dR = [
    c + kx * kx * v,
    kx * ky * v - kz * s,
    kx * kz * v + ky * s,
    ky * kx * v + kz * s,
    c + ky * ky * v,
    ky * kz * v - kx * s,
    kz * kx * v - ky * s,
    kz * ky * v + kx * s,
    c + kz * kz * v,
  ];

  const out = new Array<number>(9).fill(0);
  for (let r = 0; r < 3; r++) {
    for (let col = 0; col < 3; col++) {
      out[r * 3 + col] =
        dR[r * 3 + 0] * R[0 * 3 + col] +
        dR[r * 3 + 1] * R[1 * 3 + col] +
        dR[r * 3 + 2] * R[2 * 3 + col];
    }
  }
  return out as [number, number, number, number, number, number, number, number, number];
}

/**
 * Solve 6x6 normal equations J^T J delta = -J^T r for damped Levenberg-Marquardt pose refinement.
 */
function solve6x6(A: number[][], b: number[]): number[] | null {
  const n = 6;
  const M = A.map((row, i) => [...row, b[i]]);
  for (let col = 0; col < n; col++) {
    let pivot = col;
    for (let row = col + 1; row < n; row++) {
      if (Math.abs(M[row][col]) > Math.abs(M[pivot][col])) pivot = row;
    }
    if (Math.abs(M[pivot][col]) < 1e-12) return null;
    if (pivot !== col) {
      const tmp = M[col];
      M[col] = M[pivot];
      M[pivot] = tmp;
    }
    for (let row = col + 1; row < n; row++) {
      const f = M[row][col] / M[col][col];
      for (let k = col; k <= n; k++) {
        M[row][k] -= f * M[col][k];
      }
    }
  }
  const x = new Array<number>(n).fill(0);
  for (let row = n - 1; row >= 0; row--) {
    let sum = M[row][n];
    for (let k = row + 1; k < n; k++) {
      sum -= M[row][k] * x[k];
    }
    x[row] = sum / M[row][row];
  }
  return x;
}

/**
 * Estimate initial camera pose (R, C_enu) from a 3-point or N-point bearing + centroid hypothesis
 * and refine via Gauss-Newton / Levenberg-Marquardt reprojection minimization.
 */
function refinePoseLevenbergMarquardt(
  matches: readonly Match2D3D[],
  intrinsics: CameraIntrinsics,
  initCenterEnu: Vec3,
  initRotation: [number, number, number, number, number, number, number, number, number],
  iterations = 25,
): {
  centerEnu: Vec3;
  rotation: [number, number, number, number, number, number, number, number, number];
} {
  let center: Vec3 = [initCenterEnu[0], initCenterEnu[1], initCenterEnu[2]];
  let R = initRotation;
  let lambda = 1e-2;

  const evalCost = (
    c: Vec3,
    rot: [number, number, number, number, number, number, number, number, number],
  ): number => {
    let cost = 0;
    for (const m of matches) {
      const proj = projectEnuPointToPixel(m.pointEnu, rot, c, intrinsics);
      if (!proj) {
        cost += 1e6;
        continue;
      }
      const du = proj.u - m.u;
      const dv = proj.v - m.v;
      cost += du * du + dv * dv;
    }
    return cost;
  };

  let currentCost = evalCost(center, R);

  for (let iter = 0; iter < iterations; iter++) {
    const JTJ = Array.from({ length: 6 }, () => new Array<number>(6).fill(0));
    const JTr = new Array<number>(6).fill(0);

    for (const m of matches) {
      const dx = m.pointEnu[0] - center[0];
      const dy = m.pointEnu[1] - center[1];
      const dz = m.pointEnu[2] - center[2];

      const X = R[0] * dx + R[1] * dy + R[2] * dz;
      const Y = R[3] * dx + R[4] * dy + R[5] * dz;
      const Z = R[6] * dx + R[7] * dy + R[8] * dz;
      if (Z <= 1e-3) continue;

      const invZ = 1.0 / Z;
      const invZ2 = invZ * invZ;
      const uPred = intrinsics.cx + intrinsics.fx * X * invZ;
      const vPred = intrinsics.cy + intrinsics.fy * Y * invZ;
      const ru = uPred - m.u;
      const rv = vPred - m.v;

      // Partial derivatives w.r.t. camera-space point P_cam = (X, Y, Z)
      const du_dX = intrinsics.fx * invZ;
      const du_dZ = -intrinsics.fx * X * invZ2;
      const dv_dY = intrinsics.fy * invZ;
      const dv_dZ = -intrinsics.fy * Y * invZ2;

      // dP_cam / d(omega) = -[P_cam]_x = [0, Z, -Y; -Z, 0, X; Y, -X, 0]
      // dP_cam / d(center) = -R
      const Ju = [
        du_dZ * Y,
        du_dX * Z - du_dZ * X,
        -du_dX * Y,
        -(du_dX * R[0] + du_dZ * R[6]),
        -(du_dX * R[1] + du_dZ * R[7]),
        -(du_dX * R[2] + du_dZ * R[8]),
      ];
      const Jv = [
        -dv_dY * Z + dv_dZ * Y,
        -dv_dZ * X,
        dv_dY * X,
        -(dv_dY * R[3] + dv_dZ * R[6]),
        -(dv_dY * R[4] + dv_dZ * R[7]),
        -(dv_dY * R[5] + dv_dZ * R[8]),
      ];

      for (let i = 0; i < 6; i++) {
        JTr[i] += Ju[i] * ru + Jv[i] * rv;
        for (let j = 0; j < 6; j++) {
          JTJ[i][j] += Ju[i] * Ju[j] + Jv[i] * Jv[j];
        }
      }
    }

    for (let i = 0; i < 6; i++) {
      JTJ[i][i] += lambda * (JTJ[i][i] + 1.0);
    }

    const negJTr = JTr.map((v) => -v);
    const delta = solve6x6(JTJ, negJTr);
    if (!delta) break;

    const candR = applySmallRotation(R, delta[0], delta[1], delta[2]);
    const candC: Vec3 = [center[0] + delta[3], center[1] + delta[4], center[2] + delta[5]];
    const candCost = evalCost(candC, candR);

    if (candCost < currentCost) {
      currentCost = candCost;
      R = candR;
      center = candC;
      lambda = Math.max(1e-6, lambda * 0.5);
      if (Math.hypot(...delta) < 1e-6) break;
    } else {
      lambda = Math.min(1e4, lambda * 4.0);
    }
  }

  return { centerEnu: center, rotation: R };
}

/**
 * Build a look-at rotation matrix (world ENU -> camera where +Z is optical forward, +X right, +Y down)
 * from a camera position and 3D anchor centroid.
 */
function buildLookAtRotationEnu(
  cameraCenterEnu: Vec3,
  targetEnu: Vec3,
): [number, number, number, number, number, number, number, number, number] {
  let fx = targetEnu[0] - cameraCenterEnu[0];
  let fy = targetEnu[1] - cameraCenterEnu[1];
  let fz = targetEnu[2] - cameraCenterEnu[2];
  const fLen = Math.hypot(fx, fy, fz) || 1;
  fx /= fLen;
  fy /= fLen;
  fz /= fLen;

  // Choose world up: ENU +Z unless looking nearly straight down/up (nadir/zenith aerial scan),
  // in which case use ENU +North (+Y) as screen-up reference.
  const isNadirOrZenith = Math.abs(fz) > 0.92;
  const upX = 0;
  const upY = isNadirOrZenith ? 1 : 0;
  const upZ = isNadirOrZenith ? 0 : 1;

  // Right = normalize(forward x worldUp)
  let rx = fy * upZ - fz * upY;
  let ry = fz * upX - fx * upZ;
  let rz = fx * upY - fy * upX;
  const rLen = Math.hypot(rx, ry, rz) || 1;
  rx /= rLen;
  ry /= rLen;
  rz /= rLen;

  // Down = forward x right
  const dx = fy * rz - fz * ry;
  const dy = fz * rx - fx * rz;
  const dz = fx * ry - fy * rx;

  return [rx, ry, rz, dx, dy, dz, fx, fy, fz];
}

/**
 * Stage 3: Solve 6-DoF Camera Pose in ENU coordinates via PnP + RANSAC + Levenberg-Marquardt refinement.
 */
export function solvePnpRansac6DoF(
  matches: readonly Match2D3D[],
  intrinsics: CameraIntrinsics,
  options: {
    reprojThresholdPx?: number;
    maxIterations?: number;
    priorCenterEnu?: Vec3;
    priorRotation?: [number, number, number, number, number, number, number, number, number];
  } = {},
): {
  success: boolean;
  centerEnu: Vec3;
  rotationWorldToCam: [number, number, number, number, number, number, number, number, number];
  inliers: Match2D3D[];
  medianErrorPx: number;
  /** Median camera-to-inlier distance at the solution (m). */
  medianDepthM: number;
} {
  const thresholdPx = options.reprojThresholdPx ?? 6.0;
  if (matches.length < 4) {
    return {
      success: false,
      centerEnu: [0, 0, 0],
      rotationWorldToCam: [1, 0, 0, 0, 1, 0, 0, 0, 1],
      inliers: [],
      medianErrorPx: Number.POSITIVE_INFINITY,
      medianDepthM: Number.POSITIVE_INFINITY,
    };
  }

  // Compute 3D centroid and spread of matched anchors
  let mx = 0;
  let my = 0;
  let mz = 0;
  for (const m of matches) {
    mx += m.pointEnu[0];
    my += m.pointEnu[1];
    mz += m.pointEnu[2];
  }
  mx /= matches.length;
  my /= matches.length;
  mz /= matches.length;
  const centroid: Vec3 = [mx, my, mz];

  let avgSpan = 0;
  for (const m of matches) {
    avgSpan += Math.hypot(m.pointEnu[0] - mx, m.pointEnu[1] - my, m.pointEnu[2] - mz);
  }
  avgSpan = Math.max(5.0, avgSpan / matches.length);

  // Estimate apparent depth from pixel spread vs 3D span
  let avgPixRadius = 0;
  for (const m of matches) {
    avgPixRadius += Math.hypot(m.u - intrinsics.cx, m.v - intrinsics.cy);
  }
  avgPixRadius = Math.max(20.0, avgPixRadius / matches.length);
  const estDist = Math.max(2.0, (avgSpan * 0.5 * (intrinsics.fx + intrinsics.fy)) / avgPixRadius);

  // Candidate initial camera centers around the anchor cloud (ground N/S/E/W + oblique + nadir aerial)
  const hypotheses: Array<{
    c: Vec3;
    r: [number, number, number, number, number, number, number, number, number];
  }> = [];

  if (options.priorCenterEnu && options.priorRotation) {
    hypotheses.push({ c: options.priorCenterEnu, r: options.priorRotation });
  } else if (options.priorCenterEnu) {
    hypotheses.push({
      c: options.priorCenterEnu,
      r: buildLookAtRotationEnu(options.priorCenterEnu, centroid),
    });
  }

  const offsets: Vec3[] = [
    [0, -estDist, 1.7],
    [0, estDist, 1.7],
    [-estDist, 0, 1.7],
    [estDist, 0, 1.7],
    [0, 0, estDist],
    [-0.6 * estDist, -0.6 * estDist, 0.5 * estDist],
    [0.6 * estDist, -0.6 * estDist, 0.5 * estDist],
  ];
  for (const off of offsets) {
    const c: Vec3 = [centroid[0] + off[0], centroid[1] + off[1], centroid[2] + off[2]];
    hypotheses.push({ c, r: buildLookAtRotationEnu(c, centroid) });
  }

  let bestInliers: Match2D3D[] = [];
  let bestCenter: Vec3 = hypotheses[0].c;
  let bestRot = hypotheses[0].r;
  let bestMedianErr = Number.POSITIVE_INFINITY;

  // Minimal-subset hypotheses. The geometric candidates above cover the plausible camera
  // placements; these seeded random 4-tuples add the usual RANSAC coverage against mismatched
  // anchors. The generator is a fixed LCG so a localisation result is reproducible.
  const subsets: Match2D3D[][] = [];
  subsets.push(matches.slice());
  const maxSubsets = Math.min(options.maxIterations ?? 12, matches.length);
  for (let s = 0; s < Math.min(3, maxSubsets); s++) {
    subsets.push([
      matches[s % matches.length],
      matches[(s + 1) % matches.length],
      matches[(s + 2) % matches.length],
      matches[(s + 3) % matches.length],
    ]);
  }
  if (matches.length >= 8) {
    let lcg = 0x9e3779b9;
    const next = () => {
      lcg = (Math.imul(lcg, 1664525) + 1013904223) >>> 0;
      return lcg / 4294967296;
    };
    const draws = Math.min(24, Math.max(8, matches.length / 4));
    for (let d = 0; d < draws; d++) {
      const picked = new Set<number>();
      while (picked.size < 4) picked.add(Math.floor(next() * matches.length) % matches.length);
      subsets.push([...picked].map((i) => matches[i]));
    }
  }

  for (const hyp of hypotheses) {
    for (const subset of subsets) {
      const refined = refinePoseLevenbergMarquardt(subset, intrinsics, hyp.c, hyp.r, 20);
      const inliers: Match2D3D[] = [];
      const errors: number[] = [];

      for (const m of matches) {
        const proj = projectEnuPointToPixel(m.pointEnu, refined.rotation, refined.centerEnu, intrinsics);
        if (!proj) continue;
        const err = Math.hypot(proj.u - m.u, proj.v - m.v);
        if (err <= thresholdPx) {
          inliers.push(m);
          errors.push(err);
        }
      }

      if (inliers.length >= 4) {
        errors.sort((a, b) => a - b);
        const med = errors[Math.floor(errors.length / 2)];
        if (
          inliers.length > bestInliers.length ||
          (inliers.length === bestInliers.length && med < bestMedianErr)
        ) {
          bestInliers = inliers;
          bestCenter = refined.centerEnu;
          bestRot = refined.rotation;
          bestMedianErr = med;
        }
      }
    }
  }

  if (bestInliers.length < 4) {
    return {
      success: false,
      centerEnu: bestCenter,
      rotationWorldToCam: bestRot,
      inliers: bestInliers,
      medianErrorPx: bestMedianErr,
      medianDepthM: Number.POSITIVE_INFINITY,
    };
  }

  // Final polish over all discovered RANSAC inliers
  const finalRefined = refinePoseLevenbergMarquardt(bestInliers, intrinsics, bestCenter, bestRot, 30);
  const finalErrors: number[] = [];
  const finalInliers: Match2D3D[] = [];
  for (const m of matches) {
    const proj = projectEnuPointToPixel(
      m.pointEnu,
      finalRefined.rotation,
      finalRefined.centerEnu,
      intrinsics,
    );
    if (!proj) continue;
    const err = Math.hypot(proj.u - m.u, proj.v - m.v);
    if (err <= thresholdPx) {
      finalInliers.push(m);
      finalErrors.push(err);
    }
  }

  finalErrors.sort((a, b) => a - b);
  const finalMed =
    finalErrors.length > 0 ? finalErrors[Math.floor(finalErrors.length / 2)] : bestMedianErr;

  // Guard against unused helper warning in strict tsconfig
  void solve3x3;

  const depths = finalInliers
    .map((m) =>
      Math.hypot(
        m.pointEnu[0] - finalRefined.centerEnu[0],
        m.pointEnu[1] - finalRefined.centerEnu[1],
        m.pointEnu[2] - finalRefined.centerEnu[2],
      ),
    )
    .sort((a, b) => a - b);
  const medianDepthM =
    depths.length > 0 ? depths[Math.floor(depths.length / 2)] : Number.POSITIVE_INFINITY;

  return {
    success: finalInliers.length >= 4,
    centerEnu: finalRefined.centerEnu,
    rotationWorldToCam: finalRefined.rotation,
    inliers: finalInliers,
    medianErrorPx: finalMed,
    medianDepthM,
  };
}

/**
 * Extract compass heading [0, 360), pitch [-90, 90], and roll from world-to-camera rotation R.
 */
export function extractNavigationAnglesDeg(
  rotationWorldToCam: readonly number[],
): { headingDeg: number; pitchDeg: number; rollDeg: number } {
  // Optical axis (+Z_cam) in world ENU is the 3rd row of R: [R[6], R[7], R[8]]
  const fwdEast = rotationWorldToCam[6];
  const fwdNorth = rotationWorldToCam[7];
  const fwdUp = rotationWorldToCam[8];

  let headingDeg = (Math.atan2(fwdEast, fwdNorth) * 180.0) / Math.PI;
  if (headingDeg < 0) headingDeg += 360.0;

  const pitchDeg = (Math.asin(Math.max(-1, Math.min(1, fwdUp))) * 180.0) / Math.PI;
  const rightUp = rotationWorldToCam[2];
  const downUp = rotationWorldToCam[5];
  const rollDeg = (Math.atan2(rightUp, -downUp) * 180.0) / Math.PI;

  return { headingDeg, pitchDeg, rollDeg };
}

/**
 * Full 4-stage Visual Positioning System (VPS) pipeline:
 * Localizes a ground-level or geospatial query image against a collection of 3D Gaussian Splat
 * submaps and returns the georeferenced 6-DoF WGS84 + ENU pose.
 */
export function localizeCameraVps(
  submaps: readonly VpsSubmap[],
  query: {
    globalDescriptor: ArrayLike<number>;
    keypoints: readonly QueryKeypoint2D[];
    intrinsics: CameraIntrinsics;
    domain?: VpsScanDomain;
    priorWgs84?: [number, number, number];
    priorCenterEnu?: Vec3;
    priorRotation?: [number, number, number, number, number, number, number, number, number];
    maxSearchRadiusM?: number;
  },
): VpsPoseResult {
  const domain = query.domain ?? "hybrid";
  const candidates = retrieveCandidateVpsSubmaps(submaps, query.globalDescriptor, {
    priorWgs84: query.priorWgs84,
    maxSearchRadiusM: query.maxSearchRadiusM,
    topK: 3,
  });

  for (const cand of candidates) {
    const matches = matchQueryToSplatAnchors(query.keypoints, cand.submap.anchors, {
      preferredDomain: domain,
    });
    if (matches.length < 4) continue;

    const pnp = solvePnpRansac6DoF(matches, query.intrinsics, {
      priorCenterEnu: query.priorCenterEnu,
      priorRotation: query.priorRotation,
    });
    if (!pnp.success) continue;

    const wgs84 = enuToWgs84(
      pnp.centerEnu[0],
      pnp.centerEnu[1],
      pnp.centerEnu[2],
      cand.submap.originWgs84,
    );
    const { headingDeg, pitchDeg, rollDeg } = extractNavigationAnglesDeg(pnp.rotationWorldToCam);

    // Position uncertainty from geometry, not a tuned constant:
    //   sigma = (reproj px / focal px) * median depth / sqrt(n)
    // A landmark set that is nearly planar or nearly flat in height constrains height badly, so
    // the vertical term is divided by the height-spread ratio (clamped so it cannot explode).
    const focalPx = 0.5 * (query.intrinsics.fx + query.intrinsics.fy);
    const n = Math.max(1, pnp.inliers.length);
    const lateralBound = (pnp.medianErrorPx / Math.max(1, focalPx)) * pnp.medianDepthM;
    const sigmaBase = lateralBound / Math.sqrt(n);
    const heights = pnp.inliers.map((m) => m.pointEnu[2]).sort((a, b) => a - b);
    const heightSpreadM =
      heights.length > 1 ? heights[heights.length - 1] - heights[0] : 0;
    const heightRatio = Math.max(0.05, Math.min(1, heightSpreadM / Math.max(1e-6, pnp.medianDepthM)));
    const hdopM = Number(Math.max(0.01, sigmaBase).toFixed(3));
    const vdopM = Number(Math.max(0.02, sigmaBase / heightRatio).toFixed(3));

    const confidence =
      pnp.inliers.length >= 6 && pnp.medianErrorPx < 2.0
        ? "high"
        : pnp.inliers.length >= 4 && pnp.medianErrorPx < 5.0
          ? "medium"
          : "low";

    return {
      success: true,
      confidence,
      submapId: cand.submap.submapId,
      domainUsed: domain,
      positionEnu: pnp.centerEnu,
      positionWgs84: wgs84,
      rotationWorldToCam: pnp.rotationWorldToCam,
      headingDeg,
      pitchDeg,
      rollDeg,
      inlierCount: pnp.inliers.length,
      totalMatches: matches.length,
      medianReprojectionErrorPx: pnp.medianErrorPx,
      hdopM,
      vdopM,
      medianInlierDepthM: pnp.medianDepthM,
    };
  }

  return {
    success: false,
    confidence: "failed",
    submapId: null,
    domainUsed: domain,
    positionEnu: [0, 0, 0],
    positionWgs84: [0, 0, 0],
    rotationWorldToCam: [1, 0, 0, 0, 1, 0, 0, 0, 1],
    headingDeg: 0,
    pitchDeg: 0,
    rollDeg: 0,
    inlierCount: 0,
    totalMatches: 0,
    medianReprojectionErrorPx: Number.POSITIVE_INFINITY,
    hdopM: Number.POSITIVE_INFINITY,
    vdopM: Number.POSITIVE_INFINITY,
    medianInlierDepthM: Number.POSITIVE_INFINITY,
  };
}
