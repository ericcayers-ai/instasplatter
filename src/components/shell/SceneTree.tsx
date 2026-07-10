import { useStore } from "../../state/store";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="border-b border-edge px-3 py-3">
      <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">{title}</div>
      {children}
    </div>
  );
}

function RecentProjects() {
  const projects = useStore((s) => s.recentProjects);
  const resumeProject = useStore((s) => s.resumeProject);
  const deleteProjectEntry = useStore((s) => s.deleteProjectEntry);

  if (projects.length === 0) {
    return <div className="text-xs text-ink-dim">No saved projects yet.</div>;
  }

  return (
    <div className="flex flex-col gap-1">
      {projects.map((p) => (
        <div
          key={p.workspace}
          className="group flex items-center justify-between gap-2 rounded px-2 py-1.5 text-xs hover:bg-panel2"
        >
          <div className="min-w-0">
            <div className="truncate">{p.inputName}</div>
            <div className="text-[10px] text-ink-dim">
              {p.completed
                ? "Complete"
                : p.resumable
                  ? `Resumable, step ${p.latestIter.toLocaleString()} / ${p.totalSteps.toLocaleString()}`
                  : "Incomplete"}
            </div>
          </div>
          <div className="flex shrink-0 gap-1 opacity-0 group-hover:opacity-100">
            {p.resumable && (
              <button
                onClick={() => void resumeProject(p.workspace)}
                className="btn px-1.5 py-0.5 text-[10px]"
              >
                Resume
              </button>
            )}
            <button
              onClick={() => void deleteProjectEntry(p.workspace)}
              className="btn btn-danger px-1.5 py-0.5 text-[10px]"
              title="Delete this project"
            >
              ✕
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

function CameraList() {
  const cameras = useStore((s) => s.cameras);
  const registeredCameras = useStore((s) => s.registeredCameras);
  const totalCameras = useStore((s) => s.totalCameras);

  if (totalCameras === 0) {
    return <div className="text-xs text-ink-dim">Not using live camera tracking for this run.</div>;
  }

  const recent = cameras.slice(-12).reverse();
  return (
    <div className="flex flex-col gap-2">
      <div className="text-xs tabular-nums">
        {registeredCameras.toLocaleString()} / {totalCameras.toLocaleString()} registered
      </div>
      <div className="flex max-h-40 flex-col gap-0.5 overflow-y-auto">
        {recent.map((c, i) => (
          <div key={`${c.name}-${i}`} className="flex items-center justify-between text-[11px] text-ink-dim">
            <span className="truncate">{c.name}</span>
            <span className="tabular-nums">{Math.round(c.confidence * 100)}%</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function SceneTree() {
  const screen = useStore((s) => s.screen);
  const leftPanelOpen = useStore((s) => s.leftPanelOpen);
  const inputPath = useStore((s) => s.inputPath);
  const workspace = useStore((s) => s.workspace);
  const stages = useStore((s) => s.stages);
  const splatCount = useStore((s) => s.splatCount);
  const latestIter = useStore((s) => s.latestIter);
  const totalSteps = useStore((s) => s.totalSteps);
  const resultPath = useStore((s) => s.resultPath);

  if (!leftPanelOpen) return null;

  return (
    <div className="flex w-56 shrink-0 flex-col overflow-y-auto border-r border-edge bg-panel">
      {screen === "home" && (
        <Section title="Recent projects">
          <RecentProjects />
        </Section>
      )}

      {screen === "processing" && (
        <>
          <Section title="Input">
            <div className="truncate text-xs">{inputPath?.split(/[\\/]/).pop() ?? "Resumed project"}</div>
            {workspace && (
              <div className="mt-1 truncate text-[10px] text-ink-dim" title={workspace}>
                {workspace}
              </div>
            )}
          </Section>

          <Section title="Cameras">
            <CameraList />
          </Section>

          <Section title="Model">
            <div className="flex flex-col gap-1 text-xs">
              <div className="flex justify-between">
                <span className="text-ink-dim">Splats</span>
                <span className="tabular-nums">{splatCount.toLocaleString()}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-ink-dim">Step</span>
                <span className="tabular-nums">
                  {latestIter.toLocaleString()}
                  {totalSteps > 0 && ` / ${totalSteps.toLocaleString()}`}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-ink-dim">Status</span>
                <span>{resultPath ? "Complete" : (stages.find((s) => s.state === "active")?.label ?? "Waiting")}</span>
              </div>
            </div>
          </Section>
        </>
      )}
    </div>
  );
}
