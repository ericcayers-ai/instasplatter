import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore, type GeoDemProvider } from "../state/store";
import {
  FLOOD_LAB_HYETOGRAPHS,
  FLOOD_LAB_MANNING,
  FLOOD_LAB_OUTLET_BC,
  PLACEHOLDER_SCENARIO,
} from "./defaults";
import { floodSnapshotFromTime, hazardClassLabel } from "./floodPreview";
import CollapsibleSection from "../components/shell/CollapsibleSection";
import { api, type GeoReference } from "../lib/ipc";
import {
  readStoredOpenTopoApiKey,
  writeStoredOpenTopoApiKey,
} from "./opentopo";
import { isGeoTiffPath } from "./demAuthority";

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
  const dem = useStore((s) => s.geoDemProduct);
  const startScientificFlood = useStore((s) => s.startScientificFlood);
  const cancelScientificFlood = useStore((s) => s.cancelScientificFlood);
  const exportFloodProducts = useStore((s) => s.exportFloodProducts);
  const prepareDem = useStore((s) => s.prepareGeoDemForAoi);
  const stageUserGeoTiff = useStore((s) => s.stageUserGeoTiffDem);
  const generateSplatDem = useStore((s) => s.generateSplatDemFromRecon);
  const generateSatSplat = useStore((s) => s.generateSatSplat);
  const satSplatBusy = useStore((s) => s.geoSatSplatBusy);
  const satSplatProduct = useStore((s) => s.geoSatSplatProduct);
  const experimentalOn = useStore((s) => !!(s.resolved?.experimentalMode));
  const aoi = useStore((s) => s.geoAoiWgs84);
  const setTool = useStore((s) => s.setGeoTool);
  const preview = useStore((s) => s.geoPreview);
  const workspace = useStore((s) => s.workspace);
  const queueItems = useStore((s) => s.queueItems);
  const [hyetoId, setHyetoId] = useState(
    () => scenario?.rainfallTemplate ?? PLACEHOLDER_SCENARIO.rainfallTemplate ?? "chicago_6h",
  );
  const [manningId, setManningId] = useState(
    () => scenario?.manningPreset ?? PLACEHOLDER_SCENARIO.manningPreset ?? "mixed_urban",
  );
  const [outletId, setOutletId] = useState("south_stage");
  const [geoRef, setGeoRef] = useState<GeoReference | null | undefined>(undefined);
  const [splatDemBusy, setSplatDemBusy] = useState(false);
  const [demBusy, setDemBusy] = useState(false);
  const [openTopoKey, setOpenTopoKey] = useState(() => readStoredOpenTopoApiKey() ?? "");
  const [openTopoSaved, setOpenTopoSaved] = useState(false);

  const resolvedWorkspace =
    workspace ??
    queueItems.find((i) => (i.suite ?? "reconstruction") === "geospatial" && i.workspace)?.workspace ??
    queueItems.find((i) => i.workspace)?.workspace ??
    null;

  useEffect(() => {
    let cancelled = false;
    if (!resolvedWorkspace) {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- initialization guard
      setGeoRef(null);
      return;
    }
    void api
      .getGeoReference(resolvedWorkspace)
      .then((ref) => {
        if (!cancelled) setGeoRef(ref);
      })
      .catch(() => {
        if (!cancelled) setGeoRef(null);
      });
    return () => {
      cancelled = true;
    };
  }, [resolvedWorkspace, dem?.dtmPath]);

  const registrationOk =
    !!geoRef &&
    (geoRef.scaleStatus === "metric" || geoRef.scaleStatus === "approx") &&
    !!geoRef.localOrigin;
  const registrationBlock =
    geoRef === undefined
      ? null
      : registrationOk
        ? null
        : geoRef?.scaleStatus === "unscaled"
          ? "Geo registration is unscaled — import telemetry or set GCPs before Generate DEM from splat."
          : "Geo registration missing — import telemetry or set GCPs (metric GeoReference) before Generate DEM from splat.";

  const snap = useMemo(
    () => floodSnapshotFromTime(floodTime, preview, scenario?.durationHours),
    [floodTime, preview, scenario?.durationHours],
  );
  const meta = scenario ?? PLACEHOLDER_SCENARIO;
  const running = scientific?.state === "running";
  const demoMode =
    scientific?.mode === "demo" ||
    meta.engineLabel.includes("Demo") ||
    meta.authority === "demo" ||
    !!dem?.synthetic;
  const splatProxyDem = dem?.bedSource === "proxy";

  const runPrepare = (provider: GeoDemProvider) => {
    setDemBusy(true);
    void prepareDem({ provider }).finally(() => setDemBusy(false));
  };

  const pickGeoTiffDem = async () => {
    const sel = await open({
      title: "Use GeoTIFF DEM",
      multiple: false,
      filters: [
        {
          name: "GeoTIFF / COG",
          extensions: ["tif", "tiff", "geotiff", "cog"],
        },
      ],
    });
    if (!sel || Array.isArray(sel)) return;
    if (!isGeoTiffPath(sel)) return;
    setDemBusy(true);
    void stageUserGeoTiff(sel).finally(() => setDemBusy(false));
  };

  const saveOpenTopoKey = () => {
    writeStoredOpenTopoApiKey(openTopoKey.trim() || null);
    setOpenTopoSaved(true);
    window.setTimeout(() => setOpenTopoSaved(false), 1600);
  };

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
              Demo mode — DEM missing or synthetic. Soft/HAND = Live preview (non-authoritative). Not
              scientifically authoritative.
            </p>
          )}
          {!demoMode && dem && !dem.synthetic && (
            <p className="mt-2 rounded border border-[var(--color-hydro)]/30 bg-[var(--color-hydro)]/10 px-2 py-1.5 text-[10px] leading-snug text-ink-dim">
              {splatProxyDem
                ? "Splat→DEM proxy bed · non-authoritative vs USGS / OpenTopo. Soft + HAND remain Live preview."
                : "Real DEM staged · soft + HAND remain Live preview until ANUGA compare gates pass."}
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

      <CollapsibleSection title="Flood lab" defaultOpen badge="draft">
        <div className="space-y-2 py-2 text-[11px]">
          <div>
            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
              Rainfall hyetograph
            </div>
            <div className="flex flex-wrap gap-1">
              {FLOOD_LAB_HYETOGRAPHS.map((h) => (
                <button
                  key={h.id}
                  type="button"
                  className={`btn px-2 py-0.5 text-[10px] ${hyetoId === h.id ? "btn-active" : ""}`}
                  onClick={() => setHyetoId(h.id)}
                  title={`${h.authority} — not calibrated`}
                >
                  {h.label}
                </button>
              ))}
            </div>
          </div>
          <div>
            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
              Manning n
            </div>
            <div className="flex flex-wrap gap-1">
              {FLOOD_LAB_MANNING.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  className={`btn px-2 py-0.5 text-[10px] ${manningId === m.id ? "btn-active" : ""}`}
                  onClick={() => setManningId(m.id)}
                >
                  {m.label} ({m.n})
                </button>
              ))}
            </div>
          </div>
          <div>
            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
              Outlet BC
            </div>
            <div className="flex flex-wrap gap-1">
              {FLOOD_LAB_OUTLET_BC.map((o) => (
                <button
                  key={o.id}
                  type="button"
                  className={`btn px-2 py-0.5 text-[10px] ${outletId === o.id ? "btn-active" : ""}`}
                  onClick={() => setOutletId(o.id)}
                  title={o.detail}
                >
                  {o.label}
                </button>
              ))}
            </div>
          </div>

          <div className="border-t border-edge/60 pt-2">
            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
              DEM bed
            </div>
            <div className="flex flex-wrap gap-1">
              <button
                type="button"
                className="btn px-2 py-1 text-[11px]"
                disabled={demBusy || !aoi}
                onClick={() => runPrepare("auto")}
                title="Try USGS 3DEP, then OpenTopography if keyed; else stage local / synthetic"
              >
                {demBusy ? "Staging…" : "Stage DEM"}
              </button>
              <button
                type="button"
                className="btn px-2 py-1 text-[11px]"
                disabled={demBusy || !aoi}
                onClick={() => runPrepare("usgs-3dep")}
                title="Fetch USGS 3DEP for the AOI (CONUS / US territories)"
              >
                USGS 3DEP
              </button>
              <button
                type="button"
                className="btn px-2 py-1 text-[11px]"
                disabled={demBusy || !aoi}
                onClick={() => runPrepare("opentopography")}
                title={
                  openTopoKey.trim()
                    ? "Fetch OpenTopography COP30 for the AOI"
                    : "Needs OpenTopography API key (Advanced below)"
                }
              >
                OpenTopo
              </button>
              <button
                type="button"
                className="btn px-2 py-1 text-[11px]"
                disabled={demBusy}
                onClick={() => void pickGeoTiffDem()}
                title="Use a local GeoTIFF / COG as the flood DEM bed"
              >
                Use GeoTIFF DEM…
              </button>
            </div>
            {dem && (
              <p className="mt-1.5 font-mono text-[9px] leading-snug text-ink-dim">
                {dem.synthetic
                  ? "synthetic"
                  : dem.bedSource === "proxy"
                    ? "splat proxy"
                    : "real"}
                {dem.dtmPath ? ` · ${dem.dtmPath.split(/[/\\]/).pop()}` : ""}
              </p>
            )}
          </div>

          <div className="border-t border-edge/60 pt-2">
            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
              Terrain from splat
            </div>
            <button
              type="button"
              className="btn px-2 py-1 text-[11px]"
              disabled={splatDemBusy || !aoi || !registrationOk}
              title={
                registrationBlock ??
                (aoi
                  ? "Rasterize registered splat heights to a proxy DEM (non-authoritative)"
                  : "Draw an AOI first")
              }
              onClick={() => {
                setSplatDemBusy(true);
                void generateSplatDem().finally(() => setSplatDemBusy(false));
              }}
            >
              {splatDemBusy ? "Generating…" : "Generate DEM from splat"}
            </button>
            {registrationBlock && (
              <p className="mt-1.5 rounded border border-danger/40 bg-danger/10 px-2 py-1.5 text-[10px] leading-snug text-danger/90">
                {registrationBlock}
              </p>
            )}
            {!registrationBlock && registrationOk && (
              <p className="mt-1.5 text-[9px] leading-snug text-[var(--color-gauge)]">
                Proxy bed from reconstruction — non-authoritative vs USGS / OpenTopography.
              </p>
            )}
          </div>

          <p className="text-[9px] leading-snug text-[var(--color-gauge)]">
            Authority: draft templates — Live preview / non-authoritative until ANUGA validates.
          </p>
        </div>
      </CollapsibleSection>

      <CollapsibleSection title="Satellite Splat" defaultOpen={false} badge="anywhere">
        <div className="space-y-2 p-2">
          <p className="text-[9px] leading-snug text-ink-dim">
            Build a Gaussian splat straight from public elevation + imagery catalogs — no
            on-site capture or reconstruction required. Works for any AOI on the map.
          </p>
          <button
            type="button"
            className="btn px-2 py-1 text-[11px]"
            disabled={satSplatBusy || !aoi}
            title={aoi ? "Fetch DEM + imagery for this AOI and build a splat" : "Draw an AOI first"}
            onClick={() => {
              void generateSatSplat();
            }}
          >
            {satSplatBusy ? "Generating…" : "Generate Satellite Splat"}
          </button>
          {satSplatProduct && (
            <p className="mt-1.5 font-mono text-[9px] leading-snug text-ink-dim">
              {satSplatProduct.points} pts · {satSplatProduct.cols}×{satSplatProduct.rows} ·{" "}
              {satSplatProduct.demSource}
              {satSplatProduct.imagerySource ? ` + ${satSplatProduct.imagerySource}` : " (colormap)"}
            </p>
          )}
          <p className="text-[9px] leading-snug text-[var(--color-gauge)]">
            Authority: draft — catalog-derived, non-authoritative Live preview.
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
            {running ? "Running…" : scientific?.state === "failed" ? "Retry flood" : "Start flood"}
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
            title="Write COG/GeoTIFF, GeoJSON, time series, and scenario manifest (DEM-only OK)"
            onClick={() => void exportFloodProducts()}
          >
            Export products
          </button>
        </div>
        {floodEngine && (
          <div className="pb-2 font-mono text-[10px] text-ink-dim">
            ANUGA {floodEngine.anugaReady ? "launcher found" : "not installed"} · SWMM{" "}
            {floodEngine.swmmReady ? "scaffold ready" : "scaffold"}
            {dem ? ` · DEM ${dem.synthetic ? "synthetic" : dem.bedSource === "proxy" ? "splat proxy" : "real"}` : ""}
          </div>
        )}
        {scientific && (
          <div className="space-y-1 pb-2 text-[11px]">
            <div className="flex justify-between gap-2">
              <span className="text-ink-dim">State</span>
              <span
                className={`tabular-nums ${scientific.state === "failed" ? "text-[var(--color-danger,#e05a4e)]" : ""}`}
              >
                {scientific.state}
              </span>
            </div>
            <div className="flex justify-between gap-2">
              <span className="text-ink-dim">Progress</span>
              <span className="tabular-nums">{Math.round(scientific.progress * 100)}%</span>
            </div>
            <p
              className={`text-[10px] leading-snug ${scientific.state === "failed" ? "text-[var(--color-danger,#e05a4e)]" : "text-ink-dim"}`}
            >
              {scientific.state === "failed" && !scientific.detail
                ? "Flood run failed. Try again or check engine status above."
                : scientific.detail}
            </p>
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
              { id: "globe" as const, label: "Globe" },
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
        <div className="space-y-2 py-2">
          <div>
            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
              OpenTopography API key
            </div>
            <input
              type="password"
              autoComplete="off"
              spellCheck={false}
              className="w-full rounded border border-edge bg-panel px-2 py-1 font-mono text-[11px] text-ink"
              placeholder="Paste key (stored locally)"
              value={openTopoKey}
              onChange={(e) => setOpenTopoKey(e.target.value)}
              aria-label="OpenTopography API key"
            />
            <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
              <button type="button" className="btn px-2 py-0.5 text-[10px]" onClick={saveOpenTopoKey}>
                Save key
              </button>
              <button
                type="button"
                className="btn px-2 py-0.5 text-[10px]"
                onClick={() => {
                  setOpenTopoKey("");
                  writeStoredOpenTopoApiKey(null);
                  setOpenTopoSaved(true);
                  window.setTimeout(() => setOpenTopoSaved(false), 1600);
                }}
              >
                Clear
              </button>
              {openTopoSaved && (
                <span className="text-[9px] text-[var(--color-hydro)]">Saved</span>
              )}
            </div>
            <p className="mt-1.5 text-[9px] leading-snug text-ink-dim">
              Persisted like the Cesium ion token (
              <span className="font-mono">localStorage</span>). Register at opentopography.org.
              Env <span className="font-mono">OPENTOPOGRAPHY_API_KEY</span> also works.
            </p>
          </div>
          <div className="flex items-center justify-between gap-3 border-t border-edge/60 pt-2">
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
          <p className="pb-1 text-[10px] leading-snug text-ink-dim">
            Soft + HAND = Live preview. Scientific ANUGA streams on the CPU lane via{" "}
            <span className="font-mono">sim://event</span>. Globe uses ellipsoid + flood overlay until
            local terrain tiles (<span className="font-mono">layer.json</span>) are staged — ion stays
            blank on Standard.
          </p>
        </div>
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
            <span className="text-ink">SWMM</span> network + soft/HAND preview. Demo extents are never
            authoritative.
          </p>
          <p>
            Experimental hydro (TRITON / Wflow / GeoClaw external; GPL plugins only outside the
            installer) requires Experimental Mode from the TitleBar and must clear promotion gates
            before any Standard claim.
          </p>
          <p>
            Multi-hazard stubs (quake / wildfire / landslide / tsunami) are GDACS / USGS feed cards
            or STAC links in the left Hazards palette — <span className="text-ink">no</span>{" "}
            authoritative physics and no fake solvers.
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
