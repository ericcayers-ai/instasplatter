import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/ipc";
import type { MeshFormat, SplatFormat } from "../lib/ipc";
import { useStore } from "../state/store";
import Viewport from "./Viewport";

const SPLAT_FORMATS: { ext: SplatFormat; label: string }[] = [
  { ext: "ply", label: "Gaussian Splat PLY" },
  { ext: "splat", label: "Web splat" },
  { ext: "spz", label: "Niantic SPZ" },
];

const MESH_FORMATS: { ext: MeshFormat; label: string }[] = [
  { ext: "glb", label: "glTF binary" },
  { ext: "obj", label: "Wavefront OBJ" },
  { ext: "ply", label: "Mesh PLY" },
];

function StageDot({ state }: { state: "pending" | "active" | "done" }) {
  return (
    <span
      className={`inline-block h-2.5 w-2.5 rounded-full transition-colors ${
        state === "done"
          ? "bg-accent"
          : state === "active"
            ? "animate-pulse bg-accent2"
            : "bg-edge"
      }`}
    />
  );
}

export default function Processing() {
  const stages = useStore((s) => s.stages);
  const logs = useStore((s) => s.logs);
  const jobError = useStore((s) => s.jobError);
  const resultPath = useStore((s) => s.resultPath);
  const elapsedSecs = useStore((s) => s.elapsedSecs);
  const latestIter = useStore((s) => s.latestIter);
  const totalSteps = useStore((s) => s.totalSteps);
  const splatCount = useStore((s) => s.splatCount);
  const latestSplatPath = useStore((s) => s.latestSplatPath);
  const inputPath = useStore((s) => s.inputPath);
  const cancelJob = useStore((s) => s.cancelJob);
  const backHome = useStore((s) => s.backHome);
  const jobStartedAt = useStore((s) => s.jobStartedAt);
  const workspace = useStore((s) => s.workspace);
  const notices = useStore((s) => s.notices);
  const registeredCameras = useStore((s) => s.registeredCameras);
  const totalCameras = useStore((s) => s.totalCameras);
  const trackingConfidence = useStore((s) => s.trackingConfidence);
  const settings = useStore((s) => s.settings);
  const [showLogs, setShowLogs] = useState(false);
  const [meshStatus, setMeshStatus] = useState<string | null>(null);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const t = setInterval(() => setTick((v) => v + 1), 1000);
    return () => clearInterval(t);
  }, []);
  void tick;

  const running = !resultPath && !jobError;
  const elapsed = elapsedSecs ?? (Date.now() - jobStartedAt) / 1000;
  const trainStage = stages.find((s) => s.id === "train");
  const eta =
    running && trainStage?.state === "active" && latestIter > 100
      ? ((elapsed / latestIter) * (totalSteps - latestIter))
      : null;

  // The preferred format only decides the default name and which filter is
  // offered first. Whatever extension the user types is what gets written.
  const preferred = (settings.exportFormat as SplatFormat | undefined) ?? "ply";
  const ordered = [
    ...SPLAT_FORMATS.filter((f) => f.ext === preferred),
    ...SPLAT_FORMATS.filter((f) => f.ext !== preferred),
  ];

  const exportResult = async () => {
    if (!resultPath) return;
    const dest = await save({
      title: "Export splat",
      defaultPath: `scene.${ordered[0].ext}`,
      filters: ordered.map((f) => ({ name: f.label, extensions: [f.ext] })),
    });
    if (!dest) return;
    try {
      await api.exportSplat(resultPath, dest);
      setMeshStatus(null);
    } catch (err) {
      setMeshStatus(String(err));
    }
  };

  const exportMesh = async () => {
    if (!resultPath || !workspace) return;
    const dest = await save({
      title: "Export mesh",
      defaultPath: "scene.glb",
      filters: MESH_FORMATS.map((f) => ({ name: f.label, extensions: [f.ext] })),
    });
    if (!dest) return;
    setMeshStatus("Starting mesh extraction.");
    const unlisten = await api.onMeshProgress((e) =>
      setMeshStatus(`${e.detail} (${Math.round(e.progress * 100)}%)`),
    );
    try {
      const triangles = await api.exportMesh(workspace, resultPath, dest);
      setMeshStatus(`Wrote ${triangles.toLocaleString()} triangles.`);
    } catch (err) {
      setMeshStatus(String(err));
    } finally {
      unlisten();
    }
  };

  const fname = inputPath?.split(/[\\/]/).pop() ?? "";
  const sfmStage = stages.find((s) => s.id === "sfm");
  const solvingCameras = sfmStage?.state === "active" && totalCameras > 0;

  return (
    <div className="relative h-full w-full bg-bg">
      <Viewport />

      {/* Waiting overlay before the first splat arrives */}
      {!latestSplatPath && !jobError && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3">
          <div className="h-10 w-10 animate-spin rounded-full border-2 border-edge border-t-accent" />
          <div className="text-sm text-ink-dim">
            {stages.find((s) => s.state === "active")?.detail || "Preparing…"}
          </div>
        </div>
      )}

      {/* Error overlay */}
      {jobError && (
        <div className="absolute inset-0 flex items-center justify-center bg-bg/80 backdrop-blur-sm">
          <div className="float-in flex max-w-md flex-col items-center gap-4 rounded-2xl border border-edge bg-panel p-8 text-center">
            <div className="text-3xl">😵‍💫</div>
            <div className="font-medium">Reconstruction failed</div>
            <div className="text-sm text-ink-dim">{jobError}</div>
            <button
              onClick={backHome}
              className="mt-2 rounded-full bg-accent px-5 py-2 text-sm font-medium text-black"
            >
              Back
            </button>
          </div>
        </div>
      )}

      {/* Top bar */}
      <div className="absolute left-0 right-0 top-0 flex items-center justify-between p-4">
        <div className="flex items-center gap-3 rounded-full border border-edge bg-panel/80 px-4 py-2 text-xs backdrop-blur">
          <span className="max-w-56 truncate font-medium">{fname}</span>
          <span className="text-ink-dim">·</span>
          {stages.map((s) => (
            <span key={s.id} className="flex items-center gap-1.5 text-ink-dim">
              <StageDot state={s.state} />
              {s.label}
            </span>
          ))}
        </div>
        <div className="flex gap-2">
          {running ? (
            <button
              onClick={cancelJob}
              className="rounded-full border border-edge bg-panel/80 px-4 py-2 text-xs text-ink-dim backdrop-blur transition hover:border-red-400/50 hover:text-red-300"
            >
              Cancel
            </button>
          ) : (
            <>
              <button
                onClick={backHome}
                className="rounded-full border border-edge bg-panel/80 px-4 py-2 text-xs backdrop-blur transition hover:border-accent/50"
              >
                New scene
              </button>
              {resultPath && (
                <>
                  <button
                    onClick={exportMesh}
                    className="rounded-full border border-edge bg-panel/80 px-4 py-2 text-xs backdrop-blur transition hover:border-accent/50"
                    title="Build a textured mesh from the trained splat"
                  >
                    Export mesh
                  </button>
                  <button
                    onClick={exportResult}
                    className="rounded-full bg-accent px-4 py-2 text-xs font-medium text-black transition hover:brightness-110"
                  >
                    Export splat
                  </button>
                </>
              )}
            </>
          )}
        </div>
      </div>

      {/* Plain statements the pipeline made, such as falling back to COLMAP */}
      {(notices.length > 0 || meshStatus) && (
        <div className="absolute left-4 top-20 flex max-w-md flex-col gap-1">
          {notices.map((n, i) => (
            <div
              key={i}
              className="rounded-lg border border-edge bg-panel/80 px-3 py-2 text-xs text-ink-dim backdrop-blur"
            >
              {n}
            </div>
          ))}
          {meshStatus && (
            <div className="rounded-lg border border-edge bg-panel/80 px-3 py-2 text-xs text-ink-dim backdrop-blur">
              {meshStatus}
            </div>
          )}
        </div>
      )}

      {/* Bottom HUD */}
      <div className="absolute bottom-0 left-0 right-0 flex flex-col gap-2 p-4">
        {showLogs && (
          <div className="float-in max-h-48 overflow-y-auto rounded-xl border border-edge bg-panel/90 p-3 font-mono text-[11px] leading-relaxed text-ink-dim backdrop-blur">
            {logs.map((l, i) => (
              <div key={i}>{l}</div>
            ))}
          </div>
        )}
        <div className="flex items-center justify-between rounded-2xl border border-edge bg-panel/80 px-5 py-3 backdrop-blur">
          <div className="flex items-center gap-6 text-xs">
            <div>
              <div className="text-ink-dim">Splats</div>
              <div className="font-medium tabular-nums">
                {splatCount.toLocaleString()}
              </div>
            </div>
            <div>
              <div className="text-ink-dim">Step</div>
              <div className="font-medium tabular-nums">
                {latestIter.toLocaleString()}
                {totalSteps > 0 && (
                  <span className="text-ink-dim"> / {totalSteps.toLocaleString()}</span>
                )}
              </div>
            </div>
            {solvingCameras && (
              <div>
                <div className="text-ink-dim">Cameras</div>
                <div className="font-medium tabular-nums">
                  {registeredCameras.toLocaleString()}
                  <span className="text-ink-dim"> / {totalCameras.toLocaleString()}</span>
                  <span className="ml-2 text-ink-dim">
                    {Math.round(trackingConfidence * 100)}% confidence
                  </span>
                </div>
              </div>
            )}
            <div>
              <div className="text-ink-dim">Elapsed</div>
              <div className="font-medium tabular-nums">
                {Math.floor(elapsed / 60)}m {Math.floor(elapsed % 60)}s
              </div>
            </div>
            {eta !== null && (
              <div>
                <div className="text-ink-dim">ETA</div>
                <div className="font-medium tabular-nums">
                  ~{Math.max(0, Math.floor(eta / 60))}m {Math.max(0, Math.floor(eta % 60))}s
                </div>
              </div>
            )}
            {resultPath && (
              <div className="font-medium text-accent">✓ Complete</div>
            )}
          </div>
          <div className="flex items-center gap-3">
            {/* Overall progress bar */}
            <div className="h-1.5 w-48 overflow-hidden rounded-full bg-edge">
              <div
                className="h-full rounded-full bg-gradient-to-r from-accent to-accent2 transition-all duration-500"
                style={{
                  width: `${
                    (stages.reduce((a, s) => a + (s.state === "done" ? 1 : s.progress), 0) /
                      stages.length) *
                    100
                  }%`,
                }}
              />
            </div>
            <button
              onClick={() => setShowLogs((v) => !v)}
              className="text-xs text-ink-dim transition hover:text-ink"
            >
              {showLogs ? "Hide log" : "Log"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
