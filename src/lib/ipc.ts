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

/** A splat file format the exporter can write. */
export type SplatFormat = "ply" | "splat" | "spz";

/** A mesh file format the exporter can write. */
export type MeshFormat = "glb" | "obj" | "ply";

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
  /** Train at reduced resolution first, then raise it. Default ON. */
  progressiveResolution?: boolean | null;
  /** Bound Gaussian size to the sampling rate (Mip-Splatting). Default ON. */
  mipFilter?: boolean | null;
  /** Register cameras incrementally instead of a blocking COLMAP pass. */
  liveInit?: boolean | null;
  /** Dense MVS / neural pointmap seeding before training. Default ON. */
  denseInit?: boolean | null;
  /** Use installed DAV2 / VGGT-commercial sidecars when present. Default ON. */
  useNeuralInit?: boolean | null;
  /** Allow non-commercial research sidecars. Mirror of Experimental Mode (do not set alone). */
  allowResearchSidecars?: boolean | null;
  /** Master Experimental Mode (NC research stack). Requires license ack. */
  experimentalMode?: boolean | null;
  /** User accepted NC research license modal. */
  experimentalLicenseAcked?: boolean | null;
  /** Run NVIDIA Fixer (or Difix if Experimental) after train when installed. Default ON. */
  postPolish?: boolean | null;
  /** Trainer: auto | brush | gsplat. Auto prefers gsplat on CUDA when installed. */
  trainer?: string | null;
  gsplatStrategy?: string | null;
  gsplatAbsgrad?: boolean | null;
  gsplatAntialiased?: boolean | null;
  gsplatAppearance?: boolean | null;
  gsplatBilateralGrid?: boolean | null;
  exportFormat?: string | null;
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
  progressiveResolution: boolean;
  mipFilter: boolean;
  liveInit: boolean;
  denseInit: boolean;
  useNeuralInit: boolean;
  allowResearchSidecars: boolean;
  experimentalMode: boolean;
  experimentalLicenseAcked: boolean;
  postPolish: boolean;
  trainer: string;
  gsplatStrategy: string;
  gsplatAbsgrad: boolean;
  gsplatAntialiased: boolean;
  gsplatAppearance: boolean;
  gsplatBilateralGrid: boolean;
  romaQuality: string;
  exportFormat: string;
}

export interface EngineStatus {
  colmap: boolean;
  brush: boolean;
  brushCustom: boolean;
  ffmpeg: boolean;
  depthAnythingV2: boolean;
  vggtCommercial: boolean;
  vggtOmega: boolean;
  mast3r: boolean;
  dust3r: boolean;
  romaV2: boolean;
  fixer: boolean;
  difix: boolean;
  gsplat: boolean;
}

/** A camera solved by the live-init engine, ready to draw as a frustum. */
export interface CameraRegistered {
  kind: "cameraRegistered";
  jobId: string;
  name: string;
  registered: number;
  total: number;
  /** Share of matched features that survived pose estimation, 0 to 1. */
  confidence: number;
  apex: [number, number, number];
  corners: [
    [number, number, number],
    [number, number, number],
    [number, number, number],
    [number, number, number],
  ];
}

export type JobEvent =
  | { kind: "stageStarted"; jobId: string; stage: string; label: string }
  | { kind: "stageProgress"; jobId: string; stage: string; progress: number; detail: string }
  | { kind: "log"; jobId: string; line: string }
  | CameraRegistered
  /** Something the user should know that is not a failure. */
  | { kind: "notice"; jobId: string; message: string }
  | { kind: "splatReady"; jobId: string; path: string; iter: number; totalSteps: number }
  | { kind: "done"; jobId: string; path: string; elapsedSecs: number }
  | { kind: "error"; jobId: string; message: string }
  | { kind: "cancelled"; jobId: string };

export interface EngineDownloadEvent {
  engine: string;
  downloaded: number;
  total: number;
  phase: "downloading" | "verifying" | "extracting" | "done";
}

export interface MeshProgressEvent {
  progress: number;
  detail: string;
}

/** A saved reconstruction, as shown in the reopen list. */
export interface ProjectSummary {
  jobId: string;
  workspace: string;
  inputName: string;
  updatedUnix: number;
  completed: boolean;
  resumable: boolean;
  latestIter: number;
  totalSteps: number;
  resultPath: string | null;
}

export interface GroundPlane {
  normal: [number, number, number];
  /** The signed axis the normal is closest to, such as "+y" or "-z". */
  nearestAxis: string;
  /** Row-major 3x3 taking the ground normal onto the requested up axis. */
  rotation: number[];
}

export interface FormatChoice {
  extension: string;
  label: string;
}

/** Row-major 3x3. */
export type Mat3 = number[];

export interface QueueItem {
  id: string;
  inputPath: string;
  displayName: string;
  state: "queued" | "running" | "paused" | "done" | "failed" | "cancelled";
  jobId: string | null;
  workspace: string | null;
  error: string | null;
  progress: number;
  detail: string;
}

export interface QueueSnapshot {
  items: QueueItem[];
  paused: boolean;
}

export const api = {
  getHardwareProfile: () => invoke<HardwareProfile>("get_hardware_profile"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  getResolvedSettings: () => invoke<ResolvedSettings>("get_resolved_settings"),
  getEngineStatus: () => invoke<EngineStatus>("get_engine_status"),
  installEngines: () => invoke<EngineStatus>("install_engines"),
  getAutostart: () => invoke<string | null>("get_autostart"),

  startJob: (inputPath: string) => invoke<string>("start_job", { inputPath }),
  cancelJob: (jobId: string) => invoke<void>("cancel_job", { jobId }),
  enqueueJobs: (paths: string[]) => invoke<string[]>("enqueue_jobs", { paths }),
  listQueue: () => invoke<QueueSnapshot>("list_queue"),
  pauseQueue: (paused: boolean) => invoke<void>("pause_queue", { paused }),
  resumeQueue: () => invoke<void>("resume_queue"),
  cancelQueueItem: (id: string) => invoke<void>("cancel_queue_item", { id }),
  clearFinishedQueue: () => invoke<void>("clear_finished_queue"),

  listProjects: () => invoke<ProjectSummary[]>("list_projects"),
  resumeProject: (workspace: string) => invoke<string>("resume_project", { workspace }),
  deleteProject: (workspace: string) => invoke<void>("delete_project", { workspace }),
  saveProjectOrientation: (workspace: string, rotation: Mat3) =>
    invoke<void>("save_project_orientation", { workspace, rotation }),

  /** The ground plane of a splat, and the rotation that stands it upright. */
  estimateUpAxis: (splatPath: string, target?: string) =>
    invoke<GroundPlane | null>("estimate_up_axis", { splatPath, target: target ?? null }),

  listExportFormats: () => invoke<[FormatChoice[], FormatChoice[]]>("list_export_formats"),

  /**
   * The destination extension picks the format. When `rotation` is omitted,
   * the orientation last saved for `workspace` (if any) is used instead.
   */
  exportSplat: (resultPath: string, destPath: string, workspace?: string | null, rotation?: Mat3 | null) =>
    invoke<void>("export_splat", { resultPath, destPath, workspace: workspace ?? null, rotation: rotation ?? null }),

  /** Returns the triangle count. Long running; listen to `onMeshProgress`. */
  exportMesh: (
    workspace: string,
    splatPath: string,
    destPath: string,
    opts?: { resolution?: number; textured?: boolean; quality?: string },
  ) =>
    invoke<number>("export_mesh", {
      workspace,
      splatPath,
      destPath,
      resolution: opts?.resolution ?? null,
      textured: opts?.textured ?? null,
      quality: opts?.quality ?? null,
    }),

  /** Writes hardware, settings, engine and project state plus recent logs. */
  exportDiagnostics: (workspace: string | null, recentLogs: string[], destPath: string) =>
    invoke<void>("export_diagnostics", { workspace, recentLogs, destPath }),

  onJobEvent: (cb: (e: JobEvent) => void): Promise<UnlistenFn> =>
    listen<JobEvent>("job://event", (e) => cb(e.payload)),
  onEngineDownload: (cb: (e: EngineDownloadEvent) => void): Promise<UnlistenFn> =>
    listen<EngineDownloadEvent>("engine://download", (e) => cb(e.payload)),
  onMeshProgress: (cb: (e: MeshProgressEvent) => void): Promise<UnlistenFn> =>
    listen<MeshProgressEvent>("mesh://progress", (e) => cb(e.payload)),
  onQueueSnapshot: (cb: (e: QueueSnapshot) => void): Promise<UnlistenFn> =>
    listen<QueueSnapshot>("queue://snapshot", (e) => cb(e.payload)),
};
