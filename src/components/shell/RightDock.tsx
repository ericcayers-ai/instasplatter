import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "../../state/store";
import GeoScenarioInspector from "../../geospatial/GeoScenarioInspector";
import VpsPanel from "../../geospatial/VpsPanel";
import { floodSnapshotFromTime, hazardClassLabel } from "../../geospatial/floodPreview";
import DockSection from "./DockSection";
import type { ShellFocus } from "./shellFocus";

function VizToggle({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label
      className={`flex cursor-pointer items-center justify-between gap-2 text-xs ${disabled ? "opacity-40" : ""}`}
    >
      <span>{label}</span>
      <input
        type="checkbox"
        className="accent-[var(--color-accent,#38b7a6)]"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
        aria-label={`Toggle ${label}`}
      />
    </label>
  );
}

function ReconStats() {
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

  const running = !resultPath && !jobError;
  const [liveElapsed, setLiveElapsed] = useState<number>(() =>
    elapsedSecs ?? (jobStartedAt ? (Date.now() - jobStartedAt) / 1000 : 0),
  );

  useEffect(() => {
    if (!running) return;
    const start = jobStartedAt ?? Date.now();
    const initialElapsed = elapsedSecs ?? 0;
    const interval = setInterval(() => {
      setLiveElapsed(initialElapsed + (Date.now() - start) / 1000);
    }, 250);
    return () => clearInterval(interval);
  }, [running, jobStartedAt, elapsedSecs]);

  if (screen !== "processing") {
    return <div className="text-xs text-ink-dim">Metrics appear while a job runs.</div>;
  }

  const elapsed = elapsedSecs ?? liveElapsed;
  const trainStage = stages.find((s) => s.id === "train");
  const eta =
    running && trainStage?.state === "active" && latestIter > 100 && totalSteps > 0
      ? (elapsed / latestIter) * (totalSteps - latestIter)
      : null;
  const fmt = (secs: number) =>
    `${Math.max(0, Math.floor(secs / 60))}m ${Math.max(0, Math.floor(secs % 60))}s`;

  const row = (label: string, value: string) => (
    <div className="flex justify-between gap-2 text-xs">
      <span className="text-ink-dim">{label}</span>
      <span className="tabular-nums text-ink">{value}</span>
    </div>
  );

  return (
    <div className="flex flex-col gap-1.5">
      {row("Splats", splatCount.toLocaleString())}
      {totalSteps > 0 &&
        row("Step", `${latestIter.toLocaleString()} / ${totalSteps.toLocaleString()}`)}
      {row("Viewport", `${fps.toFixed(0)} fps`)}
      {row("Elapsed", fmt(elapsed))}
      {eta !== null && row("ETA", fmt(eta))}
      {totalCameras > 0 &&
        row(
          "Cameras",
          `${registeredCameras}/${totalCameras} · ${Math.round(trackingConfidence * 100)}%`,
        )}
      {meshStatus && (
        <div className="mt-1 truncate text-[10px] text-ink-dim" title={meshStatus}>
          {meshStatus}
        </div>
      )}
      <div className="mt-1 flex justify-between text-xs">
        <span className="text-ink-dim">Status</span>
        <span>
          {jobError
            ? "Failed"
            : resultPath
              ? "Complete"
              : (stages.find((s) => s.state === "active")?.label ?? "Waiting")}
        </span>
      </div>
    </div>
  );
}

function NestedLog() {
  const logs = useStore((s) => s.logs);
  const open = useStore((s) => s.logConsoleOpen);
  const containerRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  useEffect(() => {
    const el = containerRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [logs, open]);

  if (!open) {
    return (
      <div className="text-xs text-ink-dim">
        Log closed. Press <span className="kbd">⌃L</span> or use the rail.
      </div>
    );
  }

  const pad = (n: number) => n.toString().padStart(2, "0");
  const stamp = (t: number) => {
    const d = new Date(t);
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  };

  return (
    <div
      ref={containerRef}
      onScroll={(e) => {
        const el = e.currentTarget;
        stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
      }}
      className="max-h-56 overflow-y-auto rounded border border-edge bg-bg px-2 py-1.5 font-mono text-[10px] leading-relaxed text-ink-dim"
    >
      {logs.length === 0 && <div className="text-ink-dim/60">No log output yet.</div>}
      {logs.map((l, i) => (
        <div key={i} className="whitespace-pre-wrap">
          <span className="text-ink-dim/60">{stamp(l.time)}</span> {l.line}
        </div>
      ))}
    </div>
  );
}

function GeoQuickStats() {
  const floodTime = useStore((s) => s.geoFloodTime);
  const preview = useStore((s) => s.geoPreview);
  const scientific = useStore((s) => s.geoScientificRun);
  const viewMode = useStore((s) => s.geoViewMode);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const snap = useMemo(() => floodSnapshotFromTime(floodTime, preview), [floodTime, preview]);

  return (
    <div className="flex flex-col gap-1.5 border-b border-edge px-3 py-2 text-xs">
      <div className="text-[10px] font-bold uppercase tracking-[0.12em] text-[var(--color-hydro)]">
        Stats
      </div>
      <div className="flex justify-between gap-2">
        <span className="text-ink-dim">Time</span>
        <span className="tabular-nums">{snap.hours.toFixed(1)} h</span>
      </div>
      <div className="flex justify-between gap-2">
        <span className="text-ink-dim">Max depth</span>
        <span className="tabular-nums">{snap.maxDepthM.toFixed(2)} m</span>
      </div>
      <div className="flex justify-between gap-2">
        <span className="text-ink-dim">Hazard</span>
        <span>{hazardClassLabel(snap.hazardClass)}</span>
      </div>
      <div className="flex justify-between gap-2">
        <span className="text-ink-dim">View</span>
        <span className="uppercase">
          {viewMode} · {waterStyle}
        </span>
      </div>
      {scientific && (
        <div className="truncate text-[10px] text-ink-dim" title={scientific.detail}>
          Flood {scientific.state}
        </div>
      )}
    </div>
  );
}

export default function RightDock({ focus }: { focus: ShellFocus | null }) {
  const suite = useStore((s) => s.suite);
  const rightPanelOpen = useStore((s) => s.rightPanelOpen);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);
  const logConsoleOpen = useStore((s) => s.logConsoleOpen);
  const setLogConsoleOpen = useStore((s) => s.setLogConsoleOpen);
  const reconLayers = useStore((s) => s.reconLayers);
  const setReconLayer = useStore((s) => s.setReconLayer);
  const postProcessing = useStore((s) => s.postProcessing);
  const setPostProcessing = useStore((s) => s.setPostProcessing);
  const totalCameras = useStore((s) => s.totalCameras);
  const sparseCloudPath = useStore((s) => s.sparseCloudPath);
  const denseCloudPath = useStore((s) => s.denseCloudPath);
  const latestSplatPath = useStore((s) => s.latestSplatPath);
  const exportDiagnosticsAction = useStore((s) => s.exportDiagnosticsAction);

  if (!rightPanelOpen) return null;

  if (suite === "geospatial") {
    return (
      <aside className="shell-dock shell-dock--right" aria-label="Scenario inspector">
        <div className="shell-dock__scroll">
          <GeoQuickStats />
          <div className="geo-scenario-root">
            <GeoScenarioInspector />
          </div>
          <DockSection
            id="dock-localise"
            title="Localise"
            focused={false}
            defaultOpen={false}
          >
            <VpsPanel />
          </DockSection>
          <DockSection
            id="dock-log"
            title="Log"
            focused={focus === "log"}
            open={logConsoleOpen}
            onOpenChange={(o) => setLogConsoleOpen(o)}
          >
            <NestedLog />
          </DockSection>
        </div>
      </aside>
    );
  }

  return (
    <aside className="shell-dock shell-dock--right" aria-label="Visualization and stats">
      <div className="flex items-center justify-between border-b border-edge px-3 py-1.5">
        <span className="text-[10px] text-ink-dim">
          Right dock <span className="kbd">Esc</span>
        </span>
        <button
          type="button"
          className="btn btn-ghost px-1.5 py-0.5 text-[10px]"
          onClick={() => setRightPanelOpen(false)}
        >
          Hide
        </button>
      </div>
      <div className="shell-dock__scroll">
        <DockSection
          id="dock-visualization"
          title="Visualization"
          tone="accent"
          focused={focus === "visualization"}
          defaultOpen
        >
          <div className="flex flex-col gap-1.5">
            <VizToggle
              label="Frustums"
              checked={reconLayers.cameras}
              disabled={totalCameras === 0}
              onChange={(v) => setReconLayer("cameras", v)}
            />
            <VizToggle
              label="Camera path"
              checked={reconLayers.cameraPath}
              onChange={(v) => setReconLayer("cameraPath", v)}
            />
            <VizToggle
              label="Sparse points"
              checked={reconLayers.sparse}
              disabled={!sparseCloudPath}
              onChange={(v) => setReconLayer("sparse", v)}
            />
            <VizToggle
              label="Dense points"
              checked={reconLayers.dense}
              disabled={!denseCloudPath}
              onChange={(v) => setReconLayer("dense", v)}
            />
            <VizToggle
              label="Splats"
              checked={reconLayers.splat}
              disabled={!latestSplatPath}
              onChange={(v) => setReconLayer("splat", v)}
            />
            <div className="my-1 border-t border-edge" />
            <VizToggle
              label="AgX tone mapping"
              checked={postProcessing.tonemap}
              disabled={!latestSplatPath}
              onChange={(v) => setPostProcessing("tonemap", v)}
            />
            <VizToggle
              label="FXAA edge smoothing"
              checked={postProcessing.fxaa}
              disabled={!latestSplatPath}
              onChange={(v) => setPostProcessing("fxaa", v)}
            />
            <div className="text-[10px] leading-snug text-ink-dim">
              Display processing applies to the reconstruction viewport and can be combined.
            </div>
          </div>
        </DockSection>

        <DockSection id="dock-stats" title="Stats" tone="brass" focused={focus === "stats"} defaultOpen>
          <ReconStats />
          <button
            type="button"
            onClick={() => void exportDiagnosticsAction()}
            className="btn btn-ghost-teal mt-3 w-full text-[10px]"
          >
            Diagnostics
          </button>
        </DockSection>

        <DockSection
          id="dock-log"
          title="Log"
          focused={focus === "log"}
          open={logConsoleOpen}
          onOpenChange={(o) => setLogConsoleOpen(o)}
        >
          <NestedLog />
        </DockSection>
      </div>
    </aside>
  );
}
