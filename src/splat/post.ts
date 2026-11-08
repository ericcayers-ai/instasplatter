/**
 * Post-process chain for the WebGL2 splat renderer (Photorealism merge Item 4).
 *
 * Three independently-toggleable passes composed as a two-pass chain:
 *   pass A: AgX tonemap (MIT/three.js minimal formulation, documented below)
 *           + procedural lens glare (port of Digital-Earth lib/glare.py)
 *   pass B: simplified FXAA (port of Digital-Earth lib/fxaa.py)
 *
 * The scene renders into an offscreen HDR texture so the tonemapper sees the
 * full dynamic range before display encoding. When every toggle is off the
 * renderer keeps its legacy direct-to-canvas path untouched.
 */

const QUAD_VS = `#version 300 es
precision highp float;
out vec2 v_uv;
void main() {
  // Fullscreen triangle strip quad.
  vec2 pos = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  v_uv = pos;
  gl_Position = vec4(pos * 2.0 - 1.0, 0.0, 1.0);
}
`;

// --- AgX minimal (three.js r16x, MIT) ---------------------------------------
const TONEMAP_GLARE_FS = `#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 outColor;

uniform sampler2D uScene;
uniform vec2 uRes;
uniform bool uTonemap;
uniform bool uGlare;
uniform vec2 uSunUv;        // sun screen position, normalized
uniform float uSunVisible;  // 0..1

const mat3 LINEAR_REC2020_TO_LINEAR_SRGB = mat3(
  1.6605, -0.1246, -0.0182,
  -0.5876, 1.1329, -0.1006,
  -0.0728, -0.0083, 1.1187
);
const mat3 LINEAR_SRGB_TO_LINEAR_REC2020 = mat3(
  0.6274, 0.0691, 0.0164,
  0.3293, 0.9195, 0.0880,
  0.0433, 0.0113, 0.8956
);
const mat3 AGX_INSET = mat3(
  0.856627153315983, 0.137318972929847, 0.11189821299995,
  0.0951212405381588, 0.761241990602591, 0.0767994186031903,
  0.0482516061458583, 0.101439036467562, 0.811302368396859
);
const mat3 AGX_OUTSET = mat3(
  1.1271005818144368, -0.1413297634984383, -0.14132976349843826,
  -0.11060664309660323, 1.157823702216272, -0.11060664309660294,
  -0.016493938717834573, -0.016493938717834257, 1.2519364065950405
);
const float AGX_MIN_EV = -12.47393;
const float AGX_MAX_EV = 4.026069;

// Sigmoid approximation of the AgX contrast curve (mean err^2 ~3.7e-6).
vec3 agxContrast(vec3 x) {
  vec3 x2 = x * x;
  vec3 x4 = x2 * x2;
  return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4
       - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;
}

// Inputs/outputs Linear-sRGB (three.js/Filament/Blender minimal AgX).
vec3 agxToneMap(vec3 color) {
  color = LINEAR_SRGB_TO_LINEAR_REC2020 * color;
  color = AGX_INSET * color;
  color = max(color, 1e-10);
  color = clamp((log2(color) - AGX_MIN_EV) / (AGX_MAX_EV - AGX_MIN_EV), 0.0, 1.0);
  color = agxContrast(color);
  color = AGX_OUTSET * color;
  color = pow(max(vec3(0.0), color), vec3(2.2));
  color = LINEAR_REC2020_TO_LINEAR_SRGB * color;
  return clamp(color, 0.0, 1.0);
}

void main() {
  vec3 base = texture(uScene, v_uv).rgb;

  if (uTonemap) {
    base = agxToneMap(base);
  }

  if (uGlare && uSunVisible > 0.001) {
    // Procedural lens flare — port of Digital-Earth lib/glare.py: radial
    // exponential glow, five ghost images along the sun->center axis and a
    // six-fold diffraction streak. Distances are in pixels to keep the
    // original falloff constants meaningful.
    vec2 px = v_uv * uRes;
    vec2 sunPx = clamp(uSunUv, vec2(0.0), vec2(1.0)) * uRes;
    vec2 centerPx = uRes * 0.5;

    float dSun = distance(px, sunPx);

    float glow = exp(-dSun / 80.0) * 0.8 * uSunVisible;

    float ghost = 0.0;
    for (int k = 0; k < 5; k++) {
      float t = 0.25 + float(k) * 0.18;
      vec2 gPx = mix(sunPx, centerPx, -t);
      ghost += exp(-distance(px, gPx) / 18.0) * 0.15 * (1.0 - t) * uSunVisible;
    }

    float angle = atan(px.y - sunPx.y, px.x - sunPx.x);
    float streak = abs(cos(angle * 6.0)) * exp(-dSun / 120.0) * 0.2 * uSunVisible;

    float extra = glow + ghost * 0.7 + streak * 0.5;
    base += vec3(1.0, 0.97, 0.85) * extra * 0.6;
  }

  outColor = vec4(base, 1.0);
}
`;

// --- Simplified FXAA (Digital-Earth lib/fxaa.py) -----------------------------
const FXAA_FS = `#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 outColor;

uniform sampler2D uScene;
uniform vec2 uTexel; // 1/resolution

float lum(vec3 c) {
  return 0.299 * c.r + 0.587 * c.g + 0.114 * c.b;
}

void main() {
  vec3 c = texture(uScene, v_uv).rgb;
  vec3 lu = texture(uScene, v_uv + vec2(0.0, uTexel.y)).rgb;
  vec3 ld = texture(uScene, v_uv - vec2(0.0, uTexel.y)).rgb;
  vec3 ll = texture(uScene, v_uv - vec2(uTexel.x, 0.0)).rgb;
  vec3 lr = texture(uScene, v_uv + vec2(uTexel.x, 0.0)).rgb;

  float gx = abs(lum(lr) - lum(ll));
  float gy = abs(lum(lu) - lum(ld));
  float edgeMax = max(gx, gy);

  if (edgeMax < 0.0833) {
    outColor = vec4(c, 1.0);
    return;
  }

  float blend = min(1.0, edgeMax * 8.0);
  vec3 blendedH = (c + ll + lr) / 3.0;
  vec3 blendedV = (c + ld + lu) / 3.0;
  vec3 blended = gx > gy ? blendedH : blendedV;
  outColor = vec4(mix(c, blended, blend), 1.0);
}
`;

const BLIT_FS = `#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 outColor;
uniform sampler2D uScene;

void main() {
  outColor = texture(uScene, v_uv);
}
`;

function compile(gl: WebGL2RenderingContext, vsSrc: string, fsSrc: string): WebGLProgram {
  const compileShader = (type: number, src: string): WebGLShader => {
    const sh = gl.createShader(type)!;
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
      const info = gl.getShaderInfoLog(sh) ?? "unknown";
      gl.deleteShader(sh);
      throw new Error(`post.ts shader compile failed: ${info}`);
    }
    return sh;
  };
  const program = gl.createProgram()!;
  const vs = compileShader(gl.VERTEX_SHADER, vsSrc);
  const fs = compileShader(gl.FRAGMENT_SHADER, fsSrc);
  gl.attachShader(program, vs);
  gl.attachShader(program, fs);
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    const info = gl.getProgramInfoLog(program) ?? "unknown";
    gl.deleteProgram(program);
    throw new Error(`post.ts program link failed: ${info}`);
  }
  gl.deleteShader(vs);
  gl.deleteShader(fs);
  return program;
}

interface Targets {
  fbo: WebGLFramebuffer;
  tex: WebGLTexture;
}

export class PostChain {
  private gl: WebGL2RenderingContext;
  private developProgram: WebGLProgram;
  private fxaaProgram: WebGLProgram;
  private blitProgram: WebGLProgram;
  private vao: WebGLVertexArrayObject;
  private targets: [Targets, Targets] | null = null;
  private width = 0;
  private height = 0;
  private hdr: boolean;
  private prevFbo: WebGLFramebuffer | null = null;
  private prevViewport = new Int32Array([0, 0, 1, 1]);
  private prevBlend = false;
  private prevDepthTest = false;
  private prevDepthMask = true;
  private prevProgram: WebGLProgram | null = null;
  private prevVao: WebGLVertexArrayObject | null = null;
  private prevActiveTexture = 0;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
    this.developProgram = compile(gl, QUAD_VS, TONEMAP_GLARE_FS);
    this.fxaaProgram = compile(gl, QUAD_VS, FXAA_FS);
    this.blitProgram = compile(gl, QUAD_VS, BLIT_FS);
    this.vao = gl.createVertexArray()!;
    this.hdr = !!gl.getExtension("EXT_color_buffer_float");
  }

  private makeTarget(w: number, h: number): Targets {
    const gl = this.gl;
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    const internal = this.hdr ? gl.RGBA16F : gl.RGBA8;
    gl.texImage2D(gl.TEXTURE_2D, 0, internal, w, h, 0, gl.RGBA, this.hdr ? gl.HALF_FLOAT : gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    const fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    return { fbo, tex };
  }

  /** Allocate/reallocate the ping-pong targets for the current resolution. */
  ensureSize(w: number, h: number): void {
    if (this.targets && this.width === w && this.height === h) return;
    this.disposeTargets();
    this.width = Math.max(1, w);
    this.height = Math.max(1, h);
    this.targets = [this.makeTarget(this.width, this.height), this.makeTarget(this.width, this.height)];
  }

  /**
   * Redirect rendering into the offscreen scene target. Call right before the
   * scene draws; remembers the caller's framebuffer for endFrame().
   */
  begin(w: number, h: number): void {
    const gl = this.gl;
    const callerFbo = gl.getParameter(gl.FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
    const callerTexture = gl.getParameter(gl.TEXTURE_BINDING_2D) as WebGLTexture | null;
    const callerActiveTexture = gl.getParameter(gl.ACTIVE_TEXTURE) as number;
    const callerProgram = gl.getParameter(gl.CURRENT_PROGRAM) as WebGLProgram | null;
    const callerVao = gl.getParameter(gl.VERTEX_ARRAY_BINDING) as WebGLVertexArrayObject | null;
    const callerViewport = gl.getParameter(gl.VIEWPORT) as Int32Array;
    const callerBlend = gl.isEnabled(gl.BLEND);
    const callerDepthTest = gl.isEnabled(gl.DEPTH_TEST);
    const callerDepthMask = gl.getParameter(gl.DEPTH_WRITEMASK) as boolean;
    this.ensureSize(w, h);
    if (!this.targets) return;
    this.prevFbo = callerFbo;
    this.prevViewport = callerViewport;
    this.prevBlend = callerBlend;
    this.prevDepthTest = callerDepthTest;
    this.prevDepthMask = callerDepthMask;
    this.prevProgram = callerProgram;
    this.prevVao = callerVao;
    this.prevActiveTexture = callerActiveTexture;
    // Target allocation binds textures/framebuffers internally. Put the
    // caller's bindings back before rendering into our offscreen target.
    gl.activeTexture(callerActiveTexture);
    gl.bindTexture(gl.TEXTURE_2D, callerTexture);
    gl.bindFramebuffer(gl.FRAMEBUFFER, callerFbo);
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.targets[0].fbo);
    gl.viewport(0, 0, w, h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
  }

  /**
   * Run develop(+glare) then FXAA into the framebuffer that was bound at
   * begin(). Returns false when the chain had no targets (caller should have
   * rendered directly instead).
   */
  end(opts: {
    tonemap: boolean;
    glare: boolean;
    fxaa: boolean;
    sunUv: [number, number];
    sunVisible: number;
  }): boolean {
    const gl = this.gl;
    if (!this.targets) return false;
    const [scene, ping] = this.targets;
    const dstFbo = this.prevFbo;

    // Pass A: scene -> ping (tonemap + glare)
    gl.bindFramebuffer(gl.FRAMEBUFFER, ping.fbo);
    gl.viewport(0, 0, this.width, this.height);
    gl.disable(gl.BLEND);
    gl.disable(gl.DEPTH_TEST);
    gl.depthMask(true);
    gl.useProgram(this.developProgram);
    gl.bindVertexArray(this.vao);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, scene.tex);
    gl.uniform1i(gl.getUniformLocation(this.developProgram, "uScene"), 0);
    gl.uniform2f(gl.getUniformLocation(this.developProgram, "uRes"), this.width, this.height);
    gl.uniform1i(gl.getUniformLocation(this.developProgram, "uTonemap"), opts.tonemap ? 1 : 0);
    gl.uniform1i(gl.getUniformLocation(this.developProgram, "uGlare"), opts.glare ? 1 : 0);
    gl.uniform2f(gl.getUniformLocation(this.developProgram, "uSunUv"), opts.sunUv[0], opts.sunUv[1]);
    gl.uniform1f(gl.getUniformLocation(this.developProgram, "uSunVisible"), opts.sunVisible);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);

    const finalSrc = ping;

    // Pass B: ping -> dst. FXAA reads ping and writes the final image; when
    // disabled, a dedicated copy shader preserves pass A exactly.
    gl.bindFramebuffer(gl.FRAMEBUFFER, dstFbo);
    if (opts.fxaa) {
      gl.useProgram(this.fxaaProgram);
      gl.bindVertexArray(this.vao);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, finalSrc.tex);
      gl.uniform1i(gl.getUniformLocation(this.fxaaProgram, "uScene"), 0);
      gl.uniform2f(
        gl.getUniformLocation(this.fxaaProgram, "uTexel"),
        1 / this.width,
        1 / this.height,
      );
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      gl.bindVertexArray(null);
    } else {
      gl.useProgram(this.blitProgram);
      gl.bindVertexArray(this.vao);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, finalSrc.tex);
      gl.uniform1i(gl.getUniformLocation(this.blitProgram, "uScene"), 0);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    }
    gl.bindVertexArray(this.prevVao);
    gl.useProgram(this.prevProgram);
    gl.activeTexture(this.prevActiveTexture);
    gl.viewport(this.prevViewport[0], this.prevViewport[1], this.prevViewport[2], this.prevViewport[3]);
    gl.depthMask(this.prevDepthMask);
    if (this.prevBlend) gl.enable(gl.BLEND);
    else gl.disable(gl.BLEND);
    if (this.prevDepthTest) gl.enable(gl.DEPTH_TEST);
    else gl.disable(gl.DEPTH_TEST);
    return true;
  }

  private disposeTargets(): void {
    if (!this.targets) return;
    const gl = this.gl;
    for (const t of this.targets) {
      gl.deleteFramebuffer(t.fbo);
      gl.deleteTexture(t.tex);
    }
    this.targets = null;
  }

  dispose(): void {
    this.disposeTargets();
    this.gl.deleteProgram(this.developProgram);
    this.gl.deleteProgram(this.fxaaProgram);
    this.gl.deleteProgram(this.blitProgram);
    this.gl.deleteVertexArray(this.vao);
  }
}
