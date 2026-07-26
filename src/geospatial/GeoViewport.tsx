import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../state/store";
import type { GeoCatalogInfo } from "../lib/ipc";
import { api } from "../lib/ipc";
import BatchQueue from "../components/BatchQueue";
import GeoMap from "./GeoMap";
import GeoToolbar from "./GeoToolbar";
import GeoWorkspace3D from "./GeoWorkspace3D";
import { CesiumGlobe } from "./globe";
import HydrographTimeline from "./HydrographTimeline";
import { PreviewBadge } from "./PreviewBadge";
import { demAuthorityLabel, isGeoTiffPath } from "./demAuthority";

/**
 * Geospatial suite viewport: default 3D ENU, optional 2D MapLibre, optional Cesium Globe.
 */
export default function GeoViewport() {
  const enqueueJobs = useStore((s) => s.enqueueJobs);
  const stageUserGeoTiff = useStore((s) => s.stageUserGeoTiffDem);
  const queueItems = useStore((s) => s.queueItems);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);
  const preview = useStore((s) => s.geoPreview);
  const scientific = useStore((s) => s.geoScientificRun);
  const dem = useStore((s) => s.geoDemProduct);
  const aoi = useStore((s) => s.geoAoiWgs84);
  const setTool = useStore((s) => s.setGeoTool);
  const viewMode = useStore((s) => s.geoViewMode);
  const geoQueue = queueItems.filter((i) => (i.suite ?? "reconstruction") === "geospatial");
  const [catalog, setCatalog] = useState<GeoCatalogInfo | null>(null);
  const [showImport, setShowImport] = useState(false);

  useEffect(() => {
    void api.getGeoCatalogInfo().then(setCatalog).catch(() => setCatalog(null));
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
    if (!paths.length) return;

    const tiffs = paths.filter(isGeoTiffPath);
    const others = paths.filter((p) => !isGeoTiffPath(p));

    // GeoTIFF → stage as DEM bed (prepareGeoDem + sample), not a dead queue placeholder.
    if (tiffs.length) {
      void stageUserGeoTiff(tiffs[0]);
      if (tiffs.length > 1) void enqueueJobs(tiffs.slice(1), "geospatial");
    }
    if (others.length) void enqueueJobs(others, "geospatial");
  };

  const authority = demAuthorityLabel({
    scientificMode: scientific?.mode,
    demSynthetic: dem?.synthetic,
    demBedSource: dem?.bedSource,
  });

  const is2d = viewMode === "2d";
  const isGlobe = viewMode === "globe";
  const showMapChrome = is2d;

  return (
    <div className="geo-viewport geo-field flex h-full min-h-0 flex-col" data-water-style="depth">
      <div className="relative min-h-0 flex-1">
        {is2d ? <GeoMap /> : isGlobe ? <CesiumGlobe /> : <GeoWorkspace3D />}
        <GeoToolbar />

        {showMapChrome && (
          <div className="pointer-events-none absolute right-3 top-12 z-10 flex flex-col items-end gap-1.5">
            <div className="pointer-events-auto">
              <PreviewBadge validation={preview?.validation ?? "live"} backend={preview?.backend} />
            </div>
            <div className="pointer-events-none flex flex-wrap justify-end gap-1">
              <span
                className="rounded border border-edge bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-ink-dim backdrop-blur-sm"
                title="Flood result authority for the current domain"
              >
                {authority}
              </span>
              {aoi ? (
                <span className="rounded border border-[var(--color-hydro)]/35 bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--color-hydro)] backdrop-blur-sm">
                  AOI bound
                </span>
              ) : (
                <button
                  type="button"
                  className="pointer-events-auto rounded border border-[var(--color-gauge)]/40 bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--color-gauge)] backdrop-blur-sm"
                  onClick={() => setTool("drawAoi")}
                >
                  Draw AOI
                </button>
              )}
            </div>
          </div>
        )}

        <div
          className={`pointer-events-none absolute z-10 flex max-w-sm flex-col gap-2 ${
            is2d ? "bottom-3 left-3" : "bottom-3 right-3"
          }`}
        >
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
                GeoTIFF / COG files stage as the DEM bed. LAS/LAZ, GeoPackage, or drone imagery open a
                geospatial workspace. Draw an AOI on the 2D map, then run soft/HAND or ANUGA from the
                scenario panel. OpenTopography needs an API key under Flood lab → Advanced.
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
