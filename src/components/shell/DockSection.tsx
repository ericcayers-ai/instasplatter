import { useId, useState, type ReactNode } from "react";

export default function DockSection({
  title,
  children,
  defaultOpen = true,
  open: openProp,
  onOpenChange,
  focused = false,
  tone = "default",
  badge,
  id,
}: {
  title: string;
  children: ReactNode;
  defaultOpen?: boolean;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  focused?: boolean;
  tone?: "default" | "accent" | "hydro" | "brass";
  badge?: string;
  id?: string;
}) {
  const [uncontrolled, setUncontrolled] = useState(defaultOpen);
  const open = openProp ?? uncontrolled;
  const setOpen = (next: boolean) => {
    if (openProp == null) setUncontrolled(next);
    onOpenChange?.(next);
  };
  const panelId = useId();

  return (
    <section
      id={id}
      className="dock-section"
      data-tone={tone === "default" ? undefined : tone}
      data-focus={focused ? "true" : undefined}
    >
      <button
        type="button"
        className="dock-section__head"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen(!open)}
      >
        <span className="flex min-w-0 items-center gap-2">
          <span className="dock-section__label">{title}</span>
          {badge && (
            <span className="truncate rounded border border-edge px-1.5 py-0.5 text-[9px] text-ink-dim">
              {badge}
            </span>
          )}
        </span>
        <span className="dock-section__chevron" aria-hidden>
          {open ? "▾" : "▸"}
        </span>
      </button>
      {open && (
        <div id={panelId} className="dock-section__body">
          {children}
        </div>
      )}
    </section>
  );
}
