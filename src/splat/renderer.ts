// WebGL2 Gaussian Splat renderer with orbit camera.
// Splat data lives in an RGBA32UI texture (2 texels per splat); a per-instance
// uint index attribute (re-uploaded after each worker sort) selects splats in
// back-to-front order for correct alpha blending.

const VS = `#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;

uniform usampler2D u_texture;
uniform mat4 u_proj;
uniform mat4 u_view;
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
  vec3 center = uintBitsToFloat(cen.xyz);

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

  float invz = 1.0 / cam.z;
  mat3 J = mat3(
    u_focal.x * invz, 0.0, -u_focal.x * cam.x * invz * invz,
    0.0, u_focal.y * invz, -u_focal.y * cam.y * invz * invz,
    0.0, 0.0, 0.0
  );
  mat3 W = transpose(mat3(u_view));
  mat3 T = W * J;
  mat3 cov2d = transpose(T) * Vrk * T;

  // low-pass filter (anti-aliasing floor, mip-splatting style ~0.3px)
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

export interface CameraState {
  yaw: number;
  pitch: number;
  distance: number;
  target: [number, number, number];
  fovY: number;
}

function mat4Mul(a: Float32Array, b: Float32Array): Float32Array {
  const o = new Float32Array(16);
  for (let r = 0; r < 4; r++)
    for (let c = 0; c < 4; c++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + c] * b[r * 4 + k];
      o[r * 4 + c] = s;
    }
  return o;
}

export class SplatRenderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram;
  private texture: WebGLTexture;
  private indexBuffer: WebGLBuffer;
  private vao: WebGLVertexArrayObject;
  private worker: Worker;
  private count = 0;
  private sortedCount = 0;
  private texWidth = 1024;
  private lastSortVP: Float32Array | null = null;
  private sortPending = false;
  private disposed = false;
  private uProj: WebGLUniformLocation;
  private uView: WebGLUniformLocation;
  private uFocal: WebGLUniformLocation;
  private uViewport: WebGLUniformLocation;

  public camera: CameraState = {
    yaw: 0.4,
    pitch: -0.25,
    distance: 4,
    target: [0, 0, 0],
    fovY: (60 * Math.PI) / 180,
  };
  public autoOrbit = false;
  public onStats: ((splats: number) => void) | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", {
      antialias: false,
      alpha: true,
      premultipliedAlpha: true,
    });
    if (!gl) throw new Error("WebGL2 not available");
    this.gl = gl;

    const compile = (type: number, src: string) => {
      const sh = gl.createShader(type)!;
      gl.shaderSource(sh, src);
      gl.compileShader(sh);
      if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS))
        throw new Error(gl.getShaderInfoLog(sh) ?? "shader error");
      return sh;
    };
    const prog = gl.createProgram()!;
    gl.attachShader(prog, compile(gl.VERTEX_SHADER, VS));
    gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, FS));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS))
      throw new Error(gl.getProgramInfoLog(prog) ?? "link error");
    this.program = prog;
    gl.useProgram(prog);

    this.uProj = gl.getUniformLocation(prog, "u_proj")!;
    this.uView = gl.getUniformLocation(prog, "u_view")!;
    this.uFocal = gl.getUniformLocation(prog, "u_focal")!;
    this.uViewport = gl.getUniformLocation(prog, "u_viewport")!;

    this.vao = gl.createVertexArray()!;
    gl.bindVertexArray(this.vao);

    // quad corners
    const quad = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const aCorner = gl.getAttribLocation(prog, "a_corner");
    gl.enableVertexAttribArray(aCorner);
    gl.vertexAttribPointer(aCorner, 2, gl.FLOAT, false, 0, 0);

    // per-instance sorted index
    this.indexBuffer = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.indexBuffer);
    const aIndex = gl.getAttribLocation(prog, "a_index");
    gl.enableVertexAttribArray(aIndex);
    gl.vertexAttribIPointer(aIndex, 1, gl.UNSIGNED_INT, 0, 0);
    gl.vertexAttribDivisor(aIndex, 1);

    this.texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);

    gl.disable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    // premultiplied alpha, back-to-front "over"
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    this.worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (e) => this.onWorker(e.data);

    const loop = () => {
      if (this.disposed) return;
      this.frame();
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
  }

  /** Load (or hot-swap to) a splat .ply from raw bytes. */
  loadPly(buffer: ArrayBuffer, autoFrame: boolean) {
    this.pendingAutoFrame = autoFrame;
    this.worker.postMessage({ type: "parse", buffer }, [buffer]);
  }

  private pendingAutoFrame = false;

  private onWorker(msg: any) {
    const gl = this.gl;
    if (msg.type === "parsed") {
      const texdata: Uint32Array = msg.texdata;
      this.count = msg.count;
      const texelsPerRow = this.texWidth; // 512 splats * 2 texels
      const rows = Math.ceil((this.count * 2) / texelsPerRow);
      const padded = new Uint32Array(texelsPerRow * rows * 4);
      padded.set(texdata);
      gl.bindTexture(gl.TEXTURE_2D, this.texture);
      gl.texImage2D(
        gl.TEXTURE_2D, 0, gl.RGBA32UI, texelsPerRow / 2, rows * 2, 0,
        gl.RGBA_INTEGER, gl.UNSIGNED_INT, null,
      );
      // Repack: our shader addresses texels as (idx&511)<<1 in x, idx>>9 in y,
      // i.e. texture is 1024 texels wide and each splat is 2 adjacent texels.
      gl.texImage2D(
        gl.TEXTURE_2D, 0, gl.RGBA32UI, 1024, Math.ceil(this.count / 512), 0,
        gl.RGBA_INTEGER, gl.UNSIGNED_INT,
        this.padTo(texdata, 1024 * Math.ceil(this.count / 512) * 4),
      );
      if (this.pendingAutoFrame && msg.sceneRadius > 0) {
        this.camera.target = msg.sceneCenter;
        this.camera.distance = msg.sceneRadius * 2.2;
      }
      this.lastSortVP = null; // force re-sort
      this.onStats?.(this.count);
    } else if (msg.type === "sorted") {
      const idx: Uint32Array = msg.depthIndex;
      this.sortedCount = Math.min(msg.count, this.count);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.indexBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, idx, gl.DYNAMIC_DRAW);
      this.sortPending = false;
    } else if (msg.type === "error") {
      console.error("splat worker:", msg.message);
    }
  }

  private padTo(data: Uint32Array, len: number): Uint32Array {
    if (data.length === len) return data;
    const out = new Uint32Array(len);
    out.set(data.subarray(0, Math.min(data.length, len)));
    return out;
  }

  private viewMatrix(): Float32Array {
    const { yaw, pitch, distance, target } = this.camera;
    const cp = Math.cos(pitch), sp = Math.sin(pitch);
    const cy = Math.cos(yaw), sy = Math.sin(yaw);
    // camera position on orbit sphere (splat world is Y-down; flip via up)
    const eye = [
      target[0] + distance * cp * sy,
      target[1] + distance * sp,
      target[2] + distance * cp * cy,
    ];
    const f = [
      (target[0] - eye[0]) / distance,
      (target[1] - eye[1]) / distance,
      (target[2] - eye[2]) / distance,
    ];
    const upW = [0, -1, 0]; // COLMAP convention: Y points down
    let sx = f[1] * upW[2] - f[2] * upW[1];
    let sy2 = f[2] * upW[0] - f[0] * upW[2];
    let sz = f[0] * upW[1] - f[1] * upW[0];
    const sl = Math.hypot(sx, sy2, sz) || 1;
    sx /= sl; sy2 /= sl; sz /= sl;
    const ux = sy2 * f[2] - sz * f[1];
    const uy = sz * f[0] - sx * f[2];
    const uz = sx * f[1] - sy2 * f[0];
    return new Float32Array([
      sx, ux, f[0], 0,
      sy2, uy, f[1], 0,
      sz, uz, f[2], 0,
      -(sx * eye[0] + sy2 * eye[1] + sz * eye[2]),
      -(ux * eye[0] + uy * eye[1] + uz * eye[2]),
      -(f[0] * eye[0] + f[1] * eye[1] + f[2] * eye[2]),
      1,
    ]);
  }

  private frame() {
    const gl = this.gl;
    const canvas = this.canvas;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
    const h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    gl.viewport(0, 0, w, h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (this.count === 0) return;

    if (this.autoOrbit) this.camera.yaw += 0.002;

    const { fovY } = this.camera;
    const fy = h / (2 * Math.tan(fovY / 2));
    const fx = fy;
    const near = 0.02 * this.camera.distance;
    const far = 100 * this.camera.distance;
    const proj = new Float32Array([
      (2 * fx) / w, 0, 0, 0,
      0, -(2 * fy) / h, 0, 0,
      0, 0, far / (far - near), 1,
      0, 0, -(far * near) / (far - near), 0,
    ]);
    const view = this.viewMatrix();

    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.uniform1i(gl.getUniformLocation(this.program, "u_texture"), 0);
    gl.uniformMatrix4fv(this.uProj, false, proj);
    gl.uniformMatrix4fv(this.uView, false, view);
    gl.uniform2f(this.uFocal, fx, fy);
    gl.uniform2f(this.uViewport, w, h);

    if (this.sortedCount > 0) {
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, this.sortedCount);
    }

    // Re-sort when the view direction changed enough.
    const vp = mat4Mul(proj, view);
    if (!this.sortPending && this.needsSort(vp)) {
      this.sortPending = true;
      this.lastSortVP = vp;
      this.worker.postMessage({ type: "sort", viewProj: vp });
    }
  }

  private needsSort(vp: Float32Array): boolean {
    if (!this.lastSortVP) return true;
    let dot = 0;
    // compare view z-axis rows
    for (const i of [2, 6, 10]) dot += vp[i] * this.lastSortVP[i];
    const a = Math.hypot(vp[2], vp[6], vp[10]);
    const b = Math.hypot(this.lastSortVP[2], this.lastSortVP[6], this.lastSortVP[10]);
    return dot / (a * b || 1) < 0.999;
  }

  attachControls() {
    const el = this.canvas;
    let dragging = false;
    let lastX = 0, lastY = 0, button = 0;
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
      if (button === 2 || e.shiftKey) {
        // pan in view plane
        const s = this.camera.distance * 0.0015;
        const cy = Math.cos(this.camera.yaw), sy = Math.sin(this.camera.yaw);
        this.camera.target[0] -= (dx * cy) * s;
        this.camera.target[2] += (dx * sy) * s;
        this.camera.target[1] += dy * s;
      } else {
        this.camera.yaw -= dx * 0.005;
        this.camera.pitch = Math.max(
          -1.55,
          Math.min(1.55, this.camera.pitch - dy * 0.005),
        );
      }
    });
    el.addEventListener("pointerup", () => (dragging = false));
    el.addEventListener("wheel", (e) => {
      e.preventDefault();
      this.camera.distance *= Math.pow(1.0015, e.deltaY);
      this.camera.distance = Math.max(0.05, Math.min(1000, this.camera.distance));
    }, { passive: false });
    el.addEventListener("contextmenu", (e) => e.preventDefault());
  }

  dispose() {
    this.disposed = true;
    this.worker.terminate();
  }
}
