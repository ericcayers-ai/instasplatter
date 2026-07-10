import { useEffect, useRef } from "react";
import { useStore } from "../../state/store";

function timestamp(t: number): string {
  const d = new Date(t);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/**
 * A plain, timestamped log. Real console apps flush their log on a short
 * timer rather than reflowing on every line so a fast stream never freezes
 * the interface; here the store already caps retained lines, and this view
 * only auto-scrolls when the reader was already at the bottom, so a burst of
 * output cannot yank the view out from under someone reading upward.
 */
export default function LogConsole() {
  const logs = useStore((s) => s.logs);
  const open = useStore((s) => s.logConsoleOpen);
  const containerRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  useEffect(() => {
    const el = containerRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [logs, open]);

  if (!open) return null;

  return (
    <div
      ref={containerRef}
      onScroll={(e) => {
        const el = e.currentTarget;
        stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
      }}
      className="h-44 shrink-0 overflow-y-auto border-t border-edge bg-bg px-3 py-2 font-mono text-[11px] leading-relaxed text-ink-dim"
    >
      {logs.length === 0 && <div className="text-ink-dim/60">No log output yet.</div>}
      {logs.map((l, i) => (
        <div key={i} className="whitespace-pre-wrap">
          <span className="text-ink-dim/60">{timestamp(l.time)}</span> {l.line}
        </div>
      ))}
    </div>
  );
}
