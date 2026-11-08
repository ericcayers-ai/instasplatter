/**
 * Frustum culling unit test.
 *
 * Run: npx tsx src/splat/frustum.smoke.ts
 *
 * The renderer's per-frame culling decision is this module, so it is tested directly rather than
 * only through a screenshot. Geometry is built with the renderer's own camera helpers, so the
 * conventions (column-major matrices, +z forward, clip z in [0, w]) cannot drift apart.
 */

import {
  defaultCamera,
  mat4Mul,
  orbitBasis,
  projectionMatrix,
  trsModelMatrix,
  identity3,
  viewMatrix,
  type CameraState,
} from "./camera";
import { frustumPlanes, sphereInFrustum, visibleChunkIndices } from "./frustum";

let failures = 0;
function check(name: string, pass: boolean, detail = "") {
  if (!pass) failures++;
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `  (${detail})` : ""}`);
}

const near = 0.1;
const far = 1000;

function makeMvp(cam: CameraState, model?: Float32Array) {
  const proj = projectionMatrix(1280, 720, cam.fovY, near, far);
  const view = viewMatrix(orbitBasis(cam));
  const m = model ?? trsModelMatrix(identity3(), [0, 0, 0], [0, 0, 0], [1, 1, 1]);
  return mat4Mul(mat4Mul(proj, view), m);
}

const cam = defaultCamera();
cam.target = [0, 0, 0];
cam.distance = 10;
cam.yaw = 0;
cam.pitch = 0;
const basis = orbitBasis(cam);
const planes = frustumPlanes(makeMvp(cam));
const eye = basis.eye;
const fwd = basis.forward;
const right = basis.right;

check("6 planes extracted", planes.length === 6);
check(
  "planes are normalised",
  planes.every((p) => Math.abs(Math.hypot(p[0], p[1], p[2]) - 1) < 1e-5),
);

const at = (k: number, axis: [number, number, number]): [number, number, number] => [
  eye[0] + axis[0] * k,
  eye[1] + axis[1] * k,
  eye[2] + axis[2] * k,
];

check("target is inside", sphereInFrustum(planes, [0, 0, 0], 0.1));
check("point in front, 5 m is inside", sphereInFrustum(planes, at(5, fwd), 0.1));
check("point behind the camera is outside", !sphereInFrustum(planes, at(-5, fwd), 0.1));
check("point 100 m to the right is outside", !sphereInFrustum(planes, at(100, right), 0.1));
check("point beyond the far plane is outside", !sphereInFrustum(planes, at(5000, fwd), 0.1));
check(
  "a large sphere behind the camera still intersects (conservative)",
  sphereInFrustum(planes, at(-5, fwd), 20),
);

// Chunk selection uses the camera's own basis, so the expectation cannot depend on which way
// the default camera happens to face.
const identityModel = trsModelMatrix(identity3(), [0, 0, 0], [0, 0, 0], [1, 1, 1]);
const chunkCentres = new Float32Array([
  ...at(10, fwd), // ahead
  ...at(200, right), // far to the side
  ...at(-10, fwd), // behind the camera
  ...at(-200, right), // far to the other side
]);
const chunkRadii = new Float32Array([1, 1, 1, 1]);
const visible = visibleChunkIndices(planes, chunkCentres, chunkRadii, identityModel, 1);
check(
  "only the chunk ahead survives culling",
  visible.length === 1 && visible[0] === 0,
  `visible=[${visible.join(",")}]`,
);

// Model translation must move the chunks with the scene: pushing the whole cloud far behind the
// camera must cull everything.
const behind = [-fwd[0] * 400, -fwd[1] * 400, -fwd[2] * 400] as [number, number, number];
const shifted = visibleChunkIndices(
  planes,
  chunkCentres,
  chunkRadii,
  trsModelMatrix(identity3(), [0, 0, 0], behind, [1, 1, 1]),
  1,
);
check("model translation is applied", shifted.length === 0, `visible=[${shifted.join(",")}]`);

// Conservativeness property: any point known to be inside must keep a small sphere visible, and a
// point far outside must not. This is the invariant the renderer relies on to not drop geometry.
let falseNegatives = 0;
let falsePositives = 0;
for (let i = 0; i < 200; i++) {
  const a = (i / 200 - 0.5) * 0.6; // fraction of the half-angle
  const b = ((i * 37) % 200) / 200 - 0.5;
  const depth = 5 + (i % 50);
  const p: [number, number, number] = [
    eye[0] + (fwd[0] + right[0] * a + basis.down[0] * b) * depth,
    eye[1] + (fwd[1] + right[1] * a + basis.down[1] * b) * depth,
    eye[2] + (fwd[2] + right[2] * a + basis.down[2] * b) * depth,
  ];
  if (!sphereInFrustum(planes, p, 1e-3)) falseNegatives++;
  const out: [number, number, number] = [p[0] + fwd[0] * 100000, p[1] + fwd[1] * 100000, p[2] + fwd[2] * 100000];
  if (sphereInFrustum(planes, out, 1e-3)) falsePositives++;
}
check("no interior point is culled", falseNegatives === 0, `${falseNegatives}/200`);
check("no far-exterior point is kept", falsePositives === 0, `${falsePositives}/200`);

if (failures > 0) throw new Error(`frustum smoke: ${failures} failure(s)`);
console.log("frustum.smoke: ok");
