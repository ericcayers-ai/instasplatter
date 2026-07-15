import { useStore } from "../state/store";
import type { GeoTool, GeoViewMode } from "./types";

export default function GeoToolbar() {
  const viewMode = useStore((s) => s.geoViewMode);
  const setViewMode = useStore((s) => s.setGeoViewMode);
  const tool = useStore((s) => s.geoTool);
  const setTool = useStore((s) => s.setGeoTool);
  const inspectHint = useStore((s) => s.geoInspectHint);
  const rightPanelOpen = useStore((s) => s.rightPanelOpen);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);

  const modes: { id: GeoViewMode; label: string }[] = [
    { id: "2d", label: "2D" },
    { id: "3d", label: "3D" },
  ];
  const tools: { id: GeoTool; label: string; title: string }[] = [
    { id: "pan", label: "Pan", title: "Pan and zoom the map" },
    { id: "inspect", label: "Inspect", title: "Click a point for coordinates (solver values later)" },
    { id: "measure", label: "Measure", title: "Distance measure — stub until survey tools land" },
    { id: "profile", label: "Profile", title: "Cross-section profile — stub until DEM tools land" },
  ];

  return (
    <div className="pointer-events-none absolute inset-x-0 top-0 z-10 flex flex-col gap-2 p-2">
      <div className="pointer-events-auto flex flex-wrap items-center gap-2">
        <div className="flex overflow-hidden rounded border border-edge bg-panel/90 shadow-sm backdrop-blur-sm">
          {modes.map((m) => (
            <button
              key={m.id}
              type="button"
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
