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
  const openPrefs = useStore((s) => s.openPrefs);

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
    <div className="hero-gradient flex h-full flex-col items-center justify-center gap-8 p-10">
      <div className="float-in flex flex-col items-center gap-2">
        <div className="text-4xl font-semibold tracking-tight">InstaSplatter</div>
        <div className="text-ink-dim text-sm">
          Drop a video or photo folder — watch the 3D scene build itself.
        </div>
      </div>

      <div
        className={`float-in flex w-full max-w-2xl flex-col items-center gap-5 rounded-3xl border-2 border-dashed border-edge bg-panel/60 px-10 py-16 backdrop-blur transition-all duration-200 ${hover ? "drop-active scale-[1.02]" : ""}`}
      >
        <svg width="64" height="64" viewBox="0 0 64 64" fill="none">
          <circle cx="32" cy="30" r="14" fill="url(#g1)" opacity="0.9" />
          <circle cx="20" cy="38" r="8" fill="url(#g1)" opacity="0.6" />
          <circle cx="44" cy="39" r="7" fill="url(#g1)" opacity="0.6" />
          <circle cx="26" cy="20" r="6" fill="url(#g1)" opacity="0.5" />
          <circle cx="42" cy="21" r="5" fill="url(#g1)" opacity="0.5" />
          <defs>
            <linearGradient id="g1" x1="0" y1="0" x2="64" y2="64">
              <stop stopColor="#38e0c7" />
              <stop offset="1" stopColor="#8b5cf6" />
            </linearGradient>
          </defs>
        </svg>
        <div className="text-lg font-medium">
          {hover ? "Release to start" : "Drag & drop here"}
        </div>
        <div className="text-ink-dim text-xs">MP4 · MOV · MKV · or a folder of JPG / PNG</div>
        <div className="mt-2 flex gap-3">
          <button
            onClick={() => browse(false)}
            className="rounded-full bg-accent px-5 py-2 text-sm font-medium text-black transition hover:brightness-110"
          >
            Choose video
          </button>
          <button
            onClick={() => browse(true)}
            className="rounded-full border border-edge bg-panel2 px-5 py-2 text-sm text-ink transition hover:border-accent/50"
          >
            Choose folder
          </button>
        </div>
      </div>

      <div className="float-in flex items-center gap-3 text-xs text-ink-dim">
        {profile && (
          <span className="flex items-center gap-2 rounded-full border border-edge bg-panel px-4 py-2">
            <span className="h-2 w-2 rounded-full bg-accent" />
            {profile.gpu_name} · {vram} GB · auto preset:{" "}
            <b className="capitalize text-ink">{resolved?.preset ?? profile.auto_preset}</b>
          </span>
        )}
        <button
          onClick={() => openPrefs(true)}
          className="rounded-full border border-edge bg-panel px-4 py-2 transition hover:border-accent/50 hover:text-ink"
        >
          Preferences
        </button>
      </div>
    </div>
  );
}
