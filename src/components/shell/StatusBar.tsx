import { useEffect, useMemo, useState } from "react";
import { useStore } from "../../state/store";
import { floodSnapshotFromTime, hazardClassLabel } from "../../geospatial/floodPreview";

export default function StatusBar() {
  const suite = useStore((s) => s.suite);
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
  const floodTime = useStore((s) => s.geoFloodTime);
  const viewMode = useStore((s) => s.geoViewMode);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const preview = useStore((s) => s.geoPreview);
  const scientific = useStore((s) => s.geoScientificRun);
  const pipelineChips = useStore((s) => s.pipelineChips);
  const geoSnap = useMemo(
    () => floodSnapshotFromTime(floodTime, preview),
    [floodTime, preview],
  );

  const [tick, setTick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => setTick((v) => v + 1), 1000);
    return () => clearInterval(t);
  }, []);
  void tick;

  if (suite === "geospatial") {
    const floodPct =
      scientific?.state === "running"
        ? Math.round(scientific.progress * 100)
        : scientific?.state === "done"
          ? 100
          : null;
    return (
      <div className="flex h-6 shrink-0 items-center justify-between border-t border-edge bg-panel px-3 text-[11px] tabular-nums text-ink-dim">
        <div className="flex min-w-0 items-center gap-4 overflow-hidden">
          <span className="text-[var(--color-hydro)]">Geospatial</span>
          <span className="text-[var(--color-gauge)]">
            {preview?.validation === "validated" ? "Validated" : "Live preview"}
            {preview?.backend ? ` · ${preview.backend}` : ""}
          </span>
          {scientific && (
            <span title={scientific.detail}>
              Flood {scientific.state}
              {floodPct != null ? ` ${floodPct}%` : ""}
            </span>
          )}
          {pipelineChips.export && (
            <span className="max-w-48 truncate" title={pipelineChips.export}>
              {pipelineChips.export.replace(/^Export:\s*/i, "Export · ")}
            </span>
          )}
          <span>{geoSnap.statusLabel}</span>
          <span>
            t {geoSnap.hours.toFixed(1)} h · depth {geoSnap.maxDepthM.toFixed(2)} m ·{" "}
            {hazardClassLabel(geoSnap.hazardClass)}
          </span>
          <span className="uppercase">
            {viewMode} · {waterStyle}
          </span>
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
  const activeStage = stages.find((s) => s.state === "active");
  const eta =
    running && trainStage?.state === "active" && latestIter > 100 && totalSteps > 0
      ? (elapsed / latestIter) * (totalSteps - latestIter)
      : null;
  const solvingCameras = stages.find((s) => s.id === "sfm")?.state === "active" && totalCameras > 0;

  const fmt = (secs: number) => `${Math.max(0, Math.floor(secs / 60))}m ${Math.max(0, Math.floor(secs % 60))}s`;

  return (
    <div className="flex h-6 shrink-0 items-center justify-between border-t border-edge bg-panel px-3 text-[11px] tabular-nums text-ink-dim">
      <div className="flex items-center gap-4">
        <span>
          {jobError
            ? "Failed"
            : resultPath
              ? "Complete"
              : activeStage
                ? `${activeStage.detail || activeStage.label}${
                    activeStage.progress > 0 ? ` ${Math.round(activeStage.progress * 100)}%` : ""
                  }`
                : "Preparing"}
        </span>
        {solvingCameras && (
          <span>
            {registeredCameras} / {totalCameras} cameras, {Math.round(trackingConfidence * 100)}% confidence
          </span>
        )}
        {pipelineChips.cameras && (
          <span className="max-w-40 truncate" title={pipelineChips.cameras}>
            {pipelineChips.cameras.replace(/^Cameras:\s*/i, "")}
          </span>
        )}
        {pipelineChips.init && (
          <span className="max-w-40 truncate" title={pipelineChips.init}>
            {pipelineChips.init.replace(/^Init:\s*/i, "")}
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
