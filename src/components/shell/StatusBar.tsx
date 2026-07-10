import { useEffect, useState } from "react";
import { useStore } from "../../state/store";

export default function StatusBar() {
  const screen = useStore((s) => s.screen);
  const stages = useStore((s) => s.stages);
  const splatCount = useStore((s) => s.splatCount);
  const fps = useStore((s) => s.fps);
  const latestIter = useStore((s) => s.latestIter);
  const totalSteps = useStore((s) => s.totalSteps);
  const elapsedSecs = useStore((s) => s.elapsedSecs);
  const jobStartedAt = useStore((s) => s.jobStartedAt);
  const resultPath = useStore((s) => s.resultPath);
  const jobError = useStore((s) => s.jobError);
  const registeredCameras = useStore((s) => s.registeredCameras);
  const totalCameras = useStore((s) => s.totalCameras);
  const trackingConfidence = useStore((s) => s.trackingConfidence);
  const meshStatus = useStore((s) => s.meshStatus);
  const logConsoleOpen = useStore((s) => s.logConsoleOpen);
  const setLogConsoleOpen = useStore((s) => s.setLogConsoleOpen);
  const exportDiagnosticsAction = useStore((s) => s.exportDiagnosticsAction);

  const [tick, setTick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => setTick((v) => v + 1), 1000);
    return () => clearInterval(t);
  }, []);
  void tick;

  if (screen !== "processing") {
    return (
      <div className="flex h-6 shrink-0 items-center justify-between border-t border-edge bg-panel px-3 text-[11px] text-ink-dim">
        <span>Ready</span>
        <button onClick={() => void exportDiagnosticsAction()} className="hover:text-ink">
          Export diagnostics
        </button>
      </div>
    );
  }

  const running = !resultPath && !jobError;
  const elapsed = elapsedSecs ?? (jobStartedAt ? (Date.now() - jobStartedAt) / 1000 : 0);
  const trainStage = stages.find((s) => s.id === "train");
  const eta =
    running && trainStage?.state === "active" && latestIter > 100 && totalSteps > 0
      ? (elapsed / latestIter) * (totalSteps - latestIter)
      : null;
  const solvingCameras = stages.find((s) => s.id === "sfm")?.state === "active" && totalCameras > 0;

  const fmt = (secs: number) => `${Math.max(0, Math.floor(secs / 60))}m ${Math.max(0, Math.floor(secs % 60))}s`;

  return (
    <div className="flex h-6 shrink-0 items-center justify-between border-t border-edge bg-panel px-3 text-[11px] tabular-nums text-ink-dim">
      <div className="flex items-center gap-4">
        <span>{jobError ? "Failed" : resultPath ? "Complete" : (stages.find((s) => s.state === "active")?.detail || "Preparing")}</span>
        {solvingCameras && (
          <span>
            {registeredCameras} / {totalCameras} cameras, {Math.round(trackingConfidence * 100)}% confidence
          </span>
        )}
        <span>splats {splatCount.toLocaleString()}</span>
        {totalSteps > 0 && (
          <span>
            step {latestIter.toLocaleString()} / {totalSteps.toLocaleString()}
          </span>
        )}
        <span>fps {fps.toFixed(0)}</span>
        <span>elapsed {fmt(elapsed)}</span>
        {eta !== null && <span>eta {fmt(eta)}</span>}
        {meshStatus && <span className="max-w-96 truncate text-ink">{meshStatus}</span>}
      </div>
      <div className="flex items-center gap-3">
        <button onClick={() => void exportDiagnosticsAction()} className="hover:text-ink">
          Export diagnostics
        </button>
        <button onClick={() => setLogConsoleOpen(!logConsoleOpen)} className="hover:text-ink">
          {logConsoleOpen ? "Hide log" : "Log"}
        </button>
      </div>
    </div>
  );
}
