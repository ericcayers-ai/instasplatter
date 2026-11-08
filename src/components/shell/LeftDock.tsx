import { useEffect, useRef, useState } from "react";
import { useStore } from "../../state/store";
import GeoLayerTree from "../../geospatial/GeoLayerTree";
import { useCaptureStart } from "../CaptureStart";
import DockSection from "./DockSection";
import { ReconstructionSettings } from "./PropertiesPanel";
import type { ShellFocus } from "./shellFocus";

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

function RecentProjects() {
  const projects = useStore((s) => s.recentProjects);
  const resumeProject = useStore((s) => s.resumeProject);
  const deleteProjectEntry = useStore((s) => s.deleteProjectEntry);

  if (projects.length === 0) {
    return (
      <div className="text-xs leading-relaxed text-ink-dim">
        No saved projects yet. Drop a video or folder to create one.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-0.5">
      {projects.map((p) => (
        <div
          key={p.workspace}
          className="group flex items-center justify-between gap-2 rounded-md px-1.5 py-1.5 text-xs hover:bg-panel2"
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
            onClick={() => {
              if (
                !window.confirm(
                  `Delete project “${p.inputName}”? This removes the workspace and cannot be undone.`,
                )
              ) {
                return;
              }
              void deleteProjectEntry(p.workspace);
            }}
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
          <div
            key={`${c.name}-${i}`}
            className="flex items-center justify-between text-[11px] text-ink-dim"
          >
            <span className="truncate">{c.name}</span>
            <span className="tabular-nums">{Math.round(c.confidence * 100)}%</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function ExportMenu() {
  const [openMenu, setOpenMenu] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const exportSplatAction = useStore((s) => s.exportSplatAction);
  const exportMeshAction = useStore((s) => s.exportMeshAction);
  const exportSchematicAction = useStore((s) => s.exportSchematicAction);
  const experimentalOn = useStore((s) => !!s.resolved?.experimentalMode);

  useEffect(() => {
    if (!openMenu) return;
    const onDoc = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpenMenu(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpenMenu(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [openMenu]);

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        className="btn btn-primary w-full"
        aria-expanded={openMenu}
        aria-haspopup="menu"
        onClick={() => setOpenMenu((v) => !v)}
      >
        Export
        <span className="text-[10px] opacity-80" aria-hidden>
          ▾
        </span>
      </button>
      {openMenu && (
        <div className="menu menu--start" role="menu">
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpenMenu(false);
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
              setOpenMenu(false);
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
                setOpenMenu(false);
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

function CaptureBrowse() {
  const capture = useCaptureStart();

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap gap-1.5">
        <button
          type="button"
          onClick={() => void capture.chooseVideos(false)}
          className="btn btn-primary text-[11px]"
        >
          Add video
        </button>
        <button
          type="button"
          onClick={() => void capture.chooseImages()}
          className="btn btn-ghost-teal text-[11px]"
        >
          Add images
        </button>
      </div>
      <div className="flex flex-wrap gap-1.5">
        <button
          type="button"
          onClick={() => void capture.chooseFolder()}
          className="btn btn-ghost px-2 py-1 text-[10px]"
        >
          Image folder
        </button>
        <button
          type="button"
          onClick={() => void capture.chooseVideos(true)}
          className="btn btn-ghost px-2 py-1 text-[10px]"
        >
          Batch videos
        </button>
        <button
          type="button"
          onClick={() => void capture.choosePanoramas()}
          className="btn btn-ghost px-2 py-1 text-[10px]"
          disabled={capture.busy}
          title="Project local 2:1 panoramas from different positions into four views each"
        >
          360° panoramas
        </button>
      </div>
      <div className="text-[10px] leading-relaxed text-ink-dim">
        Panoramas need at least two capture positions for 3D depth.
      </div>
      {capture.error && (
        <div role="alert" className="text-[11px] text-danger">
          {capture.error}
        </div>
      )}
      {capture.dialog}
    </div>
  );
}

function ReconJobActions() {
  const screen = useStore((s) => s.screen);
  const resultPath = useStore((s) => s.resultPath);
  const jobError = useStore((s) => s.jobError);
  const cancelJob = useStore((s) => s.cancelJob);
  const backHome = useStore((s) => s.backHome);
  const stages = useStore((s) => s.stages);
  const inputPath = useStore((s) => s.inputPath);
  const workspace = useStore((s) => s.workspace);

  if (screen !== "processing") {
    return (
      <div className="text-xs text-ink-dim">Start a capture to run reconstruction stages here.</div>
    );
  }

  const running = !resultPath && !jobError;
  const name = inputPath?.split(/[\\/]/).pop() ?? workspace?.split(/[\\/]/).pop() ?? "";

  return (
    <div className="flex flex-col gap-3">
      {name && (
        <div className="truncate text-xs font-medium text-ink" title={name}>
          {name}
        </div>
      )}
      <div className="flex flex-col gap-1.5" aria-label="Reconstruction stages">
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
              <span>{s.label}</span>
              {pct && <span className="tabular-nums text-accent2">{pct.trim()}</span>}
            </span>
          );
        })}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {running && (
          <button type="button" onClick={cancelJob} className="btn btn-danger text-[11px]">
            Cancel
          </button>
        )}
        {!running && (
          <button type="button" onClick={backHome} className="btn btn-ghost-teal text-[11px]">
            New
          </button>
        )}
        {resultPath && <ExportMenu />}
      </div>
    </div>
  );
}

export default function LeftDock({ focus }: { focus: ShellFocus | null }) {
  const suite = useStore((s) => s.suite);
  const leftPanelOpen = useStore((s) => s.leftPanelOpen);
  const screen = useStore((s) => s.screen);
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
  const splatCount = useStore((s) => s.splatCount);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!focus || !leftPanelOpen) return;
    const id =
      focus === "capture"
        ? "dock-capture"
        : focus === "reconstruction"
          ? "dock-reconstruction"
          : focus === "scene" || focus === "layers"
            ? "dock-scene"
            : null;
    if (!id) return;
    const el = document.getElementById(id);
    el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [focus, leftPanelOpen]);

  if (!leftPanelOpen) return null;

  if (suite === "geospatial") {
    return (
      <aside className="shell-dock shell-dock--left" aria-label="Geospatial layers">
        <div className="shell-dock__scroll" ref={scrollRef}>
          <DockSection
            id="dock-scene"
            title="Layers"
            tone="hydro"
            focused={focus === "layers" || focus === "scene" || focus === "capture"}
            defaultOpen
          >
            <div className="-mx-3">
              <GeoLayerTree />
            </div>
          </DockSection>
        </div>
      </aside>
    );
  }

  return (
    <aside className="shell-dock shell-dock--left" aria-label="Capture and reconstruction">
      <div className="shell-dock__scroll" ref={scrollRef}>
        <DockSection
          id="dock-capture"
          title="Capture"
          tone="accent"
          focused={focus === "capture"}
          defaultOpen
        >
          <div className="flex flex-col gap-3">
            <CaptureBrowse />
            <div>
              <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-dim">
                Recent
              </div>
              <RecentProjects />
            </div>
          </div>
        </DockSection>

        <DockSection
          id="dock-reconstruction"
          title="Reconstruction"
          tone="brass"
          focused={focus === "reconstruction"}
          defaultOpen={screen === "processing" || focus === "reconstruction"}
        >
          <div className="flex flex-col gap-4">
            <ReconJobActions />
            <ReconstructionSettings />
          </div>
        </DockSection>

        <DockSection
          id="dock-scene"
          title="Scene"
          focused={focus === "scene"}
          defaultOpen={screen === "processing"}
        >
          {screen !== "processing" ? (
            <div className="text-xs text-ink-dim">Layers appear once a job is running.</div>
          ) : (
            <div className="flex flex-col gap-3">
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
              <div>
                <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-dim">
                  Cameras
                </div>
                <CameraList />
              </div>
            </div>
          )}
        </DockSection>
      </div>
    </aside>
  );
}
