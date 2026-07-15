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
  const snap = useMemo(() => floodSnapshotFromTime(floodTime), [floodTime]);
  const meta = scenario ?? PLACEHOLDER_SCENARIO;

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
        Compare scientific ANUGA runs with the live preview once engines are connected. This panel
        stays linked to the hydrograph scrubber.
      </div>
    </div>
  );
}
