import { useStore } from "../../state/store";

function StageDot({ state }: { state: "pending" | "active" | "done" }) {
  return (
    <span
      className={`inline-block h-1.5 w-1.5 rounded-full ${
        state === "done" ? "bg-accent" : state === "active" ? "bg-accent2" : "bg-edge"
      }`}
    />
  );
}

function ThemeToggle() {
  const themePreference = useStore((s) => s.themePreference);
  const setThemePreference = useStore((s) => s.setThemePreference);
  const options: { id: "system" | "light" | "dark"; label: string }[] = [
    { id: "system", label: "Auto" },
    { id: "light", label: "Light" },
    { id: "dark", label: "Dark" },
  ];
  return (
    <div className="flex overflow-hidden rounded border border-edge">
      {options.map((o) => (
        <button
          key={o.id}
          onClick={() => setThemePreference(o.id)}
          className={`px-2 py-1 text-[11px] transition ${
            themePreference === o.id ? "bg-accent/15 text-accent" : "text-ink-dim hover:text-ink"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function ExperimentalToggle() {
  const settings = useStore((s) => s.settings);
  const resolved = useStore((s) => s.resolved);
  const requestExperimental = useStore((s) => s.requestExperimental);
  const updateSettings = useStore((s) => s.updateSettings);
  // Effective mode only — raw settings.experimentalMode without ack must not light up.
  const on = !!(resolved?.experimentalMode);

  return (
    <button
      onClick={() => {
        if (on) {
          void updateSettings({ experimentalMode: false, allowResearchSidecars: false });
        } else {
          requestExperimental();
        }
      }}
      className={`btn px-2.5 py-1 text-[11px] font-semibold tracking-wide ${
        on
          ? "border-danger/60 bg-danger/20 text-danger shadow-[0_0_0_1px_color-mix(in_srgb,var(--color-danger)_35%,transparent)]"
          : "text-ink-dim hover:text-ink"
      }`}
      title={
        on
          ? "Experimental Mode ON — NC research stack active"
          : settings.experimentalLicenseAcked
            ? "Enable Experimental Mode (NC research models)"
            : "Enable Experimental Mode (requires one-time NC license ack)"
      }
    >
      {on ? "Experimental ON" : "Experimental"}
    </button>
  );
}

export default function TitleBar() {
  const screen = useStore((s) => s.screen);
  const stages = useStore((s) => s.stages);
  const inputPath = useStore((s) => s.inputPath);
  const workspace = useStore((s) => s.workspace);
  const resultPath = useStore((s) => s.resultPath);
  const jobError = useStore((s) => s.jobError);
  const cancelJob = useStore((s) => s.cancelJob);
  const backHome = useStore((s) => s.backHome);
  const exportSplatAction = useStore((s) => s.exportSplatAction);
  const exportMeshAction = useStore((s) => s.exportMeshAction);
  const rightPanelOpen = useStore((s) => s.rightPanelOpen);
  const toggleRightPanel = useStore((s) => s.toggleRightPanel);
  const leftPanelOpen = useStore((s) => s.leftPanelOpen);
  const setLeftPanelOpen = useStore((s) => s.setLeftPanelOpen);

  const running = screen === "processing" && !resultPath && !jobError;
  const name = inputPath?.split(/[\\/]/).pop() ?? workspace?.split(/[\\/]/).pop() ?? "";

  return (
    <div className="flex h-10 shrink-0 items-center justify-between border-b border-edge bg-panel px-3">
      <div className="flex min-w-0 items-center gap-3">
        <button
          onClick={() => setLeftPanelOpen(!leftPanelOpen)}
          className="btn"
          title={leftPanelOpen ? "Hide the scene panel" : "Show the scene panel"}
        >
          {leftPanelOpen ? "◀" : "▶"}
        </button>
        <div className="font-display text-[14px] font-bold tracking-tight">InstaSplatter</div>
        {screen === "processing" && (
          <>
            <span className="text-ink-dim">/</span>
            <span className="max-w-56 truncate text-xs text-ink-dim">{name}</span>
            <div className="ml-2 hidden items-center gap-3 sm:flex">
              {stages.map((s) => (
                <span key={s.id} className="flex items-center gap-1.5 text-[11px] text-ink-dim">
                  <StageDot state={s.state} />
                  {s.label}
                </span>
              ))}
            </div>
          </>
        )}
      </div>

      <div className="flex items-center gap-2">
        <ExperimentalToggle />
        {screen === "processing" && (
          <>
            {running && (
              <button onClick={cancelJob} className="btn btn-danger">
                Cancel
              </button>
            )}
            {!running && (
              <button onClick={backHome} className="btn">
                New scene
              </button>
            )}
            {resultPath && (
              <>
                <button onClick={() => void exportMeshAction()} className="btn">
                  Export mesh
                </button>
                <button onClick={() => void exportSplatAction()} className="btn btn-primary">
                  Export splat
                </button>
              </>
            )}
          </>
        )}
        <ThemeToggle />
        <button
          onClick={toggleRightPanel}
          className={`btn ${rightPanelOpen ? "btn-active" : ""}`}
          title="Settings"
        >
          Settings
        </button>
      </div>
    </div>
  );
}
