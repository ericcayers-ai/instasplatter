/** Shared collapsible settings group for Properties / Scenario / About. */

import { useId, useState, type ReactNode } from "react";

export default function CollapsibleSection({
  title,
  children,
  defaultOpen = true,
  badge,
  tone = "default",
}: {
  title: string;
  children: ReactNode;
  defaultOpen?: boolean;
  badge?: string;
  tone?: "default" | "danger";
}) {
  const [open, setOpen] = useState(defaultOpen);
  const panelId = useId();

  return (
    <div
      className={`border-b border-edge ${
        tone === "danger" ? "bg-danger/[0.04]" : ""
      }`}
    >
      <button
        type="button"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between gap-2 px-3 py-2.5 text-left hover:bg-panel2/60"
      >
        <span className="flex min-w-0 items-center gap-2">
          <span
            className={`text-[10px] font-semibold uppercase tracking-wider ${
              tone === "danger" ? "text-danger/90" : "text-ink-dim"
            }`}
          >
            {title}
          </span>
          {badge && (
            <span className="truncate rounded border border-edge px-1.5 py-0.5 text-[9px] text-ink-dim">
              {badge}
            </span>
          )}
        </span>
        <span className="shrink-0 text-[10px] text-ink-dim" aria-hidden>
          {open ? "▾" : "▸"}
        </span>
      </button>
      {open && (
        <div id={panelId} className="divide-y divide-edge/60 px-3 pb-3">
          {children}
        </div>
      )}
    </div>
  );
}
