import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  CAPTURE_IMAGE_EXTENSIONS,
  CAPTURE_VIDEO_EXTENSIONS,
  classifyCapturePaths,
} from "../lib/captureInput";
import { api, type PresetName } from "../lib/ipc";
import { applyPresetPatch } from "../lib/presets";
import { useFocusTrap } from "../lib/focusTrap";
import { useStore } from "../state/store";

const PRESETS: PresetName[] = ["draft", "eco", "balanced", "high", "max"];

interface PendingVideo {
  paths: string[];
  preset: PresetName;
}

export function useCaptureStart() {
  const startJob = useStore((state) => state.startJob);
  const enqueueJobs = useStore((state) => state.enqueueJobs);
  const updateSettings = useStore((state) => state.updateSettings);
  const resolved = useStore((state) => state.resolved);
  const profile = useStore((state) => state.profile);
  const [pending, setPending] = useState<PendingVideo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [busyMessage, setBusyMessage] = useState("Adding files…");
  const trapRef = useFocusTrap(pending !== null);

  const launch = useCallback(async (rawPaths: string[]) => {
    setError(null);
    const selection = classifyCapturePaths(rawPaths);
    try {
      if (selection.kind === "video") {
        setPending({
          paths: selection.paths,
          preset: resolved?.preset ?? profile?.auto_preset ?? "draft",
        });
        return;
      }
      if (selection.kind === "images") {
        // stageImageSelection copies every file synchronously on the backend
        // before returning — for a large batch this can take a while with no
        // events fired, so surface a busy state instead of leaving the drop
        // zone looking unresponsive.
        setBusy(true);
        setBusyMessage("Adding files…");
        const stagedDirectory = await api.stageImageSelection(selection.paths);
        await startJob(stagedDirectory);
        return;
      }
      if (selection.kind === "folder") {
        await startJob(selection.paths[0]);
        return;
      }
      setError("Choose videos, image files, or one image folder—not a mixed selection.");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }, [profile, resolved, startJob]);

  const chooseImages = async () => {
    const selected = await open({
      title: "Choose images",
      multiple: true,
      filters: [{ name: "Images", extensions: CAPTURE_IMAGE_EXTENSIONS }],
    });
    if (!selected) return;
    await launch(Array.isArray(selected) ? selected : [selected]);
  };

  const choosePanoramas = async () => {
    const selected = await open({
      title: "Choose 360° panoramas",
      multiple: true,
      filters: [{ name: "Images", extensions: CAPTURE_IMAGE_EXTENSIONS }],
    });
    if (!selected) return;
    setError(null);
    setBusy(true);
    setBusyMessage("Projecting panoramas…");
    try {
      const stagedDirectory = await api.stagePanoramaSelection(
        Array.isArray(selected) ? selected : [selected],
      );
      await startJob(stagedDirectory);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
      setBusyMessage("Adding files…");
    }
  };

  const chooseFolder = async () => {
    const selected = await open({
      title: "Choose an image folder",
      directory: true,
      multiple: false,
    });
    if (typeof selected === "string") await launch([selected]);
  };

  const chooseVideos = async (multiple = false) => {
    const selected = await open({
      title: multiple ? "Choose videos" : "Choose a video",
      multiple,
      filters: [{ name: "Video", extensions: CAPTURE_VIDEO_EXTENSIONS }],
    });
    if (!selected) return;
    await launch(Array.isArray(selected) ? selected : [selected]);
  };

  const confirmVideo = async () => {
    if (!pending) return;
    const selection = pending;
    setPending(null);
    try {
      if (!resolved?.experimentalMode && selection.preset !== resolved?.preset) {
        await updateSettings(applyPresetPatch(selection.preset));
      }
      if (selection.paths.length === 1) await startJob(selection.paths[0]);
      else await enqueueJobs(selection.paths);
    } catch (reason) {
      setError(String(reason));
    }
  };

  useEffect(() => {
    if (!pending) return;
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPending(null);
    };
    document.addEventListener("keydown", close);
    return () => document.removeEventListener("keydown", close);
  }, [pending]);

  const dialog = pending ? (
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-bg/75 p-4 backdrop-blur-[2px]">
      <div
        ref={trapRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="capture-confirm-title"
        className="w-full max-w-md rounded border border-edge bg-panel2 p-5 shadow-lg"
      >
        <h2 id="capture-confirm-title" className="font-display text-base font-bold text-ink">
          Confirm reconstruction
        </h2>
        <p className="mt-2 truncate text-xs text-ink-dim" title={pending.paths.join("\n")}>
          {pending.paths.length === 1
            ? pending.paths[0].split(/[\\/]/).pop()
            : `${pending.paths.length} videos queued`}
        </p>

        <div className="mt-4">
          <div className="mb-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-dim">
            Quality preset
          </div>
          <div className="seg flex-wrap" role="group" aria-label="Quality preset">
            {PRESETS.map((preset) => (
              <button
                key={preset}
                type="button"
                aria-pressed={pending.preset === preset}
                disabled={!!resolved?.experimentalMode}
                onClick={() =>
                  setPending((current) => (current ? { ...current, preset } : current))
                }
              >
                {preset[0].toUpperCase() + preset.slice(1)}
              </button>
            ))}
          </div>
          {resolved?.experimentalMode && (
            <p className="mt-2 text-[11px] text-danger">
              Experimental Mode is ON and enforces the Max quality floor.
            </p>
          )}
        </div>

        <dl className="mt-4 grid grid-cols-2 gap-x-4 gap-y-1 text-[11px]">
          <dt className="text-ink-dim">Compute</dt>
          <dd className="text-right text-ink">
            {profile?.has_cuda ? "CUDA + GPU trainer preference" : "Portable Vulkan / DX12"}
          </dd>
          <dt className="text-ink-dim">Live camera tracking</dt>
          <dd className="text-right text-ink">{resolved?.liveInit ? "On" : "Off"}</dd>
          <dt className="text-ink-dim">Dense initialization</dt>
          <dd className="text-right text-ink">{resolved?.denseInit === false ? "Off" : "On"}</dd>
        </dl>

        <div className="mt-5 flex justify-end gap-2">
          <button type="button" className="btn" onClick={() => setPending(null)}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void confirmVideo()}
            autoFocus
          >
            {pending.paths.length === 1 ? "Start reconstruction" : "Add to queue"}
          </button>
        </div>
      </div>
    </div>
  ) : null;

  return {
    chooseImages,
    choosePanoramas,
    chooseFolder,
    chooseVideos,
    launch,
    dialog,
    error,
    busy,
    busyMessage,
  };
}
