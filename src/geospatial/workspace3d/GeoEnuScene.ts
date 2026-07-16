/**
 * WebGL2 ENU scene: satellite terrain plane, depth-driven water mesh, gizmo lines,
 * and an optional shared-context splat pass so water can occlude underwater Gaussians.
 */

import {
  orbitBasis,
  orbitBy,
  panBy,
  projectionMatrix,
  viewMatrix,
  zoomAtCursor,
  type CameraState,
} from "../../splat/camera";
import type { SplatRenderer } from "../../splat/renderer";
import { frameForAoi, type Vec3 } from "../enu";
import type { AoiWgs84 } from "../aoi";
import { depthToRgba } from "../preview/raster";
import { H_DRY } from "../preview/softSolver";
import type { GeoWaterStyle } from "../types";
import type { ModelTransform } from "../modelTransform";

const TERRAIN_VS = `#version 300 es
precision highp float;
uniform mat4 u_proj;
uniform mat4 u_view;
in vec3 a_pos;
in vec2 a_uv;
out vec2 v_uv;
out float v_z;
void main() {
  v_uv = a_uv;
  v_z = a_pos.z;
  gl_Position = u_proj * u_view * vec4(a_pos, 1.0);
}
`;

const TERRAIN_FS = `#version 300 es
precision highp float;
uniform sampler2D u_tex;
uniform float u_opacity;
in vec2 v_uv;
in float v_z;
out vec4 fragColor;
void main() {
  vec3 albedo = texture(u_tex, v_uv).rgb;
  float shade = 0.85 + 0.15 * clamp(v_z * 0.02, -1.0, 1.0);
  fragColor = vec4(albedo * shade, u_opacity);
}
`;

const WATER_VS = `#version 300 es
precision highp float;
uniform mat4 u_proj;
uniform mat4 u_view;
in vec3 a_pos;
in vec4 a_color;
in float a_foam;
out vec4 v_color;
out float v_foam;
out vec3 v_world;
void main() {
  v_color = a_color;
  v_foam = a_foam;
  v_world = a_pos;
  gl_Position = u_proj * u_view * vec4(a_pos, 1.0);
}
`;

const WATER_FS = `#version 300 es
precision highp float;
uniform vec3 u_eye;
uniform float u_reduced;
in vec4 v_color;
in float v_foam;
in vec3 v_world;
out vec4 fragColor;
void main() {
  vec3 viewDir = normalize(u_eye - v_world);
  float fresnel = u_reduced > 0.5 ? 0.2 : pow(1.0 - max(0.0, viewDir.z), 3.0);
  vec3 deep = v_color.rgb * 0.72;
  vec3 col = mix(v_color.rgb, deep, 0.35 + fresnel * 0.4);
  col = mix(col, vec3(0.92, 0.97, 1.0), clamp(v_foam, 0.0, 1.0) * 0.55);
  float alpha = clamp(v_color.a + fresnel * 0.25, 0.0, 0.92);
  fragColor = vec4(col * alpha, alpha);
}
`;

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

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const sh = gl.createShader(type)!;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(sh) ?? "shader compile failed");
  }
  return sh;
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const prog = gl.createProgram()!;
  gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, vs));
  gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(prog) ?? "shader link failed");
  }
  return prog;
}

export interface WaterGridInput {
  h: Float32Array;
  u: Float32Array;
  v: Float32Array;
  zBed: Float32Array;
  cols: number;
  rows: number;
  /** Water style for colormap. */
  style: GeoWaterStyle;
  maxDepthRef: number;
  /** Lift water slightly above bed + splat (metres). */
  surfaceBiasM?: number;
}

export class GeoEnuScene {
  readonly camera: CameraState;
  private gl: WebGL2RenderingContext;
  private disposed = false;
  private raf = 0;
  private splat: SplatRenderer | null = null;

  private terrainProg: WebGLProgram;
  private waterProg: WebGLProgram;
  private lineProg: WebGLProgram;

  private terrainVao: WebGLVertexArrayObject;
  private terrainPos: WebGLBuffer;
  private terrainUv: WebGLBuffer;
  private terrainIdx: WebGLBuffer;
  private terrainCount = 0;
  private terrainTex: WebGLTexture;

  private waterVao: WebGLVertexArrayObject;
  private waterPos: WebGLBuffer;
  private waterColor: WebGLBuffer;
  private waterFoam: WebGLBuffer;
  private waterIdx: WebGLBuffer;
  private waterIndexCount = 0;

  private gizmoVao: WebGLVertexArrayObject;
  private gizmoPos: WebGLBuffer;
  private gizmoColor: WebGLBuffer;
  private gizmoCount = 0;

  private uTerrainProj: WebGLUniformLocation;
  private uTerrainView: WebGLUniformLocation;
  private uTerrainTex: WebGLUniformLocation;
  private uTerrainOpacity: WebGLUniformLocation;

  private uWaterProj: WebGLUniformLocation;
  private uWaterView: WebGLUniformLocation;
  private uWaterEye: WebGLUniformLocation;
  private uWaterReduced: WebGLUniformLocation;

  private uLineProj: WebGLUniformLocation;
  private uLineView: WebGLUniformLocation;
  private uLineModel: WebGLUniformLocation;

  private bounds = frameForAoi(null);
  private reducedMotion = false;
  private lowPower = false;
  private model: ModelTransform | null = null;
  private pivot: Vec3 = [0, 0, 0];
  private showGizmo = true;
  private gizmoAxisLen = 40;

  onFrame: ((cam: CameraState, w: number, h: number) => void) | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", {
      antialias: true,
      alpha: true,
      premultipliedAlpha: true,
      depth: true,
    });
    if (!gl) throw new Error("WebGL2 is required for the 3D geospatial workspace.");
    this.gl = gl;

    this.camera = {
      target: [0, 0, 0],
      distance: 420,
      yaw: 0.55,
      pitch: -0.42,
      fovY: (50 * Math.PI) / 180,
      worldUp: [0, 0, 1],
    };

    this.terrainProg = link(gl, TERRAIN_VS, TERRAIN_FS);
    this.waterProg = link(gl, WATER_VS, WATER_FS);
    this.lineProg = link(gl, LINE_VS, LINE_FS);

    this.uTerrainProj = gl.getUniformLocation(this.terrainProg, "u_proj")!;
    this.uTerrainView = gl.getUniformLocation(this.terrainProg, "u_view")!;
    this.uTerrainTex = gl.getUniformLocation(this.terrainProg, "u_tex")!;
    this.uTerrainOpacity = gl.getUniformLocation(this.terrainProg, "u_opacity")!;

    this.uWaterProj = gl.getUniformLocation(this.waterProg, "u_proj")!;
    this.uWaterView = gl.getUniformLocation(this.waterProg, "u_view")!;
    this.uWaterEye = gl.getUniformLocation(this.waterProg, "u_eye")!;
    this.uWaterReduced = gl.getUniformLocation(this.waterProg, "u_reduced")!;

    this.uLineProj = gl.getUniformLocation(this.lineProg, "u_proj")!;
    this.uLineView = gl.getUniformLocation(this.lineProg, "u_view")!;
    this.uLineModel = gl.getUniformLocation(this.lineProg, "u_model")!;

    this.terrainVao = gl.createVertexArray()!;
    this.terrainPos = gl.createBuffer()!;
    this.terrainUv = gl.createBuffer()!;
    this.terrainIdx = gl.createBuffer()!;
    this.terrainTex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.terrainTex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA,
      1,
      1,
      0,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      new Uint8Array([36, 52, 44, 255]),
    );

    gl.bindVertexArray(this.terrainVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.terrainPos);
    const aPos = gl.getAttribLocation(this.terrainProg, "a_pos");
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 3, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.terrainUv);
    const aUv = gl.getAttribLocation(this.terrainProg, "a_uv");
    gl.enableVertexAttribArray(aUv);
    gl.vertexAttribPointer(aUv, 2, gl.FLOAT, false, 0, 0);

    this.waterVao = gl.createVertexArray()!;
    this.waterPos = gl.createBuffer()!;
    this.waterColor = gl.createBuffer()!;
    this.waterFoam = gl.createBuffer()!;
    this.waterIdx = gl.createBuffer()!;
    gl.bindVertexArray(this.waterVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.waterPos);
    const waPos = gl.getAttribLocation(this.waterProg, "a_pos");
    gl.enableVertexAttribArray(waPos);
    gl.vertexAttribPointer(waPos, 3, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.waterColor);
    const waCol = gl.getAttribLocation(this.waterProg, "a_color");
    gl.enableVertexAttribArray(waCol);
    gl.vertexAttribPointer(waCol, 4, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.waterFoam);
    const waFoam = gl.getAttribLocation(this.waterProg, "a_foam");
    gl.enableVertexAttribArray(waFoam);
    gl.vertexAttribPointer(waFoam, 1, gl.FLOAT, false, 0, 0);

    this.gizmoVao = gl.createVertexArray()!;
    this.gizmoPos = gl.createBuffer()!;
    this.gizmoColor = gl.createBuffer()!;
    gl.bindVertexArray(this.gizmoVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gizmoPos);
    const gaPos = gl.getAttribLocation(this.lineProg, "a_pos");
    gl.enableVertexAttribArray(gaPos);
    gl.vertexAttribPointer(gaPos, 3, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gizmoColor);
    const gaCol = gl.getAttribLocation(this.lineProg, "a_color");
    gl.enableVertexAttribArray(gaCol);
    gl.vertexAttribPointer(gaCol, 4, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);

    gl.enable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    this.rebuildTerrainMesh();
    this.rebuildGizmo();

    const loop = () => {
      if (this.disposed) return;
      this.draw();
      this.raf = requestAnimationFrame(loop);
    };
    this.raf = requestAnimationFrame(loop);
  }

  dispose() {
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    this.splat = null;
  }

  /** Shared GL context for embedding a SplatRenderer into this scene. */
  get context(): WebGL2RenderingContext {
    return this.gl;
  }

  /**
   * Attach a splat renderer that was constructed with this scene's GL context.
   * Draw order becomes terrain → water (depth write) → splat (depth test).
   */
  attachSplat(splat: SplatRenderer | null) {
    this.splat = splat;
  }

  setReducedMotion(on: boolean) {
    this.reducedMotion = on;
  }

  setLowPower(on: boolean) {
    this.lowPower = on;
  }

  setAoi(aoi: AoiWgs84 | null) {
    this.bounds = frameForAoi(aoi);
    this.rebuildTerrainMesh();
    const cx = 0.5 * (this.bounds.minE + this.bounds.maxE);
    const cy = 0.5 * (this.bounds.minN + this.bounds.maxN);
    this.camera.target = [cx, cy, 0];
    this.camera.distance = Math.max(80, Math.hypot(this.bounds.widthM, this.bounds.heightM) * 0.95);
    this.gizmoAxisLen = Math.max(20, Math.min(this.bounds.widthM, this.bounds.heightM) * 0.12);
    this.rebuildGizmo();
  }

  setTerrainImage(source: TexImageSource) {
    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D, this.terrainTex);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, 1);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, source);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, 0);
  }

  setModelTransform(t: ModelTransform | null, pivot?: Vec3) {
    this.model = t;
    if (pivot) this.pivot = [...pivot] as Vec3;
    this.rebuildGizmo();
  }

  setShowGizmo(on: boolean) {
    this.showGizmo = on;
  }

  /** Update water surface from preview depth/velocity grids (ENU). */
  setWaterGrid(input: WaterGridInput | null) {
    const gl = this.gl;
    if (!input || input.cols < 2 || input.rows < 2) {
      this.waterIndexCount = 0;
      return;
    }
    const { cols, rows, h, u, v, zBed, style, maxDepthRef } = input;
    const bias = input.surfaceBiasM ?? 0.15;
    const { minE, minN, maxE, maxN } = this.bounds;
    const n = cols * rows;
    const pos = new Float32Array(n * 3);
    const col = new Float32Array(n * 4);
    const foam = new Float32Array(n);
    const rgba = new Uint8ClampedArray(4);

    for (let j = 0; j < rows; j++) {
      for (let i = 0; i < cols; i++) {
        const k = j * cols + i;
        const e = minE + ((i + 0.5) / cols) * (maxE - minE);
        const nE = minN + ((j + 0.5) / rows) * (maxN - minN);
        const depth = h[k] ?? 0;
        const bed = zBed[k] ?? 0;
        const wet = depth > H_DRY;
        const z = wet ? bed + depth + bias : bed - 0.05;
        pos[k * 3] = e;
        pos[k * 3 + 1] = nE;
        pos[k * 3 + 2] = z;

        depthToRgba(depth, maxDepthRef, style, -1, rgba, 0);
        col[k * 4] = rgba[0] / 255;
        col[k * 4 + 1] = rgba[1] / 255;
        col[k * 4 + 2] = rgba[2] / 255;
        col[k * 4 + 3] = wet ? Math.max(0.12, rgba[3] / 255) : 0;

        const speed = Math.hypot(u[k] ?? 0, v[k] ?? 0);
        foam[k] = wet ? Math.min(1, speed / 1.8) : 0;
      }
    }

    const indices = new Uint32Array((cols - 1) * (rows - 1) * 6);
    let ii = 0;
    for (let j = 0; j < rows - 1; j++) {
      for (let i = 0; i < cols - 1; i++) {
        const a = j * cols + i;
        const b = a + 1;
        const c = a + cols;
        const d = c + 1;
        // Skip fully dry quads.
        if (
          (h[a] ?? 0) <= H_DRY &&
          (h[b] ?? 0) <= H_DRY &&
          (h[c] ?? 0) <= H_DRY &&
          (h[d] ?? 0) <= H_DRY
        ) {
          continue;
        }
        indices[ii++] = a;
        indices[ii++] = c;
        indices[ii++] = b;
        indices[ii++] = b;
        indices[ii++] = c;
        indices[ii++] = d;
      }
    }
    this.waterIndexCount = ii;

    gl.bindBuffer(gl.ARRAY_BUFFER, this.waterPos);
    gl.bufferData(gl.ARRAY_BUFFER, pos, gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.waterColor);
    gl.bufferData(gl.ARRAY_BUFFER, col, gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.waterFoam);
    gl.bufferData(gl.ARRAY_BUFFER, foam, gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.waterIdx);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices.subarray(0, ii), gl.DYNAMIC_DRAW);
  }

  orbit(dx: number, dy: number) {
    if (this.reducedMotion) {
      orbitBy(this.camera, dx * 0.55, dy * 0.55);
    } else {
      orbitBy(this.camera, dx, dy);
    }
  }

  pan(dx: number, dy: number, heightPx: number) {
    panBy(this.camera, dx, dy, heightPx);
  }

  zoom(factor: number, x: number, y: number, w: number, h: number) {
    zoomAtCursor(this.camera, factor, x, y, w, h);
  }

  private rebuildTerrainMesh() {
    const gl = this.gl;
    const segs = this.lowPower ? 24 : 48;
    const { minE, minN, maxE, maxN } = this.bounds;
    const cols = segs + 1;
    const rows = segs + 1;
    const pos = new Float32Array(cols * rows * 3);
    const uv = new Float32Array(cols * rows * 2);
    for (let j = 0; j < rows; j++) {
      for (let i = 0; i < cols; i++) {
        const k = j * cols + i;
        const u = i / segs;
        const v = j / segs;
        const e = minE + u * (maxE - minE);
        const n = minN + v * (maxN - minN);
        // Gentle synthetic bed undulation (non-authoritative DEM stand-in).
        const z =
          Math.sin(u * Math.PI * 2.2) * Math.cos(v * Math.PI * 1.7) * 2.5 +
          Math.sin((u + v) * Math.PI * 3) * 0.8;
        pos[k * 3] = e;
        pos[k * 3 + 1] = n;
        pos[k * 3 + 2] = z;
        uv[k * 2] = u;
        uv[k * 2 + 1] = v;
      }
    }
    const indices = new Uint32Array(segs * segs * 6);
    let ii = 0;
    for (let j = 0; j < segs; j++) {
      for (let i = 0; i < segs; i++) {
        const a = j * cols + i;
        const b = a + 1;
        const c = a + cols;
        const d = c + 1;
        indices[ii++] = a;
        indices[ii++] = c;
        indices[ii++] = b;
        indices[ii++] = b;
        indices[ii++] = c;
        indices[ii++] = d;
      }
    }
    this.terrainCount = indices.length;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.terrainPos);
    gl.bufferData(gl.ARRAY_BUFFER, pos, gl.STATIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.terrainUv);
    gl.bufferData(gl.ARRAY_BUFFER, uv, gl.STATIC_DRAW);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.terrainIdx);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW);
  }

  private rebuildGizmo() {
    const gl = this.gl;
    const L = this.gizmoAxisLen;
    const o = this.model
      ? ([
          this.pivot[0] + this.model.translation[0],
          this.pivot[1] + this.model.translation[1],
          this.pivot[2] + this.model.translation[2],
        ] as Vec3)
      : ([...this.pivot] as Vec3);
    const axes: { dir: Vec3; rgba: [number, number, number, number] }[] = [
      { dir: [1, 0, 0], rgba: [0.92, 0.32, 0.28, 0.95] },
      { dir: [0, 1, 0], rgba: [0.35, 0.82, 0.4, 0.95] },
      { dir: [0, 0, 1], rgba: [0.3, 0.55, 0.98, 0.95] },
    ];
    const pos = new Float32Array(axes.length * 2 * 3);
    const col = new Float32Array(axes.length * 2 * 4);
    axes.forEach((a, i) => {
      pos.set(o, i * 6);
      pos[i * 6 + 3] = o[0] + a.dir[0] * L;
      pos[i * 6 + 4] = o[1] + a.dir[1] * L;
      pos[i * 6 + 5] = o[2] + a.dir[2] * L;
      col.set(a.rgba, i * 8);
      col.set(a.rgba, i * 8 + 4);
    });
    this.gizmoCount = axes.length * 2;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gizmoPos);
    gl.bufferData(gl.ARRAY_BUFFER, pos, gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gizmoColor);
    gl.bufferData(gl.ARRAY_BUFFER, col, gl.DYNAMIC_DRAW);
  }

  private draw() {
    const gl = this.gl;
    const canvas = this.canvas;
    const dpr = Math.min(window.devicePixelRatio || 1, this.lowPower ? 1 : 2);
    const w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
    const h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }

    const basis = orbitBasis(this.camera);
    const proj = projectionMatrix(w, h, this.camera.fovY, 0.5, Math.max(2000, this.camera.distance * 40));
    const view = viewMatrix(basis);

    gl.viewport(0, 0, w, h);
    gl.clearColor(0.07, 0.1, 0.12, 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    gl.depthMask(true);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    // Terrain
    gl.useProgram(this.terrainProg);
    gl.bindVertexArray(this.terrainVao);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.terrainIdx);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.terrainTex);
    gl.uniform1i(this.uTerrainTex, 0);
    gl.uniform1f(this.uTerrainOpacity, 1);
    gl.uniformMatrix4fv(this.uTerrainProj, false, proj);
    gl.uniformMatrix4fv(this.uTerrainView, false, view);
    gl.drawElements(gl.TRIANGLES, this.terrainCount, gl.UNSIGNED_INT, 0);

    // Water writes depth so underwater Gaussians fail the shared depth test.
    if (this.waterIndexCount > 0) {
      gl.depthMask(true);
      gl.useProgram(this.waterProg);
      gl.bindVertexArray(this.waterVao);
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.waterIdx);
      gl.uniformMatrix4fv(this.uWaterProj, false, proj);
      gl.uniformMatrix4fv(this.uWaterView, false, view);
      gl.uniform3f(this.uWaterEye, basis.eye[0], basis.eye[1], basis.eye[2]);
      gl.uniform1f(this.uWaterReduced, this.reducedMotion || this.lowPower ? 1 : 0);
      gl.drawElements(gl.TRIANGLES, this.waterIndexCount, gl.UNSIGNED_INT, 0);
    }

    // Shared-context splat / stage layers (depth-tested, no depth write).
    if (this.splat) {
      this.splat.renderPass(performance.now(), { proj, view, w, h });
      // Restore host blend/depth defaults after the splat pass mutates GL state.
      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LEQUAL);
      gl.depthMask(true);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    }

    // Gizmo
    if (this.showGizmo && this.gizmoCount > 0) {
      gl.disable(gl.DEPTH_TEST);
      gl.useProgram(this.lineProg);
      gl.bindVertexArray(this.gizmoVao);
      const id = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
      gl.uniformMatrix4fv(this.uLineProj, false, proj);
      gl.uniformMatrix4fv(this.uLineView, false, view);
      gl.uniformMatrix4fv(this.uLineModel, false, id);
      gl.drawArrays(gl.LINES, 0, this.gizmoCount);
      gl.enable(gl.DEPTH_TEST);
    }

    this.onFrame?.(this.camera, w, h);
  }
}
