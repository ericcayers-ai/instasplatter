import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { SplatRenderer } from "../splat/renderer";
import { orbitBasis, type Vec3 } from "../splat/camera";
import { api } from "../lib/ipc";
import { useStore } from "../state/store";

/** Row-major 3x3 as the flat array the Rust side expects. */
function flatten(rot: number[][]): number[] {
  return [...rot[0], ...rot[1], ...rot[2]];
}

const GIZMO_AXES: { label: string; dir: Vec3; color: string }[] = [
  { label: "X", dir: [1, 0, 0], color: "#e5654f" },
  { label: "Y", dir: [0, -1, 0], color: "#38b7a6" }, // world up, ROADMAP-V2 1.2
  { label: "Z", dir: [0, 0, 1], color: "#6d8dfa" },
];

/**
 * A small always-on-top indicator of which way the world axes point, updated
 * imperatively from a requestAnimationFrame loop rather than React state, so
 * a decorative overlay never costs a re-render.
 */
function AxisGizmo({ rendererRef }: { rendererRef: React.RefObject<SplatRenderer | null> }) {
  const dotsRef = useRef<(HTMLDivElement | null)[]>([]);
  const labelsRef = useRef<(HTMLDivElement | null)[]>([]);

  useEffect(() => {
    let raf = 0;
    const radius = 26;
    const tick = () => {
      const r = rendererRef.current;
      if (r) {
        const basis = orbitBasis(r.camera);
        GIZMO_AXES.forEach((a, i) => {
          const sx = a.dir[0] * basis.right[0] + a.dir[1] * basis.right[1] + a.dir[2] * basis.right[2];
          const sy = a.dir[0] * basis.down[0] + a.dir[1] * basis.down[1] + a.dir[2] * basis.down[2];
          const sz = a.dir[0] * basis.forward[0] + a.dir[1] * basis.forward[1] + a.dir[2] * basis.forward[2];
          const dot = dotsRef.current[i];
          const label = labelsRef.current[i];
          const front = sz < 0;
          if (dot) {
            dot.style.transform = `translate(${sx * radius}px, ${sy * radius}px)`;
            dot.style.opacity = front ? "1" : "0.35";
          }
          if (label) {
            label.style.transform = `translate(${sx * radius}px, ${sy * radius}px)`;
            label.style.opacity = front ? "1" : "0.35";
          }
        });
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [rendererRef]);

  return (
    <div className="pointer-events-none absolute bottom-4 left-4 h-16 w-16 rounded-full border border-edge bg-panel/70">
      <div className="absolute left-1/2 top-1/2 h-px w-3 -translate-x-1/2 -translate-y-1/2 bg-ink-dim/40" />
      {GIZMO_AXES.map((a, i) => (
        <div key={a.label}>
          <div
            ref={(el) => {
              dotsRef.current[i] = el;
            }}
            className="absolute left-1/2 top-1/2 h-2 w-2 -translate-x-1/2 -translate-y-1/2 rounded-full"
            style={{ backgroundColor: a.color }}
          />
          <div
            ref={(el) => {
              labelsRef.current[i] = el;
            }}
            className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 text-[9px] font-semibold"
            style={{ color: a.color, marginTop: "-11px" }}
          >
            {a.label}
          </div>
        </div>
      ))}
    </div>
  );
}

export default function Viewport() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<SplatRenderer | null>(null);
  const loadedPathRef = useRef<string | null>(null);
  const firstLoadRef = useRef(true);
  const drawnCamerasRef = useRef(0);

  const latestSplatPath = useStore((s) => s.latestSplatPath);
  const setSplatCount = useStore((s) => s.setSplatCount);
  const setFps = useStore((s) => s.setFps);
  const cameras = useStore((s) => s.cameras);
  const resultPath = useStore((s) => s.resultPath);
  const workspace = useStore((s) => s.workspace);
  const [orientMessage, setOrientMessage] = useState<string | null>(null);
  const [orientOpen, setOrientOpen] = useState(false);

  useEffect(() => {
    if (!canvasRef.current) return;
    const r = new SplatRenderer(canvasRef.current);
    r.attachControls();
    r.autoOrbit = true;
    r.onStats = (n) => setSplatCount(n);
    r.onFps = (n) => setFps(n);
    rendererRef.current = r;
    return () => {
      r.dispose();
      rendererRef.current = null;
    };
  }, [setSplatCount, setFps]);

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

  const hasModel = latestSplatPath !== null;

  return (
    <div className="relative h-full w-full">
      <canvas
        ref={canvasRef}
        className="h-full w-full cursor-grab active:cursor-grabbing"
        style={{ touchAction: "none" }}
      />

      <AxisGizmo rendererRef={rendererRef} />

      {hasModel && (
        <div className="absolute right-4 top-4 flex w-44 flex-col gap-2 rounded border border-edge bg-panel/85 p-3 text-xs">
          <button
            onClick={() => setOrientOpen((v) => !v)}
            className="flex items-center justify-between text-ink-dim hover:text-ink"
          >
            <span>Model orientation</span>
            <span>{orientOpen ? "-" : "+"}</span>
          </button>
          {orientOpen && (
            <>
              <div className="grid grid-cols-3 gap-1">
                {(
                  [
                    ["X", [1, 0, 0]],
                    ["Y", [0, 1, 0]],
                    ["Z", [0, 0, 1]],
                  ] as [string, [number, number, number]][]
                ).map(([label, axis]) => (
                  <div key={label} className="flex flex-col gap-1">
                    <button onClick={() => turn(axis, 90)} className="btn justify-center py-1" title={`Turn 90 degrees about ${label}`}>
                      {label}+
                    </button>
                    <button
                      onClick={() => turn(axis, -90)}
                      className="btn justify-center py-1"
                      title={`Turn 90 degrees about ${label}, the other way`}
                    >
                      {label}-
                    </button>
                  </div>
                ))}
              </div>
              <button onClick={snap} className="btn justify-center py-1">
                Snap up to axis
              </button>
              <button onClick={() => void alignToGround()} className="btn justify-center py-1">
                Align to ground
              </button>
              <button onClick={reset} className="btn justify-center py-1 text-ink-dim">
                Reset
              </button>
              {orientMessage && <div className="text-ink-dim">{orientMessage}</div>}
            </>
          )}
        </div>
      )}
    </div>
  );
}
