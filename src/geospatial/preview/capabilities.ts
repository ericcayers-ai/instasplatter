import type { PreviewBackend, PreviewCapabilities } from "./types";

export function detectPreviewCapabilities(opts?: {
  lowPower?: boolean;
}): PreviewCapabilities {
  const reducedMotion =
    typeof window !== "undefined" &&
    !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

  const saveData =
    typeof navigator !== "undefined" &&
    !!(navigator as Navigator & { connection?: { saveData?: boolean } }).connection?.saveData;

  const webgpu = typeof navigator !== "undefined" && "gpu" in navigator;
  const webgl2 = (() => {
    if (typeof document === "undefined") return false;
    try {
      const c = document.createElement("canvas");
      return !!c.getContext("webgl2");
    } catch {
      return false;
    }
  })();

  const forceCpu = !!opts?.lowPower || saveData;
  let preferredBackend: PreviewBackend = "cpu";
  if (!forceCpu && webgpu) preferredBackend = "webgpu";
  else if (!forceCpu && webgl2) preferredBackend = "webgl";

  return { webgpu, webgl2, reducedMotion, saveData, preferredBackend };
}

/** Probe WebGPU adapter; returns false if request fails. */
export async function probeWebGpu(): Promise<boolean> {
  const nav = navigator as Navigator & {
    gpu?: { requestAdapter: () => Promise<unknown> };
  };
  if (!nav.gpu) return false;
  try {
    const adapter = await nav.gpu.requestAdapter();
    return !!adapter;
  } catch {
    return false;
  }
}
