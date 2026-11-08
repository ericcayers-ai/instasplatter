// WebGL2 Gaussian Splat renderer.
//
// Splat data lives in an RGBA32UI texture (2 texels per splat); a per-instance
// uint index attribute (re-uploaded after each worker sort) selects splats in
// back-to-front order for correct alpha blending.
//
// A model matrix rotates the splat set independently of the camera, so the
// scene can be brought upright without moving the viewpoint. Camera frustums
// from the incremental solver are drawn in the same world space, through the
// same model matrix, as a second line pass.

import {
  axisAngle,
  clampDistance,
  defaultCamera,
  identity3,
  mat3Mul,
  mat4Mul,
  orbitBasis,
  orbitBy,
  orbitFromEyeTarget,
  panBy,
  projectionMatrix,
  rotationBetween,
  sampleFlythrough,
  trsModelMatrix,
  viewMatrix,
  zoomAtCursor,
  type CameraState,
  type Vec3,
} from "./camera";
import { PostChain } from "./post";
import { frustumPlanes, visibleChunkIndices } from "./frustum";
import { eyeFromViewMatrix, rotationOnlyView } from "../geospatial/precision";

import type { SplatChunkMeta } from "./worker";

type SplatWorkerMessage =
  | {
      type: "parsed";
      texdata: Uint32Array;
      sceneCenter: Vec3;
      sceneRadius: number;
      count: number;
      canInterpolate: boolean;
      chunks?: SplatChunkMeta[];
    }
  | { type: "lerped"; texdata: Uint32Array; count: number; chunks?: SplatChunkMeta[] }
  | {
      type: "sorted";
      depthIndex: Uint32Array;
      count: number;
      candidateCount?: number;
      stride?: number;
    }
  | { type: "error"; message: string };

/** Per-frame splat pipeline counters (hardware-independent). */
export interface SplatFrameStats {
  /** Splats in the loaded cloud. */
  totalSplats: number;
  /** Spatial chunks in the cloud (0 when the set is too small to partition). */
  chunkCount: number;
  /** Chunks surviving frustum culling this frame. */
  visibleChunks: number;
  /** Splats considered for sorting this frame (visible chunks before the budget stride). */
  candidateSplats: number;
  /** Splats actually drawn this frame. */
  drawnSplats: number;
  /** Uniform stride applied to stay inside the point budget (1 = none). */
  budgetStride: number;
  /** Cumulative count of sort requests posted to the worker (diagnostic). */
  sortRequests: number;
}

const VS = `#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;

uniform usampler2D u_texture;
uniform mat4 u_proj;
uniform mat4 u_view;
// Rotation-only view (u_view with its translation column zeroed) plus the camera position.
// Splat centres are made camera-relative before the view transform so float32 precision is spent
// on the distance to the camera instead of the distance to the ENU origin: 0.03 mm at 1 km,
// 0.98 mm at 20 km absolute, versus <0.01 mm for anything on screen. See geospatial/precision.ts.
uniform mat4 u_view_rot;
uniform vec3 u_eye;
uniform mat4 u_model;
uniform vec2 u_focal;
uniform vec2 u_viewport;

in vec2 a_corner;
in uint a_index;

out vec4 v_color;
out vec2 v_pos;

void main() {
  uint idx = a_index;
  ivec2 t0 = ivec2((idx & 511u) << 1, idx >> 9);
  uvec4 cen = texelFetch(u_texture, t0, 0);
  vec3 center = (u_model * vec4(uintBitsToFloat(cen.xyz), 1.0)).xyz;

  // Camera-relative: subtract the eye in the vertex stage, then rotate. The two forms are
  // mathematically identical; this one keeps the subtracted quantity small.
  vec4 cam = u_view_rot * vec4(center - u_eye, 1.0);
  // The COLMAP view convention uses positive camera-space z for points in
  // front of the camera. Reject invalid depth before the 1/z covariance
  // projection below so behind-camera splats cannot produce infinities.
  if (any(isnan(cam.xyz)) || any(isinf(cam.xyz)) || !(cam.z > 1e-4)) {
    gl_Position = vec4(0.0, 0.0, 2.0, 1.0);
    return;
  }
  vec4 pos2d = u_proj * cam;
  float clip = 1.2 * pos2d.w;
  if (pos2d.z < -clip || pos2d.x < -clip || pos2d.x > clip || pos2d.y < -clip || pos2d.y > clip) {
    gl_Position = vec4(0.0, 0.0, 2.0, 1.0);
    return;
  }

  uvec4 covu = texelFetch(u_texture, ivec2(t0.x | 1, t0.y), 0);
  vec2 c01 = unpackHalf2x16(covu.x);
  vec2 c23 = unpackHalf2x16(covu.y);
  vec2 c45 = unpackHalf2x16(covu.z);
  mat3 Vrk = mat3(
    c01.x, c01.y, c23.x,
    c01.y, c23.y, c45.x,
    c23.x, c45.x, c45.y
  );

  // Rotating the splat set rotates each Gaussian's covariance: R * S * R^T.
  mat3 Rm = mat3(u_model);
  Vrk = Rm * Vrk * transpose(Rm);

  float invz = 1.0 / cam.z;
  mat3 J = mat3(
    u_focal.x * invz, 0.0, -u_focal.x * cam.x * invz * invz,
    0.0, u_focal.y * invz, -u_focal.y * cam.y * invz * invz,
    0.0, 0.0, 0.0
  );
  mat3 W = transpose(mat3(u_view_rot));
  mat3 T = W * J;
  mat3 cov2d = transpose(T) * Vrk * T;

  float covXX = cov2d[0][0];
  float covXY = cov2d[0][1];
  float covYY = cov2d[1][1];
  if (isnan(covXX) || isinf(covXX) || isnan(covXY) || isinf(covXY) ||
      isnan(covYY) || isinf(covYY)) {
    gl_Position = vec4(0.0, 0.0, 2.0, 1.0);
    return;
  }
  // Larger ellipses are clipped to 1024px below, so cap the covariance before
  // subtraction and length() to keep finite inputs from overflowing there.
  covXX = clamp(covXX, 0.0, 1e12);
  covXY = clamp(covXY, -1e12, 1e12);
  covYY = clamp(covYY, 0.0, 1e12);

  // 2D mip filter: a low-pass floor of ~0.3px, as in Mip-Splatting.
  covXX += 0.3;
  covYY += 0.3;

  float mid = (covXX + covYY) * 0.5;
  float rad = length(vec2((covXX - covYY) * 0.5, covXY));
  // Half-float covariance packing and projection rounding can make the
  // smaller eigenvalue slightly negative. Keep the existing 0.3px low-pass
  // floor for both axes instead of dropping the whole Gaussian.
  float l1 = max(mid + rad, 0.3);
  float l2 = max(mid - rad, 0.3);
  vec2 eigenvector = vec2(covXY, l1 - covXX);
  float eigenvectorLength2 = dot(eigenvector, eigenvector);
  vec2 dir = eigenvectorLength2 > 1e-12
    ? eigenvector * inversesqrt(eigenvectorLength2)
    : (covXX >= covYY ? vec2(1.0, 0.0) : vec2(0.0, 1.0));
  vec2 major = min(sqrt(2.0 * l1), 1024.0) * dir;
  vec2 minor = min(sqrt(2.0 * l2), 1024.0) * vec2(dir.y, -dir.x);

  uint rgba = cen.w;
  v_color = vec4(
    float(rgba & 0xffu), float((rgba >> 8) & 0xffu),
    float((rgba >> 16) & 0xffu), float((rgba >> 24) & 0xffu)) / 255.0;
  v_pos = a_corner;

  vec2 corner = a_corner * 2.0;
  // Write real NDC depth so shared-scene compositing can occlude underwater
  // Gaussians against terrain/water. Billboards still expand in screen space.
  float ndcZ = pos2d.z / max(pos2d.w, 1e-6);
  gl_Position = vec4(
    pos2d.xy / pos2d.w + (corner.x * major + corner.y * minor) / u_viewport,
    clamp(ndcZ, -1.0, 1.0), 1.0);
}
`;

const FS = `#version 300 es
precision highp float;
in vec4 v_color;
in vec2 v_pos;
out vec4 fragColor;
void main() {
  float A = -dot(v_pos, v_pos) * 8.0;
  if (A < -4.0) discard;
  float alpha = exp(A) * v_color.a;
  fragColor = vec4(v_color.rgb * alpha, alpha);
}
`;

// Overlay pass: world-space line segments (camera frustums / path / mesh).
const LINE_VS = `#version 300 es
precision highp float;
uniform mat4 u_proj;
uniform mat4 u_view;
uniform mat4 u_model;
in vec3 a_pos;
in vec4 a_color;
out vec4 v_color;
void main() {
  v_color = a_color;
  gl_Position = u_proj * u_view * u_model * vec4(a_pos, 1.0);
}
`;

const LINE_FS = `#version 300 es
precision highp float;
in vec4 v_color;
out vec4 fragColor;
void main() {
  fragColor = vec4(v_color.rgb * v_color.a, v_color.a);
}
`;

// Stage point clouds (sparse / dense XYZRGB).
const POINT_VS = `#version 300 es
precision highp float;
uniform mat4 u_proj;
uniform mat4 u_view;
uniform mat4 u_model;
uniform float u_pointSize;
in vec3 a_pos;
in vec4 a_color;
out vec4 v_color;
void main() {
  v_color = a_color;
  vec4 clip = u_proj * u_view * u_model * vec4(a_pos, 1.0);
  gl_Position = clip;
  float w = max(abs(clip.w), 1e-4);
  gl_PointSize = clamp(u_pointSize * (240.0 / w), 1.0, 8.0);
}
`;

const POINT_FS = `#version 300 es
precision highp float;
in vec4 v_color;
out vec4 fragColor;
void main() {
  vec2 p = gl_PointCoord * 2.0 - 1.0;
  float d = dot(p, p);
  if (d > 1.0) discard;
  float a = v_color.a * (1.0 - smoothstep(0.55, 1.0, d));
  fragColor = vec4(v_color.rgb * a, a);
}
`;

export type PointLayerId = "sparse" | "dense";

/** Options for embedding the splat pass into another WebGL2 scene. */
export interface SplatRendererOptions {
  /** Reuse an existing WebGL2 context (must own the same canvas). */
  gl?: WebGL2RenderingContext;
  /** When false, the host scene drives frames via `renderPass`. Default true. */
  manageLoop?: boolean;
  /** When false, skip clear/resize (host owns the framebuffer). Default true. */
  clearOnFrame?: boolean;
  /** Depth-test against the existing depth buffer (shared geo scene). Default false. */
  depthTest?: boolean;
}

/** Shared projection/view from a host ENU scene. */
export interface SharedFrameMatrices {
  proj: Float32Array;
  view: Float32Array;
  w: number;
  h: number;
}

/** A solved camera pose, in world space, ready to draw. */
export interface CameraFrustum {
  apex: Vec3;
  /** Far-plane corners, in order: top-left, top-right, bottom-right, bottom-left. */
  corners: [Vec3, Vec3, Vec3, Vec3];
}

/** Fade-in duration for a newly registered camera, in milliseconds. */
const FRUSTUM_FADE_MS = 450;

export class SplatRenderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram;
  private lineProgram: WebGLProgram;
  private pointProgram: WebGLProgram;
  private texture: WebGLTexture;
  private indexBuffer: WebGLBuffer;
  private vao: WebGLVertexArrayObject;
  private lineVao: WebGLVertexArrayObject;
  private linePosBuffer: WebGLBuffer;
  private lineColorBuffer: WebGLBuffer;
  private lineVertexCount = 0;
  private pathVao: WebGLVertexArrayObject;
  private pathPosBuffer: WebGLBuffer;
  private pathColorBuffer: WebGLBuffer;
  private pathVertexCount = 0;
  private meshVao: WebGLVertexArrayObject;
  private meshPosBuffer: WebGLBuffer;
  private meshColorBuffer: WebGLBuffer;
  private meshVertexCount = 0;
  private sparseVao: WebGLVertexArrayObject;
  private sparsePosBuffer: WebGLBuffer;
  private sparseColorBuffer: WebGLBuffer;
  private sparseCount = 0;
  private denseVao: WebGLVertexArrayObject;
  private densePosBuffer: WebGLBuffer;
  private denseColorBuffer: WebGLBuffer;
  private denseCount = 0;
  private worker: Worker;
  private count = 0;
  private sortedCount = 0;
  /** Spatial chunks reported by the worker, in model space. */
  private chunks: SplatChunkMeta[] = [];
  private chunkCenters: Float32Array = new Float32Array(0);
  private chunkRadii: Float32Array = new Float32Array(0);
  private visibleChunks: number[] | null = null;
  private lastVisibleChunks: number[] | null = null;
  private sortRequests = 0;
  private lastFrameStats: SplatFrameStats = {
    totalSplats: 0,
    chunkCount: 0,
    visibleChunks: 0,
    candidateSplats: 0,
    drawnSplats: 0,
    budgetStride: 1,
    sortRequests: 0,
  };
  /** Frustum culling on the spatial chunks. Large geospatial sets need this. */
  public cullingEnabled = true;
  /** Upper bound on splats submitted for sorting/drawing; excess is stride-sampled. */
  public pointBudget = 4_000_000;
  public onSplatStats: ((stats: SplatFrameStats) => void) | null = null;
  private lastSortVP: Float32Array | null = null;
  private sortPending = false;
  private disposed = false;
  private manageLoop: boolean;
  private clearOnFrame: boolean;
  private depthTest: boolean;
  private uProj: WebGLUniformLocation;
  private uView: WebGLUniformLocation;
  private uViewRot: WebGLUniformLocation;
  private uEye: WebGLUniformLocation;
  private uModel: WebGLUniformLocation;
  private uFocal: WebGLUniformLocation;
  private uViewport: WebGLUniformLocation;
  private uTexture: WebGLUniformLocation;
  private uLineProj: WebGLUniformLocation;
  private uLineView: WebGLUniformLocation;
  private uLineModel: WebGLUniformLocation;
  private uPointProj: WebGLUniformLocation;
  private uPointView: WebGLUniformLocation;
  private uPointModel: WebGLUniformLocation;
  private uPointSize: WebGLUniformLocation;

  private pendingAutoFrame = false;
  /** Row-major 3x3 model rotation, applied about `pivot`. */
  private modelRot: number[][] = identity3();
  private pivot: Vec3 = [0, 0, 0];
  /** Optional ENU / geo offset after rotation (metres). */
  private modelTranslation: Vec3 = [0, 0, 0];
  /** Optional non-uniform scale about `pivot`. */
  private modelScale: Vec3 = [1, 1, 1];
  private frustums: { f: CameraFrustum; addedAt: number }[] = [];
  private frustumsDirty = false;
  private lastFrameTime = 0;
  private fpsAccum = 0;
  private fpsFrames = 0;
  /** Live checkpoint interpolation: ease toward the newest PLY. */
  private lerpActive = false;
  private lerpStart = 0;
  private lerpDurationMs = 2200;
  private lerpPending = false;
  /** Smoothed world-space samples for path flythrough (auto-orbit). */
  private flyPath: Vec3[] = [];
  private flyT = 0;

  public camera: CameraState = defaultCamera();
  public autoOrbit = false;
  /** When auto-orbiting with a camera path, follow it instead of yaw-spin. */
  public pathFollow = true;
  public showFrustums = true;
  public showCameraPath = true;
  public showSparse = true;
  public showDense = true;
  public showSplat = true;
  public showMesh = true;
  public onStats: ((splats: number) => void) | null = null;
  public onFps: ((fps: number) => void) | null = null;
  /**
   * Post-process toggles (Item 4). All default off — the legacy direct path
   * is untouched until one of these flips.
   */
  public postToggles = { tonemap: false, fxaa: false, glare: false };
  /** Unit world-space direction toward the sun for glare positioning. */
  public sunDirectionWorld: [number, number, number] | null = null;
  private post: PostChain | null = null;

  constructor(
    private canvas: HTMLCanvasElement,
    opts: SplatRendererOptions = {},
  ) {
    this.manageLoop = opts.manageLoop ?? true;
    this.clearOnFrame = opts.clearOnFrame ?? true;
    this.depthTest = opts.depthTest ?? false;

    const gl =
      opts.gl ??
      canvas.getContext("webgl2", {
        antialias: false,
        alpha: true,
        premultipliedAlpha: true,
        depth: true,
      });
    if (!gl) throw new Error("WebGL2 is not available on this system.");
    this.gl = gl;

    const compile = (type: number, src: string) => {
      const sh = gl.createShader(type)!;
      gl.shaderSource(sh, src);
      gl.compileShader(sh);
      if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS))
        throw new Error(gl.getShaderInfoLog(sh) ?? "shader error");
      return sh;
    };
    const link = (vs: string, fs: string) => {
      const prog = gl.createProgram()!;
      gl.attachShader(prog, compile(gl.VERTEX_SHADER, vs));
      gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, fs));
      gl.linkProgram(prog);
      if (!gl.getProgramParameter(prog, gl.LINK_STATUS))
        throw new Error(gl.getProgramInfoLog(prog) ?? "link error");
      return prog;
    };

    this.program = link(VS, FS);
    this.lineProgram = link(LINE_VS, LINE_FS);
    this.pointProgram = link(POINT_VS, POINT_FS);

    this.uProj = gl.getUniformLocation(this.program, "u_proj")!;
    this.uView = gl.getUniformLocation(this.program, "u_view")!;
    this.uViewRot = gl.getUniformLocation(this.program, "u_view_rot")!;
    this.uEye = gl.getUniformLocation(this.program, "u_eye")!;
    this.uModel = gl.getUniformLocation(this.program, "u_model")!;
    this.uFocal = gl.getUniformLocation(this.program, "u_focal")!;
    this.uViewport = gl.getUniformLocation(this.program, "u_viewport")!;
    this.uTexture = gl.getUniformLocation(this.program, "u_texture")!;
    this.uLineProj = gl.getUniformLocation(this.lineProgram, "u_proj")!;
    this.uLineView = gl.getUniformLocation(this.lineProgram, "u_view")!;
    this.uLineModel = gl.getUniformLocation(this.lineProgram, "u_model")!;
    this.uPointProj = gl.getUniformLocation(this.pointProgram, "u_proj")!;
    this.uPointView = gl.getUniformLocation(this.pointProgram, "u_view")!;
    this.uPointModel = gl.getUniformLocation(this.pointProgram, "u_model")!;
    this.uPointSize = gl.getUniformLocation(this.pointProgram, "u_pointSize")!;

    // Splat VAO.
    this.vao = gl.createVertexArray()!;
    gl.bindVertexArray(this.vao);
    const quad = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const aCorner = gl.getAttribLocation(this.program, "a_corner");
    gl.enableVertexAttribArray(aCorner);
    gl.vertexAttribPointer(aCorner, 2, gl.FLOAT, false, 0, 0);

    this.indexBuffer = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.indexBuffer);
    const aIndex = gl.getAttribLocation(this.program, "a_index");
    gl.enableVertexAttribArray(aIndex);
    gl.vertexAttribIPointer(aIndex, 1, gl.UNSIGNED_INT, 0, 0);
    gl.vertexAttribDivisor(aIndex, 1);

    // Line VAO (frustums).
    this.lineVao = gl.createVertexArray()!;
    gl.bindVertexArray(this.lineVao);
    this.linePosBuffer = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.linePosBuffer);
    const aPos = gl.getAttribLocation(this.lineProgram, "a_pos");
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 3, gl.FLOAT, false, 0, 0);
    this.lineColorBuffer = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.lineColorBuffer);
    const aColor = gl.getAttribLocation(this.lineProgram, "a_color");
    gl.enableVertexAttribArray(aColor);
    gl.vertexAttribPointer(aColor, 4, gl.FLOAT, false, 0, 0);

    const makeLineLayer = () => {
      const vao = gl.createVertexArray()!;
      gl.bindVertexArray(vao);
      const pos = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, pos);
      gl.enableVertexAttribArray(aPos);
      gl.vertexAttribPointer(aPos, 3, gl.FLOAT, false, 0, 0);
      const col = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, col);
      gl.enableVertexAttribArray(aColor);
      gl.vertexAttribPointer(aColor, 4, gl.FLOAT, false, 0, 0);
      return { vao, pos, col };
    };
    const path = makeLineLayer();
    this.pathVao = path.vao;
    this.pathPosBuffer = path.pos;
    this.pathColorBuffer = path.col;
    const mesh = makeLineLayer();
    this.meshVao = mesh.vao;
    this.meshPosBuffer = mesh.pos;
    this.meshColorBuffer = mesh.col;

    const makePointLayer = () => {
      const vao = gl.createVertexArray()!;
      gl.bindVertexArray(vao);
      const pos = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, pos);
      const pPos = gl.getAttribLocation(this.pointProgram, "a_pos");
      gl.enableVertexAttribArray(pPos);
      gl.vertexAttribPointer(pPos, 3, gl.FLOAT, false, 0, 0);
      const col = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, col);
      const pCol = gl.getAttribLocation(this.pointProgram, "a_color");
      gl.enableVertexAttribArray(pCol);
      gl.vertexAttribPointer(pCol, 4, gl.FLOAT, false, 0, 0);
      return { vao, pos, col };
    };
    const sparse = makePointLayer();
    this.sparseVao = sparse.vao;
    this.sparsePosBuffer = sparse.pos;
    this.sparseColorBuffer = sparse.col;
    const dense = makePointLayer();
    this.denseVao = dense.vao;
    this.densePosBuffer = dense.pos;
    this.denseColorBuffer = dense.col;
    gl.bindVertexArray(null);

    this.texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);

    if (this.depthTest) {
      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LEQUAL);
    } else {
      gl.disable(gl.DEPTH_TEST);
    }
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    this.worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (e) => this.onWorker(e.data);

    if (this.manageLoop) {
      const loop = (now: number) => {
        if (this.disposed) return;
        this.frame(now);
        requestAnimationFrame(loop);
      };
      requestAnimationFrame(loop);
    }
  }

  /** Load (or hot-swap to) a splat .ply from raw bytes.
   *  When `interpolate` is true and the splat count matches the previous
   *  load, attributes ease across `durationMs` instead of snapping. */
  loadPly(buffer: ArrayBuffer, autoFrame: boolean, interpolate = true, durationMs = 2200) {
    this.pendingAutoFrame = autoFrame;
    this.lerpDurationMs = durationMs;
    this.worker.postMessage({ type: "parse", buffer, interpolate: interpolate && !autoFrame }, [
      buffer,
    ]);
  }

  // ---- Model orientation (ROADMAP-V2 1.3) ----

  /** Current model rotation, row-major 3x3. */
  get modelRotation(): number[][] {
    return this.modelRot.map((r) => [...r]);
  }

  /** Scene centre, which is the pivot every model rotation turns about. */
  get modelPivot(): Vec3 {
    return [...this.pivot] as Vec3;
  }

  setModelRotation(rot: number[][]) {
    this.modelRot = rot.map((r) => [...r]);
    this.lastSortVP = null;
  }

  /** Extra world-space translation after rotation (geo ENU gizmos). */
  setModelTranslation(t: Vec3) {
    this.modelTranslation = [...t] as Vec3;
    this.lastSortVP = null;
  }

  setModelScale(s: Vec3) {
    this.modelScale = [
      Math.max(1e-4, s[0]),
      Math.max(1e-4, s[1]),
      Math.max(1e-4, s[2]),
    ];
    this.lastSortVP = null;
  }

  resetModelRotation() {
    this.setModelRotation(identity3());
    this.setModelTranslation([0, 0, 0]);
    this.setModelScale([1, 1, 1]);
  }

  /** Turn the model about a world axis, on top of the current rotation. */
  rotateModel(axis: Vec3, angleRadians: number) {
    this.setModelRotation(mat3Mul(axisAngle(axis, angleRadians), this.modelRot));
  }

  /**
   * Rotate so `up` (a direction in the *unrotated* splat frame) becomes the
   * screen-up direction. Used by both axis snapping and ground-plane
   * alignment; they differ only in where `up` comes from.
   */
  alignUp(up: Vec3) {
    this.setModelRotation(rotationBetween(up, this.camera.worldUp));
  }

  /** Snap the current up direction to the nearest signed world axis. */
  snapUpToNearestAxis() {
    // Where the model currently sends screen-up, expressed before rotation.
    const wu = this.camera.worldUp;
    const r = this.modelRot;
    // current = R^T * worldUp
    const current: Vec3 = [
      r[0][0] * wu[0] + r[1][0] * wu[1] + r[2][0] * wu[2],
      r[0][1] * wu[0] + r[1][1] * wu[1] + r[2][1] * wu[2],
      r[0][2] * wu[0] + r[1][2] * wu[1] + r[2][2] * wu[2],
    ];
    let best: Vec3 = [0, 1, 0];
    let bestDot = -Infinity;
    for (const a of [
      [1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1],
    ] as Vec3[]) {
      const d = a[0] * current[0] + a[1] * current[1] + a[2] * current[2];
      if (d > bestDot) {
        bestDot = d;
        best = a;
      }
    }
    this.alignUp(best);
  }

  // ---- Camera frustums (ROADMAP-V2 2.4) ----

  addFrustum(f: CameraFrustum) {
    this.frustums.push({ f, addedAt: performance.now() });
    this.frustumsDirty = true;
  }

  clearFrustums() {
    this.frustums = [];
    this.frustumsDirty = true;
    this.lineVertexCount = 0;
  }

  get frustumCount(): number {
    return this.frustums.length;
  }

  /** Ingest / camera-path polyline in world space (already smoothed by caller). */
  setCameraPath(points: Vec3[]) {
    const gl = this.gl;
    this.flyPath = points.length >= 2 ? points.slice() : [];
    this.flyT = 0;
    if (points.length < 2) {
      this.pathVertexCount = 0;
      return;
    }
    const nSeg = points.length - 1;
    const verts = new Float32Array(nSeg * 2 * 3);
    const colors = new Float32Array(nSeg * 2 * 4);
    for (let i = 0; i < nSeg; i++) {
      const a = points[i];
      const b = points[i + 1];
      const t = i / Math.max(1, nSeg - 1);
      const rgba: [number, number, number, number] = [0.55 + 0.35 * t, 0.72, 0.95, 0.75];
      verts.set(a, i * 6);
      verts.set(b, i * 6 + 3);
      for (let k = 0; k < 2; k++) {
        colors.set(rgba, i * 8 + k * 4);
      }
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, this.pathPosBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, verts, gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.pathColorBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, colors, gl.DYNAMIC_DRAW);
    this.pathVertexCount = nSeg * 2;
  }

  clearCameraPath() {
    this.pathVertexCount = 0;
    this.flyPath = [];
    this.flyT = 0;
  }

  setPointCloud(layer: PointLayerId, positions: Float32Array, colors: Float32Array, count: number) {
    const gl = this.gl;
    const vao = layer === "sparse" ? this.sparseVao : this.denseVao;
    const posBuf = layer === "sparse" ? this.sparsePosBuffer : this.densePosBuffer;
    const colBuf = layer === "sparse" ? this.sparseColorBuffer : this.denseColorBuffer;
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, posBuf);
    gl.bufferData(gl.ARRAY_BUFFER, positions, gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, colBuf);
    gl.bufferData(gl.ARRAY_BUFFER, colors, gl.DYNAMIC_DRAW);
    gl.bindVertexArray(null);
    if (layer === "sparse") this.sparseCount = count;
    else this.denseCount = count;

    // Frame empty scenes onto the first point cloud that arrives.
    if (this.count === 0 && count > 0 && this.sceneRadius <= 0) {
      this.fitPoints(positions, count);
    }
  }

  clearPointCloud(layer: PointLayerId) {
    if (layer === "sparse") this.sparseCount = 0;
    else this.denseCount = 0;
  }

  /** Wireframe edges: flat xyzxyz… line list. */
  setMeshWire(edgePositions: Float32Array) {
    const gl = this.gl;
    const n = Math.floor(edgePositions.length / 3);
    const colors = new Float32Array(n * 4);
    for (let i = 0; i < n; i++) {
      colors[i * 4] = 0.92;
      colors[i * 4 + 1] = 0.78;
      colors[i * 4 + 2] = 0.35;
      colors[i * 4 + 3] = 0.55;
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, this.meshPosBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, edgePositions, gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.meshColorBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, colors, gl.DYNAMIC_DRAW);
    this.meshVertexCount = n;
  }

  clearMeshWire() {
    this.meshVertexCount = 0;
  }

  clearStageLayers() {
    this.clearFrustums();
    this.clearCameraPath();
    this.clearPointCloud("sparse");
    this.clearPointCloud("dense");
    this.clearMeshWire();
  }

  private fitPoints(positions: Float32Array, count: number) {
    let cx = 0;
    let cy = 0;
    let cz = 0;
    for (let i = 0; i < count; i++) {
      cx += positions[i * 3];
      cy += positions[i * 3 + 1];
      cz += positions[i * 3 + 2];
    }
    cx /= count;
    cy /= count;
    cz /= count;
    let r2 = 0;
    for (let i = 0; i < count; i++) {
      const dx = positions[i * 3] - cx;
      const dy = positions[i * 3 + 1] - cy;
      const dz = positions[i * 3 + 2] - cz;
      r2 = Math.max(r2, dx * dx + dy * dy + dz * dz);
    }
    this.sceneCenter = [cx, cy, cz];
    this.sceneRadius = Math.sqrt(r2) || 1;
    this.pivot = [cx, cy, cz];
    this.frameScene();
  }

  /** Frame the whole scene, keeping the current orbit angles. */
  frameScene() {
    if (this.sceneRadius > 0) {
      this.camera.target = [...this.sceneCenter] as Vec3;
      this.camera.distance = clampDistance(this.sceneRadius * 2.2);
    }
  }

  private sceneCenter: Vec3 = [0, 0, 0];
  private sceneRadius = 0;

  private uploadTex(texdata: Uint32Array, count: number) {
    const gl = this.gl;
    this.count = count;
    const rows = Math.max(1, Math.ceil(this.count / 512));
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA32UI,
      1024,
      rows,
      0,
      gl.RGBA_INTEGER,
      gl.UNSIGNED_INT,
      this.padTo(texdata, 1024 * rows * 4),
    );
    this.lastSortVP = null;
    this.onStats?.(this.count);
  }

  private onWorker(msg: SplatWorkerMessage) {
    if (msg.type === "parsed") {
      const texdata: Uint32Array = msg.texdata;
      this.sceneCenter = msg.sceneCenter;
      this.sceneRadius = msg.sceneRadius;
      this.pivot = msg.sceneCenter;
      this.uploadTex(texdata, msg.count);
      if (msg.chunks) this.setChunks(msg.chunks);

      if (this.pendingAutoFrame && msg.sceneRadius > 0) {
        this.frameScene();
      }
      if (msg.canInterpolate) {
        this.lerpActive = true;
        this.lerpStart = performance.now();
        this.lerpPending = false;
      } else {
        this.lerpActive = false;
      }
    } else if (msg.type === "lerped") {
      this.lerpPending = false;
      this.uploadTex(msg.texdata, msg.count);
      if (msg.chunks) this.setChunks(msg.chunks);
    } else if (msg.type === "sorted") {
      const idx: Uint32Array = msg.depthIndex;
      this.sortedCount = Math.min(msg.count, this.count);
      this.gl.bindBuffer(this.gl.ARRAY_BUFFER, this.indexBuffer);
      this.gl.bufferData(this.gl.ARRAY_BUFFER, idx, this.gl.DYNAMIC_DRAW);
      this.sortPending = false;
      this.lastFrameStats = {
        totalSplats: this.count,
        chunkCount: this.chunks.length,
        visibleChunks: this.visibleChunks ? this.visibleChunks.length : this.chunks.length,
        candidateSplats: msg.candidateCount ?? msg.count,
        drawnSplats: this.sortedCount,
        budgetStride: msg.stride ?? 1,
        sortRequests: this.sortRequests,
      };
      this.onSplatStats?.(this.lastFrameStats);
    } else if (msg.type === "error") {
      console.error("splat worker:", msg.message);
      this.sortPending = false;
      this.lerpPending = false;
    }
  }

  private padTo(data: Uint32Array, len: number): Uint32Array {
    if (data.length === len) return data;
    const out = new Uint32Array(len);
    out.set(data.subarray(0, Math.min(data.length, len)));
    return out;
  }

  /** Rebuild the frustum line buffers, fading in recently added cameras. */
  private rebuildFrustumBuffers(now: number) {
    const gl = this.gl;
    const n = this.frustums.length;
    // 8 segments per frustum: 4 apex-to-corner, 4 around the far rectangle.
    const verts = new Float32Array(n * 8 * 2 * 3);
    const colors = new Float32Array(n * 8 * 2 * 4);
    let vi = 0;
    let ci = 0;

    const pushVertex = (p: Vec3, rgba: [number, number, number, number]) => {
      verts[vi++] = p[0]; verts[vi++] = p[1]; verts[vi++] = p[2];
      colors[ci++] = rgba[0]; colors[ci++] = rgba[1]; colors[ci++] = rgba[2]; colors[ci++] = rgba[3];
    };

    for (let k = 0; k < n; k++) {
      const { f, addedAt } = this.frustums[k];
      const age = now - addedAt;
      const fade = Math.min(1, age / FRUSTUM_FADE_MS);
      // The newest few cameras stay highlighted; older ones recede.
      const fresh = k >= n - 3;
      const rgb: [number, number, number] = fresh ? [0.22, 0.88, 0.78] : [0.42, 0.47, 0.58];
      const a = (fresh ? 0.95 : 0.35) * fade;
      const col: [number, number, number, number] = [rgb[0], rgb[1], rgb[2], a];

      for (let c = 0; c < 4; c++) {
        pushVertex(f.apex, col);
        pushVertex(f.corners[c], col);
      }
      for (let c = 0; c < 4; c++) {
        pushVertex(f.corners[c], col);
        pushVertex(f.corners[(c + 1) % 4], col);
      }
    }

    gl.bindBuffer(gl.ARRAY_BUFFER, this.linePosBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, verts, gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.lineColorBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, colors, gl.DYNAMIC_DRAW);
    this.lineVertexCount = n * 16;
  }

  /**
   * Host-driven draw for a shared WebGL scene. Uses the caller's projection/view
   * so terrain/water/splat share one depth buffer.
   */
  renderPass(now: number, shared: SharedFrameMatrices) {
    this.frame(now, shared);
  }

  /**
   * Draw one frame on demand. Used by hosts that own the loop (`manageLoop: false`) and by
   * `tools/bench/splat-bench.ts`, which has to time frames without depending on
   * requestAnimationFrame (headless Chromium throttles rAF, which would measure the throttler).
   */
  renderFrame(now: number) {
    this.frame(now);
  }

  private frame(now: number, shared?: SharedFrameMatrices) {
    const gl = this.gl;
    const canvas = this.canvas;
    let w: number;
    let h: number;
    if (shared) {
      w = shared.w;
      h = shared.h;
    } else {
      const dpr = window.devicePixelRatio || 1;
      w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
      h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
      if (this.clearOnFrame && (canvas.width !== w || canvas.height !== h)) {
        canvas.width = w;
        canvas.height = h;
      }
    }

    const prevFrameTime = this.lastFrameTime;
    if (this.lastFrameTime > 0) {
      this.fpsAccum += now - this.lastFrameTime;
      this.fpsFrames++;
      if (this.fpsAccum >= 500) {
        this.onFps?.((this.fpsFrames * 1000) / this.fpsAccum);
        this.fpsAccum = 0;
        this.fpsFrames = 0;
      }
    }
    this.lastFrameTime = now;

    // Ease Gaussian attributes between Brush checkpoint exports so the
    // viewport feels continuous without exporting every few hundred steps.
    if (this.lerpActive && !this.lerpPending) {
      const t = Math.min(1, (now - this.lerpStart) / this.lerpDurationMs);
      // Smoothstep.
      const s = t * t * (3 - 2 * t);
      this.worker.postMessage({ type: "lerp", t: s });
      this.lerpPending = true;
      if (t >= 1) this.lerpActive = false;
    }

    const usePost = this.postToggles.tonemap || this.postToggles.fxaa || this.postToggles.glare;
    if (usePost && !this.post) {
      this.post = new PostChain(gl);
    }
    if (usePost) {
      this.post!.begin(w, h); // binds scene FBO + viewport + clear
    } else if (!shared) {
      gl.viewport(0, 0, w, h);
      if (this.clearOnFrame) {
        gl.clearColor(0, 0, 0, 0);
        gl.clear(gl.COLOR_BUFFER_BIT);
      }
    }

    if (this.autoOrbit) {
      if (this.pathFollow && this.flyPath.length >= 2) {
        const dt = prevFrameTime > 0 ? Math.min(0.05, (now - prevFrameTime) / 1000) : 0.016;
        // ~40s for a full loop on a typical capture path.
        this.flyT = (this.flyT + dt / 40) % 1;
        const { eye, target } = sampleFlythrough(this.flyPath, this.flyT);
        orbitFromEyeTarget(this.camera, eye, target);
      } else {
        this.camera.yaw += 0.002;
      }
    }

    const proj =
      shared?.proj ??
      projectionMatrix(
        w,
        h,
        this.camera.fovY,
        0.02 * this.camera.distance,
        100 * this.camera.distance,
      );
    const view = shared?.view ?? viewMatrix(orbitBasis(this.camera));
    const model = trsModelMatrix(
      this.modelRot,
      this.pivot,
      this.modelTranslation,
      this.modelScale,
    );
    const focal = h / (2 * Math.tan(this.camera.fovY / 2));

    if (this.depthTest) {
      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LEQUAL);
      gl.depthMask(false);
    }

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    if (this.showSplat && this.count > 0 && this.sortedCount > 0) {
      gl.useProgram(this.program);
      gl.bindVertexArray(this.vao);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, this.texture);
      gl.uniform1i(this.uTexture, 0);
      gl.uniformMatrix4fv(this.uProj, false, proj);
      gl.uniformMatrix4fv(this.uView, false, view);
      gl.uniformMatrix4fv(this.uViewRot, false, rotationOnlyView(view));
      const eye = eyeFromViewMatrix(view);
      gl.uniform3f(this.uEye, eye[0], eye[1], eye[2]);
      gl.uniformMatrix4fv(this.uModel, false, model);
      gl.uniform2f(this.uFocal, focal, focal);
      gl.uniform2f(this.uViewport, w, h);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, this.sortedCount);
    }

    // Point clouds under the splat so training can replace densify visually.
    const drawPoints = (vao: WebGLVertexArrayObject, n: number, size: number) => {
      if (n <= 0) return;
      gl.useProgram(this.pointProgram);
      gl.bindVertexArray(vao);
      gl.uniformMatrix4fv(this.uPointProj, false, proj);
      gl.uniformMatrix4fv(this.uPointView, false, view);
      gl.uniformMatrix4fv(this.uPointModel, false, model);
      gl.uniform1f(this.uPointSize, size);
      gl.drawArrays(gl.POINTS, 0, n);
    };
    if (this.showSparse) drawPoints(this.sparseVao, this.sparseCount, 2.2);
    if (this.showDense) drawPoints(this.denseVao, this.denseCount, 1.6);

    const drawLines = (vao: WebGLVertexArrayObject, n: number) => {
      if (n <= 0) return;
      gl.useProgram(this.lineProgram);
      gl.bindVertexArray(vao);
      gl.uniformMatrix4fv(this.uLineProj, false, proj);
      gl.uniformMatrix4fv(this.uLineView, false, view);
      gl.uniformMatrix4fv(this.uLineModel, false, model);
      gl.drawArrays(gl.LINES, 0, n);
    };

    // Frustums fade in, so keep rebuilding while any of them is still young.
    const animating = this.frustums.some((f) => now - f.addedAt < FRUSTUM_FADE_MS);
    if (this.showFrustums && this.frustums.length > 0 && (this.frustumsDirty || animating)) {
      this.rebuildFrustumBuffers(now);
      this.frustumsDirty = false;
    }
    if (this.showFrustums) drawLines(this.lineVao, this.lineVertexCount);
    if (this.showCameraPath) drawLines(this.pathVao, this.pathVertexCount);
    if (this.showMesh) drawLines(this.meshVao, this.meshVertexCount);
    gl.bindVertexArray(null);

    if (this.depthTest) {
      gl.depthMask(true);
    }

    if (this.post && usePost) {
      const sv = this.projectSunToScreen(proj, view);
      this.post.end({
        tonemap: this.postToggles.tonemap,
        glare: this.postToggles.glare,
        fxaa: this.postToggles.fxaa,
        sunUv: sv.uv,
        sunVisible: sv.visible,
      });
    }

    if (this.count === 0) return;

    // Sorting happens against proj * view * model, so a model rotation
    // re-sorts exactly like a camera move.
    const vp = mat4Mul(mat4Mul(proj, view), model);
    const visible = this.cullingEnabled
      ? this.computeVisibleChunks(model, proj, view)
      : null;
    const visibleChanged = !SplatRenderer.listsEqual(visible, this.lastVisibleChunks);
    if (!this.sortPending && (this.needsSort(vp) || visibleChanged)) {
      this.sortPending = true;
      this.sortRequests++;
      this.lastSortVP = vp;
      this.visibleChunks = visible;
      this.lastVisibleChunks = visible ? visible.slice() : null;
      this.worker.postMessage({
        type: "sort",
        viewProj: vp,
        visibleChunks: visible,
        budget: this.pointBudget,
      });
    }
  }

  /** Adopt the worker's spatial partition and cache per-chunk spheres for culling. */
  private setChunks(chunks: SplatChunkMeta[]) {
    this.chunks = chunks;
    const n = chunks.length;
    this.chunkCenters = new Float32Array(n * 3);
    this.chunkRadii = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const ch = chunks[i];
      const cx = 0.5 * (ch.minM[0] + ch.maxM[0]);
      const cy = 0.5 * (ch.minM[1] + ch.maxM[1]);
      const cz = 0.5 * (ch.minM[2] + ch.maxM[2]);
      this.chunkCenters[i * 3] = cx;
      this.chunkCenters[i * 3 + 1] = cy;
      this.chunkCenters[i * 3 + 2] = cz;
      this.chunkRadii[i] = Math.hypot(
        0.5 * (ch.maxM[0] - ch.minM[0]),
        0.5 * (ch.maxM[1] - ch.minM[1]),
        0.5 * (ch.maxM[2] - ch.minM[2]),
      );
    }
    this.visibleChunks = null;
    this.lastVisibleChunks = null;
    this.lastSortVP = null;
  }

  /** Live counters from the last completed frame. */
  get frameStats(): SplatFrameStats {
    return { ...this.lastFrameStats };
  }

  get chunkCount(): number {
    return this.chunks.length;
  }

  /** Diagnostic snapshot of the spatial partition (used by tools/bench and bug reports). */
  debugChunks(limit = 3): {
    count: number;
    radii: number[];
    centres: [number, number, number][];
    splatsPerChunk: number[];
  } {
    const n = Math.min(limit, this.chunks.length);
    const radii: number[] = [];
    const centres: [number, number, number][] = [];
    const splatsPerChunk: number[] = [];
    for (let i = 0; i < n; i++) {
      radii.push(this.chunkRadii[i]);
      centres.push([
        this.chunkCenters[i * 3],
        this.chunkCenters[i * 3 + 1],
        this.chunkCenters[i * 3 + 2],
      ]);
      splatsPerChunk.push(this.chunks[i].count);
    }
    return { count: this.chunks.length, radii, centres, splatsPerChunk };
  }

  /**
   * Frustum culling of the spatial chunks in one pass over their bounding spheres.
   *
   * Planes are extracted from `proj * view * model` once per frame; the test is conservative
   * (a chunk is kept unless it is fully outside a plane), so no visible geometry is dropped.
   * This is the Hierarchical-3DGS / Octree-GS idea at chunk granularity: the cost of a frame
   * follows what is on screen instead of the size of the scene.
   */
  private computeVisibleChunks(
    model: Float32Array,
    proj: Float32Array,
    view: Float32Array,
  ): number[] | null {
    if (this.chunks.length <= 1) return null;
    const mvp = mat4Mul(mat4Mul(proj, view), model);
    const planes = frustumPlanes(mvp);
    const scale = Math.max(this.modelScale[0], this.modelScale[1], this.modelScale[2]);
    return visibleChunkIndices(planes, this.chunkCenters, this.chunkRadii, model, scale);
  }

  private static listsEqual(a: number[] | null, b: number[] | null): boolean {
    if (a === b) return true;
    if (!a || !b || a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (a[i] !== b[i]) return false;
    }
    return true;
  }

  private projectSunToScreen(
    proj: Float32Array,
    view: Float32Array,
  ): { uv: [number, number]; visible: number } {
    const d = this.sunDirectionWorld;
    if (!d) return { uv: [0.5, 0.5], visible: 0 };
    const vp = mat4Mul(proj, view);
    const c = [0, 0, 0, 0];
    for (let i = 0; i < 4; i++) {
      c[i] = vp[i] * d[0] + vp[4 + i] * d[1] + vp[8 + i] * d[2];
    }
    if (!(Math.abs(c[3]) > 1e-8) || c[3] <= 0) return { uv: [0.5, 0.5], visible: 0 };
    const nx = c[0] / c[3];
    const ny = c[1] / c[3];
    if (!Number.isFinite(nx) || !Number.isFinite(ny) || Math.abs(nx) > 3 || Math.abs(ny) > 3) {
      return { uv: [0.5, 0.5], visible: 0 };
    }
    return { uv: [(nx + 1) / 2, (ny + 1) / 2], visible: 1 };
  }

  private needsSort(vp: Float32Array): boolean {
    if (!this.lastSortVP) return true;
    let dot = 0;
    for (const i of [2, 6, 10]) dot += vp[i] * this.lastSortVP[i];
    const a = Math.hypot(vp[2], vp[6], vp[10]);
    const b = Math.hypot(this.lastSortVP[2], this.lastSortVP[6], this.lastSortVP[10]);
    return dot / (a * b || 1) < 0.999;
  }

  attachControls() {
    const el = this.canvas;
    let dragging = false;
    let lastX = 0;
    let lastY = 0;
    let button = 0;

    const cssSize = () => ({
      w: el.clientWidth || 1,
      h: el.clientHeight || 1,
    });

    el.addEventListener("pointerdown", (e) => {
      dragging = true;
      button = e.button;
      lastX = e.clientX;
      lastY = e.clientY;
      el.setPointerCapture(e.pointerId);
      this.autoOrbit = false;
    });

    el.addEventListener("pointermove", (e) => {
      if (!dragging) return;
      const dx = e.clientX - lastX;
      const dy = e.clientY - lastY;
      lastX = e.clientX;
      lastY = e.clientY;
      const { h } = cssSize();
      // Middle button or right button or shift-drag pans; left orbits.
      if (button === 1 || button === 2 || e.shiftKey) {
        panBy(this.camera, dx, dy, h);
      } else {
        orbitBy(this.camera, dx, dy);
      }
    });

    const endDrag = (e: PointerEvent) => {
      dragging = false;
      if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    };
    el.addEventListener("pointerup", endDrag);
    el.addEventListener("pointercancel", endDrag);

    el.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        this.autoOrbit = false;
        const rect = el.getBoundingClientRect();
        const { w, h } = cssSize();
        zoomAtCursor(
          this.camera,
          Math.pow(1.0015, e.deltaY),
          e.clientX - rect.left,
          e.clientY - rect.top,
          w,
          h,
        );
      },
      { passive: false },
    );

    el.addEventListener("contextmenu", (e) => e.preventDefault());
  }

  dispose() {
    this.disposed = true;
    this.worker.terminate();
    this.post?.dispose();
    this.post = null;
  }
}
