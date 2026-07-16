import { useStore } from "../state/store";
import type { GeoBasemapMode, GeoTool, GeoViewMode } from "./types";

export default function GeoToolbar() {
  const viewMode = useStore((s) => s.geoViewMode);
  const setViewMode = useStore((s) => s.setGeoViewMode);
  const tool = useStore((s) => s.geoTool);
  const setTool = useStore((s) => s.setGeoTool);
  const inspectHint = useStore((s) => s.geoInspectHint);
  const rightPanelOpen = useStore((s) => s.rightPanelOpen);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);
  const basemapMode = useStore((s) => s.geoBasemapMode);
  const setBasemapMode = useStore((s) => s.setGeoBasemapMode);
  const aoi = useStore((s) => s.geoAoiWgs84);
  const clearGeoAoi = useStore((s) => s.clearGeoAoi);

  const modes: { id: GeoViewMode; label: string; title: string }[] = [
    { id: "3d", label: "3D", title: "ENU 3D workspace (terrain, water, splat)" },
    { id: "2d", label: "2D", title: "2D satellite MapLibre (AOI draw, flood overlay)" },
  ];
  const tools: { id: GeoTool; label: string; title: string }[] = [
    { id: "pan", label: "Pan", title: "Pan and zoom the map" },
    { id: "drawAoi", label: "AOI", title: "Draw or replace the flood area of interest" },
    { id: "inspect", label: "Inspect", title: "Click to sample flood depth under the cursor" },
    { id: "measure", label: "Measure", title: "Distance measure — stub until survey tools land" },
    { id: "profile", label: "Profile", title: "Cross-section profile — stub until DEM tools land" },
  ];
  const basemaps: { id: GeoBasemapMode; label: string; title: string }[] = [
    { id: "satellite", label: "Satellite", title: "Esri World Imagery (attribution required)" },
    { id: "lowBandwidth", label: "Low BW", title: "Carto dark / OSM fallback" },
  ];

  return (
    <div className="pointer-events-none absolute inset-x-0 top-0 z-10 flex flex-col gap-2 p-2">
      <div className="pointer-events-auto flex flex-wrap items-center gap-2">
        <div className="flex overflow-hidden rounded border border-edge bg-panel/90 shadow-sm backdrop-blur-sm">
          {modes.map((m) => (
            <button
              key={m.id}
              type="button"
              title={m.title}
              onClick={() => setViewMode(m.id)}
              className={`px-2.5 py-1 text-[11px] font-medium transition ${
                viewMode === m.id
                  ? "bg-[color-mix(in_srgb,var(--color-hydro)_18%,transparent)] text-[var(--color-hydro)]"
                  : "text-ink-dim hover:text-ink"
              }`}
              aria-pressed={viewMode === m.id}
            >
              {m.label}
            </button>
          ))}
        </div>

        <div className="flex overflow-hidden rounded border border-edge bg-panel/90 shadow-sm backdrop-blur-sm">
          {basemaps.map((b) => (
            <button
              key={b.id}
              type="button"
              title={b.title}
              onClick={() => setBasemapMode(b.id)}
              className={`px-2.5 py-1 text-[11px] transition ${
                basemapMode === b.id
                  ? "bg-[color-mix(in_srgb,var(--color-hydro)_18%,transparent)] text-[var(--color-hydro)]"
                  : "text-ink-dim hover:text-ink"
              }`}
              aria-pressed={basemapMode === b.id}
            >
              {b.label}
            </button>
          ))}
        </div>

        <div className="flex overflow-hidden rounded border border-edge bg-panel/90 shadow-sm backdrop-blur-sm">
          {tools.map((t) => (
            <button
              key={t.id}
              type="button"
              title={t.title}
              onClick={() => setTool(t.id)}
              className={`px-2.5 py-1 text-[11px] transition ${
                tool === t.id
                  ? "bg-[color-mix(in_srgb,var(--color-gauge)_16%,transparent)] text-[var(--color-gauge)]"
                  : "text-ink-dim hover:text-ink"
              }`}
              aria-pressed={tool === t.id}
            >
              {t.label}
            </button>
          ))}
        </div>

        {aoi && (
          <button
            type="button"
            className="btn bg-panel/90 text-[11px] backdrop-blur-sm"
            onClick={() => clearGeoAoi()}
            title="Clear AOI and unbind flood domain"
          >
            Clear AOI
          </button>
        )}

        <button
          type="button"
          className={`btn bg-panel/90 text-[11px] backdrop-blur-sm ${rightPanelOpen ? "btn-active" : ""}`}
          onClick={() => setRightPanelOpen(!rightPanelOpen)}
        >
          Scenario
        </button>
      </div>

      {inspectHint && (
        <div
          className="pointer-events-none max-w-xl rounded border border-edge bg-panel/95 px-2.5 py-1.5 text-[11px] text-ink shadow-sm backdrop-blur-sm"
          role="status"
        >
          {inspectHint}
        </div>
      )}
    </div>
  );
}
