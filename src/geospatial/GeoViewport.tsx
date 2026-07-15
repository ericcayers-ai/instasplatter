import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../state/store";
import type { GeoCatalogInfo } from "../lib/ipc";
import { api } from "../lib/ipc";
import BatchQueue from "../components/BatchQueue";
import GeoMap from "./GeoMap";
import GeoToolbar from "./GeoToolbar";
import HydrographTimeline from "./HydrographTimeline";
import { PreviewBadge } from "./PreviewBadge";

/**
 * Geospatial suite viewport: MapLibre map, tools, hydrograph timeline, and import entry.
 */
export default function GeoViewport() {
  const enqueueJobs = useStore((s) => s.enqueueJobs);
  const queueItems = useStore((s) => s.queueItems);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);
  const preview = useStore((s) => s.geoPreview);
  const geoQueue = queueItems.filter((i) => (i.suite ?? "reconstruction") === "geospatial");
  const [catalog, setCatalog] = useState<GeoCatalogInfo | null>(null);
  const [showImport, setShowImport] = useState(false);

  useEffect(() => {
    void api.getGeoCatalogInfo().then(setCatalog).catch(() => setCatalog(null));
    // Scenario panel is useful on first enter; user can hide it.
    setRightPanelOpen(true);
  }, [setRightPanelOpen]);

  const browse = async () => {
    const sel = await open({
      title: "Add survey or terrain source",
      multiple: true,
      filters: [
        {
          name: "Geospatial & imagery",
          extensions: [
            "tif",
            "tiff",
            "gpkg",
            "geojson",
            "json",
            "las",
            "laz",
            "mp4",
            "mov",
            "jpg",
            "jpeg",
            "png",
          ],
        },
      ],
    });
    if (!sel) return;
    const paths = Array.isArray(sel) ? sel : [sel];
    if (paths.length) void enqueueJobs(paths, "geospatial");
  };

  return (
    <div className="geo-viewport geo-field flex h-full min-h-0 flex-col" data-water-style="depth">
      <div className="relative min-h-0 flex-1">
        <GeoMap />
        <GeoToolbar />

        <div className="pointer-events-none absolute right-3 top-12 z-10">
          <div className="pointer-events-auto">
            <PreviewBadge validation={preview?.validation ?? "live"} backend={preview?.backend} />
          </div>
        </div>

        <div className="pointer-events-none absolute bottom-3 left-3 z-10 flex max-w-sm flex-col gap-2">
          <div className="pointer-events-auto flex flex-wrap gap-1.5">
            <button type="button" className="btn bg-panel/90 text-[11px] backdrop-blur-sm" onClick={() => void browse()}>
              Add sources
            </button>
            <button
              type="button"
              className={`btn bg-panel/90 text-[11px] backdrop-blur-sm ${showImport ? "btn-active" : ""}`}
              onClick={() => setShowImport((v) => !v)}
            >
              {showImport ? "Hide details" : "Catalog"}
            </button>
          </div>

          {showImport && (
            <div className="pointer-events-auto float-in rounded border border-edge bg-panel/95 p-3 text-[11px] shadow-sm backdrop-blur-sm">
              <p className="leading-relaxed text-ink-dim">
                Import GeoTIFF, LAS/LAZ, GeoPackage, or drone imagery. Start a scientific flood from
                the scenario panel (ANUGA or labelled demo when the engine is missing).
              </p>
              {catalog && (
                <div className="mt-2 space-y-1 text-ink-dim">
                  <div>
                    <span className="text-ink">Formats · </span>
                    {catalog.formats
                      .slice(0, 5)
                      .map((f) => f.label)
                      .join(", ")}
                    {catalog.formats.length > 5 ? "…" : ""}
                  </div>
                  <div>
                    <span className="text-ink">Connectors · </span>
                    {catalog.connectors.slice(0, 3).join(", ")}
                    {catalog.connectors.length > 3 ? "…" : ""}
                  </div>
                </div>
              )}
              {geoQueue.length > 0 && (
                <div className="mt-2 max-h-40 overflow-y-auto">
                  <BatchQueue />
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      <HydrographTimeline />
    </div>
  );
}
