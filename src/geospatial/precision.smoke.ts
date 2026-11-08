/**
 * Geospatial precision gate.
 *
 * Run: npx tsx src/geospatial/precision.smoke.ts
 *
 * These are arithmetic facts about float32, not benchmark results: the point is to pin the
 * threshold where absolute ENU coordinates stop being usable and to prove that the
 * camera-relative path the splat shader now uses keeps the error tied to camera distance.
 */

import {
  comparePrecision,
  eyeFromViewMatrix,
  float32RepresentationErrorM,
  float32Spacing,
  recommendFrameStrategy,
  relativeToEye,
  rotationOnlyView,
} from "./precision";
import { defaultCamera, mat4Mul, orbitBasis, viewMatrix } from "../splat/camera";

let failures = 0;
function check(name: string, pass: boolean, detail = "") {
  if (!pass) failures++;
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `  (${detail})` : ""}`);
}

// 1. ULP growth: 24-bit mantissa means 1 mm is lost somewhere around 16.8 km.
const err1km = float32RepresentationErrorM(1000);
const err10km = float32RepresentationErrorM(10_000);
const err20km = float32RepresentationErrorM(20_000);
const err100km = float32RepresentationErrorM(100_000);
check("1 km absolute error is sub-0.1 mm", err1km < 1e-4, `${(err1km * 1000).toFixed(4)} mm`);
check("10 km absolute error is under 0.5 mm", err10km < 5e-4, `${(err10km * 1000).toFixed(4)} mm`);
check(
  "20 km absolute error is just under 1 mm",
  err20km < 1e-3 && err20km > 9e-4,
  `${(err20km * 1000).toFixed(3)} mm`,
);
check(
  "the 1 mm mark is crossed at 32.8 km",
  float32RepresentationErrorM(32_768) >= 1e-3,
  `${(float32RepresentationErrorM(32_768) * 1000).toFixed(3)} mm at 32.768 km`,
);
check("100 km absolute error is ~3.9 mm", Math.abs(err100km - 0.00390625) < 1e-9, `${(err100km * 1000).toFixed(3)} mm`);
check(
  "spacing doubles per octave",
  Math.abs(float32Spacing(20_000) / float32Spacing(10_000) - 2) < 1e-9,
);

// 2. Camera-relative beats absolute by the ratio of the distances.
const point: [number, number, number] = [18_000, 12_000, 40];
const camera: [number, number, number] = [17_960, 11_990, 1.7];
const cmp = comparePrecision(point, camera);
check(
  "camera-relative error is at least 100x smaller at 21 km from origin",
  cmp.improvementFactor > 100,
  `${cmp.improvementFactor.toFixed(0)}x (${(cmp.absoluteErrorM * 1000).toFixed(2)} mm -> ${(cmp.relativeErrorM * 1000).toFixed(4)} mm)`,
);
check(
  "camera-relative error is tied to camera distance, not origin distance",
  cmp.relativeErrorM <= float32RepresentationErrorM(cmp.cameraDistanceM) * 1.000001,
  `camera distance ${cmp.cameraDistanceM.toFixed(1)} m`,
);

// 3. Framing strategy: a 2 km site is fine, a 40 km one needs tiles.
const near = recommendFrameStrategy(2000, 0.001);
const far = recommendFrameStrategy(40_000, 0.001);
check("2 km site needs no tiles at 1 mm budget", !near.requiresTiledFrames, near.reason);
check("40 km scene requires tiled frames", far.requiresTiledFrames, far.reason);
check(
  "tile extent respects the budget",
  !recommendFrameStrategy(far.tileExtentM, 0.001).requiresTiledFrames,
  `tile ${far.tileExtentM} m`,
);

// 4. The view-matrix helpers are consistent: R*(p - eye) == R*p + t.
const cam = defaultCamera();
cam.target = [500, -300, 20];
cam.distance = 120;
cam.yaw = 1.1;
cam.pitch = -0.4;
const basis = orbitBasis(cam);
const view = viewMatrix(basis);
const eye = eyeFromViewMatrix(view);
const identityProj = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
const vpPlain = mat4Mul(identityProj, view);
const vpRtc = mat4Mul(identityProj, rotationOnlyView(view));

const p: [number, number, number] = [520, -280, 24];
const plain = applyMat4(vpPlain, p);
const rtc = applyMat4(vpRtc, relativeToEye(p, eye));
check(
  "RTC and absolute transforms agree mathematically",
  plain.every((v, i) => Math.abs(v - rtc[i]) < 1e-3),
  `max delta ${Math.max(...plain.map((v, i) => Math.abs(v - rtc[i]))).toExponential(2)}`,
);
// The view matrix is a Float32Array, so recovering the eye is only accurate to the float32
// spacing of its components (~3e-5 m at 500 m). 1 cm is a generous pass mark.
const eyeErr = Math.hypot(
  eye[0] - basis.eye[0],
  eye[1] - basis.eye[1],
  eye[2] - basis.eye[2],
);
check(
  "recovered eye matches the camera position",
  eyeErr < 1e-2,
  `eye ${eye.map((v) => v.toFixed(3)).join(", ")}; error ${(eyeErr * 1000).toFixed(2)} mm`,
);

function applyMat4(m: ArrayLike<number>, v: readonly [number, number, number]): [number, number, number] {
  return [
    m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12],
    m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13],
    m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14],
  ];
}

if (failures > 0) throw new Error(`precision smoke: ${failures} failure(s)`);
console.log("precision.smoke: ok");
