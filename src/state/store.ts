import { create } from "zustand";
import { save } from "@tauri-apps/plugin-dialog";
import type {
  CameraRegistered,
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
import type {
  GeoLayer,
  GeoPreviewRuntime,
  GeoScenarioMeta,
  GeoScientificRun,
  GeoTool,
  GeoViewMode,
  GeoWaterStyle,
} from "../geospatial/types";

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
  /** Status chips emitted by the pipeline (Cameras / Init / Polish / Trainer). */
  pipelineChips: {
    cameras?: string;
    init?: string;
    polish?: string;
    trainer?: string;
  };
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

  init: () => Promise<void>;
  setSuite: (suite: Suite) => Promise<void>;
  setGeoLayerVisible: (id: string, visible: boolean) => void;
  setGeoLayerOpacity: (id: string, opacity: number) => void;
  setGeoFloodTime: (t01: number) => void;
  setGeoFloodPlaying: (playing: boolean) => void;
  toggleGeoFloodPlaying: () => void;
  setGeoFloodLowPower: (on: boolean) => void;
  setGeoPreview: (runtime: GeoPreviewRuntime | null) => void;
  startScientificFlood: (opts?: { allowDemo?: boolean; enableSwmm?: boolean }) => Promise<void>;
  cancelScientificFlood: () => Promise<void>;
  exportFloodProducts: () => Promise<void>;
  handleGeoEvent: (e: GeoEvent) => void;
  handleSimEvent: (e: SimEvent) => void;
  setGeoViewMode: (mode: GeoViewMode) => void;
  setGeoWaterStyle: (style: GeoWaterStyle) => void;
  setGeoTool: (tool: GeoTool) => void;
  setGeoInspectHint: (hint: string | null) => void;
  setThemePreference: (p: ThemePreference) => void;
  setLeftPanelOpen: (open: boolean) => void;
  setRightPanelOpen: (open: boolean) => void;
  toggleRightPanel: () => void;
  setLogConsoleOpen: (open: boolean) => void;
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
  exportSplatAction: (rotation?: number[] | null) => Promise<void>;
  exportMeshAction: () => Promise<void>;
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
  experimentalModalOpen: false,

  geoLayers: DEFAULT_GEO_LAYERS.map((l) => ({ ...l })),
  geoFloodTime: 0.35,
  geoFloodPlaying: false,
  geoFloodLowPower: false,
  geoScenario: PLACEHOLDER_SCENARIO,
  geoViewMode: "2d",
  geoWaterStyle: "depth",
  geoTool: "pan",
  geoInspectHint: null,
  geoPreview: null,
  geoScientificRun: null,
  geoFloodEngine: null,
  geoScientificExtent: null,

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
    try {
      const st = await api.startScientificFlood(workspace, scenarioId, {
        allowDemo: opts?.allowDemo ?? true,
        enableSwmm: opts?.enableSwmm ?? false,
      });
      set({
        workspace,
        geoScientificRun: {
          runId: st.runId,
          state: st.state,
          progress: st.progress,
          detail: st.detail,
          mode: st.mode,
          label: st.label,
          massBalance: st.massBalance,
        },
        geoScenario: {
          ...(get().geoScenario ?? PLACEHOLDER_SCENARIO),
          engineLabel: "ANUGA scientific (starting…)",
          note: "Streaming scientific checkpoints over sim://event.",
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
      set({
        workspace,
        geoScientificRun: {
          runId: result.runId,
          state: get().geoScientificRun?.state ?? "done",
          progress: get().geoScientificRun?.progress ?? 1,
          detail: `Exported ${n} products → ${result.exportDir} (${auth})`,
          mode: result.mode ?? get().geoScientificRun?.mode,
          label: get().geoScientificRun?.label,
          massBalance: get().geoScientificRun?.massBalance,
        },
      });
    } catch (err) {
      set({
        geoScientificRun: {
          runId: get().geoScientificRun?.runId ?? "",
          state: get().geoScientificRun?.state ?? "failed",
          progress: get().geoScientificRun?.progress ?? 0,
          detail: `Export failed: ${String(err)}`,
          mode: get().geoScientificRun?.mode,
          label: get().geoScientificRun?.label,
          massBalance: get().geoScientificRun?.massBalance,
        },
      });
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
    });
    void get().refreshProjects();
  },

  handleJobEvent: (e) => {
    const { jobId } = get();
    if (jobId && e.jobId !== jobId) return;
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
          return { notices: [...s.notices, msg], pipelineChips: chips };
        });
        break;
      case "cameraRegistered":
        set((s) => ({
          cameras: [...s.cameras, e],
          registeredCameras: e.registered,
          totalCameras: e.total,
          trackingConfidence: e.confidence,
        }));
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
      set({ meshStatus: `Wrote ${triangles.toLocaleString()} triangles to ${dest}` });
    } catch (err) {
      set({ meshStatus: String(err) });
    } finally {
      unlisten();
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
