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

/** Top-level product suite (reconstruction splat workflow vs geospatial flood suite). */
export type Suite = "reconstruction" | "geospatial";

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
  /** Preferred shell suite. */
  defaultSuite?: Suite | null;
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
  depthAnything3: boolean;
  vggtCommercial: boolean;
  vggtOmega: boolean;
  mast3r: boolean;
  dust3r: boolean;
  mapanything: boolean;
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
  | { kind: "camerasReset"; jobId: string }
  | CameraRegistered
  | {
      kind: "ingestPreview";
      jobId: string;
      frameCount: number;
      path: [number, number, number][];
    }
  | { kind: "sparseCloudReady"; jobId: string; path: string; pointCount: number }
  | { kind: "denseCloudReady"; jobId: string; path: string; pointCount: number }
  | { kind: "meshReady"; jobId: string; path: string; triangleCount: number }
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
  suite?: Suite;
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

export type Mat3 = number[];

/** Geo splat TRS override (ENU metres). */
export interface ModelTransformDto {
  translation: [number, number, number];
  /** Row-major 3×3. */
  rotation: number[];
  scale: [number, number, number];
}

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
  suite?: Suite;
  lane?: "gpu" | "cpu";
}

export interface QueueSnapshot {
  items: QueueItem[];
  paused: boolean;
}

export interface GeoCatalogEntry {
  id: string;
  title: string;
  provider: string;
  format: string;
  license?: string | null;
  bounds?: {
    minX: number;
    minY: number;
    maxX: number;
    maxY: number;
  } | null;
  resolutionM?: number | null;
  url?: string | null;
  stale: boolean;
  localPath?: string | null;
  connectorId?: string | null;
  attribution?: string | null;
  notes?: string | null;
}

export interface GeoCatalogInfo {
  connectors: string[];
  entries?: GeoCatalogEntry[];
  formats: { id: string; label: string }[];
  exports: { id: string; label: string; worksOffline?: boolean }[];
}

export interface DemProduct {
  dtmPath?: string | null;
  dsmPath?: string | null;
  orthomosaicPath?: string | null;
  cellSizeM?: number | null;
  crs?: string | null;
  synthetic: boolean;
  notes: string[];
  aoiWgs84?: [number, number, number, number] | null;
  conditioned?: boolean;
  nodata?: number | null;
  previewGridPath?: string | null;
  /** Local Cesium terrain root when layer.json exists. */
  terrainTilesUrl?: string | null;
  /** real | synthetic | proxy */
  bedSource?: string | null;
}

export interface DemSampleGrid {
  cols: number;
  rows: number;
  bounds: [number, number, number, number];
  z: number[];
  synthetic: boolean;
  bedSource: string;
  notes: string[];
  sourcePath?: string | null;
}

export interface CatalogFetchOpts {
  aoiWgs84?: [number, number, number, number] | null;
  cellSizeM?: number | null;
  userFile?: string | null;
  apiKey?: string | null;
}

export interface FloodRunStatus {
  runId: string;
  scenarioId: string;
  workspace: string;
  state: string;
  progress: number;
  detail: string;
  mode?: string | null;
  engine?: string | null;
  engineVersion?: string | null;
  resultPaths: string[];
  massBalance?: number | null;
  label?: string | null;
  createdUnix: number;
}

export interface FloodEngineStatus {
  anugaLauncher: string | null;
  swmmLauncher: string | null;
  anugaReady: boolean;
  swmmReady: boolean;
  cpuLane: string;
  demoAvailable: boolean;
}

export interface ExtentPlanInput {
  cameraEnu?: [number, number, number][];
  splatBoundsEnu?: [number, number, number, number, number, number] | null;
  demBoundsEnu?: [number, number, number, number] | null;
  demAccuracyM?: number | null;
  previewBudgetCells?: number | null;
  enuOrigin?: [number, number, number] | null;
  geoReference?: GeoReference | null;
}

export interface ExtentPlan {
  workingCrs: string;
  enuOrigin: [number, number, number];
  boundsEnu: [number, number, number, number];
  extentDiagM: number;
  demResolutionM: number;
  previewCellM: number;
  scientificMeshMaxAreaM2: number;
  regionalMeshMaxAreaM2: number;
  terrainTileLevels: number[];
  scaleStatus: string;
  notes: string[];
}

export interface GeoReference {
  sourceCrs?: string | null;
  verticalDatum?: string | null;
  units?: string | null;
  workingCrs?: string | null;
  ecefToEnu?: number[] | null;
  enuToEcef?: number[] | null;
  localOrigin?: [number, number, number] | null;
  localOriginEcef?: [number, number, number] | null;
  uncertaintyM?: number | null;
  gcpResidualM?: number | null;
  gcpResidualMaxM?: number | null;
  provenance?: string | null;
  scaleStatus?: string | null;
}

export interface GcpPoint {
  id: string;
  surveyXyz: [number, number, number];
  surveyCrs?: string;
  localXyz?: [number, number, number] | null;
  imageName?: string | null;
  pixelUv?: [number, number] | null;
  covarianceM?: [number, number, number] | null;
  outlier?: boolean;
}

export interface GcpResidualReport {
  scale: number;
  rotation: number[][];
  translation: [number, number, number];
  meanResidualM: number;
  maxResidualM: number;
  rmsResidualM: number;
  inlierIds: string[];
  outlierIds: string[];
  perPointM: [string, number][];
}

export interface RegistrationResult {
  geoReference: GeoReference;
  cameraCount: number;
  telemetryCount: number;
  matchedFrames: number;
  warnings: string[];
  posePriorsPath?: string | null;
}

export interface FloodScenarioDto {
  id: string;
  name: string;
  aoiWgs84?: [number, number, number, number] | null;
  validationState?: string | null;
  solverSettings?: Record<string, unknown> | null;
}

export interface CommitFloodAoiResult {
  scenario: FloodScenarioDto;
  extentPlan: ExtentPlan;
  geoReference?: GeoReference | null;
}

export interface FloodExportArtifact {
  kind: string;
  path: string;
  format: string;
  writer: string;
  notes: string[];
}

export interface FloodExportResult {
  exportDir: string;
  runId: string;
  mode?: string | null;
  authoritative: boolean;
  gdal: {
    available: boolean;
    gdalTranslate?: string | null;
    ogr2ogr?: string | null;
    pythonOsgeo: boolean;
    notes: string[];
  };
  artifacts: FloodExportArtifact[];
  manifestPath: string;
}

export interface LayerExportResult {
  kind: string;
  path: string;
  writer: string;
  notes: string[];
}

export type GeoEvent =
  | { kind: "layerAdded"; workspace: string; layerId: string; name: string }
  | { kind: "scenarioUpdated"; workspace: string; scenarioId: string }
  | { kind: "runProgress"; runId: string; progress: number; detail: string }
  | {
      kind: "runDone";
      runId: string;
      resultPaths: string[];
      mode?: string;
      massBalance?: number;
    }
  | { kind: "runCancelled"; runId: string }
  | { kind: "engineMissing"; engine: string; message: string; demoAvailable: boolean }
  | { kind: "error"; message: string; runId?: string };

export type SimEvent =
  | {
      kind: "checkpoint";
      runId: string;
      progress: number;
      simTimeHours: number;
      checkpointPath?: string | null;
      detail: string;
      mode: string;
      maxDepthM?: number | null;
      wetFraction?: number | null;
      massM3?: number | null;
    }
  | { kind: "hydrograph"; runId: string; path: string }
  | {
      kind: "done";
      runId: string;
      mode: string;
      resultPaths: string[];
      massBalance?: number | null;
      label?: string | null;
    };

export interface SchematicExportResult {
  path: string;
  width: number;
  height: number;
  length: number;
  occupied: number;
  paletteSize: number;
  metresPerBlock: number;
}

export const api = {
  getHardwareProfile: () => invoke<HardwareProfile>("get_hardware_profile"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  getResolvedSettings: () => invoke<ResolvedSettings>("get_resolved_settings"),
  getEngineStatus: () => invoke<EngineStatus>("get_engine_status"),
  installEngines: () => invoke<EngineStatus>("install_engines"),
  getAutostart: () => invoke<string | null>("get_autostart"),

  getSuite: () => invoke<Suite>("get_suite"),
  setSuite: (suite: Suite) => invoke<Suite>("set_suite", { suite }),
  getGeoCatalogInfo: () => invoke<GeoCatalogInfo>("get_geo_catalog_info"),
  listGeoCatalog: (workspace?: string | null, aoiWgs84?: [number, number, number, number] | null) =>
    invoke<GeoCatalogEntry[]>("list_geo_catalog", {
      workspace: workspace ?? null,
      aoiWgs84: aoiWgs84 ?? null,
    }),
  fetchGeoCatalogAsset: (
    workspace: string,
    entryId: string,
    opts?: CatalogFetchOpts,
  ) =>
    invoke<GeoCatalogEntry>("fetch_geo_catalog_asset", {
      workspace,
      entryId,
      aoiWgs84: opts?.aoiWgs84 ?? null,
      cellSizeM: opts?.cellSizeM ?? null,
      userFile: opts?.userFile ?? null,
      apiKey: opts?.apiKey ?? null,
    }),
  prepareGeoDem: (
    workspace: string,
    opts?: {
      sourcePath?: string | null;
      aoiWgs84?: [number, number, number, number] | null;
      cellSizeM?: number | null;
      crs?: string | null;
      nodata?: number | null;
    },
  ) =>
    invoke<DemProduct>("prepare_geo_dem", {
      workspace,
      sourcePath: opts?.sourcePath ?? null,
      aoiWgs84: opts?.aoiWgs84 ?? null,
      cellSizeM: opts?.cellSizeM ?? null,
      crs: opts?.crs ?? null,
      nodata: opts?.nodata ?? null,
    }),
  sampleGeoDem: (
    workspace: string,
    cols: number,
    rows: number,
    aoiWgs84?: [number, number, number, number] | null,
  ) =>
    invoke<DemSampleGrid>("sample_geo_dem", {
      workspace,
      cols,
      rows,
      aoiWgs84: aoiWgs84 ?? null,
    }),

  getFloodEngineStatus: () => invoke<FloodEngineStatus>("get_flood_engine_status"),
  planGeoExtent: (input: ExtentPlanInput) => invoke<ExtentPlan>("plan_geo_extent", { input }),
  computeGeoReference: (workspace: string, originLonLatH?: [number, number, number] | null) =>
    invoke<RegistrationResult>("compute_geo_reference", {
      workspace,
      originLonLatH: originLonLatH ?? null,
    }),
  importGeoTelemetry: (workspace: string, paths: string[]) =>
    invoke<RegistrationResult>("import_geo_telemetry", { workspace, paths }),
  setGeoGcps: (workspace: string, gcps: GcpPoint[], refine?: boolean) =>
    invoke<{ geoReference: GeoReference; residualReport: GcpResidualReport }>("set_geo_gcps", {
      workspace,
      gcps,
      refine: refine ?? false,
    }),
  getGeoReference: (workspace: string) =>
    invoke<GeoReference | null>("get_geo_reference", { workspace }),
  commitFloodAoi: (workspace: string, scenarioId: string, aoiWgs84: [number, number, number, number]) =>
    invoke<CommitFloodAoiResult>("commit_flood_aoi", {
      workspace,
      scenarioId,
      aoiWgs84,
    }),
  startScientificFlood: (
    workspace: string,
    scenarioId: string,
    opts?: { allowDemo?: boolean; demPath?: string | null; enableSwmm?: boolean },
  ) =>
    invoke<FloodRunStatus>("start_scientific_flood", {
      workspace,
      scenarioId,
      allowDemo: opts?.allowDemo ?? true,
      demPath: opts?.demPath ?? null,
      enableSwmm: opts?.enableSwmm ?? false,
    }),
  cancelScientificFlood: (runId: string) => invoke<void>("cancel_scientific_flood", { runId }),
  listFloodRunStatus: (workspace?: string | null) =>
    invoke<FloodRunStatus[]>("list_flood_run_status", { workspace: workspace ?? null }),

  exportFloodProducts: (workspace: string, runId?: string | null) =>
    invoke<FloodExportResult>("export_flood_products", {
      workspace,
      runId: runId ?? null,
    }),

  exportGeoLayer: (
    workspace: string,
    kind: string,
    opts?: { runId?: string | null; destPath?: string | null },
  ) =>
    invoke<LayerExportResult>("export_geo_layer", {
      workspace,
      kind,
      runId: opts?.runId ?? null,
      destPath: opts?.destPath ?? null,
    }),

  startJob: (inputPath: string) => invoke<string>("start_job", { inputPath }),
  cancelJob: (jobId: string) => invoke<void>("cancel_job", { jobId }),
  enqueueJobs: (paths: string[], suite?: Suite) =>
    invoke<string[]>("enqueue_jobs", { paths, suite: suite ?? null }),
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
  saveModelTransform: (workspace: string, transform: ModelTransformDto) =>
    invoke<void>("save_model_transform", { workspace, transform }),
  getModelTransform: (workspace: string) =>
    invoke<ModelTransformDto | null>("get_model_transform", { workspace }),

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

  /**
   * Experimental: voxelize the finished splat into a Sponge Schematic v2 `.schem`.
   * Requires Experimental Mode. Destination must end in `.schem`.
   */
  exportMinecraftSchematic: (
    resultPath: string,
    destPath: string,
    opts?: {
      workspace?: string | null;
      rotation?: Mat3 | null;
      maxExtent?: number | null;
      opacityMin?: number | null;
    },
  ) =>
    invoke<SchematicExportResult>("export_minecraft_schematic", {
      resultPath,
      destPath,
      workspace: opts?.workspace ?? null,
      rotation: opts?.rotation ?? null,
      maxExtent: opts?.maxExtent ?? null,
      opacityMin: opts?.opacityMin ?? null,
    }),

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
  onGeoEvent: (cb: (e: GeoEvent) => void): Promise<UnlistenFn> =>
    listen<GeoEvent>("geo://event", (e) => cb(e.payload)),
  onSimEvent: (cb: (e: SimEvent) => void): Promise<UnlistenFn> =>
    listen<SimEvent>("sim://event", (e) => cb(e.payload)),
};
