// Orbit camera math for the splat viewport (ROADMAP-V2 1.2).
//
// Two conventions drive everything here.
//
// 1. The splat projection matrix negates y, so camera-space +y points DOWN
//    the screen. The view matrix therefore has `down` as its second row, not
//    an "up" vector. Getting this backwards renders the scene upside down.
//
// 2. COLMAP's world has +y pointing down, so the direction that should appear
//    up on screen is world -y. That is `worldUp` below, and it is the only
//    place the convention is encoded.
//
// The orbit basis is built directly from yaw and pitch rather than by crossing
// the forward vector with a world up vector. Crossing degenerates when the
// camera looks along the up axis, which is what makes naive orbit cameras flip
// or gimbal at the poles. Here `right` comes from the yaw derivative, which is
// unit length at every pitch including exactly +/- 90 degrees.

export type Vec3 = [number, number, number];

export interface CameraState {
  target: Vec3;
  distance: number;
  yaw: number;
  pitch: number;
  fovY: number;
  /** World-space direction that should appear up on screen. */
  worldUp: Vec3;
}

export interface CameraBasis {
  eye: Vec3;
  /** Camera +z. */
  forward: Vec3;
  /** Camera +x. */
  right: Vec3;
  /** Camera +y, which is down the screen. */
  down: Vec3;
}

export const HALF_PI = Math.PI / 2;
/** Pitch is clamped here so the camera reaches straight up and down without inverting. */
export const MAX_PITCH = HALF_PI;

export const MIN_DISTANCE = 1e-3;
export const MAX_DISTANCE = 1e5;

export function add(a: Vec3, b: Vec3): Vec3 {
  return [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
}
export function sub(a: Vec3, b: Vec3): Vec3 {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
}
export function scale(a: Vec3, s: number): Vec3 {
  return [a[0] * s, a[1] * s, a[2] * s];
}
export function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}
export function cross(a: Vec3, b: Vec3): Vec3 {
  return [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
}
export function length(a: Vec3): number {
  return Math.hypot(a[0], a[1], a[2]);
}
export function normalize(a: Vec3): Vec3 {
  const n = length(a);
  return n < 1e-12 ? [0, 0, 0] : scale(a, 1 / n);
}

export function defaultCamera(): CameraState {
  return {
    target: [0, 0, 0],
    distance: 4,
    yaw: 0.4,
    pitch: -0.25,
    fovY: (60 * Math.PI) / 180,
    // COLMAP world is y-down, so up on screen is world -y.
    worldUp: [0, -1, 0],
  };
}

/** Orthonormal frame `(e1, e2, up)` with `e1 x e2 = up`. */
function upFrame(worldUp: Vec3): [Vec3, Vec3, Vec3] {
  const up = normalize(worldUp);
  const seed: Vec3 = Math.abs(up[0]) < 0.9 ? [1, 0, 0] : [0, 0, 1];
  const e1 = normalize(cross(seed, up));
  const e2 = cross(up, e1);
  return [e1, e2, up];
}

/**
 * Camera frame for the current orbit angles. Well defined at every pitch: the
 * right vector is the normalized yaw derivative, which never collapses.
 */
export function orbitBasis(cam: CameraState): CameraBasis {
  const [e1, e2, up] = upFrame(cam.worldUp);
  const cp = Math.cos(cam.pitch);
  const sp = Math.sin(cam.pitch);
  const cy = Math.cos(cam.yaw);
  const sy = Math.sin(cam.yaw);

  // Unit vector from target to eye.
  const offset = add(add(scale(e1, cp * sy), scale(e2, cp * cy)), scale(up, sp));
  const eye = add(cam.target, scale(offset, cam.distance));
  const forward = scale(offset, -1);
  const right = sub(scale(e2, sy), scale(e1, cy));
  const down = cross(forward, right);
  return { eye, forward, right, down };
}

/** Column-major view matrix with rows `right`, `down`, `forward`. */
export function viewMatrix(b: CameraBasis): Float32Array {
  const { right: r, down: d, forward: f, eye } = b;
  return new Float32Array([
    r[0], d[0], f[0], 0,
    r[1], d[1], f[1], 0,
    r[2], d[2], f[2], 0,
    -dot(r, eye), -dot(d, eye), -dot(f, eye), 1,
  ]);
}

/** Column-major projection. Row 1 is negated because camera +y is screen down. */
export function projectionMatrix(
  widthPx: number,
  heightPx: number,
  fovY: number,
  near: number,
  far: number,
): Float32Array {
  const fy = heightPx / (2 * Math.tan(fovY / 2));
  const fx = fy;
  return new Float32Array([
    (2 * fx) / widthPx, 0, 0, 0,
    0, -(2 * fy) / heightPx, 0, 0,
    0, 0, far / (far - near), 1,
    0, 0, -(far * near) / (far - near), 0,
  ]);
}

/** Focal lengths in pixels, matching `projectionMatrix`. */
export function focalPx(heightPx: number, fovY: number): number {
  return heightPx / (2 * Math.tan(fovY / 2));
}

export function clampPitch(pitch: number): number {
  return Math.max(-MAX_PITCH, Math.min(MAX_PITCH, pitch));
}

export function clampDistance(d: number): number {
  return Math.max(MIN_DISTANCE, Math.min(MAX_DISTANCE, d));
}

/** Orbit by pointer deltas in CSS pixels. */
export function orbitBy(cam: CameraState, dxPx: number, dyPx: number, speed = 0.005): void {
  cam.yaw -= dxPx * speed;
  cam.pitch = clampPitch(cam.pitch - dyPx * speed);
}

/**
 * World units covered by one CSS pixel on the plane through the target.
 * Scaling by distance is what makes a grabbed point track the cursor exactly,
 * at any zoom level.
 */
export function unitsPerPixel(cam: CameraState, viewportHeightPx: number): number {
  if (viewportHeightPx <= 0) return 0;
  return (2 * cam.distance * Math.tan(cam.fovY / 2)) / viewportHeightPx;
}

/**
 * Pan so the point grabbed under the cursor stays under the cursor: move the
 * camera opposite the drag, in the view plane.
 */
export function panBy(
  cam: CameraState,
  dxPx: number,
  dyPx: number,
  viewportHeightPx: number,
): void {
  const upp = unitsPerPixel(cam, viewportHeightPx);
  if (upp === 0) return;
  const b = orbitBasis(cam);
  cam.target = sub(cam.target, scale(b.right, dxPx * upp));
  cam.target = sub(cam.target, scale(b.down, dyPx * upp));
}

/**
 * Dolly by `factor` while holding the point under the cursor fixed on screen.
 *
 * The reference point is where the cursor ray meets the plane through the
 * target, perpendicular to the view direction. Writing that point as
 * `P = T + tanHalf * d * (right * sx * aspect + down * sy)` for both the old
 * and new distance and eliminating `P` gives the closed form below.
 *
 * `cursorX`/`cursorY` are CSS pixels measured from the top-left of the canvas.
 */
export function zoomAtCursor(
  cam: CameraState,
  factor: number,
  cursorX: number,
  cursorY: number,
  widthPx: number,
  heightPx: number,
): void {
  const d0 = cam.distance;
  const d1 = clampDistance(d0 * factor);
  if (widthPx <= 0 || heightPx <= 0) {
    cam.distance = d1;
    return;
  }
  const sx = (cursorX / widthPx) * 2 - 1;
  const sy = (cursorY / heightPx) * 2 - 1;
  const tanHalf = Math.tan(cam.fovY / 2);
  const aspect = widthPx / heightPx;
  const b = orbitBasis(cam);
  const k = (d0 - d1) * tanHalf;
  cam.target = add(
    cam.target,
    add(scale(b.right, k * sx * aspect), scale(b.down, k * sy)),
  );
  cam.distance = d1;
}

/** `a * b`, both column-major. */
export function mat4Mul(a: Float32Array, b: Float32Array): Float32Array {
  const o = new Float32Array(16);
  for (let r = 0; r < 4; r++) {
    for (let c = 0; c < 4; c++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + c] * b[r * 4 + k];
      o[r * 4 + c] = s;
    }
  }
  return o;
}

/** Column-major 4x4 from a row-major 3x3 rotation about `pivot`. */
export function modelMatrix(rot: number[][], pivot: Vec3): Float32Array {
  return trsModelMatrix(rot, pivot);
}

/**
 * Column-major 4×4: scale about pivot, then rotate about pivot, then translate.
 * `p' = R S (p - pivot) + pivot + translation`
 */
export function trsModelMatrix(
  rot: number[][],
  pivot: Vec3,
  translation: Vec3 = [0, 0, 0],
  scale: Vec3 = [1, 1, 1],
): Float32Array {
  const sx = scale[0];
  const sy = scale[1];
  const sz = scale[2];
  const m00 = rot[0][0] * sx;
  const m01 = rot[0][1] * sy;
  const m02 = rot[0][2] * sz;
  const m10 = rot[1][0] * sx;
  const m11 = rot[1][1] * sy;
  const m12 = rot[1][2] * sz;
  const m20 = rot[2][0] * sx;
  const m21 = rot[2][1] * sy;
  const m22 = rot[2][2] * sz;
  const rp: Vec3 = [
    m00 * pivot[0] + m01 * pivot[1] + m02 * pivot[2],
    m10 * pivot[0] + m11 * pivot[1] + m12 * pivot[2],
    m20 * pivot[0] + m21 * pivot[1] + m22 * pivot[2],
  ];
  const t = add(sub(pivot, rp), translation);
  return new Float32Array([
    m00, m10, m20, 0,
    m01, m11, m21, 0,
    m02, m12, m22, 0,
    t[0], t[1], t[2], 1,
  ]);
}

export function identity3(): number[][] {
  return [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ];
}

/** Row-major 3x3 product. */
export function mat3Mul(a: number[][], b: number[][]): number[][] {
  const o = [
    [0, 0, 0],
    [0, 0, 0],
    [0, 0, 0],
  ];
  for (let r = 0; r < 3; r++)
    for (let c = 0; c < 3; c++) o[r][c] = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c];
  return o;
}

/** Rotation of `angle` radians about a unit `axis` (Rodrigues, row-major). */
export function axisAngle(axis: Vec3, angle: number): number[][] {
  const k = normalize(axis);
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  const t = 1 - c;
  const [x, y, z] = k;
  return [
    [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
    [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
    [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
  ];
}

/** Shortest-arc rotation carrying unit `from` onto unit `to` (row-major). */
export function rotationBetween(from: Vec3, to: Vec3): number[][] {
  const a = normalize(from);
  const b = normalize(to);
  const d = Math.max(-1, Math.min(1, dot(a, b)));
  if (d > 1 - 1e-9) return identity3();
  if (d < -1 + 1e-9) {
    const seed: Vec3 = Math.abs(a[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
    return axisAngle(normalize(cross(a, seed)), Math.PI);
  }
  return axisAngle(normalize(cross(a, b)), Math.acos(d));
}

// ---------------------------------------------------------------------------
// Camera path smoothing (Catmull-Rom → cubic Bézier) + orientation slerp.
// Shared by viewport path viz, flythrough animation, and path export helpers.
// Does not alter SfM / live-tracking solver poses — visualization only.
// ---------------------------------------------------------------------------

/** Unit quaternion `(w, x, y, z)`. */
export type Quat = [number, number, number, number];

export interface PathPose {
  position: Vec3;
  /** Optional look orientation; when absent, derived from path tangents. */
  orientation?: Quat;
}

export interface SmoothPathOptions {
  /** Samples per Catmull-Rom segment (inclusive of start; end shared). Default 8. */
  samplesPerSegment?: number;
}

export function quatIdentity(): Quat {
  return [1, 0, 0, 0];
}

export function quatDot(a: Quat, b: Quat): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
}

export function quatNormalize(q: Quat): Quat {
  const n = Math.hypot(q[0], q[1], q[2], q[3]);
  return n < 1e-12 ? quatIdentity() : [q[0] / n, q[1] / n, q[2] / n, q[3] / n];
}

/** Shortest-arc spherical linear interpolation of unit quaternions. */
export function quatSlerp(a: Quat, b: Quat, t: number): Quat {
  const qa = quatNormalize(a);
  let qb = quatNormalize(b);
  let d = quatDot(qa, qb);
  if (d < 0) {
    qb = [-qb[0], -qb[1], -qb[2], -qb[3]];
    d = -d;
  }
  if (d > 0.9995) {
    return quatNormalize([
      qa[0] + t * (qb[0] - qa[0]),
      qa[1] + t * (qb[1] - qa[1]),
      qa[2] + t * (qb[2] - qa[2]),
      qa[3] + t * (qb[3] - qa[3]),
    ]);
  }
  const th = Math.acos(Math.max(-1, Math.min(1, d)));
  const s = Math.sin(th);
  const w0 = Math.sin((1 - t) * th) / s;
  const w1 = Math.sin(t * th) / s;
  return [w0 * qa[0] + w1 * qb[0], w0 * qa[1] + w1 * qb[1], w0 * qa[2] + w1 * qb[2], w0 * qa[3] + w1 * qb[3]];
}

/** Cubic Bézier at parameter `t` in [0, 1]. */
export function evalCubicBezier(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: number): Vec3 {
  const u = 1 - t;
  const uu = u * u;
  const tt = t * t;
  const uuu = uu * u;
  const ttt = tt * t;
  return [
    uuu * p0[0] + 3 * uu * t * p1[0] + 3 * u * tt * p2[0] + ttt * p3[0],
    uuu * p0[1] + 3 * uu * t * p1[1] + 3 * u * tt * p2[1] + ttt * p3[1],
    uuu * p0[2] + 3 * uu * t * p1[2] + 3 * u * tt * p2[2] + ttt * p3[2],
  ];
}

/**
 * Convert four Catmull-Rom points into cubic Bézier controls for the middle
 * segment (P1 → P2). Uniform parameterization — C¹ continuous in position.
 */
export function catmullRomToBezier(
  p0: Vec3,
  p1: Vec3,
  p2: Vec3,
  p3: Vec3,
): [Vec3, Vec3, Vec3, Vec3] {
  const b1: Vec3 = [
    p1[0] + (p2[0] - p0[0]) / 6,
    p1[1] + (p2[1] - p0[1]) / 6,
    p1[2] + (p2[2] - p0[2]) / 6,
  ];
  const b2: Vec3 = [
    p2[0] - (p3[0] - p1[0]) / 6,
    p2[1] - (p3[1] - p1[1]) / 6,
    p2[2] - (p3[2] - p1[2]) / 6,
  ];
  return [p1, b1, b2, p2];
}

/**
 * Resample a polyline through cubic Bézier segments derived from Catmull-Rom.
 * Endpoints are preserved; interior motion is C1-smooth.
 */
export function smoothPolyline(points: Vec3[], opts: SmoothPathOptions = {}): Vec3[] {
  const n = points.length;
  if (n === 0) return [];
  if (n === 1) return [points[0]];
  if (n === 2) return [points[0], points[1]];

  const samples = Math.max(2, Math.floor(opts.samplesPerSegment ?? 8));
  const out: Vec3[] = [];

  for (let i = 0; i < n - 1; i++) {
    const p0 = points[Math.max(0, i - 1)];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = points[Math.min(n - 1, i + 2)];
    const [b0, b1, b2, b3] = catmullRomToBezier(p0, p1, p2, p3);
    const last = i === n - 2;
    const count = last ? samples : samples - 1;
    for (let s = 0; s < count; s++) {
      const t = s / (samples - 1);
      out.push(evalCubicBezier(b0, b1, b2, b3, t));
    }
  }
  return out;
}

/** Forward direction from consecutive samples (fallback: +Z). */
export function pathTangent(points: Vec3[], i: number): Vec3 {
  if (points.length < 2) return [0, 0, 1];
  const a = points[Math.max(0, i - 1)];
  const b = points[Math.min(points.length - 1, i + 1)];
  const d = sub(b, a);
  const n = length(d);
  return n < 1e-12 ? [0, 0, 1] : scale(d, 1 / n);
}

/**
 * Orientation looking along `forward` with `worldUp` as the preferred up.
 * Returns `(w,x,y,z)` for a camera whose -Z (or +Z COLMAP forward) aligns with
 * `forward`. Here we encode look-at as rotation taking +Z onto `forward`.
 */
export function quatLookRotation(forward: Vec3, worldUp: Vec3 = [0, -1, 0]): Quat {
  const f = normalize(forward);
  if (length(f) < 1e-12) return quatIdentity();
  let up = normalize(worldUp);
  let right = cross(up, f);
  if (length(right) < 1e-8) {
    const seed: Vec3 = Math.abs(f[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
    right = normalize(cross(seed, f));
    up = cross(f, right);
  } else {
    right = normalize(right);
    up = cross(f, right);
  }
  // Rotation matrix columns = right, up, forward (row-major rows).
  const m = [
    [right[0], up[0], f[0]],
    [right[1], up[1], f[1]],
    [right[2], up[2], f[2]],
  ];
  return quatFromMat3(m);
}

export function quatFromMat3(m: number[][]): Quat {
  const tr = m[0][0] + m[1][1] + m[2][2];
  if (tr > 0) {
    const s = Math.sqrt(tr + 1) * 2;
    return quatNormalize([0.25 * s, (m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s]);
  }
  if (m[0][0] > m[1][1] && m[0][0] > m[2][2]) {
    const s = Math.sqrt(1 + m[0][0] - m[1][1] - m[2][2]) * 2;
    return quatNormalize([(m[2][1] - m[1][2]) / s, 0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s]);
  }
  if (m[1][1] > m[2][2]) {
    const s = Math.sqrt(1 + m[1][1] - m[0][0] - m[2][2]) * 2;
    return quatNormalize([(m[0][2] - m[2][0]) / s, (m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s]);
  }
  const s = Math.sqrt(1 + m[2][2] - m[0][0] - m[1][1]) * 2;
  return quatNormalize([(m[1][0] - m[0][1]) / s, (m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s]);
}

/**
 * Smooth positions with Bézier and orientations with slerp.
 * Missing orientations are filled from path tangents before interpolation.
 */
export function smoothCameraPath(poses: PathPose[], opts: SmoothPathOptions = {}): PathPose[] {
  if (poses.length === 0) return [];
  if (poses.length === 1) {
    const p = poses[0];
    return [{ position: p.position, orientation: p.orientation ?? quatIdentity() }];
  }
  const positions = poses.map((p) => p.position);
  const smoothed = smoothPolyline(positions, opts);
  // Map each smoothed sample to a fractional index along the original polyline.
  const filled: Quat[] = poses.map((p, i) => {
    if (p.orientation) return quatNormalize(p.orientation);
    return quatLookRotation(pathTangent(positions, i));
  });
  // Ensure consecutive quats take the short arc.
  for (let i = 1; i < filled.length; i++) {
    if (quatDot(filled[i - 1], filled[i]) < 0) {
      filled[i] = [-filled[i][0], -filled[i][1], -filled[i][2], -filled[i][3]];
    }
  }

  const out: PathPose[] = [];
  const segs = poses.length - 1;
  const samples = Math.max(2, Math.floor(opts.samplesPerSegment ?? 8));
  let idx = 0;
  for (let i = 0; i < segs; i++) {
    const last = i === segs - 1;
    const count = last ? samples : samples - 1;
    for (let s = 0; s < count; s++) {
      const t = s / (samples - 1);
      const pos =
        smoothed[idx++] ??
        evalCubicBezier(
          ...catmullRomToBezier(
            positions[Math.max(0, i - 1)],
            positions[i],
            positions[i + 1],
            positions[Math.min(poses.length - 1, i + 2)],
          ),
          t,
        );
      out.push({
        position: pos,
        orientation: quatSlerp(filled[i], filled[i + 1], t),
      });
    }
  }
  return out;
}

/**
 * Sample a smoothed position path at normalized `u` in [0, 1].
 * Returns eye position and a look-at point slightly ahead on the path.
 */
export function sampleFlythrough(
  points: Vec3[],
  u: number,
  lookAhead = 0.02,
): { eye: Vec3; target: Vec3 } {
  if (points.length === 0) return { eye: [0, 0, 0], target: [0, 0, -1] };
  if (points.length === 1) return { eye: points[0], target: add(points[0], [0, 0, -1]) };
  const uu = ((u % 1) + 1) % 1;
  const f = uu * (points.length - 1);
  const i = Math.min(points.length - 2, Math.floor(f));
  const t = f - i;
  const eye = add(scale(points[i], 1 - t), scale(points[i + 1], t));
  const ua = Math.min(1, uu + lookAhead);
  const fa = ua * (points.length - 1);
  const ia = Math.min(points.length - 2, Math.floor(fa));
  const ta = fa - ia;
  const target = add(scale(points[ia], 1 - ta), scale(points[ia + 1], ta));
  if (length(sub(target, eye)) < 1e-8) {
    return { eye, target: add(eye, pathTangent(points, i)) };
  }
  return { eye, target };
}

/** Fit orbit camera so `eye` looks at `target` (COLMAP y-down worldUp). */
export function orbitFromEyeTarget(cam: CameraState, eye: Vec3, target: Vec3): void {
  const [e1, e2, up] = upFrame(cam.worldUp);
  const offset = sub(eye, target);
  const dist = length(offset);
  if (dist < 1e-8) return;
  const dir = scale(offset, 1 / dist);
  const sp = Math.max(-1, Math.min(1, dot(dir, up)));
  cam.pitch = clampPitch(Math.asin(sp));
  const cp = Math.cos(cam.pitch);
  if (Math.abs(cp) < 1e-6) {
    cam.distance = clampDistance(dist);
    cam.target = target;
    return;
  }
  const flat = normalize(sub(dir, scale(up, sp)));
  // flat = e1 * sy + e2 * cy  → yaw from atan2 against e1/e2.
  cam.yaw = Math.atan2(dot(flat, e1), dot(flat, e2));
  cam.distance = clampDistance(dist);
  cam.target = target;
}

// --- Self-tests (`node --experimental-strip-types src/splat/camera.ts`) ------

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(msg);
}

function runSelfTests(): void {
  assert(Math.abs(clampPitch(10) - MAX_PITCH) < 1e-12, "clampPitch high");
  assert(Math.abs(clampPitch(-10) + MAX_PITCH) < 1e-12, "clampPitch low");

  const pts: Vec3[] = [
    [0, 0, 0],
    [1, 0, 0],
    [2, 1, 0],
    [3, 1, 1],
  ];
  const smooth = smoothPolyline(pts, { samplesPerSegment: 6 });
  assert(smooth.length >= pts.length, "smooth densifies");
  assert(length(sub(smooth[0], pts[0])) < 1e-9, "preserves start");
  assert(length(sub(smooth[smooth.length - 1], pts[pts.length - 1])) < 1e-9, "preserves end");

  // Mid-segment should not sit exactly on the sharp polyline corner at index 1→2.
  const mid = smooth[Math.floor(smooth.length / 2)];
  assert(Number.isFinite(mid[0]) && Number.isFinite(mid[1]), "finite samples");

  const q0 = quatIdentity();
  const q1 = quatLookRotation([1, 0, 0]);
  const qm = quatSlerp(q0, q1, 0.5);
  assert(Math.abs(Math.hypot(qm[0], qm[1], qm[2], qm[3]) - 1) < 1e-6, "slerp unit");

  const poses = smoothCameraPath(
    pts.map((position) => ({ position })),
    { samplesPerSegment: 4 },
  );
  assert(poses.length >= pts.length, "pose path densifies");
  assert(poses.every((p) => p.orientation != null), "orientations filled");

  const fly = sampleFlythrough(smooth, 0.5);
  assert(length(sub(fly.target, fly.eye)) > 1e-6, "flythrough look-ahead");

  const cam = defaultCamera();
  orbitFromEyeTarget(cam, [0, 0, 4], [0, 0, 0]);
  const b = orbitBasis(cam);
  assert(length(sub(b.eye, [0, 0, 4])) < 1e-4, "orbitFromEyeTarget eye");

  console.log("camera.ts: ok");
}

const isDirectRun = (() => {
  const proc = (globalThis as { process?: { argv?: string[] } }).process;
  const arg = proc?.argv?.[1];
  return typeof arg === "string" && /camera\.(ts|js)$/.test(arg.replace(/\\/g, "/"));
})();

if (isDirectRun) {
  runSelfTests();
}
