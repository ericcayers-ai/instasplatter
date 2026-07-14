import { useStore } from "../state/store";

const STATE_LABEL: Record<string, string> = {
  queued: "Queued",
  running: "Running",
  paused: "Paused",
  done: "Done",
  failed: "Failed",
  cancelled: "Cancelled",
};

export default function BatchQueue() {
  const items = useStore((s) => s.queueItems);
  const paused = useStore((s) => s.queuePaused);
  const pauseQueue = useStore((s) => s.pauseQueue);
  const resumeQueue = useStore((s) => s.resumeQueue);
  const cancelQueueItem = useStore((s) => s.cancelQueueItem);
  const clearFinishedQueue = useStore((s) => s.clearFinishedQueue);

  if (items.length === 0) return null;

  return (
    <div className="rounded border border-edge bg-panel">
      <div className="flex items-center justify-between border-b border-edge px-3 py-2">
        <div className="text-xs font-semibold">Batch queue</div>
        <div className="flex gap-1.5">
          {paused ? (
            <button onClick={() => void resumeQueue()} className="btn px-2 py-0.5 text-[10px]">
              Resume
            </button>
          ) : (
            <button onClick={() => void pauseQueue()} className="btn px-2 py-0.5 text-[10px]">
              Pause
            </button>
          )}
          <button onClick={() => void clearFinishedQueue()} className="btn px-2 py-0.5 text-[10px] text-ink-dim">
            Clear finished
          </button>
        </div>
      </div>
      <ul className="max-h-48 divide-y divide-edge/60 overflow-y-auto">
        {items.map((item) => (
          <li key={item.id} className="flex items-center gap-2 px-3 py-2 text-xs">
            <div className="min-w-0 flex-1">
              <div className="truncate font-medium">{item.displayName}</div>
              <div className="text-[10px] text-ink-dim">
                {STATE_LABEL[item.state] ?? item.state}
                {item.detail ? ` · ${item.detail}` : ""}
              </div>
              {item.state === "running" && (
                <div className="mt-1 h-1 overflow-hidden rounded bg-edge">
                  <div className="h-full bg-accent" style={{ width: `${Math.round(item.progress * 100)}%` }} />
                </div>
              )}
            </div>
            {(item.state === "queued" || item.state === "running") && (
              <button
                onClick={() => void cancelQueueItem(item.id)}
                className="btn shrink-0 px-2 py-0.5 text-[10px]"
              >
                Cancel
              </button>
            )}
          </li>
        ))}
      </ul>
      <div className="border-t border-edge px-3 py-1.5 text-[10px] text-ink-dim">
        One reconstruction at a time so the GPU stays free of contention.
      </div>
    </div>
  );
}
