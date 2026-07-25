import type { ReactNode } from "react";
import type { IconRailAction } from "./shellFocus";

function CaptureIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 15 15" fill="none" aria-hidden>
      <rect x="2" y="3.5" width="11" height="8" rx="1.5" stroke="currentColor" strokeWidth="1.25" />
      <circle cx="7.5" cy="7.5" r="2.25" stroke="currentColor" strokeWidth="1.25" />
    </svg>
  );
}

function SceneIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 15 15" fill="none" aria-hidden>
      <path d="M2.5 11.5 7.5 3l5 8.5H2.5Z" stroke="currentColor" strokeWidth="1.25" strokeLinejoin="round" />
      <path d="M5.2 11.5h4.6" stroke="currentColor" strokeWidth="1.25" strokeLinecap="round" />
    </svg>
  );
}

function SettingsIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 15 15" fill="none" aria-hidden>
      <circle cx="7.5" cy="7.5" r="2.1" stroke="currentColor" strokeWidth="1.25" />
      <path
        d="M7.5 1.75v1.4M7.5 11.85v1.4M1.75 7.5h1.4M11.85 7.5h1.4M3.3 3.3l1 1M10.7 10.7l1 1M11.7 3.3l-1 1M4.3 10.7l-1 1"
        stroke="currentColor"
        strokeWidth="1.25"
        strokeLinecap="round"
      />
    </svg>
  );
}

function LogIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 15 15" fill="none" aria-hidden>
      <path d="M3 3.5h9M3 7.5h9M3 11.5h6" stroke="currentColor" strokeWidth="1.25" strokeLinecap="round" />
    </svg>
  );
}

const ITEMS: {
  id: IconRailAction;
  label: string;
  title: string;
  icon: ReactNode;
}[] = [
  { id: "capture", label: "Capture", title: "Capture dock (Ctrl+B)", icon: <CaptureIcon /> },
  { id: "scene", label: "Scene", title: "Scene layers", icon: <SceneIcon /> },
  { id: "settings", label: "Settings", title: "Settings (Ctrl+,)", icon: <SettingsIcon /> },
  { id: "log", label: "Log", title: "Log (Ctrl+L)", icon: <LogIcon /> },
];

export default function IconRail({
  active,
  onAction,
}: {
  active: IconRailAction | null;
  onAction: (action: IconRailAction) => void;
}) {
  return (
    <nav className="icon-rail" aria-label="Shell focus">
      {ITEMS.map((item) => (
        <button
          key={item.id}
          type="button"
          className="icon-rail__btn"
          title={item.title}
          aria-label={item.label}
          aria-pressed={active === item.id}
          onClick={() => onAction(item.id)}
        >
          {item.icon}
        </button>
      ))}
    </nav>
  );
}
