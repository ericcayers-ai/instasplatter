import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../state/store";
import type { GeoCatalogInfo } from "../lib/ipc";
import { api } from "../lib/ipc";
import BatchQueue from "../components/BatchQueue";

/**
 * Empty geospatial home / viewport shell. Full MapLibre + deck.gl lands in the
 * geo-viewport phase; this screen establishes suite routing and import entry.
 */
export default function GeoHome() {
  const enqueueJobs = useStore((s) => s.enqueueJobs);
  const queueItems = useStore((s) => s.queueItems);
  const geoQueue = queueItems.filter((i) => (i.suite ?? "reconstruction") === "geospatial");
  const [catalog, setCatalog] = useState<GeoCatalogInfo | null>(null);

  useEffect(() => {
    void api.getGeoCatalogInfo().then(setCatalog).catch(() => setCatalog(null));
  }, []);

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
    <div className="geo-field flex h-full flex-col">
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-8 p-10">
        <header className="float-in max-w-lg text-center">
          <p className="text-[11px] font-medium uppercase tracking-[0.14em] text-[var(--color-hydro)]">
            Geospatial
          </p>
          <h1 className="font-display mt-2 text-[2rem] font-bold leading-tight tracking-tight text-ink">
            Site to flood extent
          </h1>
          <p className="mt-2 text-sm leading-relaxed text-ink-dim">
            Import drone surveys, DEMs, and gauges. Metric georegistration and flood engines arrive
            in the next phases — this workspace is ready for them.
          </p>
        </header>

        <div
          role="button"
          tabIndex={0}
          aria-label="Add geospatial sources"
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              void browse();
            }
          }}
          className="float-in flex w-full max-w-xl flex-col items-center gap-4 rounded border-2 border-dashed border-[var(--color-contour)] bg-[var(--color-survey)]/40 px-10 py-14 transition-colors hover:border-[var(--color-hydro)]"
        >
          <div className="text-sm font-medium text-ink">Add sources</div>
          <div className="text-xs text-ink-dim">
            GeoTIFF, LAS/LAZ, GeoPackage, drone imagery, or folders
          </div>
          <button onClick={() => void browse()} className="btn btn-primary mt-2">
            Choose files
          </button>
        </div>

        {catalog && (
          <div className="float-in grid w-full max-w-xl gap-4 text-left sm:grid-cols-2">
            <div className="rounded border border-edge bg-panel/80 px-3 py-2.5">
              <div className="text-[10px] font-medium uppercase tracking-wide text-ink-dim">
                Formats
              </div>
              <div className="mt-1.5 text-[11px] leading-relaxed text-ink-dim">
                {catalog.formats
                  .slice(0, 6)
                  .map((f) => f.label)
                  .join(" · ")}
                {catalog.formats.length > 6 ? " · …" : ""}
              </div>
            </div>
            <div className="rounded border border-edge bg-panel/80 px-3 py-2.5">
              <div className="text-[10px] font-medium uppercase tracking-wide text-ink-dim">
                Data connectors
              </div>
              <div className="mt-1.5 text-[11px] leading-relaxed text-ink-dim">
                {catalog.connectors.slice(0, 4).join(" · ")}
                {catalog.connectors.length > 4 ? " · …" : ""}
              </div>
            </div>
          </div>
        )}

        {geoQueue.length > 0 && (
          <div className="float-in w-full max-w-xl">
            <BatchQueue />
          </div>
        )}
      </div>

      {/* Timeline / hydrograph dock placeholder */}
      <div className="flex h-14 shrink-0 items-center justify-between border-t border-edge bg-[var(--color-basin)]/80 px-4">
        <span className="text-[11px] text-ink-dim">Hydrograph timeline</span>
        <span className="font-mono text-[10px] text-[var(--color-gauge)]">Awaiting scenario</span>
      </div>
    </div>
  );
}
