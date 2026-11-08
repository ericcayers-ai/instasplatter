import { useStore } from "../../state/store";

function ThemeToggle() {
  const themePreference = useStore((s) => s.themePreference);
  const setThemePreference = useStore((s) => s.setThemePreference);
  const cycle = () => {
    const order: Array<"system" | "light" | "dark"> = ["system", "light", "dark"];
    const i = order.indexOf(themePreference);
    setThemePreference(order[(i + 1) % order.length]);
  };
  const label = themePreference === "system" ? "Auto" : themePreference === "light" ? "Light" : "Dark";
  return (
    <button
      type="button"
      onClick={cycle}
      className="btn btn-ghost px-2 py-1 text-[11px]"
      title={`Theme: ${label}. Click to cycle.`}
    >
      {label}
    </button>
  );
}

function ExperimentalToggle() {
  const settings = useStore((s) => s.settings);
  const resolved = useStore((s) => s.resolved);
  const requestExperimental = useStore((s) => s.requestExperimental);
  const updateSettings = useStore((s) => s.updateSettings);
  const on = !!(resolved?.experimentalMode);

  return (
    <button
      type="button"
      onClick={() => {
        if (on) {
          void updateSettings({ experimentalMode: false, allowResearchSidecars: false });
        } else {
          requestExperimental();
        }
      }}
      className={`btn px-2.5 py-1 text-[11px] font-semibold tracking-wide ${
        on ? "border-danger/55 bg-danger/15 text-danger" : "btn-ghost"
      }`}
      title={
        on
          ? "Experimental Mode ON — NC research stack active"
          : settings.experimentalLicenseAcked
            ? "Enable Experimental Mode (NC research models)"
            : "Enable Experimental Mode (requires one-time NC license ack)"
      }
      aria-pressed={on}
    >
      {on ? "Exp ON" : "Exp"}
    </button>
  );
}

function SuiteSwitch() {
  const suite = useStore((s) => s.suite);
  const setSuite = useStore((s) => s.setSuite);
  const screen = useStore((s) => s.screen);
  const resultPath = useStore((s) => s.resultPath);
  const jobError = useStore((s) => s.jobError);
  const busy = screen === "processing" && !resultPath && !jobError;
  const options: { id: "reconstruction" | "geospatial"; label: string }[] = [
    { id: "reconstruction", label: "Recon" },
    { id: "geospatial", label: "Geo" },
  ];
  return (
    <div className="seg" role="group" aria-label="Product suite">
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          disabled={busy && o.id !== suite}
          aria-pressed={suite === o.id}
          onClick={() => void setSuite(o.id)}
          title={o.id === "reconstruction" ? "Reconstruction suite" : "Geospatial suite"}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function StatusPill() {
  const suite = useStore((s) => s.suite);
  const screen = useStore((s) => s.screen);
  const resultPath = useStore((s) => s.resultPath);
  const jobError = useStore((s) => s.jobError);
  const scientific = useStore((s) => s.geoScientificRun);
  const preview = useStore((s) => s.geoPreview);

  let tone: "idle" | "live" | "ok" | "err" = "idle";
  let label = "Ready";

  if (suite === "geospatial") {
    if (scientific?.state === "running") {
      tone = "live";
      label = "Live";
    } else if (scientific?.state === "failed") {
      tone = "err";
      label = "Failed";
    } else if (scientific?.state === "done" || preview?.validation === "validated") {
      tone = "ok";
      label = preview?.validation === "validated" ? "Validated" : "Complete";
    } else if (preview) {
      tone = "live";
      label = "Live";
    }
  } else if (screen === "processing") {
    if (jobError) {
      tone = "err";
      label = "Failed";
    } else if (resultPath) {
      tone = "ok";
      label = "Complete";
    } else {
      tone = "live";
      label = "Live";
    }
  }

  return (
    <span className="status-pill" data-tone={tone}>
      {label}
    </span>
  );
}

export default function TitleBar({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const suite = useStore((s) => s.suite);
  const inputPath = useStore((s) => s.inputPath);
  const workspace = useStore((s) => s.workspace);
  const screen = useStore((s) => s.screen);
  const setAboutOpen = useStore((s) => s.setAboutOpen);
  const rightPanelOpen = useStore((s) => s.rightPanelOpen);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);

  const name =
    suite === "reconstruction" && screen === "processing"
      ? (inputPath?.split(/[\\/]/).pop() ?? workspace?.split(/[\\/]/).pop() ?? "")
      : "";

  return (
    <header className="flex h-10 shrink-0 items-center justify-between gap-3 border-b border-edge bg-panel px-3">
      <div className="flex min-w-0 items-center gap-2.5">
        <div className="flex min-w-0 items-baseline gap-2">
          <div className="font-display text-[15px] font-bold tracking-tight text-ink">InstaSplatter</div>
          {name && (
            <>
              <span className="text-ink-dim" aria-hidden>
                /
              </span>
              <span className="max-w-36 truncate text-xs text-ink-dim" title={name}>
                {name}
              </span>
            </>
          )}
        </div>
        <SuiteSwitch />
        <StatusPill />
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        <ExperimentalToggle />
        <ThemeToggle />
        <button
          type="button"
          onClick={() => setAboutOpen(true)}
          className="btn btn-ghost px-2 py-1 text-[11px]"
          title="About implementations, licenses, and docs"
        >
          About
        </button>
        <button
          type="button"
          onClick={() => {
            if (onOpenSettings) onOpenSettings();
            else setRightPanelOpen(!rightPanelOpen);
          }}
          className={`btn btn-ghost-teal px-2 py-1 text-[11px] ${
            suite === "geospatial" && rightPanelOpen ? "btn-active" : ""
          }`}
          title="Settings (Ctrl+,)"
          aria-pressed={suite === "geospatial" ? rightPanelOpen : undefined}
        >
          Settings
        </button>
      </div>
    </header>
  );
}
