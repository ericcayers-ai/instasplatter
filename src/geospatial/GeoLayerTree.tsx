import { useMemo } from "react";
import { useStore } from "../state/store";
import { LAYER_GROUP_LABELS } from "./defaults";
import { floodSnapshotFromTime, hazardClassLabel, waterStyleLabel } from "./floodPreview";
import type { GeoLayerGroup, GeoWaterStyle } from "./types";

const GROUP_ORDER: GeoLayerGroup[] = ["basemap", "terrain", "survey", "network", "flood"];

export default function GeoLayerTree() {
  const layers = useStore((s) => s.geoLayers);
  const setLayerVisible = useStore((s) => s.setGeoLayerVisible);
  const setLayerOpacity = useStore((s) => s.setGeoLayerOpacity);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const setWaterStyle = useStore((s) => s.setGeoWaterStyle);
  const floodTime = useStore((s) => s.geoFloodTime);
  const preview = useStore((s) => s.geoPreview);
  const scenario = useStore((s) => s.geoScenario);
  const aoi = useStore((s) => s.geoAoiWgs84);
  const snap = useMemo(
    () => floodSnapshotFromTime(floodTime, preview, scenario?.durationHours),
    [floodTime, preview, scenario?.durationHours],
  );

  // Hide Empty / Hook layers until real data arrives (plan: layer honesty).
  const readyLayers = useMemo(() => layers.filter((l) => l.status === "ready"), [layers]);

  const grouped = useMemo(() => {
    return GROUP_ORDER.map((g) => ({
      group: g,
      label: LAYER_GROUP_LABELS[g],
      items: readyLayers.filter((l) => l.group === g),
    })).filter((g) => g.items.length > 0);
  }, [readyLayers]);

  const styles: { id: GeoWaterStyle; label: string }[] = [
    { id: "depth", label: "Depth" },
    { id: "hazard", label: "Hazard" },
    { id: "contour", label: "Contours" },
  ];

  return (
    <div className="flex flex-col">
      <div className="border-b border-edge px-3 py-3">
        <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
          Water display
        </div>
        <div className="flex flex-wrap gap-1">
          {styles.map((s) => (
            <button
              key={s.id}
              type="button"
              onClick={() => setWaterStyle(s.id)}
              className={`btn px-2 py-0.5 text-[10px] ${waterStyle === s.id ? "btn-active" : ""}`}
              aria-pressed={waterStyle === s.id}
              disabled={!aoi}
            >
              {s.label}
            </button>
          ))}
        </div>
        <div className="mt-2 space-y-0.5 text-[11px] tabular-nums text-ink-dim">
          <div className="flex justify-between">
            <span>Mode</span>
            <span className="text-ink">{waterStyleLabel(waterStyle)}</span>
          </div>
          <div className="flex justify-between">
            <span>Max depth</span>
            <span className="text-ink">{aoi ? `${snap.maxDepthM.toFixed(2)} m` : "—"}</span>
          </div>
          <div className="flex justify-between">
            <span>Hazard</span>
            <span className="text-ink">{aoi ? hazardClassLabel(snap.hazardClass) : "—"}</span>
          </div>
          {!aoi && (
            <p className="pt-1 text-[10px] leading-snug text-[var(--color-gauge)]">
              Draw an AOI to bind the flood domain.
            </p>
          )}
        </div>
      </div>

      {grouped.map((g) => (
        <div key={g.group} className="border-b border-edge px-3 py-3">
          <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
            {g.label}
          </div>
          <ul className="flex flex-col gap-1.5">
            {g.items.map((layer) => (
              <li key={layer.id} className="flex flex-col gap-1">
                <label className="flex cursor-pointer items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={layer.visible}
                    onChange={(e) => setLayerVisible(layer.id, e.target.checked)}
                    className="accent-[var(--color-hydro)]"
                  />
                  <span className="min-w-0 flex-1 truncate">{layer.label}</span>
                </label>
                {layer.visible && (
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.05}
                    value={layer.opacity}
                    onChange={(e) => setLayerOpacity(layer.id, Number(e.target.value))}
                    aria-label={`${layer.label} opacity`}
                    className="ml-5 w-[calc(100%-1.25rem)]"
                  />
                )}
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}
