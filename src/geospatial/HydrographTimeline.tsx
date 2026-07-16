import { useMemo, useRef } from "react";
import { useStore } from "../state/store";
import { PLACEHOLDER_HYDROGRAPH, PLACEHOLDER_SCENARIO } from "./defaults";
import { floodSnapshotFromTime, hazardClassLabel } from "./floodPreview";

/**
 * Signature hydrograph timeline: scrub / play drives the live flood preview.
 */
export default function HydrographTimeline() {
  const floodTime = useStore((s) => s.geoFloodTime);
  const setFloodTime = useStore((s) => s.setGeoFloodTime);
  const playing = useStore((s) => s.geoFloodPlaying);
  const togglePlaying = useStore((s) => s.toggleGeoFloodPlaying);
  const lowPower = useStore((s) => s.geoFloodLowPower);
  const setLowPower = useStore((s) => s.setGeoFloodLowPower);
  const scenario = useStore((s) => s.geoScenario);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const preview = useStore((s) => s.geoPreview);
  const series = PLACEHOLDER_HYDROGRAPH;
  const w = 640;
  const h = 72;
  const padX = 8;
  const padY = 10;
  const maxQ = Math.max(...series.map((s) => s.dischargeCms), 1);
  const maxHours = scenario?.durationHours ?? PLACEHOLDER_SCENARIO.durationHours;
  const snap = useMemo(
    () => floodSnapshotFromTime(floodTime, preview, maxHours),
    [floodTime, preview, maxHours],
  );
  const trackRef = useRef<SVGSVGElement>(null);

  const points = series.map((s) => {
    const x = padX + (s.hours / maxHours) * (w - padX * 2);
    const y = h - padY - (s.dischargeCms / maxQ) * (h - padY * 2);
    return `${x},${y}`;
  });
  const area = `${padX},${h - padY} ${points.join(" ")} ${w - padX},${h - padY}`;

  const scrubX = padX + floodTime * (w - padX * 2);

  const onPointer = (clientX: number) => {
    const svg = trackRef.current;
    if (!svg) return;
    if (playing) togglePlaying();
    const rect = svg.getBoundingClientRect();
    const x = clientX - rect.left;
    const t = (x - padX * (rect.width / w)) / ((w - padX * 2) * (rect.width / w));
    setFloodTime(Math.max(0, Math.min(1, t)));
  };

  return (
    <div className="geo-hydrograph flex h-[7.25rem] shrink-0 flex-col border-t border-edge bg-[var(--color-basin)]/90">
      <div className="flex items-center justify-between gap-3 px-3 pt-1.5">
        <div className="flex min-w-0 items-center gap-2">
          <button
            type="button"
            className={`btn px-2 py-0.5 text-[10px] ${playing ? "btn-active" : ""}`}
            onClick={() => togglePlaying()}
            aria-pressed={playing}
            title={playing ? "Pause preview" : "Play preview along hydrograph"}
          >
            {playing ? "Pause" : "Play"}
          </button>
          <button
            type="button"
            className={`btn px-2 py-0.5 text-[10px] ${lowPower ? "btn-active" : ""}`}
            onClick={() => setLowPower(!lowPower)}
            aria-pressed={lowPower}
            title="Low-power: coarser grid, fewer particles"
          >
            Low power
          </button>
          <div className="min-w-0">
            <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-[var(--color-hydro)]">
              Hydrograph
            </div>
            <div className="truncate text-[11px] text-ink-dim">
              {scenario?.name ?? PLACEHOLDER_SCENARIO.name} · {snap.statusLabel}
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-x-3 gap-y-0.5 font-mono text-[10px] tabular-nums text-ink-dim">
          <span>
            t <span className="text-ink">{snap.hours.toFixed(1)} h</span>
          </span>
          <span>
            Q <span className="text-[var(--color-gauge)]">{snap.dischargeCms.toFixed(0)} m³/s</span>
          </span>
          <span>
            stage <span className="text-ink">{snap.stageM.toFixed(2)} m</span>
          </span>
          <span>
            depth <span className="text-[var(--color-hydro)]">{snap.maxDepthM.toFixed(2)} m</span>
          </span>
          <span>
            {waterStyle === "hazard" ? (
              <span className="text-[var(--color-critical)]">{hazardClassLabel(snap.hazardClass)}</span>
            ) : (
              <span className="text-ink">{Math.round(snap.wetFraction * 100)}% wet</span>
            )}
          </span>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 items-stretch gap-2 px-2 pb-1.5">
        <svg
          ref={trackRef}
          viewBox={`0 0 ${w} ${h}`}
          className="geo-hydrograph__chart h-full min-w-0 flex-1 cursor-ew-resize touch-none"
          role="slider"
          tabIndex={0}
          aria-label="Flood time scrubber"
          aria-valuemin={0}
          aria-valuemax={maxHours}
          aria-valuenow={Number(snap.hours.toFixed(1))}
          aria-valuetext={`${snap.hours.toFixed(1)} hours, ${snap.statusLabel}`}
          onPointerDown={(e) => {
            (e.target as Element).setPointerCapture?.(e.pointerId);
            onPointer(e.clientX);
          }}
          onPointerMove={(e) => {
            if (e.buttons === 0) return;
            onPointer(e.clientX);
          }}
          onKeyDown={(e) => {
            const step = e.shiftKey ? 0.05 : 0.02;
            if (e.key === " ") {
              e.preventDefault();
              togglePlaying();
            } else if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
              e.preventDefault();
              setFloodTime(floodTime - step);
            } else if (e.key === "ArrowRight" || e.key === "ArrowUp") {
              e.preventDefault();
              setFloodTime(floodTime + step);
            } else if (e.key === "Home") {
              e.preventDefault();
              setFloodTime(0);
            } else if (e.key === "End") {
              e.preventDefault();
              setFloodTime(1);
            }
          }}
        >
          <defs>
            <linearGradient id="hydro-fill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--color-hydro)" stopOpacity="0.35" />
              <stop offset="100%" stopColor="var(--color-hydro)" stopOpacity="0.02" />
            </linearGradient>
          </defs>
          <polygon points={area} fill="url(#hydro-fill)" className="geo-hydrograph__area" />
          <polyline
            points={points.join(" ")}
            fill="none"
            stroke="var(--color-hydro)"
            strokeWidth="2"
            strokeLinejoin="round"
            strokeLinecap="round"
            className="geo-hydrograph__line"
          />
          <line
            x1={scrubX}
            x2={scrubX}
            y1={padY * 0.4}
            y2={h - padY * 0.4}
            stroke="var(--color-gauge)"
            strokeWidth="1.5"
            className="geo-hydrograph__scrub"
          />
          <circle
            cx={scrubX}
            cy={
              h -
              padY -
              (snap.dischargeCms / maxQ) * (h - padY * 2)
            }
            r="4"
            fill="var(--color-gauge)"
            stroke="var(--color-basin)"
            strokeWidth="1.5"
          />
        </svg>

        <div className="geo-legend flex w-28 shrink-0 flex-col justify-center gap-1 rounded border border-edge bg-panel/60 px-2 py-1">
          <div className="text-[9px] font-semibold uppercase tracking-wide text-ink-dim">Legend</div>
          <div className="geo-legend__swatch" data-mode={waterStyle} />
          <div className="text-[10px] leading-tight text-ink-dim">
            {waterStyle === "depth" && "Depth colormap"}
            {waterStyle === "hazard" && "H0–H3 pattern"}
            {waterStyle === "contour" && "Shoreline contours"}
          </div>
        </div>
      </div>
    </div>
  );
}
