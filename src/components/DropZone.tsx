import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../state/store";
import BatchQueue from "./BatchQueue";

const VIDEO_EXTS = ["mp4", "mov", "avi", "mkv", "webm", "m4v", "mts", "3gp"];

export default function DropZone() {
  const [hover, setHover] = useState(false);
  const startJob = useStore((s) => s.startJob);
  const enqueueJobs = useStore((s) => s.enqueueJobs);
  const profile = useStore((s) => s.profile);
  const resolved = useStore((s) => s.resolved);
  const queueItems = useStore((s) => s.queueItems);
  const projects = useStore((s) => s.recentProjects);
  const resumeProject = useStore((s) => s.resumeProject);
  const setLeftPanelOpen = useStore((s) => s.setLeftPanelOpen);

  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "over") setHover(true);
      else if (event.payload.type === "drop") {
        setHover(false);
        const paths = event.payload.paths ?? [];
        if (paths.length > 1) void enqueueJobs(paths);
        else if (paths[0]) void startJob(paths[0]);
      } else setHover(false);
    });
    return () => {
      un.then((f) => f());
    };
  }, [startJob, enqueueJobs]);

  const browse = async (dir: boolean, multiple: boolean) => {
    const sel = await open(
      dir
        ? { directory: true, multiple, title: multiple ? "Choose image folders" : "Choose an image folder" }
        : {
            title: multiple ? "Choose videos" : "Choose a video",
            multiple,
            filters: [{ name: "Video", extensions: VIDEO_EXTS }],
          },
    );
    if (!sel) return;
    const paths = Array.isArray(sel) ? sel : [sel];
    if (paths.length > 1) void enqueueJobs(paths);
    else if (paths[0]) void startJob(paths[0]);
  };

  const vram = profile ? Math.round(profile.vram_mb / 1024) : 0;
  const recent = projects.slice(0, 4);

  return (
    <div className="splatter-field flex h-full flex-col items-center justify-center gap-7 p-8 md:p-12">
      <header className="float-in max-w-xl text-center">
        <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-accent">Reconstruction</p>
        <h1 className="font-display mt-2 text-[clamp(2rem,4vw,2.75rem)] font-bold leading-[1.05] tracking-tight text-ink text-balance">
          InstaSplatter
        </h1>
        <p className="mt-3 text-sm leading-relaxed text-ink-dim text-pretty">
          Drop a video or photo folder. A navigable Gaussian splat forms while you watch.
        </p>
      </header>

      <div
        role="button"
        tabIndex={0}
        aria-label="Drop a video or image folder to start reconstruction"
        data-hover={hover}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            void browse(false, false);
          }
        }}
        className="drop-plane float-in float-in-delay-1 flex flex-col items-center gap-5 px-8 py-12 md:px-12 md:py-14"
      >
        <div className="text-center">
          <div className="text-sm font-semibold text-ink">
            {hover ? "Release to start" : "Drop capture here"}
          </div>
          <div className="mt-1 text-xs text-ink-dim">MP4 · MOV · MKV · JPG / PNG folders · batch OK</div>
        </div>
        <div className="flex flex-wrap justify-center gap-2">
          <button type="button" onClick={() => void browse(false, false)} className="btn btn-primary">
            Choose video
          </button>
          <button type="button" onClick={() => void browse(true, false)} className="btn">
            Choose folder
          </button>
          <button type="button" onClick={() => void browse(false, true)} className="btn btn-ghost">
            Batch videos
          </button>
          <button type="button" onClick={() => void browse(true, true)} className="btn btn-ghost">
            Batch folders
          </button>
        </div>
      </div>

      {profile && (
        <div className="float-in float-in-delay-2 flex flex-wrap items-center justify-center gap-x-3 gap-y-1 font-mono text-[11px] text-ink-dim">
          <span>
            {profile.gpu_name} · {vram} GB
          </span>
          <span aria-hidden>·</span>
          <span>
            Preset <b className="text-ink">{resolved?.preset ?? profile.auto_preset}</b>
          </span>
          {(resolved?.denseInit !== false ||
            resolved?.progressiveResolution !== false ||
            resolved?.mipFilter !== false) && (
            <>
              <span aria-hidden>·</span>
              <span className="text-accent">Auto pipeline on</span>
            </>
          )}
        </div>
      )}

      {recent.length > 0 && (
        <section className="float-in float-in-delay-3 w-full max-w-xl" aria-label="Recent projects">
          <div className="mb-2 flex items-center justify-between">
            <h2 className="text-[10px] font-semibold uppercase tracking-wider text-ink-dim">Recent</h2>
            <button
              type="button"
              className="btn btn-ghost px-1.5 py-0.5 text-[10px]"
              onClick={() => setLeftPanelOpen(true)}
            >
              Open panel
            </button>
          </div>
          <ul className="flex flex-col gap-1">
            {recent.map((p) => (
              <li key={p.workspace}>
                <button
                  type="button"
                  onClick={() => void resumeProject(p.workspace)}
                  className="flex w-full items-center justify-between gap-3 rounded-md border border-transparent px-3 py-2 text-left text-xs transition hover:border-edge hover:bg-panel/80"
                  disabled={!p.resumable && !p.completed}
                  title={p.workspace}
                >
                  <span className="min-w-0 truncate font-medium text-ink">{p.inputName}</span>
                  <span className="shrink-0 text-[10px] text-ink-dim">
                    {p.completed
                      ? "Complete"
                      : p.resumable
                        ? `Resume · ${p.latestIter.toLocaleString()}`
                        : "Incomplete"}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      {queueItems.length > 0 && (
        <div className="float-in w-full max-w-xl">
          <BatchQueue />
        </div>
      )}
    </div>
  );
}
