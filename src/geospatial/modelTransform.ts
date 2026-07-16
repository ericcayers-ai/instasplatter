/**
 * Manual geo splat pose override (translate / rotate / non-uniform scale).
 * Persisted on the project as `modelTransform` — overrides auto geo registration pose.
 */

export type Vec3 = [number, number, number];

export type GizmoMode = "translate" | "rotate" | "scale";

/** TRS in local ENU metres; rotation is row-major 3×3. */
export interface ModelTransform {
  translation: Vec3;
  /** Row-major 3×3. */
  rotation: number[];
  scale: Vec3;
}

export function identityModelTransform(): ModelTransform {
  return {
    translation: [0, 0, 0],
    rotation: [1, 0, 0, 0, 1, 0, 0, 0, 1],
    scale: [1, 1, 1],
  };
}

export function cloneModelTransform(t: ModelTransform): ModelTransform {
  return {
    translation: [...t.translation] as Vec3,
    rotation: [...t.rotation],
    scale: [...t.scale] as Vec3,
  };
}

export function normalizeModelTransform(raw: Partial<ModelTransform> | null | undefined): ModelTransform {
  const id = identityModelTransform();
  if (!raw) return id;
  const tr = raw.translation;
  const sc = raw.scale;
  const rot = raw.rotation;
  return {
    translation:
      Array.isArray(tr) && tr.length >= 3
        ? [Number(tr[0]) || 0, Number(tr[1]) || 0, Number(tr[2]) || 0]
        : id.translation,
    rotation:
      Array.isArray(rot) && rot.length >= 9
        ? rot.slice(0, 9).map((v) => Number(v) || 0)
        : id.rotation,
    scale:
      Array.isArray(sc) && sc.length >= 3
        ? [
            Math.max(1e-4, Number(sc[0]) || 1),
            Math.max(1e-4, Number(sc[1]) || 1),
            Math.max(1e-4, Number(sc[2]) || 1),
          ]
        : id.scale,
  };
}

/** Row-major 3×3 → nested rows for splat `modelRotation`. */
export function rotationToMat3(rot: number[]): number[][] {
  return [
    [rot[0], rot[1], rot[2]],
    [rot[3], rot[4], rot[5]],
    [rot[6], rot[7], rot[8]],
  ];
}

export function mat3ToRotation(m: number[][]): number[] {
  return [m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2]];
}

/**
 * Column-major 4×4: scale → rotate → translate about `pivot`
 * (same pivot convention as splat `modelMatrix`, then extra translation).
 */
export function modelTransformMatrix(t: ModelTransform, pivot: Vec3 = [0, 0, 0]): Float32Array {
  const r = t.rotation;
  const sx = t.scale[0];
  const sy = t.scale[1];
  const sz = t.scale[2];
  // R * S
  const m00 = r[0] * sx;
  const m01 = r[1] * sy;
  const m02 = r[2] * sz;
  const m10 = r[3] * sx;
  const m11 = r[4] * sy;
  const m12 = r[5] * sz;
  const m20 = r[6] * sx;
  const m21 = r[7] * sy;
  const m22 = r[8] * sz;
  // p' = R S (p - pivot) + pivot + translation
  const rp0 = m00 * pivot[0] + m01 * pivot[1] + m02 * pivot[2];
  const rp1 = m10 * pivot[0] + m11 * pivot[1] + m12 * pivot[2];
  const rp2 = m20 * pivot[0] + m21 * pivot[1] + m22 * pivot[2];
  const tx = pivot[0] - rp0 + t.translation[0];
  const ty = pivot[1] - rp1 + t.translation[1];
  const tz = pivot[2] - rp2 + t.translation[2];
  return new Float32Array([
    m00, m10, m20, 0,
    m01, m11, m21, 0,
    m02, m12, m22, 0,
    tx, ty, tz, 1,
  ]);
}

/** Rodrigues row-major 3×3 about a unit axis. */
export function axisAngleRotation(axis: Vec3, angle: number): number[] {
  const [x, y, z] = axis;
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  const t = 1 - c;
  return [
    t * x * x + c,
    t * x * y - s * z,
    t * x * z + s * y,
    t * x * y + s * z,
    t * y * y + c,
    t * y * z - s * x,
    t * x * z - s * y,
    t * y * z + s * x,
    t * z * z + c,
  ];
}

/** Row-major 3×3 product `a * b`. */
export function mulRot(a: number[], b: number[]): number[] {
  const o = new Array<number>(9);
  for (let r = 0; r < 3; r++) {
    for (let c = 0; c < 3; c++) {
      o[r * 3 + c] =
        a[r * 3] * b[c] + a[r * 3 + 1] * b[3 + c] + a[r * 3 + 2] * b[6 + c];
    }
  }
  return o;
}
