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

  return (
    <div className="splatter-field flex h-full flex-col items-center justify-center gap-8 p-10">
      <header className="float-in max-w-lg text-center">
        <h1 className="font-display text-[2rem] font-bold leading-tight tracking-tight text-ink">
          Capture becomes form
        </h1>
        <p className="mt-2 text-sm leading-relaxed text-ink-dim">
          Drop a video, a folder of photos, or several at once. InstaSplatter builds a navigable Gaussian splat while you watch.
        </p>
      </header>

      <div
        role="button"
        tabIndex={0}
        aria-label="Drop a video or image folder to start reconstruction"
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            void browse(false, false);
          }
        }}
        className={`float-in flex w-full max-w-xl flex-col items-center gap-4 rounded border-2 border-dashed px-10 py-14 transition-colors ${
          hover ? "border-accent bg-accent/5" : "border-edge bg-panel"
        }`}
      >
        <div className="text-sm font-medium">{hover ? "Release to start" : "Drag and drop here"}</div>
        <div className="text-xs text-ink-dim">MP4, MOV, MKV, folders of JPG / PNG, or multiple files</div>
        <div className="mt-2 flex flex-wrap justify-center gap-2">
          <button onClick={() => browse(false, false)} className="btn btn-primary">
            Choose video
          </button>
          <button onClick={() => browse(true, false)} className="btn">
            Choose folder
          </button>
          <button onClick={() => browse(false, true)} className="btn">
            Batch videos
          </button>
          <button onClick={() => browse(true, true)} className="btn">
            Batch folders
          </button>
        </div>
      </div>

      {profile && (
        <div className="float-in flex items-center gap-2 rounded border border-edge bg-panel px-3 py-1.5 font-mono text-xs text-ink-dim">
          {profile.gpu_name}, {vram} GB, auto preset:{" "}
          <b className="text-ink">{resolved?.preset ?? profile.auto_preset}</b>
          {resolved?.denseInit !== false && <span className="text-accent"> · dense init</span>}
          {resolved?.progressiveResolution !== false && <span className="text-accent"> · progressive</span>}
          {resolved?.mipFilter !== false && <span className="text-accent"> · mip</span>}
          {resolved?.postPolish !== false && <span className="text-accent"> · polish</span>}
        </div>
      )}

      {queueItems.length > 0 && (
        <div className="float-in w-full max-w-xl">
          <BatchQueue />
        </div>
      )}
    </div>
  );
}
