/**
 * Optional WebGPU acceleration probe + metadata.
 * Physics remains CPU-authoritative in this phase; when an adapter is present we
 * mark the backend as webgpu and keep a path open for compute ports later.
 * WebGL2 is used as an intermediate raster path when WebGPU is unavailable.
 */

export type GpuAcceleratorKind = "webgpu" | "webgl" | "cpu";

export interface GpuAccelerator {
  kind: GpuAcceleratorKind;
  label: string;
  /** Dispose GPU resources. */
  destroy(): void;
}

export async function createGpuAccelerator(
  prefer: "webgpu" | "webgl" | "cpu",
): Promise<GpuAccelerator> {
  if (prefer === "cpu") {
    return { kind: "cpu", label: "CPU soft solver", destroy() {} };
  }

  if (prefer === "webgpu") {
    const nav = navigator as Navigator & {
      gpu?: {
        requestAdapter: () => Promise<{ requestDevice: () => Promise<unknown> } | null>;
      };
    };
    if (nav.gpu) {
      try {
        const adapter = await nav.gpu.requestAdapter();
        if (adapter) {
          const device = await adapter.requestDevice();
          void device;
          return {
            kind: "webgpu",
            label: "WebGPU (preview path)",
            destroy() {},
          };
        }
      } catch {
        // fall through
      }
    }
  }

  // WebGL2 fallthrough: prove context; raster still goes through ImageData → MapLibre.
  try {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl2", { antialias: false, depth: false });
    if (gl) {
      const lose = gl.getExtension("WEBGL_lose_context");
      return {
        kind: "webgl",
        label: "WebGL2 raster + CPU physics",
        destroy() {
          lose?.loseContext();
        },
      };
    }
  } catch {
    // ignore
  }

  return { kind: "cpu", label: "CPU soft solver", destroy() {} };
}
