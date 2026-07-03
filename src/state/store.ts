import { create } from "zustand";
import type {
  EngineStatus,
  HardwareProfile,
  JobEvent,
  ResolvedSettings,
  Settings,
} from "../lib/ipc";
import { api } from "../lib/ipc";

export type Screen = "home" | "processing";

export interface StageInfo {
  id: string;
  label: string;
  progress: number;
  detail: string;
  state: "pending" | "active" | "done";
}

const STAGE_ORDER = ["ingest", "sfm", "train", "finalize"];
const STAGE_LABELS: Record<string, string> = {
  ingest: "Frames",
  sfm: "Cameras",
  train: "Splatting",
  finalize: "Finalize",
};

interface AppStore {
  screen: Screen;
  profile: HardwareProfile | null;
  engineStatus: EngineStatus | null;
  settings: Settings;
  resolved: ResolvedSettings | null;
  prefsOpen: boolean;

  jobId: string | null;
  inputPath: string | null;
  stages: StageInfo[];
  logs: string[];
  latestSplatPath: string | null;
  latestIter: number;
  totalSteps: number;
  splatCount: number;
  resultPath: string | null;
  jobError: string | null;
  jobStartedAt: number;
  elapsedSecs: number | null;

  init: () => Promise<void>;
  openPrefs: (open: boolean) => void;
  updateSettings: (patch: Partial<Settings>) => Promise<void>;
  startJob: (path: string) => Promise<void>;
  cancelJob: () => Promise<void>;
  backHome: () => void;
  handleJobEvent: (e: JobEvent) => void;
  setSplatCount: (n: number) => void;
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
  profile: null,
  engineStatus: null,
  settings: {},
  resolved: null,
  prefsOpen: false,

  jobId: null,
  inputPath: null,
  stages: freshStages(),
  logs: [],
  latestSplatPath: null,
  latestIter: 0,
  totalSteps: 0,
  splatCount: 0,
  resultPath: null,
  jobError: null,
  jobStartedAt: 0,
  elapsedSecs: null,

  init: async () => {
    // React StrictMode double-invokes effects in dev; init exactly once.
    if (initStarted) return;
    initStarted = true;
    const [profile, settings, resolved, engineStatus] = await Promise.all([
      api.getHardwareProfile(),
      api.getSettings(),
      api.getResolvedSettings(),
      api.getEngineStatus(),
    ]);
    set({ profile, settings, resolved, engineStatus });
    await api.onJobEvent((e) => get().handleJobEvent(e));
    // Dev/test hook: start a job immediately if requested via env var.
    const auto = await api.getAutostart().catch(() => null);
    if (auto && !get().jobId) void get().startJob(auto);
  },

  openPrefs: (open) => set({ prefsOpen: open }),

  updateSettings: async (patch) => {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    await api.setSettings(next);
    set({ resolved: await api.getResolvedSettings() });
  },

  startJob: async (path) => {
    set({
      screen: "processing",
      inputPath: path,
      stages: freshStages(),
      logs: [],
      latestSplatPath: null,
      latestIter: 0,
      totalSteps: 0,
      splatCount: 0,
      resultPath: null,
      jobError: null,
      elapsedSecs: null,
      jobStartedAt: Date.now(),
    });
    try {
      // Make sure engines are present (first-run download).
      const st = await api.getEngineStatus();
      if (!st.colmap || !st.brush) {
        set((s) => ({ logs: [...s.logs, "Downloading reconstruction engines (first run)…"] }));
        await api.installEngines();
      }
      const jobId = await api.startJob(path);
      set({ jobId });
    } catch (err) {
      set({ jobError: String(err) });
    }
  },

  cancelJob: async () => {
    const { jobId } = get();
    if (jobId) await api.cancelJob(jobId);
  },

  backHome: () =>
    set({
      screen: "home",
      jobId: null,
      jobError: null,
      latestSplatPath: null,
      resultPath: null,
    }),

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
        }));
        break;
      case "log":
        set((s) => ({ logs: [...s.logs.slice(-400), e.line] }));
        break;
      case "splatReady":
        set({ latestSplatPath: e.path, latestIter: e.iter, totalSteps: e.totalSteps });
        break;
      case "done":
        set((s) => ({
          resultPath: e.path,
          elapsedSecs: e.elapsedSecs,
          stages: s.stages.map((st) => ({ ...st, state: "done", progress: 1 })),
        }));
        break;
      case "error":
        set({ jobError: e.message });
        break;
      case "cancelled":
        get().backHome();
        break;
    }
  },

  setSplatCount: (n) => set({ splatCount: n }),
}));
