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
  modelMatrix,
  orbitBasis,
  orbitBy,
  panBy,
  projectionMatrix,
  rotationBetween,
  viewMatrix,
  zoomAtCursor,
  type CameraState,
  type Vec3,
} from "./camera";

const VS = `#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;

uniform usampler2D u_texture;
uniform mat4 u_proj;
uniform mat4 u_view;
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

  vec4 cam = u_view * vec4(center, 1.0);
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
  mat3 W = transpose(mat3(u_view));
  mat3 T = W * J;
  mat3 cov2d = transpose(T) * Vrk * T;

  // 2D mip filter: a low-pass floor of ~0.3px, as in Mip-Splatting.
  cov2d[0][0] += 0.3;
  cov2d[1][1] += 0.3;

  float mid = (cov2d[0][0] + cov2d[1][1]) * 0.5;
  float rad = length(vec2((cov2d[0][0] - cov2d[1][1]) * 0.5, cov2d[0][1]));
  float l1 = mid + rad;
  float l2 = mid - rad;
  if (l2 < 0.0) { gl_Position = vec4(0.0, 0.0, 2.0, 1.0); return; }
  vec2 dir = normalize(vec2(cov2d[0][1], l1 - cov2d[0][0]));
  vec2 major = min(sqrt(2.0 * l1), 1024.0) * dir;
  vec2 minor = min(sqrt(2.0 * l2), 1024.0) * vec2(dir.y, -dir.x);

  uint rgba = cen.w;
  v_color = vec4(
    float(rgba & 0xffu), float((rgba >> 8) & 0xffu),
    float((rgba >> 16) & 0xffu), float((rgba >> 24) & 0xffu)) / 255.0;
  v_pos = a_corner;

  vec2 corner = a_corner * 2.0;
  gl_Position = vec4(
    pos2d.xy / pos2d.w + (corner.x * major + corner.y * minor) / u_viewport,
    0.0, 1.0);
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

// Overlay pass: world-space line segments (camera frustums).
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
  private texture: WebGLTexture;
  private indexBuffer: WebGLBuffer;
  private vao: WebGLVertexArrayObject;
  private lineVao: WebGLVertexArrayObject;
  private linePosBuffer: WebGLBuffer;
  private lineColorBuffer: WebGLBuffer;
  private lineVertexCount = 0;
  private worker: Worker;
  private count = 0;
  private sortedCount = 0;
  private lastSortVP: Float32Array | null = null;
  private sortPending = false;
  private disposed = false;
  private uProj: WebGLUniformLocation;
  private uView: WebGLUniformLocation;
  private uModel: WebGLUniformLocation;
  private uFocal: WebGLUniformLocation;
  private uViewport: WebGLUniformLocation;
  private uTexture: WebGLUniformLocation;
  private uLineProj: WebGLUniformLocation;
  private uLineView: WebGLUniformLocation;
  private uLineModel: WebGLUniformLocation;

  private pendingAutoFrame = false;
  /** Row-major 3x3 model rotation, applied about `pivot`. */
  private modelRot: number[][] = identity3();
  private pivot: Vec3 = [0, 0, 0];
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

  public camera: CameraState = defaultCamera();
  public autoOrbit = false;
  public showFrustums = true;
  public onStats: ((splats: number) => void) | null = null;
  public onFps: ((fps: number) => void) | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", {
      antialias: false,
      alpha: true,
      premultipliedAlpha: true,
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

    this.uProj = gl.getUniformLocation(this.program, "u_proj")!;
    this.uView = gl.getUniformLocation(this.program, "u_view")!;
    this.uModel = gl.getUniformLocation(this.program, "u_model")!;
    this.uFocal = gl.getUniformLocation(this.program, "u_focal")!;
    this.uViewport = gl.getUniformLocation(this.program, "u_viewport")!;
    this.uTexture = gl.getUniformLocation(this.program, "u_texture")!;
    this.uLineProj = gl.getUniformLocation(this.lineProgram, "u_proj")!;
    this.uLineView = gl.getUniformLocation(this.lineProgram, "u_view")!;
    this.uLineModel = gl.getUniformLocation(this.lineProgram, "u_model")!;

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

    // Line VAO.
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
    gl.bindVertexArray(null);

    this.texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);

    gl.disable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    this.worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (e) => this.onWorker(e.data);

    const loop = (now: number) => {
      if (this.disposed) return;
      this.frame(now);
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
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

  resetModelRotation() {
    this.setModelRotation(identity3());
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

  private onWorker(msg: any) {
    if (msg.type === "parsed") {
      const texdata: Uint32Array = msg.texdata;
      this.sceneCenter = msg.sceneCenter;
      this.sceneRadius = msg.sceneRadius;
      this.pivot = msg.sceneCenter;
      this.uploadTex(texdata, msg.count);

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
    } else if (msg.type === "sorted") {
      const idx: Uint32Array = msg.depthIndex;
      this.sortedCount = Math.min(msg.count, this.count);
      this.gl.bindBuffer(this.gl.ARRAY_BUFFER, this.indexBuffer);
      this.gl.bufferData(this.gl.ARRAY_BUFFER, idx, this.gl.DYNAMIC_DRAW);
      this.sortPending = false;
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

  private frame(now: number) {
    const gl = this.gl;
    const canvas = this.canvas;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
    const h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }

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

    gl.viewport(0, 0, w, h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);

    if (this.autoOrbit) this.camera.yaw += 0.002;

    const proj = projectionMatrix(
      w, h, this.camera.fovY,
      0.02 * this.camera.distance,
      100 * this.camera.distance,
    );
    const view = viewMatrix(orbitBasis(this.camera));
    const model = modelMatrix(this.modelRot, this.pivot);
    const focal = h / (2 * Math.tan(this.camera.fovY / 2));

    if (this.count > 0 && this.sortedCount > 0) {
      gl.useProgram(this.program);
      gl.bindVertexArray(this.vao);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, this.texture);
      gl.uniform1i(this.uTexture, 0);
      gl.uniformMatrix4fv(this.uProj, false, proj);
      gl.uniformMatrix4fv(this.uView, false, view);
      gl.uniformMatrix4fv(this.uModel, false, model);
      gl.uniform2f(this.uFocal, focal, focal);
      gl.uniform2f(this.uViewport, w, h);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, this.sortedCount);
    }

    // Frustums fade in, so keep rebuilding while any of them is still young.
    const animating = this.frustums.some((f) => now - f.addedAt < FRUSTUM_FADE_MS);
    if (this.showFrustums && this.frustums.length > 0 && (this.frustumsDirty || animating)) {
      this.rebuildFrustumBuffers(now);
      this.frustumsDirty = false;
    }
    if (this.showFrustums && this.lineVertexCount > 0) {
      gl.useProgram(this.lineProgram);
      gl.bindVertexArray(this.lineVao);
      gl.uniformMatrix4fv(this.uLineProj, false, proj);
      gl.uniformMatrix4fv(this.uLineView, false, view);
      gl.uniformMatrix4fv(this.uLineModel, false, model);
      gl.drawArrays(gl.LINES, 0, this.lineVertexCount);
    }
    gl.bindVertexArray(null);

    if (this.count === 0) return;

    // Sorting happens against proj * view * model, so a model rotation
    // re-sorts exactly like a camera move.
    const vp = mat4Mul(mat4Mul(proj, view), model);
    if (!this.sortPending && this.needsSort(vp)) {
      this.sortPending = true;
      this.lastSortVP = vp;
      this.worker.postMessage({ type: "sort", viewProj: vp });
    }
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
  }
}
