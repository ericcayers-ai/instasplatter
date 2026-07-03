// Typed bridge to the Rust core.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface HardwareProfile {
  gpu_name: string;
  gpu_vendor: "nvidia" | "amd" | "intel" | "unknown";
  vram_mb: number;
  has_cuda: boolean;
  cpu_name: string;
  cpu_threads: number;
  ram_mb: number;
  auto_preset: PresetName;
}

export type PresetName = "draft" | "eco" | "balanced" | "high" | "max";

export interface Settings {
  preset?: string | null;
  maxFrames?: number | null;
  maxResolution?: number | null;
  blurRejectFraction?: number | null;
  matcher?: string | null;
  siftGpu?: boolean | null;
  totalSteps?: number | null;
  maxSplats?: number | null;
  shDegree?: number | null;
  refineEvery?: number | null;
  ssimWeight?: number | null;
  exportEvery?: number | null;
  strictness?: number | null;
  keepIntermediates?: boolean | null;
}

export interface ResolvedSettings {
  preset: PresetName;
  maxFrames: number;
  maxResolution: number;
  blurRejectFraction: number;
  matcher: string;
  siftGpu: boolean;
  totalSteps: number;
  maxSplats: number;
  shDegree: number;
  refineEvery: number;
  ssimWeight: number;
  exportEvery: number;
  strictness: number;
  keepIntermediates: boolean;
}

export interface EngineStatus {
  colmap: boolean;
  brush: boolean;
  ffmpeg: boolean;
}

export type JobEvent =
  | { kind: "stageStarted"; jobId: string; stage: string; label: string }
  | { kind: "stageProgress"; jobId: string; stage: string; progress: number; detail: string }
  | { kind: "log"; jobId: string; line: string }
  | { kind: "splatReady"; jobId: string; path: string; iter: number; totalSteps: number }
  | { kind: "done"; jobId: string; path: string; elapsedSecs: number }
  | { kind: "error"; jobId: string; message: string }
  | { kind: "cancelled"; jobId: string };

export interface EngineDownloadEvent {
  engine: string;
  downloaded: number;
  total: number;
  phase: "downloading" | "extracting" | "done";
}

export const api = {
  getHardwareProfile: () => invoke<HardwareProfile>("get_hardware_profile"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  getResolvedSettings: () => invoke<ResolvedSettings>("get_resolved_settings"),
  getEngineStatus: () => invoke<EngineStatus>("get_engine_status"),
  installEngines: () => invoke<EngineStatus>("install_engines"),
  startJob: (inputPath: string) => invoke<string>("start_job", { inputPath }),
  getAutostart: () => invoke<string | null>("get_autostart"),
  cancelJob: (jobId: string) => invoke<void>("cancel_job", { jobId }),
  exportSplat: (resultPath: string, destPath: string) =>
    invoke<void>("export_splat", { resultPath, destPath }),

  onJobEvent: (cb: (e: JobEvent) => void): Promise<UnlistenFn> =>
    listen<JobEvent>("job://event", (e) => cb(e.payload)),
  onEngineDownload: (cb: (e: EngineDownloadEvent) => void): Promise<UnlistenFn> =>
    listen<EngineDownloadEvent>("engine://download", (e) => cb(e.payload)),
};
