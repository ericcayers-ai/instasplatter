import type { ReactNode } from "react";
import { useMemo } from "react";
import { useStore } from "../../state/store";
import { floodSnapshotFromTime, hazardClassLabel } from "../../geospatial/floodPreview";

function Pill({
  tone,
  children,
}: {
  tone: "idle" | "live" | "ok" | "err";
  children: ReactNode;
}) {
  return (
    <span className="status-pill" data-tone={tone}>
      {children}
    </span>
  );
}

function BrassProgress({ pct }: { pct: number }) {
  const clamped = Math.max(0, Math.min(100, pct));
  return (
    <div className="status-progress" title={`${clamped}%`}>
      <div className="status-progress__fill" style={{ width: `${clamped}%` }} />
      {clamped > 0 && clamped < 100 && (
        <span className="status-progress__spark" style={{ left: `${clamped}%` }} aria-hidden />
      )}
    </div>
  );
}

export default function StatusBar() {
  const suite = useStore((s) => s.suite);
  const screen = useStore((s) => s.screen);
  const stages = useStore((s) => s.stages);
  const resultPath = useStore((s) => s.resultPath);
  const jobError = useStore((s) => s.jobError);
  const logConsoleOpen = useStore((s) => s.logConsoleOpen);
  const setLogConsoleOpen = useStore((s) => s.setLogConsoleOpen);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);
  const floodTime = useStore((s) => s.geoFloodTime);
  const preview = useStore((s) => s.geoPreview);
  const scientific = useStore((s) => s.geoScientificRun);
  const pipelineChips = useStore((s) => s.pipelineChips);
  const geoSnap = useMemo(
    () => floodSnapshotFromTime(floodTime, preview),
    [floodTime, preview],
  );

  const openLog = () => {
    setRightPanelOpen(true);
    setLogConsoleOpen(!logConsoleOpen);
  };

  const openStats = () => {
    setRightPanelOpen(true);
  };

  const actions = (
    <div className="flex items-center gap-2">
      <button type="button" onClick={openStats} className="btn btn-ghost px-1.5 py-0.5 text-[10px]" aria-label="Open statistics">
        Stats
      </button>
      <button
        type="button"
        onClick={openLog}
        className={`btn btn-ghost px-1.5 py-0.5 text-[10px] ${logConsoleOpen ? "btn-active" : ""}`}
        title="Toggle log (Ctrl+L)"
        aria-label={logConsoleOpen ? "Hide log" : "Show log"}
      >
        {logConsoleOpen ? "Hide log" : "Log"}
        <span className="kbd ml-1 hidden sm:inline">Ctrl+L</span>
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
      <footer className="status-ribbon" role="status">
        <div className="status-ribbon__live min-w-0 overflow-hidden">
          <Pill tone={tone}>
            {scientific?.state === "running"
              ? "Live"
              : preview?.validation === "validated"
                ? "Validated"
                : "Ready"}
          </Pill>
          {floodPct != null && (
            <>
              <BrassProgress pct={floodPct} />
              <span className="tabular-nums text-accent2">{floodPct}%</span>
            </>
          )}
          {scientific && (
            <span title={scientific.detail} className="hidden truncate md:inline">
              Flood {scientific.state}
            </span>
          )}
          {pipelineChips.export && (
            <span className="hidden max-w-40 truncate lg:inline" title={pipelineChips.export}>
              {pipelineChips.export.replace(/^Export:\s*/i, "Export · ")}
            </span>
          )}
          <span className="hidden truncate xl:inline">
            t {geoSnap.hours.toFixed(1)} h · {geoSnap.maxDepthM.toFixed(2)} m ·{" "}
            {hazardClassLabel(geoSnap.hazardClass)}
          </span>
        </div>
        {actions}
      </footer>
    );
  }

  if (screen !== "processing") {
    return (
      <footer className="status-ribbon" role="status">
        <div className="status-ribbon__live">
          <Pill tone="idle">Ready</Pill>
          <span className="hidden sm:inline">Drop a capture or open a recent project</span>
        </div>
        {actions}
      </footer>
    );
  }

  const running = !resultPath && !jobError;
  const activeStage = stages.find((s) => s.state === "active");
  const pct =
    running && activeStage && activeStage.progress > 0
      ? Math.round(activeStage.progress * 100)
      : resultPath
        ? 100
        : 0;
  const tone = jobError ? "err" : resultPath ? "ok" : "live";
  const label = jobError ? "Failed" : resultPath ? "Complete" : "Live";

  return (
    <footer className="status-ribbon" role="status">
      <div className="status-ribbon__live min-w-0 overflow-hidden">
        <Pill tone={tone}>{label}</Pill>
        {(running || resultPath) && !jobError && (
          <>
            <BrassProgress pct={pct} />
            <span className="tabular-nums text-accent2">{pct}%</span>
          </>
        )}
        {activeStage && running && (
          <span className="hidden truncate sm:inline">{activeStage.label}</span>
        )}
        <span className="hidden text-[10px] text-ink-dim md:inline">Details in STATS</span>
      </div>
      {actions}
    </footer>
  );
}
