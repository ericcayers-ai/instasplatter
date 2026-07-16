import { useMemo } from "react";
import { useStore } from "../state/store";
import { PLACEHOLDER_SCENARIO } from "./defaults";
import { floodSnapshotFromTime, hazardClassLabel } from "./floodPreview";
import CollapsibleSection from "../components/shell/CollapsibleSection";

/**
 * Scenario inspector for the right panel when the geospatial suite is active.
 * Grouped to match Settings navigation: Geospatial / Flood / View / Advanced.
 */
export default function GeoScenarioInspector() {
  const scenario = useStore((s) => s.geoScenario);
  const floodTime = useStore((s) => s.geoFloodTime);
  const setFloodTime = useStore((s) => s.setGeoFloodTime);
  const viewMode = useStore((s) => s.geoViewMode);
  const setViewMode = useStore((s) => s.setGeoViewMode);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const setWaterStyle = useStore((s) => s.setGeoWaterStyle);
  const lowPower = useStore((s) => s.geoFloodLowPower);
  const setLowPower = useStore((s) => s.setGeoFloodLowPower);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);
  const scientific = useStore((s) => s.geoScientificRun);
  const floodEngine = useStore((s) => s.geoFloodEngine);
  const startScientificFlood = useStore((s) => s.startScientificFlood);
  const cancelScientificFlood = useStore((s) => s.cancelScientificFlood);
  const exportFloodProducts = useStore((s) => s.exportFloodProducts);
  const experimentalOn = useStore((s) => !!(s.resolved?.experimentalMode));
  const aoi = useStore((s) => s.geoAoiWgs84);
  const setTool = useStore((s) => s.setGeoTool);
  const preview = useStore((s) => s.geoPreview);
  const snap = useMemo(
    () => floodSnapshotFromTime(floodTime, preview, scenario?.durationHours),
    [floodTime, preview, scenario?.durationHours],
  );
  const meta = scenario ?? PLACEHOLDER_SCENARIO;
  const running = scientific?.state === "running";
  const demoMode = scientific?.mode === "demo" || meta.engineLabel.includes("Demo");

  return (
    <div className="flex w-72 shrink-0 flex-col overflow-y-auto border-l border-edge bg-panel">
      <div className="flex items-center justify-between border-b border-edge px-3 py-2">
        <div className="text-xs font-semibold">Settings</div>
        <button
          type="button"
          className="btn px-2 py-0.5 text-[10px]"
          onClick={() => setRightPanelOpen(false)}
          title="Hide settings panel"
        >
          Hide
        </button>
      </div>

      {experimentalOn && (
        <div className="border-b border-danger/30 bg-danger/10 px-3 py-2 text-[10px] leading-snug text-danger/90">
          Experimental Mode is on via the TitleBar. Experimental hydro adapters stay behind promotion
          gates.
        </div>
      )}

      <CollapsibleSection title="Geospatial" defaultOpen badge={meta.name}>
        <div className="py-2">
          <div className="font-display text-sm font-semibold tracking-tight text-ink">{meta.name}</div>
          <div className="mt-1 text-[11px] text-ink-dim">{meta.engineLabel}</div>
          <p className="mt-2 text-[11px] leading-relaxed text-ink-dim">{meta.note}</p>
          {demoMode && (
            <p className="mt-2 rounded border border-[var(--color-gauge)]/40 bg-[var(--color-gauge)]/10 px-2 py-1.5 text-[10px] leading-snug text-[var(--color-gauge)]">
              Demo mode — extents are synthetic. Not scientifically authoritative.
            </p>
          )}
          <div className="mt-2 flex flex-wrap items-center gap-1.5">
            <button
              type="button"
              className="btn px-2 py-1 text-[11px]"
              onClick={() => setTool("drawAoi")}
              title="Draw or edit the area of interest on the map"
            >
              {aoi ? "Edit AOI" : "Draw AOI"}
            </button>
            {aoi && (
              <span className="font-mono text-[9px] text-ink-dim">
                [{aoi.map((v) => v.toFixed(4)).join(", ")}]
              </span>
            )}
          </div>
          <p className="mt-2 text-[10px] leading-snug text-ink-dim">
            AOI binds the flood domain. Esri World Imagery is the default basemap — attribution
            required (see About).
          </p>
        </div>
      </CollapsibleSection>

      <CollapsibleSection
        title="Flood engine"
        defaultOpen
        badge={
          scientific
            ? `${scientific.state}${scientific.progress > 0 ? ` ${Math.round(scientific.progress * 100)}%` : ""}`
            : undefined
        }
      >
        <div className="flex flex-wrap gap-1.5 py-2">
          <button
            type="button"
            className="btn btn-primary px-2 py-1 text-[11px]"
            disabled={running || !aoi}
            title={aoi ? "Start scientific flood on the committed AOI" : "Draw an AOI first"}
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
          <div className="pb-2 font-mono text-[10px] text-ink-dim">
            ANUGA {floodEngine.anugaReady ? "launcher found" : "not installed"} · SWMM{" "}
            {floodEngine.swmmReady ? "scaffold ready" : "scaffold"}
          </div>
        )}
        {scientific && (
          <div className="space-y-1 pb-2 text-[11px]">
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
        <div className="border-t border-edge/60 py-2">
          <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
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
        <dl className="space-y-1.5 border-t border-edge/60 py-2 text-xs">
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Status</dt>
            <dd>{snap.statusLabel}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Stage</dt>
            <dd className="tabular-nums">{snap.stageM.toFixed(2)} m</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Max depth</dt>
            <dd className="tabular-nums text-[var(--color-hydro)]">{snap.maxDepthM.toFixed(2)} m</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Hazard</dt>
            <dd>{hazardClassLabel(snap.hazardClass)}</dd>
          </div>
        </dl>
      </CollapsibleSection>

      <CollapsibleSection title="Water style" defaultOpen={false}>
        <div className="flex flex-wrap gap-1.5 py-2">
          {(
            [
              { id: "depth" as const, label: "Depth" },
              { id: "hazard" as const, label: "Hazard" },
              { id: "contour" as const, label: "Contour" },
            ] as const
          ).map((s) => (
            <button
              key={s.id}
              type="button"
              className={`btn px-2 py-1 text-[11px] ${waterStyle === s.id ? "btn-active" : ""}`}
              aria-pressed={waterStyle === s.id}
              onClick={() => setWaterStyle(s.id)}
            >
              {s.label}
            </button>
          ))}
        </div>
        <div className="flex flex-wrap gap-1.5 pb-2">
          {(
            [
              { id: "2d" as const, label: "2D map" },
              { id: "3d" as const, label: "3D workspace" },
            ] as const
          ).map((m) => (
            <button
              key={m.id}
              type="button"
              className={`btn px-2 py-1 text-[11px] ${viewMode === m.id ? "btn-active" : ""}`}
              aria-pressed={viewMode === m.id}
              onClick={() => setViewMode(m.id)}
            >
              {m.label}
            </button>
          ))}
        </div>
        <dl className="space-y-1.5 pb-2 text-xs">
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Mode</dt>
            <dd className="uppercase">{viewMode}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-ink-dim">Water graphics</dt>
            <dd className="capitalize">{waterStyle}</dd>
          </div>
        </dl>
      </CollapsibleSection>

      <CollapsibleSection title="Advanced" defaultOpen={false}>
        <div className="flex items-center justify-between gap-3 py-2">
          <div>
            <div className="text-xs">Low-power preview</div>
            <div className="text-[10px] leading-snug text-ink-dim">
              Coarser soft-solver grid; better on battery / integrated GPUs.
            </div>
          </div>
          <button
            type="button"
            className={`btn px-2 py-0.5 text-[10px] ${lowPower ? "btn-active" : ""}`}
            aria-pressed={lowPower}
            onClick={() => setLowPower(!lowPower)}
          >
            {lowPower ? "On" : "Off"}
          </button>
        </div>
        <p className="pb-2 text-[10px] leading-snug text-ink-dim">
          Scientific ANUGA runs stream on the CPU lane via <span className="font-mono">sim://event</span>.
          Live preview stays labelled until validation tolerances pass.
        </p>
      </CollapsibleSection>

      <CollapsibleSection
        title="Experimental engines"
        defaultOpen={false}
        tone="danger"
        badge={experimentalOn ? "ON" : "off"}
      >
        <div className="space-y-2 py-2 text-[10px] leading-snug text-ink-dim">
          <p>
            Standard flood path: <span className="text-ink">ANUGA</span> (scientific) +{" "}
            <span className="text-ink">SWMM</span> network + soft preview. Demo extents are never
            authoritative.
          </p>
          <p>
            Experimental hydro (TRITON / Wflow / GeoClaw external; GPL plugins only outside the
            installer) requires Experimental Mode from the TitleBar and must clear promotion gates
            before any Standard claim.
          </p>
          <p className="font-mono text-[9px]">
            ANUGA {floodEngine?.anugaReady ? "ready" : "missing"} · SWMM{" "}
            {floodEngine?.swmmReady ? "ready" : "scaffold"}
          </p>
        </div>
      </CollapsibleSection>
    </div>
  );
}
