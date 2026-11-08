/**
 * Frustum culling for the splat renderer.
 *
 * Pure functions, so the geometry can be tested without a GPU (`frustum.smoke.ts`). The renderer
 * uses these to decide which spatial chunks are worth sorting and drawing this frame, which is
 * what keeps a geospatial scene's per-frame cost proportional to what is on screen
 * (chunk-level LOD culling, the Hierarchical-3DGS / Octree-GS idea).
 *
 * Conventions (must match `camera.ts`):
 * - matrices are column-major `Float32Array(16)`, applied as `clip = mvp * world`;
 * - the projection maps camera-space +z forward to clip `z in [0, w]` (not the OpenGL
 *   `[-w, w]` range), which is why the near plane is `row2` and the far plane is `row3 - row2`.
 */

export type FrustumPlane = [number, number, number, number];

/**
 * Six normalised clip-space planes from a combined projection*view*model matrix.
 * A point is inside when `dot(plane.xyz, p) + plane.w >= 0` for every plane.
 */
export function frustumPlanes(mvp: ArrayLike<number>): FrustumPlane[] {
  // Column-major storage: row i is (m[i], m[4+i], m[8+i], m[12+i]).
  const r0: FrustumPlane = [mvp[0], mvp[4], mvp[8], mvp[12]];
  const r1: FrustumPlane = [mvp[1], mvp[5], mvp[9], mvp[13]];
  const r2: FrustumPlane = [mvp[2], mvp[6], mvp[10], mvp[14]];
  const r3: FrustumPlane = [mvp[3], mvp[7], mvp[11], mvp[15]];
  const planes: FrustumPlane[] = [
    [r3[0] + r0[0], r3[1] + r0[1], r3[2] + r0[2], r3[3] + r0[3]], // x >= -w
    [r3[0] - r0[0], r3[1] - r0[1], r3[2] - r0[2], r3[3] - r0[3]], // x <=  w
    [r3[0] + r1[0], r3[1] + r1[1], r3[2] + r1[2], r3[3] + r1[3]], // y >= -w
    [r3[0] - r1[0], r3[1] - r1[1], r3[2] - r1[2], r3[3] - r1[3]], // y <=  w
    [r2[0], r2[1], r2[2], r2[3]], // z >= 0   (near)
    [r3[0] - r2[0], r3[1] - r2[1], r3[2] - r2[2], r3[3] - r2[3]], // z <= w (far)
  ];
  for (const p of planes) {
    const len = Math.hypot(p[0], p[1], p[2]) || 1;
    p[0] /= len;
    p[1] /= len;
    p[2] /= len;
    p[3] /= len;
  }
  return planes;
}

/** Conservative sphere test: false only when the sphere is fully outside some plane. */
export function sphereInFrustum(
  planes: readonly FrustumPlane[],
  centre: ArrayLike<number>,
  radius: number,
): boolean {
  for (let i = 0; i < planes.length; i++) {
    const p = planes[i];
    if (p[0] * centre[0] + p[1] * centre[1] + p[2] * centre[2] + p[3] < -radius) return false;
  }
  return true;
}

/**
 * Indices of the chunk spheres that survive culling.
 * `model` maps model space to world space (the splat shader applies the same matrix).
 */
export function visibleChunkIndices(
  planes: readonly FrustumPlane[],
  chunkCentres: Float32Array,
  chunkRadii: Float32Array,
  model: ArrayLike<number>,
  modelScale: number,
): number[] {
  const n = chunkRadii.length;
  const out: number[] = [];
  const world: [number, number, number] = [0, 0, 0];
  for (let i = 0; i < n; i++) {
    const mx = chunkCentres[i * 3];
    const my = chunkCentres[i * 3 + 1];
    const mz = chunkCentres[i * 3 + 2];
    world[0] = model[0] * mx + model[4] * my + model[8] * mz + model[12];
    world[1] = model[1] * mx + model[5] * my + model[9] * mz + model[13];
    world[2] = model[2] * mx + model[6] * my + model[10] * mz + model[14];
    if (sphereInFrustum(planes, world, chunkRadii[i] * modelScale)) out.push(i);
  }
  return out;
}
