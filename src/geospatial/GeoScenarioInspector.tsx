import { useMemo } from "react";
import { useStore } from "../state/store";
import { PLACEHOLDER_SCENARIO } from "./defaults";
import { floodSnapshotFromTime, hazardClassLabel } from "./floodPreview";

/**
 * Scenario inspector for the right panel when the geospatial suite is active.
 */
export default function GeoScenarioInspector() {
  const scenario = useStore((s) => s.geoScenario);
  const floodTime = useStore((s) => s.geoFloodTime);
  const setFloodTime = useStore((s) => s.setGeoFloodTime);
  const viewMode = useStore((s) => s.geoViewMode);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);
  const scientific = useStore((s) => s.geoScientificRun);
  const floodEngine = useStore((s) => s.geoFloodEngine);
  const startScientificFlood = useStore((s) => s.startScientificFlood);
  const cancelScientificFlood = useStore((s) => s.cancelScientificFlood);
  const exportFloodProducts = useStore((s) => s.exportFloodProducts);
  const snap = useMemo(() => floodSnapshotFromTime(floodTime), [floodTime]);
  const meta = scenario ?? PLACEHOLDER_SCENARIO;
  const running = scientific?.state === "running";
  const demoMode = scientific?.mode === "demo" || meta.engineLabel.includes("Demo");

  return (
    <div className="flex w-72 shrink-0 flex-col overflow-y-auto border-l border-edge bg-panel">
      <div className="flex items-center justify-between border-b border-edge px-3 py-2">
        <div className="text-xs font-semibold">Scenario</div>
        <button
          type="button"
          className="btn px-2 py-0.5 text-[10px]"
          onClick={() => setRightPanelOpen(false)}
          title="Hide scenario panel"
        >
          Hide
        </button>
      </div>

      <div className="border-b border-edge px-3 py-3">
        <div className="font-display text-sm font-semibold tracking-tight text-ink">{meta.name}</div>
        <div className="mt-1 text-[11px] text-ink-dim">{meta.engineLabel}</div>
        <p className="mt-2 text-[11px] leading-relaxed text-ink-dim">{meta.note}</p>
        {demoMode && (
          <p className="mt-2 rounded border border-[var(--color-gauge)]/40 bg-[var(--color-gauge)]/10 px-2 py-1.5 text-[10px] leading-snug text-[var(--color-gauge)]">
            Demo mode — extents are synthetic. Not scientifically authoritative.
          </p>
        )}
      </div>

      <div className="border-b border-edge px-3 py-3">
        <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
          Scientific run
        </div>
        <div className="flex flex-wrap gap-1.5">
          <button
            type="button"
            className="btn btn-primary px-2 py-1 text-[11px]"
            disabled={running}
            onClick={() => void startScientificFlood({ allowDemo: true })}
          >
            {running ? "Running…" : "Start flood"}
          </button>
          <button
            type="button"
            className="btn px-2 py-1 text-[11px]"
            disabled={!running}
            onClick={() => void cancelScientificFlood()}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn px-2 py-1 text-[11px]"
            disabled={running}
            title="Write COG/GeoTIFF, GeoJSON, time series, SPZ, and scenario manifest under geo/exports"
            onClick={() => void exportFloodProducts()}
          >
            Export products
          </button>
        </div>
        {floodEngine && (
          <div className="mt-2 font-mono text-[10px] text-ink-dim">
            ANUGA {floodEngine.anugaReady ? "launcher found" : "not installed"} · SWMM{" "}
            {floodEngine.swmmReady ? "scaffold ready" : "scaffold"}
          </div>
        )}
        {scientific && (
          <div className="mt-2 space-y-1 text-[11px]">
            <div className="flex justify-between gap-2">
              <span className="text-ink-dim">State</span>
              <span className="tabular-nums">{scientific.state}</span>
            </div>
            <div className="flex justify-between gap-2">
              <span className="text-ink-dim">Progress</span>
              <span className="tabular-nums">{Math.round(scientific.progress * 100)}%</span>
            </div>
            <p className="text-[10px] leading-snug text-ink-dim">{scientific.detail}</p>
            {scientific.massBalance != null && (
              <div className="flex justify-between gap-2">
                <span className="text-ink-dim">Mass bal.</span>
                <span className="tabular-nums">{scientific.massBalance.toFixed(4)}</span>
              </div>
            )}
          </div>
        )}
      </div>

      <div className="border-b border-edge px-3 py-3">
        <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
          Time
        </div>
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={floodTime}
          onChange={(e) => setFloodTime(Number(e.target.value))}
          aria-label="Scenario time"
          className="w-full"
        />
        <div className="mt-1.5 flex justify-between font-mono text-[10px] tabular-nums text-ink-dim">
          <span>0 h</span>
          <span className="text-ink">{snap.hours.toFixed(1)} h</span>
          <span>{meta.durationHours} h</span>
        </div>
      </div>

      <div className="border-b border-edge px-3 py-3">
        <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
          At scrub
        </div>
        <dl className="space-y-1.5 text-xs">
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Status</dt>
            <dd>{snap.statusLabel}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Stage</dt>
            <dd className="tabular-nums">{snap.stageM.toFixed(2)} m</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Discharge</dt>
            <dd className="tabular-nums">{snap.dischargeCms.toFixed(0)} m³/s</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Max depth</dt>
            <dd className="tabular-nums text-[var(--color-hydro)]">{snap.maxDepthM.toFixed(2)} m</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Wet area</dt>
            <dd className="tabular-nums">{Math.round(snap.wetFraction * 100)}%</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Hazard</dt>
            <dd>{hazardClassLabel(snap.hazardClass)}</dd>
          </div>
        </dl>
      </div>

      <div className="border-b border-edge px-3 py-3">
        <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
          View
        </div>
        <dl className="space-y-1.5 text-xs">
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Mode</dt>
            <dd className="uppercase">{viewMode}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Water graphics</dt>
            <dd className="capitalize">{waterStyle}</dd>
          </div>
        </dl>
      </div>

      <div className="px-3 py-3 text-[11px] leading-relaxed text-ink-dim">
        Scientific ANUGA runs stream on the CPU lane via <span className="font-mono">sim://event</span>.
        Live preview remains a separate labelled path until validation tolerances pass.
      </div>
    </div>
  );
}
