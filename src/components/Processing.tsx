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

  return (
    <div className="relative h-full w-full bg-bg">
      <Viewport />

      {!latestSplatPath && !jobError && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-edge border-t-accent" />
          <div className="text-xs text-ink-dim">
            {stages.find((s) => s.state === "active")?.detail || "Preparing"}
          </div>
        </div>
      )}

      {jobError && (
        <div className="absolute inset-0 flex items-center justify-center bg-bg/85">
          <div className="float-in flex max-w-md flex-col items-center gap-3 rounded border border-edge bg-panel p-8 text-center">
            <div className="text-sm font-medium">Reconstruction failed</div>
            <div className="text-xs text-ink-dim">{jobError}</div>
            <div className="mt-2 flex gap-2">
              <button onClick={backHome} className="btn">
                Back
              </button>
              <button onClick={() => void exportDiagnosticsAction()} className="btn">
                Export diagnostics
              </button>
            </div>
          </div>
        </div>
      )}

      {notices.length > 0 && (
        <div className="absolute left-4 top-4 flex max-w-md flex-col gap-1">
          {notices.map((n, i) => (
            <div key={i} className="rounded border border-edge bg-panel/90 px-3 py-2 text-xs text-ink-dim">
              {n}
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
