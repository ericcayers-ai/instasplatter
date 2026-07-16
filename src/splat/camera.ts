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
