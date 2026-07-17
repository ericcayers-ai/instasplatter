import { create } from "zustand";
import { save } from "@tauri-apps/plugin-dialog";
import type {
  CameraRegistered,
  DemProduct,
  DemSampleGrid,
  EngineStatus,
  FloodEngineStatus,
  GeoEvent,
  HardwareProfile,
  JobEvent,
  ProjectSummary,
  QueueItem,
  ResolvedSettings,
  Settings,
  SimEvent,
  SplatFormat,
  Suite,
} from "../lib/ipc";
import { api } from "../lib/ipc";
import { DEFAULT_GEO_LAYERS, PLACEHOLDER_SCENARIO } from "../geospatial/defaults";
import { aoiIsValid, aoiToEnuBox, domainFromAoi, normalizeAoi, type AoiWgs84 } from "../geospatial/aoi";
import {
  identityModelTransform,
  normalizeModelTransform,
  type ModelTransform,
} from "../geospatial/modelTransform";
import { setLocalDemTerrainUrl } from "../geospatial/globe/terrain";
import type {
  GeoBasemapMode,
  GeoLayer,
  GeoPreviewRuntime,
  GeoScenarioMeta,
  GeoScientificRun,
  GeoTool,
  GeoViewMode,
  GeoWaterStyle,
} from "../geospatial/types";
import type { ExtentPlan } from "../lib/ipc";
import { convertFileSrc } from "@tauri-apps/api/core";

export type Screen = "home" | "processing";
export type ThemePreference = "system" | "light" | "dark";
export type Theme = "light" | "dark";
export type { Suite };

export interface StageInfo {
  id: string;
  label: string;
  progress: number;
  detail: string;
  state: "pending" | "active" | "done";
}

export interface LogLine {
  time: number;
  line: string;
}

const STAGE_ORDER = ["ingest", "sfm", "train", "finalize"];
const STAGE_LABELS: Record<string, string> = {
  ingest: "Frames",
  sfm: "Cameras",
  train: "Splatting",
  finalize: "Finalize",
};

const THEME_KEY = "instasplatter:theme";
const LEFT_PANEL_KEY = "instasplatter:leftPanel";
const RIGHT_PANEL_KEY = "instasplatter:rightPanel";
const LOG_CONSOLE_KEY = "instasplatter:logConsole";

function readBool(key: string, fallback: boolean): boolean {
  const v = localStorage.getItem(key);
  return v === null ? fallback : v === "1";
}

function writeBool(key: string, value: boolean) {
  localStorage.setItem(key, value ? "1" : "0");
}

function systemPrefersDark(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? true;
}

function resolveTheme(pref: ThemePreference): Theme {
  if (pref === "system") return systemPrefersDark() ? "dark" : "light";
  return pref;
}

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

interface AppStore {
  screen: Screen;
  suite: Suite;
  profile: HardwareProfile | null;
  engineStatus: EngineStatus | null;
  settings: Settings;
  resolved: ResolvedSettings | null;
  /** The settings a running job actually started with, for "applies on next run" hints. */
  jobSettingsSnapshot: ResolvedSettings | null;

  themePreference: ThemePreference;
  theme: Theme;
  leftPanelOpen: boolean;
  rightPanelOpen: boolean;
  logConsoleOpen: boolean;

  recentProjects: ProjectSummary[];
  queueItems: QueueItem[];
  queuePaused: boolean;

  jobId: string | null;
  inputPath: string | null;
  /** Directory holding the project manifest, poses and frames. */
  workspace: string | null;
  stages: StageInfo[];
  logs: LogLine[];
  /** Plain statements the pipeline made that are not failures. */
  notices: string[];
  clearNotices: () => void;
  /** Status chips emitted by the pipeline (Cameras / Init / Polish / Trainer / Flood / Export). */
  pipelineChips: {
    cameras?: string;
    init?: string;
    polish?: string;
    trainer?: string;
    flood?: string;
    export?: string;
  };
  /** About / implementations panel. */
  aboutOpen: boolean;
  /** Cameras the live-init engine has solved, in registration order. */
  cameras: CameraRegistered[];
  registeredCameras: number;
  totalCameras: number;
  /** Confidence of the most recently registered camera, 0 to 1. */
  trackingConfidence: number;
  latestSplatPath: string | null;
  latestIter: number;
  totalSteps: number;
  splatCount: number;
  fps: number;
  resultPath: string | null;
  jobError: string | null;
  jobStartedAt: number;
  elapsedSecs: number | null;
  meshStatus: string | null;
  /** Live recon stage layers (paths + visibility). */
  sparseCloudPath: string | null;
  denseCloudPath: string | null;
  latestMeshPath: string | null;
  sparsePointCount: number;
  densePointCount: number;
  ingestFrameCount: number;
  ingestPath: [number, number, number][];
  reconLayers: {
    cameras: boolean;
    cameraPath: boolean;
    sparse: boolean;
    dense: boolean;
    splat: boolean;
    mesh: boolean;
  };
  /** First-enable Experimental Mode license modal. */
  experimentalModalOpen: boolean;

  /** Geospatial suite: layer tree, flood scrub, view tools. */
  geoLayers: GeoLayer[];
  /** Normalised scenario time 0–1 (linked to hydrograph). */
  geoFloodTime: number;
  /** Live preview playhead. */
  geoFloodPlaying: boolean;
  /** Prefer coarser grid / no particles. */
  geoFloodLowPower: boolean;
  geoScenario: GeoScenarioMeta | null;
  geoViewMode: GeoViewMode;
  geoWaterStyle: GeoWaterStyle;
  geoTool: GeoTool;
  geoInspectHint: string | null;
  /** Latest live-preview runtime stats (null until engine mounts). */
  geoPreview: GeoPreviewRuntime | null;
  /** Scientific ANUGA / demo run status (null when idle). */
  geoScientificRun: GeoScientificRun | null;
  /** Flood engine discovery (ANUGA/SWMM launchers). */
  geoFloodEngine: FloodEngineStatus | null;
  /** Latest scientific checkpoint GeoJSON for the depth layer (WGS84 or local). */
  geoScientificExtent: GeoJSON.FeatureCollection | null;
  /** Committed AOI (WGS84). Null until the user draws/edits a box. */
  geoAoiWgs84: AoiWgs84 | null;
  /** Esri satellite (default) or low-bandwidth Carto/OSM. */
  geoBasemapMode: GeoBasemapMode;
  /** Last extent plan from AOI commit (preview cell size etc.). */
  geoExtentPlan: ExtentPlan | null;
  /** Bumped when AOI is committed so the map engine can rebind. */
  geoAoiRevision: number;
  /** Manual splat TRS in ENU (persisted as modelTransform on project). */
  geoModelTransform: ModelTransform;
  /** Staged DEM product (synthetic or real). */
  geoDemProduct: DemProduct | null;
  /** Soft-preview / HAND bed samples from staged DEM. */
  geoDemSample: DemSampleGrid | null;
  /** Catalog overlay GeoJSON paths keyed by layer id (nfhl, hydrosheds, gauges, waterways). */
  geoOverlayPaths: Record<string, string>;
  /** Bumped when DEM bed / overlays change so map engines can refresh. */
  geoDemRevision: number;

  init: () => Promise<void>;
  setSuite: (suite: Suite) => Promise<void>;
  setGeoLayerVisible: (id: string, visible: boolean) => void;
  setGeoLayerOpacity: (id: string, opacity: number) => void;
  setGeoFloodTime: (t01: number) => void;
  setGeoFloodPlaying: (playing: boolean) => void;
  toggleGeoFloodPlaying: () => void;
  setGeoFloodLowPower: (on: boolean) => void;
  setGeoPreview: (runtime: GeoPreviewRuntime | null) => void;
  setGeoBasemapMode: (mode: GeoBasemapMode) => void;
  /** Draw/edit commit: persist AOI, plan extent, rebind soft-solver domain. */
  commitGeoAoi: (aoi: AoiWgs84) => Promise<void>;
  clearGeoAoi: () => void;
  /** Stage DEM + sample bed for soft/HAND; wire Globe terrain URL when tiles exist. */
  prepareGeoDemForAoi: () => Promise<void>;
  /** Fetch catalog overlay (nfhl / hydrosheds / gauges / osm-waterways) into layer tree. */
  fetchGeoOverlayLayer: (layerId: string) => Promise<void>;
  startScientificFlood: (opts?: { allowDemo?: boolean; enableSwmm?: boolean }) => Promise<void>;
  cancelScientificFlood: () => Promise<void>;
  exportFloodProducts: () => Promise<void>;
  handleGeoEvent: (e: GeoEvent) => void;
  handleSimEvent: (e: SimEvent) => void;
  setGeoViewMode: (mode: GeoViewMode) => void;
  setGeoWaterStyle: (style: GeoWaterStyle) => void;
  setGeoTool: (tool: GeoTool) => void;
  setGeoInspectHint: (hint: string | null) => void;
  setGeoModelTransform: (tf: ModelTransform) => void;
  persistGeoModelTransform: () => Promise<void>;
  setThemePreference: (p: ThemePreference) => void;
  setLeftPanelOpen: (open: boolean) => void;
  setRightPanelOpen: (open: boolean) => void;
  toggleRightPanel: () => void;
  setLogConsoleOpen: (open: boolean) => void;
  setAboutOpen: (open: boolean) => void;
  updateSettings: (patch: Partial<Settings>) => Promise<void>;
  requestExperimental: () => void;
  acceptExperimental: () => Promise<void>;
  declineExperimental: () => void;
  refreshProjects: () => Promise<void>;
  resumeProject: (workspace: string) => Promise<void>;
  deleteProjectEntry: (workspace: string) => Promise<void>;
  startJob: (path: string) => Promise<void>;
  enqueueJobs: (paths: string[], suite?: Suite) => Promise<void>;
  pauseQueue: () => Promise<void>;
  resumeQueue: () => Promise<void>;
  cancelQueueItem: (id: string) => Promise<void>;
  clearFinishedQueue: () => Promise<void>;
  cancelJob: () => Promise<void>;
  backHome: () => void;
  handleJobEvent: (e: JobEvent) => void;
  setSplatCount: (n: number) => void;
  setFps: (n: number) => void;
  setReconLayer: (id: keyof AppStore["reconLayers"], visible: boolean) => void;
  exportSplatAction: (rotation?: number[] | null) => Promise<void>;
  exportMeshAction: () => Promise<void>;
  /** Experimental Mode: splat → Sponge Schematic v2 `.schem`. */
  exportSchematicAction: (rotation?: number[] | null) => Promise<void>;
  exportDiagnosticsAction: () => Promise<void>;
}

function freshStages(): StageInfo[] {
  return STAGE_ORDER.map((id) => ({
    id,
    label: STAGE_LABELS[id],
    progress: 0,
    detail: "",
    state: "pending",
  }));
}

let initStarted = false;

export const useStore = create<AppStore>((set, get) => ({
  screen: "home",
  suite: "reconstruction",
  profile: null,
  engineStatus: null,
  settings: {},
  resolved: null,
  jobSettingsSnapshot: null,

  themePreference: "system",
  theme: "dark",
  leftPanelOpen: readBool(LEFT_PANEL_KEY, true),
  rightPanelOpen: readBool(RIGHT_PANEL_KEY, false),
  logConsoleOpen: readBool(LOG_CONSOLE_KEY, false),

  recentProjects: [],
  queueItems: [],
  queuePaused: false,

  jobId: null,
  inputPath: null,
  workspace: null,
  stages: freshStages(),
  logs: [],
  notices: [],
  clearNotices: () => set({ notices: [] }),
  pipelineChips: {},
  cameras: [],
  registeredCameras: 0,
  totalCameras: 0,
  trackingConfidence: 0,
  latestSplatPath: null,
  latestIter: 0,
  totalSteps: 0,
  splatCount: 0,
  fps: 0,
  resultPath: null,
  jobError: null,
  jobStartedAt: 0,
  elapsedSecs: null,
  meshStatus: null,
  sparseCloudPath: null,
  denseCloudPath: null,
  latestMeshPath: null,
  sparsePointCount: 0,
  densePointCount: 0,
  ingestFrameCount: 0,
  ingestPath: [],
  reconLayers: {
    cameras: true,
    cameraPath: true,
    sparse: true,
    dense: true,
    splat: true,
    mesh: true,
  },
  experimentalModalOpen: false,
  aboutOpen: false,

  geoLayers: DEFAULT_GEO_LAYERS.map((l) => ({ ...l })),
  geoFloodTime: 0.35,
  geoFloodPlaying: false,
  geoFloodLowPower: false,
  geoScenario: PLACEHOLDER_SCENARIO,
  geoViewMode: "3d",
  geoWaterStyle: "depth",
  geoTool: "pan",
  geoInspectHint: null,
  geoPreview: null,
  geoScientificRun: null,
  geoFloodEngine: null,
  geoScientificExtent: null,
  geoAoiWgs84: null,
  geoBasemapMode: "satellite",
  geoExtentPlan: null,
  geoAoiRevision: 0,
  geoModelTransform: identityModelTransform(),
  geoDemProduct: null,
  geoDemSample: null,
  geoOverlayPaths: {},
  geoDemRevision: 0,

  init: async () => {
    // React StrictMode double-invokes effects in dev; init exactly once.
    if (initStarted) return;
    initStarted = true;

    const storedTheme = localStorage.getItem(THEME_KEY) as ThemePreference | null;
    const pref = storedTheme ?? "system";
    const theme = resolveTheme(pref);
    applyTheme(theme);
    set({ themePreference: pref, theme });
    window.matchMedia?.("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (get().themePreference === "system") {
        const t = resolveTheme("system");
        applyTheme(t);
        set({ theme: t });
      }
    });

    const [profile, settings, resolved, engineStatus, suite] = await Promise.all([
      api.getHardwareProfile(),
      api.getSettings(),
      api.getResolvedSettings(),
      api.getEngineStatus(),
      api.getSuite().catch(() => "reconstruction" as Suite),
    ]);
    document.documentElement.dataset.suite = suite;
    set({ profile, settings, resolved, suite });
    void get().refreshProjects();
    // Engine status can change once install_engines finishes, so events that
    // affect it are not tracked live; a plain re-read on init is enough here.
    set({ engineStatus });
    void api.getFloodEngineStatus().then((geoFloodEngine) => set({ geoFloodEngine })).catch(() => {});
    await api.onJobEvent((e) => get().handleJobEvent(e));
    await api.onGeoEvent((e) => get().handleGeoEvent(e));
    await api.onSimEvent((e) => get().handleSimEvent(e));
    await api.onQueueSnapshot((snap) => {
      set({ queueItems: snap.items, queuePaused: snap.paused });
      const running = snap.items.find((i) => i.state === "running");
      if (
        running?.jobId &&
        get().jobId !== running.jobId &&
        get().screen === "home" &&
        (running.suite ?? "reconstruction") === "reconstruction"
      ) {
        // Promote the active batch item into the processing screen.
        set({
          screen: "processing",
          jobId: running.jobId,
          inputPath: running.inputPath,
          workspace: running.workspace,
          stages: freshStages(),
          logs: [],
          notices: [],
          pipelineChips: {},
          cameras: [],
          jobStartedAt: Date.now(),
          jobSettingsSnapshot: get().resolved,
        });
      }
    });
    const snap = await api.listQueue().catch(() => null);
    if (snap) set({ queueItems: snap.items, queuePaused: snap.paused });
    // Dev/test hook: start a job immediately if requested via env var.
    const auto = await api.getAutostart().catch(() => null);
    if (auto && !get().jobId) void get().startJob(auto);
  },

  setSuite: async (suite) => {
    const next = await api.setSuite(suite);
    document.documentElement.dataset.suite = next;
    set({
      suite: next,
      settings: { ...get().settings, defaultSuite: next },
      geoInspectHint: null,
      geoTool: "pan",
    });
  },

  setGeoLayerVisible: (id, visible) => {
    set({
      geoLayers: get().geoLayers.map((l) => (l.id === id ? { ...l, visible } : l)),
    });
  },

  setGeoLayerOpacity: (id, opacity) => {
    const o = Math.max(0, Math.min(1, opacity));
    set({
      geoLayers: get().geoLayers.map((l) => (l.id === id ? { ...l, opacity: o } : l)),
    });
  },

  setGeoFloodTime: (t01) => {
    set({ geoFloodTime: Math.max(0, Math.min(1, t01)) });
  },

  setGeoFloodPlaying: (playing) => set({ geoFloodPlaying: playing }),

  toggleGeoFloodPlaying: () => {
    const playing = !get().geoFloodPlaying;
    // Restart from beginning when play hits the end.
    if (playing && get().geoFloodTime >= 0.999) {
      set({ geoFloodPlaying: true, geoFloodTime: 0 });
      return;
    }
    set({ geoFloodPlaying: playing });
  },

  setGeoFloodLowPower: (on) => set({ geoFloodLowPower: on }),

  setGeoPreview: (runtime) => set({ geoPreview: runtime }),

  setGeoBasemapMode: (mode) => set({ geoBasemapMode: mode }),

  clearGeoAoi: () => {
    setLocalDemTerrainUrl(null);
    set({
      geoAoiWgs84: null,
      geoExtentPlan: null,
      geoAoiRevision: get().geoAoiRevision + 1,
      geoDemProduct: null,
      geoDemSample: null,
      geoDemRevision: get().geoDemRevision + 1,
      geoScenario: {
        ...(get().geoScenario ?? PLACEHOLDER_SCENARIO),
        aoiWgs84: null,
      },
      geoInspectHint: "Draw an AOI to bind the flood domain",
    });
  },

  commitGeoAoi: async (raw) => {
    if (!aoiIsValid(raw)) {
      set({ geoInspectHint: "AOI must be a non-empty rectangle" });
      return;
    }
    const aoi = normalizeAoi(raw);
    const domain = domainFromAoi(aoi, get().geoFloodLowPower);
    const { origin, demBoundsEnu } = aoiToEnuBox(aoi);
    let extentPlan: ExtentPlan | null = null;
    let persistNote = "AOI set (local preview)";

    try {
      extentPlan = await api.planGeoExtent({
        cameraEnu: [],
        demBoundsEnu,
        demAccuracyM: 2,
        previewBudgetCells: 1024,
        enuOrigin: origin,
      });
      if (extentPlan?.previewCellM && extentPlan.previewCellM > 0) {
        domain.dxM = Math.max(domain.dxM, extentPlan.previewCellM);
      }
    } catch {
      // Planner is pure; if invoke fails (tests / missing IPC), keep local domain.
    }

    let workspace = get().workspace;
    if (!workspace) {
      const geo = get().queueItems.find(
        (i) => (i.suite ?? "reconstruction") === "geospatial" && i.workspace,
      );
      workspace = geo?.workspace ?? null;
    }

    const scenarioId = get().geoScenario?.id ?? PLACEHOLDER_SCENARIO.id;
    if (workspace) {
      try {
        const result = await api.commitFloodAoi(workspace, scenarioId, aoi);
        extentPlan = result.extentPlan ?? extentPlan;
        persistNote = "AOI saved on project";
        set({ workspace });
      } catch (err) {
        persistNote = `AOI local only (${String(err)})`;
      }
    }

    set({
      geoAoiWgs84: aoi,
      geoExtentPlan: extentPlan,
      geoAoiRevision: get().geoAoiRevision + 1,
      geoScenario: {
        ...(get().geoScenario ?? PLACEHOLDER_SCENARIO),
        aoiWgs84: aoi,
      },
      geoTool: "pan",
      geoInspectHint: `${persistNote} · ${domain.cols}×${domain.rows} @ ${domain.dxM.toFixed(1)} m`,
      geoFloodPlaying: false,
    });

    // Stage DEM + sample bed for soft/HAND (non-blocking).
    void get().prepareGeoDemForAoi();
  },

  prepareGeoDemForAoi: async () => {
    const aoi = get().geoAoiWgs84;
    let workspace = get().workspace;
    if (!workspace) {
      const geo = get().queueItems.find(
        (i) => (i.suite ?? "reconstruction") === "geospatial" && i.workspace,
      );
      workspace = geo?.workspace ?? null;
    }
    if (!workspace || !aoiIsValid(aoi)) return;

    const domain = domainFromAoi(aoi!, get().geoFloodLowPower);
    const plan = get().geoExtentPlan;
    const cell = plan?.demResolutionM ?? plan?.previewCellM ?? domain.dxM;

    try {
      // Prefer USGS 3DEP when AOI looks US-local; otherwise stage whatever is on disk.
      try {
        await api.fetchGeoCatalogAsset(workspace, "usgs-3dep", {
          aoiWgs84: aoi,
          cellSizeM: cell,
        });
      } catch {
        // Network / coverage — dem stage will fall back to catalog files or synthetic.
      }

      const dem = await api.prepareGeoDem(workspace, {
        aoiWgs84: aoi,
        cellSizeM: cell,
        crs: "EPSG:4326",
      });

      // Globe: only set terrain URL when quantized-mesh/heightmap layer.json exists.
      if (dem.terrainTilesUrl) {
        try {
          setLocalDemTerrainUrl(convertFileSrc(dem.terrainTilesUrl));
        } catch {
          setLocalDemTerrainUrl(null);
        }
      } else {
        setLocalDemTerrainUrl(null);
      }

      const sample = await api.sampleGeoDem(workspace, domain.cols, domain.rows, aoi);
      const bedNote = dem.synthetic
        ? "Synthetic DEM bed — Demo / Live preview only"
        : sample.bedSource === "real"
          ? "DEM bed sampled for soft + HAND preview"
          : "DEM staged; preview using undulation proxy";

      set({
        workspace,
        geoDemProduct: dem,
        geoDemSample: sample,
        geoDemRevision: get().geoDemRevision + 1,
        geoLayers: get().geoLayers.map((l) =>
          l.id === "dtm"
            ? {
                ...l,
                status: dem.synthetic ? "empty" : "ready",
                placeholder: dem.synthetic,
                visible: !dem.synthetic,
              }
            : l,
        ),
        geoInspectHint: bedNote,
        geoScenario: {
          ...(get().geoScenario ?? PLACEHOLDER_SCENARIO),
          authority: dem.synthetic ? "demo" : "live-preview",
          note: dem.synthetic
            ? "Synthetic DEM — soft/HAND stay Live preview; Demo until a real DEM is fetched."
            : "Real DEM bed feeding soft preview + HAND (Live preview / non-authoritative until ANUGA validates).",
        },
      });
    } catch (err) {
      set({
        geoInspectHint: `DEM stage: ${String(err)}`,
      });
    }
  },

  fetchGeoOverlayLayer: async (layerId) => {
    const aoi = get().geoAoiWgs84;
    let workspace = get().workspace;
    if (!workspace) {
      const geo = get().queueItems.find(
        (i) => (i.suite ?? "reconstruction") === "geospatial" && i.workspace,
      );
      workspace = geo?.workspace ?? null;
    }
    if (!workspace || !aoiIsValid(aoi)) {
      set({ geoInspectHint: "Draw an AOI before fetching overlay layers" });
      return;
    }

    const connectorByLayer: Record<string, string> = {
      nfhl: "fema-nfhl",
      hydrosheds: "hydrosheds",
      gauges: "usgs-nwis-gauges",
      waterways: "osm-waterways",
      dtm: "usgs-3dep",
    };
    const entryId = connectorByLayer[layerId];
    if (!entryId) return;

    set({
      geoLayers: get().geoLayers.map((l) =>
        l.id === layerId ? { ...l, status: "hook", placeholder: true } : l,
      ),
      geoInspectHint: `Fetching ${entryId}…`,
    });

    try {
      const entry = await api.fetchGeoCatalogAsset(workspace, entryId, { aoiWgs84: aoi });
      const path = entry.localPath ?? "";
      if (layerId === "dtm") {
        await get().prepareGeoDemForAoi();
        return;
      }
      set({
        workspace,
        geoOverlayPaths: { ...get().geoOverlayPaths, [layerId]: path },
        geoLayers: get().geoLayers.map((l) =>
          l.id === layerId
            ? {
                ...l,
                visible: true,
                placeholder: false,
                status: "ready",
              }
            : l,
        ),
        geoDemRevision: get().geoDemRevision + 1,
        geoInspectHint: entry.notes ?? `Fetched ${entry.title}`,
      });
    } catch (err) {
      set({
        geoLayers: get().geoLayers.map((l) =>
          l.id === layerId ? { ...l, status: "ready", placeholder: true } : l,
        ),
        geoInspectHint: `Overlay fetch failed: ${String(err)}`,
      });
    }
  },

  startScientificFlood: async (opts) => {
    let workspace = get().workspace;
    if (!workspace) {
      // Prefer an existing geospatial project workspace from the queue.
      const geo = get().queueItems.find(
        (i) => (i.suite ?? "reconstruction") === "geospatial" && i.workspace,
      );
      workspace = geo?.workspace ?? null;
    }
    if (!workspace) {
      set({
        geoScientificRun: {
          runId: "",
          state: "failed",
          progress: 0,
          detail: "Open or enqueue a geospatial project first (Add sources).",
        },
      });
      return;
    }
    const scenarioId = get().geoScenario?.id ?? PLACEHOLDER_SCENARIO.id;
    const demPath = get().geoDemProduct?.dtmPath ?? null;
    try {
      // Ensure DEM is staged with AOI before ANUGA.
      if (!get().geoDemProduct) {
        await get().prepareGeoDemForAoi();
      }
      const st = await api.startScientificFlood(workspace, scenarioId, {
        allowDemo: opts?.allowDemo ?? true,
        enableSwmm: opts?.enableSwmm ?? false,
        demPath: get().geoDemProduct?.dtmPath ?? demPath,
      });
      const demSynthetic = get().geoDemProduct?.synthetic ?? true;
      set({
        workspace,
        geoScientificRun: {
          runId: st.runId,
          state: st.state,
          progress: st.progress,
          detail: st.detail,
          mode: st.mode ?? (demSynthetic ? "demo" : undefined),
          label: st.label,
          massBalance: st.massBalance,
        },
        geoScenario: {
          ...(get().geoScenario ?? PLACEHOLDER_SCENARIO),
          engineLabel: demSynthetic
            ? "Demo (synthetic DEM)…"
            : "ANUGA scientific (starting…)",
          note: demSynthetic
            ? "Synthetic DEM — Demo badge until a real DEM is staged."
            : "Streaming scientific checkpoints over sim://event.",
          authority: demSynthetic ? "demo" : "scientific",
        },
      });
    } catch (err) {
      set({
        geoScientificRun: {
          runId: "",
          state: "failed",
          progress: 0,
          detail: String(err),
        },
      });
    }
  },

  cancelScientificFlood: async () => {
    const runId = get().geoScientificRun?.runId;
    if (!runId) return;
    try {
      await api.cancelScientificFlood(runId);
    } catch (err) {
      set({
        geoScientificRun: {
          ...(get().geoScientificRun as GeoScientificRun),
          detail: String(err),
        },
      });
    }
  },

  exportFloodProducts: async () => {
    let workspace = get().workspace;
    if (!workspace) {
      const geo = get().queueItems.find(
        (i) => (i.suite ?? "reconstruction") === "geospatial" && i.workspace,
      );
      workspace = geo?.workspace ?? null;
    }
    if (!workspace) {
      set({
        geoScientificRun: {
          runId: get().geoScientificRun?.runId ?? "",
          state: get().geoScientificRun?.state ?? "failed",
          progress: get().geoScientificRun?.progress ?? 0,
          detail: "Open or enqueue a geospatial project before exporting.",
          mode: get().geoScientificRun?.mode,
          label: get().geoScientificRun?.label,
          massBalance: get().geoScientificRun?.massBalance,
        },
      });
      return;
    }
    const runId = get().geoScientificRun?.runId || null;
    try {
      const result = await api.exportFloodProducts(workspace, runId);
      const n = result.artifacts.length;
      const auth = result.authoritative
        ? "calibrated scientific"
        : "non-authoritative (demo/preview/uncalibrated)";
      set((s) => ({
        workspace,
        geoScientificRun: {
          runId: result.runId,
          state: s.geoScientificRun?.state ?? "done",
          progress: s.geoScientificRun?.progress ?? 1,
          detail: `Exported ${n} products → ${result.exportDir} (${auth})`,
          mode: result.mode ?? s.geoScientificRun?.mode,
          label: s.geoScientificRun?.label,
          massBalance: s.geoScientificRun?.massBalance,
        },
        pipelineChips: {
          ...s.pipelineChips,
          export: `Export: ${n} products (${auth})`,
        },
      }));
    } catch (err) {
      set((s) => ({
        geoScientificRun: {
          runId: s.geoScientificRun?.runId ?? "",
          state: s.geoScientificRun?.state ?? "failed",
          progress: s.geoScientificRun?.progress ?? 0,
          detail: `Export failed: ${String(err)}`,
          mode: s.geoScientificRun?.mode,
          label: s.geoScientificRun?.label,
          massBalance: s.geoScientificRun?.massBalance,
        },
        pipelineChips: {
          ...s.pipelineChips,
          export: `Export: failed`,
        },
      }));
    }
  },

  handleGeoEvent: (e) => {
    switch (e.kind) {
      case "runProgress":
        set((s) => ({
          geoScientificRun: s.geoScientificRun?.runId === e.runId
            ? { ...s.geoScientificRun, progress: e.progress, detail: e.detail, state: "running" }
            : s.geoScientificRun ?? {
                runId: e.runId,
                state: "running",
                progress: e.progress,
                detail: e.detail,
              },
          pipelineChips: {
            ...s.pipelineChips,
            flood: `Flood: ${e.detail || "running"} (${Math.round(e.progress * 100)}%)`,
          },
        }));
        break;
      case "runDone":
        set((s) => ({
          geoScientificRun: {
            runId: e.runId,
            state: "done",
            progress: 1,
            detail: e.mode === "demo" ? "Demo run complete (not authoritative)" : "Scientific run complete",
            mode: e.mode,
            massBalance: e.massBalance,
            label: e.mode === "demo" ? "Demo mode — synthetic extents" : undefined,
          },
          pipelineChips: {
            ...s.pipelineChips,
            flood: e.mode === "demo" ? "Flood: demo complete" : "Flood: scientific complete",
          },
          geoScenario: {
            ...(s.geoScenario ?? PLACEHOLDER_SCENARIO),
            engineLabel:
              e.mode === "demo" ? "Demo (ANUGA missing)" : "ANUGA scientific",
            note:
              e.mode === "demo"
                ? "Engine missing — extents are labelled synthetic for UI continuity."
                : `Mass balance residual ≈ ${e.massBalance?.toFixed?.(4) ?? "n/a"}`,
          },
        }));
        break;
      case "runCancelled":
        set({
          geoScientificRun: {
            runId: e.runId,
            state: "cancelled",
            progress: 0,
            detail: "Cancelled",
          },
        });
        break;
      case "engineMissing":
        set((s) => ({
          geoScientificRun: s.geoScientificRun
            ? { ...s.geoScientificRun, detail: e.message, mode: "demo" }
            : {
                runId: "",
                state: "running",
                progress: 0,
                detail: e.message,
                mode: "demo",
              },
          geoScenario: {
            ...(s.geoScenario ?? PLACEHOLDER_SCENARIO),
            engineLabel: "Demo (ANUGA missing)",
            note: e.message,
          },
        }));
        break;
      case "error":
        set({
          geoScientificRun: {
            runId: e.runId ?? get().geoScientificRun?.runId ?? "",
            state: "failed",
            progress: 0,
            detail: e.message,
          },
        });
        break;
      default:
        break;
    }
  },

  handleSimEvent: (e) => {
    if (e.kind === "checkpoint") {
      const duration = get().geoScenario?.durationHours ?? PLACEHOLDER_SCENARIO.durationHours;
      const t01 = duration > 0 ? Math.min(1, Math.max(0, e.simTimeHours / duration)) : e.progress;
      set((s) => ({
        geoFloodTime: t01,
        geoScientificRun: {
          runId: e.runId,
          state: "running",
          progress: e.progress,
          detail: e.detail,
          mode: e.mode,
        },
        pipelineChips: {
          ...s.pipelineChips,
          flood: `Flood: ${e.detail || e.mode || "checkpoint"} (${Math.round(e.progress * 100)}%)`,
        },
        geoScenario:
          e.mode === "demo"
            ? {
                ...(s.geoScenario ?? PLACEHOLDER_SCENARIO),
                engineLabel: "Demo (ANUGA missing)",
              }
            : s.geoScenario,
      }));
      // Checkpoint GeoJSON is loaded lazily when path is absolute and readable
      // from the webview later; progress alone advances the scrubber for now.
    } else if (e.kind === "done") {
      set((s) => ({
        geoScientificRun: {
          runId: e.runId,
          state: "done",
          progress: 1,
          detail: e.label ?? e.mode,
          mode: e.mode,
          label: e.label,
          massBalance: e.massBalance,
        },
        geoScenario: {
          ...(s.geoScenario ?? PLACEHOLDER_SCENARIO),
          engineLabel: e.mode === "demo" ? "Demo (ANUGA missing)" : "ANUGA scientific",
          note: e.label ?? (e.mode === "demo" ? "Synthetic extents — not authoritative." : "Scientific run finished."),
        },
      }));
    } else if (e.kind === "hydrograph") {
      // Path available for future File API load into the scrubber series.
      void e.path;
    }
  },

  setGeoViewMode: (mode) => set({ geoViewMode: mode }),

  setGeoModelTransform: (tf) => set({ geoModelTransform: normalizeModelTransform(tf) }),

  persistGeoModelTransform: async () => {
    const { workspace, geoModelTransform } = get();
    if (!workspace) return;
    try {
      await api.saveModelTransform(workspace, geoModelTransform);
    } catch {
      // Workspace may be a draft without project.json yet.
    }
  },

  setGeoWaterStyle: (style) => {
    const layers = get().geoLayers.map((l) => {
      if (style === "hazard" && l.id === "flood_hazard") return { ...l, visible: true };
      if (style === "depth" && l.id === "flood_depth") return { ...l, visible: true };
      if (style === "contour" && l.id === "flood_depth") return { ...l, visible: true };
      return l;
    });
    set({ geoWaterStyle: style, geoLayers: layers });
  },

  setGeoTool: (tool) => {
    set({
      geoTool: tool,
      geoInspectHint:
        tool === "pan"
          ? null
          : tool === "inspect"
            ? "Inspect: click the map for coordinates"
            : tool === "measure"
              ? "Measure: click the map (stub)"
              : "Profile: click the map (stub)",
    });
  },

  setGeoInspectHint: (hint) => set({ geoInspectHint: hint }),

  setThemePreference: (p) => {
    localStorage.setItem(THEME_KEY, p);
    const theme = resolveTheme(p);
    applyTheme(theme);
    set({ themePreference: p, theme });
  },
  setLeftPanelOpen: (open) => {
    writeBool(LEFT_PANEL_KEY, open);
    set({ leftPanelOpen: open });
  },
  setRightPanelOpen: (open) => {
    writeBool(RIGHT_PANEL_KEY, open);
    set({ rightPanelOpen: open });
  },
  toggleRightPanel: () => get().setRightPanelOpen(!get().rightPanelOpen),
  setLogConsoleOpen: (open) => {
    writeBool(LOG_CONSOLE_KEY, open);
    set({ logConsoleOpen: open });
  },

  setAboutOpen: (open) => set({ aboutOpen: open }),

  updateSettings: async (patch) => {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    await api.setSettings(next);
    set({ resolved: await api.getResolvedSettings() });
  },

  requestExperimental: () => {
    const s = get().settings;
    if (s.experimentalLicenseAcked) {
      void get().updateSettings({ experimentalMode: true, allowResearchSidecars: true });
      return;
    }
    set({ experimentalModalOpen: true });
  },

  acceptExperimental: async () => {
    set({ experimentalModalOpen: false });
    await get().updateSettings({
      experimentalMode: true,
      experimentalLicenseAcked: true,
      allowResearchSidecars: true,
    });
  },

  declineExperimental: () => {
    set({ experimentalModalOpen: false });
  },

  refreshProjects: async () => {
    try {
      const list = await api.listProjects();
      set({ recentProjects: list });
    } catch {
      // The jobs directory may not exist yet on a brand new install.
      set({ recentProjects: [] });
    }
  },

  resumeProject: async (workspace) => {
    set({
      screen: "processing",
      inputPath: null,
      workspace,
      stages: freshStages(),
      logs: [],
      notices: [],
      pipelineChips: {},
      cameras: [],
      registeredCameras: 0,
      totalCameras: 0,
      trackingConfidence: 0,
      latestSplatPath: null,
      latestIter: 0,
      totalSteps: 0,
      splatCount: 0,
      resultPath: null,
      jobError: null,
      elapsedSecs: null,
      jobStartedAt: Date.now(),
      meshStatus: null,
      // A resumed job's settings are whatever it was started with, not the
      // live `resolved` snapshot, so there is nothing correct to diff
      // against here. Leaving this null just suppresses the "changed since
      // start" banner rather than showing one built from a stale, unrelated
      // job's snapshot.
      jobSettingsSnapshot: null,
    });
    try {
      const jobId = await api.resumeProject(workspace);
      set({ jobId });
    } catch (err) {
      set({ jobError: String(err) });
    }
  },

  deleteProjectEntry: async (workspace) => {
    await api.deleteProject(workspace);
    await get().refreshProjects();
  },

  startJob: async (path) => {
    set({
      screen: "processing",
      inputPath: path,
      workspace: null,
      stages: freshStages(),
      logs: [],
      notices: [],
      pipelineChips: {},
      cameras: [],
      registeredCameras: 0,
      totalCameras: 0,
      trackingConfidence: 0,
      latestSplatPath: null,
      latestIter: 0,
      totalSteps: 0,
      splatCount: 0,
      resultPath: null,
      jobError: null,
      elapsedSecs: null,
      jobStartedAt: Date.now(),
      jobSettingsSnapshot: get().resolved,
      meshStatus: null,
      sparseCloudPath: null,
      denseCloudPath: null,
      latestMeshPath: null,
      sparsePointCount: 0,
      densePointCount: 0,
      ingestFrameCount: 0,
      ingestPath: [],
    });
    try {
      // Make sure engines are present (first-run download).
      const st = await api.getEngineStatus();
      if (!st.colmap || !st.brush) {
        set((s) => ({
          logs: [...s.logs, { time: Date.now(), line: "Downloading reconstruction engines (first run)." }],
        }));
        await api.installEngines();
        set({ engineStatus: await api.getEngineStatus() });
      }
      const jobId = await api.startJob(path);
      set({ jobId });
    } catch (err) {
      set({ jobError: String(err) });
    }
  },

  enqueueJobs: async (paths, suite) => {
    if (paths.length === 0) return;
    const activeSuite = suite ?? get().suite;
    try {
      if (activeSuite === "reconstruction") {
        const st = await api.getEngineStatus();
        if (!st.colmap || !st.brush) {
          await api.installEngines();
          set({ engineStatus: await api.getEngineStatus() });
        }
      }
      await api.enqueueJobs(paths, activeSuite);
      const snap = await api.listQueue();
      set({ queueItems: snap.items, queuePaused: snap.paused });
    } catch (err) {
      set({ jobError: String(err), screen: activeSuite === "reconstruction" ? "processing" : "home" });
    }
  },

  pauseQueue: async () => {
    await api.pauseQueue(true);
    set({ queuePaused: true });
  },

  resumeQueue: async () => {
    await api.resumeQueue();
    set({ queuePaused: false });
  },

  cancelQueueItem: async (id) => {
    await api.cancelQueueItem(id);
  },

  clearFinishedQueue: async () => {
    await api.clearFinishedQueue();
  },

  cancelJob: async () => {
    const { jobId } = get();
    if (jobId) await api.cancelJob(jobId);
  },

  backHome: () => {
    set({
      screen: "home",
      jobId: null,
      jobError: null,
      latestSplatPath: null,
      resultPath: null,
      workspace: null,
      cameras: [],
      notices: [],
      pipelineChips: {},
      meshStatus: null,
      sparseCloudPath: null,
      denseCloudPath: null,
      latestMeshPath: null,
      sparsePointCount: 0,
      densePointCount: 0,
      ingestFrameCount: 0,
      ingestPath: [],
    });
    void get().refreshProjects();
  },

  handleJobEvent: (e) => {
    const { jobId } = get();
    if (jobId && e.jobId && e.jobId !== jobId) return;
    switch (e.kind) {
      case "stageStarted":
        set((s) => ({
          stages: s.stages.map((st) => {
            if (st.id === e.stage) return { ...st, state: "active" };
            const activeIdx = STAGE_ORDER.indexOf(e.stage);
            const idx = STAGE_ORDER.indexOf(st.id);
            return idx < activeIdx ? { ...st, state: "done", progress: 1 } : st;
          }),
        }));
        break;
      case "stageProgress":
        set((s) => ({
          stages: s.stages.map((st) =>
            st.id === e.stage ? { ...st, progress: e.progress, detail: e.detail, state: "active" } : st,
          ),
          queueItems: s.queueItems.map((q) =>
            q.jobId === e.jobId
              ? { ...q, progress: e.progress, detail: e.detail }
              : q,
          ),
        }));
        break;
      case "log":
        set((s) => ({ logs: [...s.logs.slice(-800), { time: Date.now(), line: e.line }] }));
        break;
      case "notice":
        set((s) => {
          const chips = { ...s.pipelineChips };
          const msg = e.message;
          if (/^Cameras:/i.test(msg)) chips.cameras = msg;
          else if (/^Init:/i.test(msg)) chips.init = msg;
          else if (/^Polish:/i.test(msg)) chips.polish = msg;
          else if (/^Trainer:/i.test(msg)) chips.trainer = msg;
          else if (/^Flood:/i.test(msg)) chips.flood = msg;
          else if (/^Export:/i.test(msg)) chips.export = msg;
          return { notices: [...s.notices, msg], pipelineChips: chips };
        });
        break;
      case "camerasReset":
        set({ cameras: [], registeredCameras: 0, trackingConfidence: 0 });
        break;
      case "cameraRegistered":
        set((s) => ({
          cameras: [...s.cameras, e],
          registeredCameras: e.registered,
          totalCameras: e.total,
          trackingConfidence: e.confidence,
        }));
        break;
      case "ingestPreview":
        set({
          ingestFrameCount: e.frameCount,
          ingestPath: e.path,
        });
        break;
      case "sparseCloudReady":
        set({ sparseCloudPath: e.path, sparsePointCount: e.pointCount });
        break;
      case "denseCloudReady":
        set({ denseCloudPath: e.path, densePointCount: e.pointCount });
        break;
      case "meshReady":
        set({ latestMeshPath: e.path });
        break;
      case "splatReady":
        set({ latestSplatPath: e.path, latestIter: e.iter, totalSteps: e.totalSteps });
        break;
      case "done":
        set((s) => ({
          resultPath: e.path,
          // The result always sits at the top of its workspace, and both mesh
          // export and orientation saving need that directory.
          workspace: e.path.replace(/[\\/][^\\/]+$/, ""),
          elapsedSecs: e.elapsedSecs,
          stages: s.stages.map((st) => ({ ...st, state: "done", progress: 1 })),
        }));
        void get().refreshProjects();
        break;
      case "error":
        set({ jobError: e.message });
        void get().refreshProjects();
        break;
      case "cancelled":
        get().backHome();
        break;
    }
  },

  setSplatCount: (n) => set({ splatCount: n }),
  setFps: (n) => set({ fps: n }),
  setReconLayer: (id, visible) =>
    set((s) => ({ reconLayers: { ...s.reconLayers, [id]: visible } })),

  exportSplatAction: async (rotation) => {
    const { resultPath, workspace, settings } = get();
    if (!resultPath) return;
    const preferred = (settings.exportFormat as SplatFormat | undefined) ?? "ply";
    const formats: { ext: SplatFormat; label: string }[] = [
      { ext: "ply", label: "Gaussian Splat PLY" },
      { ext: "splat", label: "Web splat" },
      { ext: "spz", label: "Niantic SPZ" },
    ];
    const ordered = [...formats.filter((f) => f.ext === preferred), ...formats.filter((f) => f.ext !== preferred)];
    const dest = await save({
      title: "Export splat",
      defaultPath: `scene.${ordered[0].ext}`,
      filters: ordered.map((f) => ({ name: f.label, extensions: [f.ext] })),
    });
    if (!dest) return;
    try {
      await api.exportSplat(resultPath, dest, workspace, rotation ?? null);
      set({ meshStatus: `Splat saved to ${dest}` });
    } catch (err) {
      set({ meshStatus: String(err) });
    }
  },

  exportMeshAction: async () => {
    const { resultPath, workspace } = get();
    if (!resultPath || !workspace) return;
    const dest = await save({
      title: "Export mesh",
      defaultPath: "scene.glb",
      filters: [
        { name: "glTF binary", extensions: ["glb"] },
        { name: "Wavefront OBJ", extensions: ["obj"] },
        { name: "Mesh PLY", extensions: ["ply"] },
      ],
    });
    if (!dest) return;
    set({ meshStatus: "Starting mesh extraction." });
    const unlisten = await api.onMeshProgress((e) =>
      set({ meshStatus: `${e.detail} (${Math.round(e.progress * 100)}%)` }),
    );
    try {
      const triangles = await api.exportMesh(workspace, resultPath, dest);
      set({
        meshStatus: `Wrote ${triangles.toLocaleString()} triangles to ${dest}`,
        latestMeshPath: dest,
      });
    } catch (err) {
      set({ meshStatus: String(err) });
    } finally {
      unlisten();
    }
  },

  exportSchematicAction: async (rotation) => {
    const { resultPath, workspace, resolved } = get();
    if (!resultPath) return;
    if (!resolved?.experimentalMode) {
      set({
        meshStatus:
          "Minecraft schematic export needs Experimental Mode (TitleBar toggle).",
      });
      return;
    }
    const dest = await save({
      title: "Export Minecraft schematic (experimental)",
      defaultPath: "scene.schem",
      filters: [{ name: "Sponge Schematic v2", extensions: ["schem"] }],
    });
    if (!dest) return;
    set({ meshStatus: "Voxelizing splat → Minecraft schematic…" });
    try {
      const result = await api.exportMinecraftSchematic(resultPath, dest, {
        workspace,
        rotation: rotation ?? null,
      });
      set({
        meshStatus: `Schematic ${result.width}×${result.height}×${result.length} (${result.occupied.toLocaleString()} blocks) → ${result.path}`,
        pipelineChips: {
          ...get().pipelineChips,
          export: `Export: schematic ${result.width}×${result.height}×${result.length}`,
        },
      });
    } catch (err) {
      set({
        meshStatus: String(err),
        pipelineChips: {
          ...get().pipelineChips,
          export: "Export: schematic failed",
        },
      });
    }
  },

  exportDiagnosticsAction: async () => {
    const { workspace, logs } = get();
    const dest = await save({
      title: "Export diagnostics",
      defaultPath: "instasplatter-diagnostics.txt",
      filters: [{ name: "Text", extensions: ["txt"] }],
    });
    if (!dest) return;
    const lines = logs.slice(-500).map((l) => `[${new Date(l.time).toISOString()}] ${l.line}`);
    try {
      await api.exportDiagnostics(workspace, lines, dest);
      set({ meshStatus: `Diagnostics saved to ${dest}` });
    } catch (err) {
      set({ meshStatus: String(err) });
    }
  },
}));
