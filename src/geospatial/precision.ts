/**
 * Large-scene precision for geospatial ENU work.
 *
 * The problem, stated exactly: a float32 stores 24 bits of mantissa, so the spacing between
 * representable values near a coordinate of magnitude `m` is `2^(floor(log2 m) - 23)`. Half of that
 * is the worst-case representation error. Measured: 0.03 mm at 1 km, 0.49 mm at 10 km, 0.98 mm at
 * 20 km, 1.95 mm at 40 km, 3.9 mm at 100 km. The 1 mm mark is crossed between 16.4 km and 32.8 km,
 * and beyond that a metre-scale georeferenced scene can no longer be carried in one float32 frame.
 * A splat or a camera transform that carries an absolute ENU coordinate therefore loses sub-mm
 * geometry somewhere between 10 and 20 km from the ENU origin — not because the data is wrong, but
 * because the arithmetic is done in single precision.
 *
 * Two mitigations, both cheap:
 *
 * 1. **Camera-relative ("relative to eye", RTC) rendering.** Subtract the camera position in double
 *    precision on the CPU (or once per vertex) and transform only the small residual. The error
 *    then scales with *distance from the camera*, not distance from the origin, so a 100 km scene
 *    behaves like a 100 m scene. This is the standard fix and it is exact: `R*(x - eye) == R*x + t`
 *    for `t = -R*eye`, only better conditioned.
 * 2. **Per-tile frames.** Past tens of kilometres, keep each tile in its own ENU frame and store the
 *    tile origin as double-precision metadata (Cesium 3D Tiles does this with `RTC_CENTER`). That is
 *    a data-layout change, not a shader change, and is recorded as a gap in docs/AUDIT-2026-11-03.md.
 *
 * The functions below are the arithmetic behind both statements, so the claims are testable rather
 * than asserted. See `precision.smoke.ts` for the measured numbers.
 */

/** Spacing between adjacent float32 values at `magnitude` (the ULP). */
export function float32Spacing(magnitude: number): number {
  const m = Math.abs(magnitude);
  if (!Number.isFinite(m) || m === 0) return 0;
  return Math.pow(2, Math.floor(Math.log2(m)) - 23);
}

/** Worst-case absolute error of representing a coordinate as float32 (half an ULP). */
export function float32RepresentationErrorM(coordinateM: number): number {
  return 0.5 * float32Spacing(coordinateM);
}

/**
 * Error introduced by transforming an absolute ENU coordinate in float32, versus the same point
 * handled relative to the camera.
 */
export interface PrecisionComparison {
  /** Distance of the point from the ENU origin (m). */
  originDistanceM: number;
  /** Distance of the point from the camera (m). */
  cameraDistanceM: number;
  /** Half-ULP error when the absolute coordinate is used (m). */
  absoluteErrorM: number;
  /** Half-ULP error when the camera-relative coordinate is used (m). */
  relativeErrorM: number;
  /** How much better the camera-relative form is (x). */
  improvementFactor: number;
}

export function comparePrecision(
  pointEnu: readonly [number, number, number],
  cameraEnu: readonly [number, number, number],
): PrecisionComparison {
  const originDistanceM = Math.hypot(pointEnu[0], pointEnu[1], pointEnu[2]);
  const rx = pointEnu[0] - cameraEnu[0];
  const ry = pointEnu[1] - cameraEnu[1];
  const rz = pointEnu[2] - cameraEnu[2];
  const cameraDistanceM = Math.hypot(rx, ry, rz);
  // Worst component dominates the error.
  const absoluteErrorM = Math.max(
    float32RepresentationErrorM(pointEnu[0]),
    float32RepresentationErrorM(pointEnu[1]),
    float32RepresentationErrorM(pointEnu[2]),
  );
  const relativeErrorM = Math.max(
    float32RepresentationErrorM(rx),
    float32RepresentationErrorM(ry),
    float32RepresentationErrorM(rz),
  );
  return {
    originDistanceM,
    cameraDistanceM,
    absoluteErrorM,
    relativeErrorM,
    improvementFactor: relativeErrorM > 0 ? absoluteErrorM / relativeErrorM : 1,
  };
}

export interface FrameStrategy {
  /** Largest scene extent that stays within the requested positional budget. */
  maxExtentM: number;
  /** Worst-case float32 error at that extent (m). */
  errorAtMaxExtentM: number;
  /** True when the scene must be split into independently framed tiles. */
  requiresTiledFrames: boolean;
  /** Suggested tile edge for that split (m). */
  tileExtentM: number;
  reason: string;
}

/**
 * Pick a framing strategy for a scene of `extentM` that must hold `budgetM` positional accuracy.
 *
 * With camera-relative rendering the governing quantity is the *scene extent* (how far a vertex can
 * be from the camera), so the rule is: keep the extent inside the budget, or tile.
 */
export function recommendFrameStrategy(extentM: number, budgetM = 0.005): FrameStrategy {
  const safeExtent = (budget: number): number => {
    // Largest extent whose half-ULP error is still within `budget`.
    let lo = 1;
    let hi = 1e9;
    for (let i = 0; i < 60; i++) {
      const mid = Math.sqrt(lo * hi);
      if (float32RepresentationErrorM(mid) <= budget) lo = mid;
      else hi = mid;
    }
    return lo;
  };
  const maxExtentM = safeExtent(budgetM);
  const requiresTiledFrames = extentM > maxExtentM;
  return {
    maxExtentM,
    errorAtMaxExtentM: float32RepresentationErrorM(maxExtentM),
    requiresTiledFrames,
    tileExtentM: requiresTiledFrames ? Math.max(100, Math.floor(maxExtentM)) : extentM,
    reason: requiresTiledFrames
      ? `extent ${Math.round(extentM)} m exceeds the ${maxExtentM.toFixed(0)} m float32 budget for ${budgetM * 1000} mm; split into tiles of ~${Math.max(100, Math.floor(maxExtentM))} m with per-tile ENU origins`
      : `extent ${Math.round(extentM)} m fits the float32 budget (error <= ${(budgetM * 1000).toFixed(1)} mm) with camera-relative rendering`,
  };
}

/**
 * Camera-relative transform: the residual a shader should actually transform.
 * Subtraction happens in double precision (JS numbers), which is the whole point.
 */
export function relativeToEye(
  pointEnu: readonly [number, number, number],
  eyeEnu: readonly [number, number, number],
): [number, number, number] {
  return [pointEnu[0] - eyeEnu[0], pointEnu[1] - eyeEnu[1], pointEnu[2] - eyeEnu[2]];
}

/**
 * Recover the camera position from a world-to-camera view matrix (column-major), so a caller that
 * only has a view matrix (the shared geospatial scene) can still use camera-relative rendering.
 * `eye = -Rᵀ t` where the view is `[R | t]`.
 */
export function eyeFromViewMatrix(viewColMajor: ArrayLike<number>): [number, number, number] {
  // Column-major [R | t]: element (row, col) lives at index col*4 + row, so the columns of R are
  // (view[0..2], view[4..6], view[8..10]) and the translation is (view[12..14]).
  const c0x = viewColMajor[0], c0y = viewColMajor[1], c0z = viewColMajor[2];
  const c1x = viewColMajor[4], c1y = viewColMajor[5], c1z = viewColMajor[6];
  const c2x = viewColMajor[8], c2y = viewColMajor[9], c2z = viewColMajor[10];
  const t0 = viewColMajor[12], t1 = viewColMajor[13], t2 = viewColMajor[14];
  // eye = -Rᵀ t, i.e. component i is -(column i of R) · t
  return [
    -(c0x * t0 + c0y * t1 + c0z * t2),
    -(c1x * t0 + c1y * t1 + c1z * t2),
    -(c2x * t0 + c2y * t1 + c2z * t2),
  ];
}

/** The same view matrix with its translation column removed (rotation only). */
export function rotationOnlyView(viewColMajor: ArrayLike<number>): Float32Array {
  const out = new Float32Array(16);
  for (let i = 0; i < 16; i++) out[i] = viewColMajor[i];
  out[12] = 0;
  out[13] = 0;
  out[14] = 0;
  return out;
}
