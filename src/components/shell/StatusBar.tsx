import { useEffect, useMemo, useState } from "react";
import { useStore } from "../../state/store";
import { floodSnapshotFromTime, hazardClassLabel } from "../../geospatial/floodPreview";

function Pill({
  tone,
  children,
}: {
  tone: "idle" | "live" | "ok" | "err";
  children: React.ReactNode;
}) {
  return (
    <span className="status-pill" data-tone={tone}>
      {children}
    </span>
  );
}

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

  const actions = (
    <div className="flex items-center gap-2">
      <button
        type="button"
        onClick={() => void exportDiagnosticsAction()}
        className="btn btn-ghost px-1.5 py-0.5 text-[10px]"
      >
        Diagnostics
      </button>
      <button
        type="button"
        onClick={() => setLogConsoleOpen(!logConsoleOpen)}
        className={`btn btn-ghost px-1.5 py-0.5 text-[10px] ${logConsoleOpen ? "btn-active" : ""}`}
        title="Toggle log (Ctrl+L)"
      >
        {logConsoleOpen ? "Hide log" : "Log"}
        <span className="kbd ml-1 hidden sm:inline">⌃L</span>
      </button>
    </div>
  );

  if (suite === "geospatial") {
    const floodPct =
      scientific?.state === "running"
        ? Math.round(scientific.progress * 100)
        : scientific?.state === "done"
          ? 100
          : null;
    const tone =
      scientific?.state === "running"
        ? "live"
        : scientific?.state === "done"
          ? "ok"
          : preview?.validation === "validated"
            ? "ok"
            : "idle";
    return (
      <footer className="flex h-7 shrink-0 items-center justify-between gap-3 border-t border-edge bg-panel px-3 text-[11px] tabular-nums text-ink-dim">
        <div className="flex min-w-0 items-center gap-3 overflow-hidden">
          <Pill tone={tone}>
            {preview?.validation === "validated" ? "Validated" : "Live preview"}
          </Pill>
          {scientific && (
            <span title={scientific.detail} className="truncate">
              Flood {scientific.state}
              {floodPct != null ? ` ${floodPct}%` : ""}
            </span>
          )}
          {pipelineChips.export && (
            <span className="max-w-40 truncate" title={pipelineChips.export}>
              {pipelineChips.export.replace(/^Export:\s*/i, "Export · ")}
            </span>
          )}
          <span className="hidden truncate md:inline">
            t {geoSnap.hours.toFixed(1)} h · {geoSnap.maxDepthM.toFixed(2)} m ·{" "}
            {hazardClassLabel(geoSnap.hazardClass)}
          </span>
          <span className="hidden uppercase lg:inline">
            {viewMode} · {waterStyle}
          </span>
        </div>
        {actions}
      </footer>
    );
  }

  if (screen !== "processing") {
    return (
      <footer className="flex h-7 shrink-0 items-center justify-between border-t border-edge bg-panel px-3 text-[11px] text-ink-dim">
        <div className="flex items-center gap-2">
          <Pill tone="idle">Ready</Pill>
          <span className="hidden sm:inline">Drop a capture or open a recent project</span>
        </div>
        {actions}
      </footer>
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

  const fmt = (secs: number) =>
    `${Math.max(0, Math.floor(secs / 60))}m ${Math.max(0, Math.floor(secs % 60))}s`;

  const tone = jobError ? "err" : resultPath ? "ok" : "live";
  const label = jobError
    ? "Failed"
    : resultPath
      ? "Complete"
      : activeStage
        ? activeStage.label
        : "Preparing";

  return (
    <footer className="flex h-7 shrink-0 items-center justify-between gap-3 border-t border-edge bg-panel px-3 text-[11px] tabular-nums text-ink-dim">
      <div className="flex min-w-0 items-center gap-3 overflow-hidden">
        <Pill tone={tone}>{label}</Pill>
        {running && activeStage && activeStage.progress > 0 && (
          <span className="text-accent2">{Math.round(activeStage.progress * 100)}%</span>
        )}
        {solvingCameras && (
          <span className="hidden truncate sm:inline">
            {registeredCameras}/{totalCameras} cams · {Math.round(trackingConfidence * 100)}%
          </span>
        )}
        <span className="hidden md:inline">{splatCount.toLocaleString()} splats</span>
        {totalSteps > 0 && (
          <span className="hidden lg:inline">
            {latestIter.toLocaleString()}/{totalSteps.toLocaleString()}
          </span>
        )}
        <span className="hidden xl:inline">{fps.toFixed(0)} fps</span>
        <span>{fmt(elapsed)}</span>
        {eta !== null && <span className="hidden sm:inline">ETA {fmt(eta)}</span>}
        {meshStatus && <span className="max-w-56 truncate text-ink" title={meshStatus}>{meshStatus}</span>}
      </div>
      {actions}
    </footer>
  );
}
