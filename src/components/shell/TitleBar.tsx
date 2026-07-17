import { useEffect, useRef, useState } from "react";
import { useStore } from "../../state/store";

function StageDot({ state }: { state: "pending" | "active" | "done" }) {
  return (
    <span
      className={`inline-block h-1.5 w-1.5 rounded-full ${
        state === "done" ? "bg-accent" : state === "active" ? "bg-accent2" : "bg-edge"
      }`}
      aria-hidden
    />
  );
}

function PanelIcon({ open }: { open: boolean }) {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden>
      <rect x="1.5" y="2" width="11" height="10" rx="1.5" stroke="currentColor" strokeWidth="1.25" />
      <path d={open ? "M5 2v10" : "M9 2v10"} stroke="currentColor" strokeWidth="1.25" />
    </svg>
  );
}

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
  const busy = screen === "processing";
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

function ReconStageStrip() {
  const stages = useStore((s) => s.stages);

  return (
    <div className="ml-1 hidden items-center gap-2.5 md:flex" aria-label="Reconstruction stages">
      {stages.map((s) => {
        const pct =
          s.state === "active" && s.progress > 0 ? ` ${Math.round(s.progress * 100)}%` : "";
        return (
          <span
            key={s.id}
            className={`flex items-center gap-1.5 text-[11px] ${
              s.state === "active" ? "text-ink" : "text-ink-dim"
            }`}
            title={s.detail || s.label}
          >
            <StageDot state={s.state} />
            <span className="hidden lg:inline">{s.label}</span>
            {pct && <span className="tabular-nums text-accent2">{pct.trim()}</span>}
          </span>
        );
      })}
    </div>
  );
}

function GeoStageStrip() {
  const scientific = useStore((s) => s.geoScientificRun);
  const pipelineChips = useStore((s) => s.pipelineChips);
  const preview = useStore((s) => s.geoPreview);

  const floodState: "pending" | "active" | "done" = !scientific
    ? "pending"
    : scientific.state === "running"
      ? "active"
      : scientific.state === "done"
        ? "done"
        : "pending";
  const exportState: "pending" | "active" | "done" = pipelineChips.export
    ? pipelineChips.export.includes("failed")
      ? "pending"
      : "done"
    : "pending";

  return (
    <div className="ml-1 hidden items-center gap-2.5 md:flex" aria-label="Geospatial stages">
      <span
        className={`flex items-center gap-1.5 text-[11px] ${
          floodState === "active" ? "text-ink" : "text-ink-dim"
        }`}
        title={scientific?.detail || "Flood"}
      >
        <StageDot state={floodState} />
        Flood
        {floodState === "active" && scientific && (
          <span className="tabular-nums text-[var(--color-hydro)]">
            {Math.round(scientific.progress * 100)}%
          </span>
        )}
      </span>
      <span className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Export products">
        <StageDot state={exportState} />
        Export
      </span>
      {preview && (
        <span className="rounded border border-edge bg-panel2/80 px-1.5 py-0.5 text-[9px] text-ink-dim">
          {preview.backend}
        </span>
      )}
    </div>
  );
}

function ExportMenu() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const exportSplatAction = useStore((s) => s.exportSplatAction);
  const exportMeshAction = useStore((s) => s.exportMeshAction);
  const exportSchematicAction = useStore((s) => s.exportSchematicAction);
  const experimentalOn = useStore((s) => !!(s.resolved?.experimentalMode));

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        className="btn btn-primary"
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen((v) => !v)}
      >
        Export
        <span className="text-[10px] opacity-80" aria-hidden>
          ▾
        </span>
      </button>
      {open && (
        <div className="menu" role="menu">
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              void exportSplatAction();
            }}
          >
            Splat
            <span className="hint">PLY · SPZ · .splat</span>
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              void exportMeshAction();
            }}
          >
            Mesh
            <span className="hint">GLB · OBJ · PLY</span>
          </button>
          {experimentalOn && (
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                void exportSchematicAction();
              }}
            >
              Schematic
              <span className="hint">Minecraft .schem</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export default function TitleBar() {
  const screen = useStore((s) => s.screen);
  const suite = useStore((s) => s.suite);
  const inputPath = useStore((s) => s.inputPath);
  const workspace = useStore((s) => s.workspace);
  const resultPath = useStore((s) => s.resultPath);
  const jobError = useStore((s) => s.jobError);
  const cancelJob = useStore((s) => s.cancelJob);
  const backHome = useStore((s) => s.backHome);
  const rightPanelOpen = useStore((s) => s.rightPanelOpen);
  const toggleRightPanel = useStore((s) => s.toggleRightPanel);
  const leftPanelOpen = useStore((s) => s.leftPanelOpen);
  const setLeftPanelOpen = useStore((s) => s.setLeftPanelOpen);
  const setAboutOpen = useStore((s) => s.setAboutOpen);

  const running = screen === "processing" && !resultPath && !jobError;
  const name = inputPath?.split(/[\\/]/).pop() ?? workspace?.split(/[\\/]/).pop() ?? "";
  const showReconChrome = suite === "reconstruction" && screen === "processing";
  const showGeoChrome = suite === "geospatial";

  return (
    <header className="flex h-11 shrink-0 items-center justify-between gap-3 border-b border-edge bg-panel px-3">
      <div className="flex min-w-0 items-center gap-2.5">
        <button
          type="button"
          onClick={() => setLeftPanelOpen(!leftPanelOpen)}
          className={`btn btn-icon ${leftPanelOpen ? "btn-active" : "btn-ghost"}`}
          title={leftPanelOpen ? "Hide side panel (Ctrl+B)" : "Show side panel (Ctrl+B)"}
          aria-pressed={leftPanelOpen}
          aria-label={leftPanelOpen ? "Hide side panel" : "Show side panel"}
        >
          <PanelIcon open={leftPanelOpen} />
        </button>

        <div className="flex min-w-0 items-baseline gap-2">
          <div className="font-display text-[15px] font-bold tracking-tight text-ink">InstaSplatter</div>
          {showReconChrome && name && (
            <>
              <span className="text-ink-dim" aria-hidden>
                /
              </span>
              <span className="max-w-40 truncate text-xs text-ink-dim" title={name}>
                {name}
              </span>
            </>
          )}
        </div>

        <SuiteSwitch />
        {showReconChrome && <ReconStageStrip />}
        {showGeoChrome && <GeoStageStrip />}
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        <ExperimentalToggle />
        {showReconChrome && (
          <>
            {running && (
              <button type="button" onClick={cancelJob} className="btn btn-danger">
                Cancel
              </button>
            )}
            {!running && (
              <button type="button" onClick={backHome} className="btn">
                New
              </button>
            )}
            {resultPath && <ExportMenu />}
          </>
        )}
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
          onClick={toggleRightPanel}
          className={`btn ${rightPanelOpen ? "btn-active" : ""}`}
          title="Settings (Ctrl+,)"
          aria-pressed={rightPanelOpen}
        >
          Settings
        </button>
      </div>
    </header>
  );
}
