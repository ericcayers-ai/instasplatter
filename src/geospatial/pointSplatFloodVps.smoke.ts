/**
 * Verification smoke suite for:
 * 1. SIGGRAPH '26 Gaussian Point Splatting (`src/splat/pointSplatting.ts`)
 * 2. Splat-to-Mesh Saint-Venant Shallow Water Flood Simulation (`src/geospatial/hazards/floodSim.ts`)
 * 3. Ground + Geospatial 6-DoF Visual Positioning System (`src/geospatial/vps.ts`)
 *
 * Run: npx --yes tsx src/geospatial/pointSplatFloodVps.smoke.ts
 */

import {
  computeAdaptivePointCount,
  cullGeospatialSplatChunks,
  packDepthColor64,
  rasterizeGaussianPointSplatting,
  unpackDepthColor64,
  WGSL_GAUSSIAN_POINT_SPLATTING_SHADER,
  type GaussianPrimitive3D,
  type GeospatialSplatChunk,
} from "../splat/pointSplatting";
import {
  runSplatToMeshFloodSimulation,
  splatCloudToTerrainMeshAndDem,
} from "./hazards/floodSim";
import {
  localizeCameraVps,
  projectEnuPointToPixel,
  type CameraIntrinsics,
  type QueryKeypoint2D,
  type SplatAnchor3D,
  type VpsSubmap,
} from "./vps";

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`Assertion failed: ${msg}`);
}

// 1. Test SIGGRAPH '26 64-bit depth+color atomic packing & order-independent point splatting
{
  const pNear = packDepthColor64(4.25, 255, 64, 32, 255);
  const pFar = packDepthColor64(18.75, 10, 200, 120, 255);
  assert(pNear < pFar, "64-bit atomicMin ordering must rank nearer depth smaller");

  const uNear = unpackDepthColor64(pNear);
  assert(Math.abs(uNear.depthM - 4.25) < 1e-4, "unpacked depth must match");
  assert(
    uNear.rgba[0] === 255 && uNear.rgba[1] === 64 && uNear.rgba[2] === 32,
    "unpacked RGBA8 must match",
  );

  const splatFront: GaussianPrimitive3D = {
    center: [0, 0, 6],
    scale: [0.15, 0.15, 0.15],
    rotation: [1, 0, 0, 0],
    opacity: 0.99,
    colorRgba: [240, 30, 40, 255],
  };
  const splatBack: GaussianPrimitive3D = {
    center: [0, 0, 14],
    scale: [0.25, 0.25, 0.25],
    rotation: [1, 0, 0, 0],
    opacity: 0.99,
    colorRgba: [20, 80, 240, 255],
  };

  const nNear = computeAdaptivePointCount(splatFront, 4.0);
  const nFar = computeAdaptivePointCount(splatFront, 400.0);
  assert(nNear >= nFar && nNear > 0, "adaptive point count must increase at closer range");

  // Identity view matrix (column-major)
  const identityView = [
    1, 0, 0, 0,
    0, 1, 0, 0,
    0, 0, 1, 0,
    0, 0, 0, 1,
  ];
  const cfg = {
    width: 64,
    height: 64,
    focalX: 48,
    focalY: 48,
    nearClipM: 0.2,
    farClipM: 1000,
    pointBudgetScale: 2.0,
    maxPointsPerGaussian: 24,
    minOpacityCutoff: 0.05,
    samplesPerPixel: 2,
  };

  const renderAB = rasterizeGaussianPointSplatting([splatFront, splatBack], identityView, cfg);
  const renderBA = rasterizeGaussianPointSplatting([splatBack, splatFront], identityView, cfg);
  assert(renderAB.emittedPoints === renderBA.emittedPoints && renderAB.emittedPoints > 0, "emitted points must match");

  // Prove order-independence across every pixel (no global radix sort needed!)
  for (let i = 0; i < renderAB.rgbaBuffer.length; i++) {
    assert(
      renderAB.rgbaBuffer[i] === renderBA.rgbaBuffer[i],
      `pixel byte ${i} differed under permutation`,
    );
  }

  // Test geospatial chunk culling
  const chunks: GeospatialSplatChunk[] = [
    { chunkId: "in_front", minEnu: [-10, 20, -2], maxEnu: [10, 40, 5], splats: [splatFront] },
    { chunkId: "behind_camera", minEnu: [-10, -80, -2], maxEnu: [10, -60, 5], splats: [splatBack] },
  ];
  const visible = cullGeospatialSplatChunks(chunks, [0, 0, 2], [0, 1, 0], 500, 0.3);
  assert(visible.length === 1 && visible[0].chunkId === "in_front", "frustum chunk culling");
  assert(WGSL_GAUSSIAN_POINT_SPLATTING_SHADER.includes("atomicMin"), "WGSL shader must use atomicMin");
}

// 2. Test Splat-to-Mesh Terrain Extraction & Saint-Venant Shallow Water Flood Simulation
{
  const terrainSplats: GaussianPrimitive3D[] = [];
  for (let r = 0; r < 12; r++) {
    for (let c = 0; c < 12; c++) {
      const x = (c + 0.5) * 5.0;
      const y = (r + 0.5) * 5.0;
      // Bowl-shaped valley so water ponds in the center
      const distFromCenter = Math.hypot(c - 5.5, r - 5.5);
      const zGround = 2.0 + 0.18 * distFromCenter * distFromCenter;
      terrainSplats.push({
        center: [x, y, zGround],
        scale: [0.5, 0.5, 0.2],
        rotation: [1, 0, 0, 0],
        opacity: 0.92,
        colorRgba: [90, 130, 75, 255],
      });
    }
  }

  const mesh = splatCloudToTerrainMeshAndDem(terrainSplats, {
    cols: 12,
    rows: 12,
    dxM: 5.0,
    originEnu: [0, 0, 0],
  });
  assert(mesh.vertices.length === 12 * 12 * 3, "terrain mesh vertex count");
  assert(mesh.indices.length === 11 * 11 * 6, "terrain mesh triangle index count");
  assert(mesh.nodataMask.length === 12 * 12, "terrain mesh must carry a nodata mask");

  const sim = runSplatToMeshFloodSimulation(
    terrainSplats,
    { cols: 12, rows: 12, dxM: 5.0, originEnu: [0, 0, 0] },
    15.0,
    {
      rainfallMmPerHour: 72.0,
      pointSources: [{ col: 6, row: 6, dischargeM3s: 4.5 }],
    },
  );
  assert(sim.stepReport.steps > 0, "shallow-water solver must take positive steps");
  assert(sim.stepReport.maxDepthM > 0.05, "water must accumulate in the valley bowl");
  assert(sim.domain.provenance.coverageFraction > 0.99, "all 12x12 cells must be observed");
  assert(sim.domain.provenance.verticalDatum === "local-arbitrary", "datum must be stated");

  // Mass conservation on a closed domain: V(T) == rain + point source. The solver reports the
  // realised relative error; assert the measured number, not a hope.
  const domainAreaM2 = 12 * 12 * 25.0;
  const rainVolM3 = ((72.0 / 1000.0) / 3600.0) * domainAreaM2 * 15.0;
  const sourceVolM3 = 4.5 * 15.0;
  const expectedVolM3 = rainVolM3 + sourceVolM3;
  const relErr = Math.abs(sim.totalVolumeM3 - expectedVolM3) / expectedVolM3;
  assert(relErr < 1e-6, `mass conservation error too high: ${relErr}`);
  assert(sim.stepReport.positivityRepairs === 0, "no cell may be clipped negative");
  assert(sim.waterSurfaceMesh.wetTriangleCount > 0, "water surface mesh must emit wet triangles");
  assert(sim.connectivity.componentCount >= 1, "wet connectivity must be audited");
}

// 3. Test Ground + Geospatial 6-DoF Visual Positioning System (VPS)
{
  const originWgs84: [number, number, number] = [174.7762, -41.2865, 12.0]; // Wellington NZ
  const intrinsics: CameraIntrinsics = {
    width: 1280,
    height: 720,
    fx: 920,
    fy: 920,
    cx: 640,
    cy: 360,
  };

  // True ground-level camera at ENU [4.0, -18.0, 1.8] looking North (+Y in ENU)
  // For camera looking North (+Y_enu = +Z_cam), +East (+X_enu = +X_cam), -Up (-Z_enu = +Y_cam):
  const trueRot: [number, number, number, number, number, number, number, number, number] = [
    1, 0, 0,
    0, 0, -1,
    0, 1, 0,
  ];
  const trueCenterEnu: [number, number, number] = [4.0, -18.0, 1.8];

  const anchorCoords: Array<[number, number, number]> = [
    [-4.0, 10.0, 2.5],
    [10.0, 12.0, 3.2],
    [-2.5, 22.0, 6.0],
    [11.0, 25.0, 7.5],
    [3.0, 15.0, 1.2],
    [6.5, 18.0, 4.8],
    [-5.5, 16.0, 5.1],
    [8.0, 28.0, 2.0],
  ];

  const anchors: SplatAnchor3D[] = [];
  const keypoints: QueryKeypoint2D[] = [];

  for (let i = 0; i < anchorCoords.length; i++) {
    const desc = new Float32Array(16);
    desc[i % 16] = 1.0;
    desc[(i * 3 + 1) % 16] = 0.5;

    const pt = anchorCoords[i];
    anchors.push({
      anchorId: `anc_${i}`,
      positionEnu: pt,
      descriptor: desc,
      saliency: 0.95,
      domain: "ground",
    });

    const proj = projectEnuPointToPixel(pt, trueRot, trueCenterEnu, intrinsics);
    assert(proj !== null, `anchor ${i} must project in front of ground camera`);
    keypoints.push({
      u: proj!.u,
      v: proj!.v,
      descriptor: desc.slice(),
    });
  }

  const submap: VpsSubmap = {
    submapId: "wellington_cbd_tile_01",
    originWgs84,
    globalDescriptor: [0.8, 0.5, 0.2, 0.1],
    anchors,
  };

  const pose = localizeCameraVps([submap], {
    globalDescriptor: [0.82, 0.48, 0.21, 0.09],
    keypoints,
    intrinsics,
    domain: "ground",
    priorWgs84: originWgs84,
    priorCenterEnu: [3.2, -16.5, 2.0],
  });

  assert(pose.success, "VPS must localize camera successfully");
  assert(pose.confidence === "high", `expected high confidence, got ${pose.confidence}`);
  assert(pose.inlierCount === anchorCoords.length, "all 8 anchors must be RANSAC inliers");
  const posErrM = Math.hypot(
    pose.positionEnu[0] - trueCenterEnu[0],
    pose.positionEnu[1] - trueCenterEnu[1],
    pose.positionEnu[2] - trueCenterEnu[2],
  );
  assert(posErrM < 0.25, `6-DoF ENU camera position error too large: ${posErrM} m`);
  assert(
    Math.abs(pose.positionWgs84[0] - originWgs84[0]) < 0.01 &&
      Math.abs(pose.positionWgs84[1] - originWgs84[1]) < 0.01,
    "WGS84 registration must map back to Wellington AOI",
  );
}

// 4. Robustness: the test above uses perfect correspondences, which proves the algebra but not the
// estimator. This one adds image-space noise and 30% mismatched anchors, which is the case a real
// descriptor extractor produces, and still requires the right pose.
{
  const originWgs84: [number, number, number] = [174.7762, -41.2865, 12];
  const intrinsics: CameraIntrinsics = { width: 1280, height: 720, fx: 920, fy: 920, cx: 640, cy: 360 };
  const trueRot = [1, 0, 0, 0, 0, -1, 0, 1, 0];
  const trueCenterEnu: [number, number, number] = [4.0, -18.0, 1.8];

  // Deterministic LCG so the fixture is reproducible.
  let lcg = 12345;
  const rand = () => {
    lcg = (Math.imul(lcg, 1664525) + 1013904223) >>> 0;
    return lcg / 4294967296;
  };

  const anchors: SplatAnchor3D[] = [];
  const keypoints: QueryKeypoint2D[] = [];
  for (let i = 0; i < 24; i++) {
    const descriptor = new Float32Array(16);
    descriptor[i % 16] = 1;
    descriptor[(i * 5 + 2) % 16] = 0.4;
    const pt: [number, number, number] = [
      -8 + rand() * 24,
      4 + rand() * 26,
      0.5 + rand() * 8,
    ];
    anchors.push({
      anchorId: `noisy_${i}`,
      positionEnu: pt,
      descriptor,
      saliency: 0.8,
      domain: "ground",
    });

    if (i < 16) {
      // True correspondence with +/-1.5 px of keypoint noise.
      const proj = projectEnuPointToPixel(pt, trueRot, trueCenterEnu, intrinsics);
      if (!proj) continue;
      keypoints.push({
        u: proj.u + (rand() - 0.5) * 3,
        v: proj.v + (rand() - 0.5) * 3,
        descriptor: Array.from(descriptor),
      });
    } else {
      // Mismatch: descriptor of this anchor, pixels from somewhere else entirely.
      keypoints.push({
        u: rand() * intrinsics.width,
        v: rand() * intrinsics.height,
        descriptor: Array.from(descriptor),
      });
    }
  }

  const submap: VpsSubmap = {
    submapId: "noisy_tile",
    originWgs84,
    globalDescriptor: [0.7, 0.4, 0.3, 0.2],
    anchors,
  };
  const pose = localizeCameraVps([submap], {
    globalDescriptor: [0.71, 0.41, 0.29, 0.19],
    keypoints,
    intrinsics,
    domain: "ground",
  });
  assert(pose.success, "VPS must survive 1.5 px noise and 33% mismatched anchors");
  assert(pose.inlierCount >= 8, `expected >=8 inliers, got ${pose.inlierCount}`);
  const errM = Math.hypot(
    pose.positionEnu[0] - trueCenterEnu[0],
    pose.positionEnu[1] - trueCenterEnu[1],
    pose.positionEnu[2] - trueCenterEnu[2],
  );
  assert(errM < 2.0, `position error under noise too large: ${errM.toFixed(3)} m`);
  assert(pose.hdopM > 0 && pose.vdopM > 0, "uncertainty must be positive");
  assert(
    pose.medianInlierDepthM > 0 && Number.isFinite(pose.medianInlierDepthM),
    "median inlier depth must be reported",
  );
  console.log(
    `noisy VPS: inliers ${pose.inlierCount}/${pose.totalMatches}, error ${errM.toFixed(3)} m, ` +
      `median reproj ${pose.medianReprojectionErrorPx.toFixed(2)} px, sigma ${pose.hdopM}/${pose.vdopM} m`,
  );
}

console.log("pointSplatFloodVps.smoke: ok");
