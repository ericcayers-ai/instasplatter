import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../state/store";

const VIDEO_EXTS = ["mp4", "mov", "avi", "mkv", "webm", "m4v", "mts", "3gp"];

export default function DropZone() {
  const [hover, setHover] = useState(false);
  const startJob = useStore((s) => s.startJob);
  const profile = useStore((s) => s.profile);
  const resolved = useStore((s) => s.resolved);

  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "over") setHover(true);
      else if (event.payload.type === "drop") {
        setHover(false);
        const path = event.payload.paths[0];
        if (path) startJob(path);
      } else setHover(false);
    });
    return () => {
      un.then((f) => f());
    };
  }, [startJob]);

  const browse = async (dir: boolean) => {
    const sel = await open(
      dir
        ? { directory: true, title: "Choose an image folder" }
        : {
            title: "Choose a video",
            filters: [{ name: "Video", extensions: VIDEO_EXTS }],
          },
    );
    if (typeof sel === "string") startJob(sel);
  };

  const vram = profile ? Math.round(profile.vram_mb / 1024) : 0;

  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 p-10">
      <div className="float-in text-sm text-ink-dim">Drop a video or a folder of photos to reconstruct a scene.</div>

      <div
        className={`float-in flex w-full max-w-xl flex-col items-center gap-4 rounded border-2 border-dashed px-10 py-14 transition-colors ${
          hover ? "border-accent bg-accent/5" : "border-edge bg-panel"
        }`}
      >
        <div className="text-sm font-medium">{hover ? "Release to start" : "Drag and drop here"}</div>
        <div className="text-xs text-ink-dim">MP4, MOV, MKV, or a folder of JPG / PNG</div>
        <div className="mt-2 flex gap-2">
          <button onClick={() => browse(false)} className="btn btn-primary">
            Choose video
          </button>
          <button onClick={() => browse(true)} className="btn">
            Choose folder
          </button>
        </div>
      </div>

      {profile && (
        <div className="float-in flex items-center gap-2 rounded border border-edge bg-panel px-3 py-1.5 text-xs text-ink-dim">
          {profile.gpu_name}, {vram} GB, auto preset: <b className="text-ink">{resolved?.preset ?? profile.auto_preset}</b>
        </div>
      )}
    </div>
  );
}
