import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { SplatRenderer } from "../splat/renderer";
import { api } from "../lib/ipc";
import { useStore } from "../state/store";

/** Row-major 3x3 as the flat array the Rust side expects. */
function flatten(rot: number[][]): number[] {
  return [...rot[0], ...rot[1], ...rot[2]];
}

export default function Viewport() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<SplatRenderer | null>(null);
  const loadedPathRef = useRef<string | null>(null);
  const firstLoadRef = useRef(true);
  const drawnCamerasRef = useRef(0);

  const latestSplatPath = useStore((s) => s.latestSplatPath);
  const setSplatCount = useStore((s) => s.setSplatCount);
  const cameras = useStore((s) => s.cameras);
  const resultPath = useStore((s) => s.resultPath);
  const workspace = useStore((s) => s.workspace);
  const [orientMessage, setOrientMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!canvasRef.current) return;
    const r = new SplatRenderer(canvasRef.current);
    r.attachControls();
    r.autoOrbit = true;
    r.onStats = (n) => setSplatCount(n);
    rendererRef.current = r;
    return () => {
      r.dispose();
      rendererRef.current = null;
    };
  }, [setSplatCount]);

  // Cameras arrive one at a time and only ever grow, so push the new tail
  // rather than rebuilding the whole line buffer on every registration.
  useEffect(() => {
    const r = rendererRef.current;
    if (!r) return;
    if (cameras.length < drawnCamerasRef.current) {
      r.clearFrustums();
      drawnCamerasRef.current = 0;
    }
    for (let i = drawnCamerasRef.current; i < cameras.length; i++) {
      r.addFrustum({ apex: cameras[i].apex, corners: cameras[i].corners });
    }
    drawnCamerasRef.current = cameras.length;
  }, [cameras]);

  useEffect(() => {
    const r = rendererRef.current;
    if (!r || !latestSplatPath || loadedPathRef.current === latestSplatPath) return;
    loadedPathRef.current = latestSplatPath;
    let stale = false;
    (async () => {
      try {
        const url = convertFileSrc(latestSplatPath);
        const resp = await fetch(url);
        const buf = await resp.arrayBuffer();
        if (stale) return;
        r.loadPly(buf, firstLoadRef.current);
        firstLoadRef.current = false;
      } catch (err) {
        console.error("failed to load splat:", err);
      }
    })();
    return () => {
      stale = true;
    };
  }, [latestSplatPath]);

  // The orientation the user settles on is what a later export bakes in, so
  // persist it with the project rather than only in the viewport.
  const persist = () => {
    const r = rendererRef.current;
    if (!r || !workspace) return;
    void api.saveProjectOrientation(workspace, flatten(r.modelRotation)).catch(() => {});
  };

  const turn = (axis: [number, number, number], degrees: number) => {
    rendererRef.current?.rotateModel(axis, (degrees * Math.PI) / 180);
    persist();
  };

  const snap = () => {
    rendererRef.current?.snapUpToNearestAxis();
    persist();
    setOrientMessage("Snapped up to the nearest axis.");
  };

  const reset = () => {
    rendererRef.current?.resetModelRotation();
    persist();
    setOrientMessage(null);
  };

  const alignToGround = async () => {
    const r = rendererRef.current;
    const source = resultPath ?? latestSplatPath;
    if (!r || !source) return;
    setOrientMessage("Looking for a ground plane.");
    try {
      const plane = await api.estimateUpAxis(source);
      if (!plane) {
        setOrientMessage("No ground plane was found in this scene.");
        return;
      }
      r.alignUp(plane.normal);
      persist();
      setOrientMessage(`Ground plane found, closest to ${plane.nearestAxis}.`);
    } catch (err) {
      setOrientMessage(String(err));
    }
  };

  // Rotating the model is only meaningful once there is a model to rotate.
  const hasModel = latestSplatPath !== null;

  return (
    <div className="relative h-full w-full">
      <canvas
        ref={canvasRef}
        className="h-full w-full cursor-grab active:cursor-grabbing"
        style={{ touchAction: "none" }}
      />

      {hasModel && (
        <div className="absolute right-4 top-20 flex w-40 flex-col gap-2 rounded-xl border border-edge bg-panel/80 p-3 text-xs backdrop-blur">
          <div className="text-ink-dim">Model orientation</div>
          <div className="grid grid-cols-3 gap-1">
            {(
              [
                ["X", [1, 0, 0]],
                ["Y", [0, 1, 0]],
                ["Z", [0, 0, 1]],
              ] as [string, [number, number, number]][]
            ).map(([label, axis]) => (
              <div key={label} className="flex flex-col gap-1">
                <button
                  onClick={() => turn(axis, 90)}
                  className="rounded border border-edge py-1 transition hover:border-accent/50"
                  title={`Turn 90 degrees about ${label}`}
                >
                  {label}+
                </button>
                <button
                  onClick={() => turn(axis, -90)}
                  className="rounded border border-edge py-1 transition hover:border-accent/50"
                  title={`Turn 90 degrees about ${label}, the other way`}
                >
                  {label}-
                </button>
              </div>
            ))}
          </div>
          <button
            onClick={snap}
            className="rounded border border-edge py-1 transition hover:border-accent/50"
          >
            Snap up to axis
          </button>
          <button
            onClick={alignToGround}
            className="rounded border border-edge py-1 transition hover:border-accent/50"
          >
            Align to ground
          </button>
          <button
            onClick={reset}
            className="rounded border border-edge py-1 text-ink-dim transition hover:border-accent/50"
          >
            Reset
          </button>
          {orientMessage && <div className="text-ink-dim">{orientMessage}</div>}
        </div>
      )}
    </div>
  );
}
