import { useStore } from "../../state/store";
import GeoLayerTree from "../../geospatial/GeoLayerTree";

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
    return (
      <div className="text-xs leading-relaxed text-ink-dim">
        No saved projects yet. Drop a video or folder to create one — runs resume from the last checkpoint.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-0.5">
      {projects.map((p) => (
        <div
          key={p.workspace}
          className="group flex items-center justify-between gap-2 rounded-md px-2 py-1.5 text-xs hover:bg-panel2"
        >
          <button
            type="button"
            className="min-w-0 flex-1 text-left"
            onClick={() => void resumeProject(p.workspace)}
            disabled={!p.resumable && !p.completed}
            title={p.workspace}
          >
            <div className="truncate font-medium text-ink">{p.inputName}</div>
            <div className="text-[10px] text-ink-dim">
              {p.completed
                ? "Complete — open"
                : p.resumable
                  ? `Resume · step ${p.latestIter.toLocaleString()} / ${p.totalSteps.toLocaleString()}`
                  : "Incomplete"}
            </div>
          </button>
          <button
            type="button"
            onClick={() => void deleteProjectEntry(p.workspace)}
            className="btn btn-ghost btn-danger px-1.5 py-0.5 text-[10px] opacity-0 group-hover:opacity-100 focus:opacity-100"
            title="Delete this project"
            aria-label={`Delete ${p.inputName}`}
          >
            Delete
          </button>
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

function LayerToggle({
  label,
  checked,
  disabled,
  detail,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  detail?: string;
  onChange: (v: boolean) => void;
}) {
  return (
    <label
      className={`flex cursor-pointer items-center justify-between gap-2 text-xs ${disabled ? "opacity-40" : ""}`}
    >
      <span className="flex min-w-0 items-center gap-2">
        <input
          type="checkbox"
          className="accent-[var(--color-accent,#38b7a6)]"
          checked={checked}
          disabled={disabled}
          onChange={(e) => onChange(e.target.checked)}
        />
        <span className="truncate">{label}</span>
      </span>
      {detail && <span className="shrink-0 tabular-nums text-[10px] text-ink-dim">{detail}</span>}
    </label>
  );
}

export default function SceneTree() {
  const suite = useStore((s) => s.suite);
  const screen = useStore((s) => s.screen);
  const leftPanelOpen = useStore((s) => s.leftPanelOpen);
  const inputPath = useStore((s) => s.inputPath);
  const workspace = useStore((s) => s.workspace);
  const stages = useStore((s) => s.stages);
  const splatCount = useStore((s) => s.splatCount);
  const latestIter = useStore((s) => s.latestIter);
  const totalSteps = useStore((s) => s.totalSteps);
  const resultPath = useStore((s) => s.resultPath);
  const reconLayers = useStore((s) => s.reconLayers);
  const setReconLayer = useStore((s) => s.setReconLayer);
  const sparseCloudPath = useStore((s) => s.sparseCloudPath);
  const denseCloudPath = useStore((s) => s.denseCloudPath);
  const latestSplatPath = useStore((s) => s.latestSplatPath);
  const latestMeshPath = useStore((s) => s.latestMeshPath);
  const sparsePointCount = useStore((s) => s.sparsePointCount);
  const densePointCount = useStore((s) => s.densePointCount);
  const ingestFrameCount = useStore((s) => s.ingestFrameCount);
  const totalCameras = useStore((s) => s.totalCameras);

  if (!leftPanelOpen) return null;

  if (suite === "geospatial") {
    return (
      <aside
        className="shell-panel flex w-60 shrink-0 flex-col overflow-y-auto border-r border-edge bg-panel"
        aria-label="Geospatial layers"
      >
        <div className="border-b border-edge px-3 py-2.5">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-[var(--color-hydro)]">
            Layers
          </div>
          <div className="mt-0.5 text-[11px] text-ink-dim">Basemap, terrain, flood</div>
        </div>
        <GeoLayerTree />
      </aside>
    );
  }

  return (
    <aside
      className="shell-panel flex w-60 shrink-0 flex-col overflow-y-auto border-r border-edge bg-panel"
      aria-label="Scene and projects"
    >
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
            {ingestFrameCount > 0 && (
              <div className="mt-1 text-[10px] text-ink-dim">
                {ingestFrameCount.toLocaleString()} frames gated
              </div>
            )}
          </Section>

          <Section title="Stage layers">
            <div className="flex flex-col gap-1.5">
              <LayerToggle
                label="Cameras"
                checked={reconLayers.cameras}
                disabled={totalCameras === 0}
                detail={totalCameras > 0 ? String(totalCameras) : undefined}
                onChange={(v) => setReconLayer("cameras", v)}
              />
              <LayerToggle
                label="Camera path"
                checked={reconLayers.cameraPath}
                disabled={ingestFrameCount === 0 && totalCameras === 0}
                onChange={(v) => setReconLayer("cameraPath", v)}
              />
              <LayerToggle
                label="Sparse cloud"
                checked={reconLayers.sparse}
                disabled={!sparseCloudPath}
                detail={sparsePointCount > 0 ? sparsePointCount.toLocaleString() : undefined}
                onChange={(v) => setReconLayer("sparse", v)}
              />
              <LayerToggle
                label="Dense cloud"
                checked={reconLayers.dense}
                disabled={!denseCloudPath}
                detail={densePointCount > 0 ? densePointCount.toLocaleString() : undefined}
                onChange={(v) => setReconLayer("dense", v)}
              />
              <LayerToggle
                label="Splats"
                checked={reconLayers.splat}
                disabled={!latestSplatPath}
                detail={splatCount > 0 ? splatCount.toLocaleString() : undefined}
                onChange={(v) => setReconLayer("splat", v)}
              />
              <LayerToggle
                label="Mesh"
                checked={reconLayers.mesh}
                disabled={!latestMeshPath}
                detail={latestMeshPath ? "ready" : "export"}
                onChange={(v) => setReconLayer("mesh", v)}
              />
            </div>
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
    </aside>
  );
}
