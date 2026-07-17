import { useStore } from "../state/store";
import Viewport from "./Viewport";
import BatchQueue from "./BatchQueue";

export default function Processing() {
  const stages = useStore((s) => s.stages);
  const jobError = useStore((s) => s.jobError);
  const latestSplatPath = useStore((s) => s.latestSplatPath);
  const backHome = useStore((s) => s.backHome);
  const notices = useStore((s) => s.notices);
  const exportDiagnosticsAction = useStore((s) => s.exportDiagnosticsAction);
  const queueItems = useStore((s) => s.queueItems);
  const clearNotices = useStore((s) => s.clearNotices);

  const active = stages.find((s) => s.state === "active");
  const progress = active?.progress ?? 0;

  return (
    <div className="relative h-full w-full bg-bg">
      <Viewport />

      {!latestSplatPath && !jobError && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-4">
          <div className="loading-ring" aria-hidden />
          <div className="text-center">
            <div className="text-sm font-medium text-ink">{active?.label || "Preparing"}</div>
            <div className="mt-1 max-w-sm text-xs text-ink-dim">
              {active?.detail || "Setting up the reconstruction pipeline"}
            </div>
          </div>
          <div className="w-48">
            <div
              className="progress-bar"
              role="progressbar"
              aria-valuenow={Math.round(progress * 100)}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <span style={{ transform: `scaleX(${Math.max(0.08, progress || 0.08)})` }} />
            </div>
          </div>
        </div>
      )}

      {jobError && (
        <div className="absolute inset-0 flex items-center justify-center bg-bg/88 backdrop-blur-[2px]">
          <div
            role="alertdialog"
            aria-labelledby="recon-fail-title"
            className="float-in flex max-w-md flex-col gap-3 border border-edge bg-panel p-8 text-center"
          >
            <div id="recon-fail-title" className="text-sm font-semibold text-ink">
              Reconstruction failed
            </div>
            <div className="text-xs leading-relaxed text-ink-dim">{jobError}</div>
            <div className="mt-2 flex justify-center gap-2">
              <button type="button" onClick={backHome} className="btn btn-primary">
                Back home
              </button>
              <button type="button" onClick={() => void exportDiagnosticsAction()} className="btn">
                Diagnostics
              </button>
            </div>
          </div>
        </div>
      )}

      {notices.length > 0 && (
        <div className="absolute left-4 top-4 z-[var(--z-toast)] flex max-w-md flex-col gap-1.5">
          {notices.map((n, i) => (
            <div
              key={`${n}-${i}`}
              className="flex items-start gap-2 border border-edge bg-panel/95 px-3 py-2 text-xs text-ink-dim shadow-sm backdrop-blur-sm"
            >
              <span className="min-w-0 flex-1 leading-snug">{n}</span>
              {i === 0 && clearNotices && (
                <button
                  type="button"
                  className="btn btn-ghost shrink-0 px-1 py-0 text-[10px]"
                  onClick={() => clearNotices()}
                  title="Dismiss notices"
                >
                  Dismiss
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      {queueItems.length > 0 && (
        <div className="absolute bottom-4 right-4 w-80">
          <BatchQueue />
        </div>
      )}
    </div>
  );
}
