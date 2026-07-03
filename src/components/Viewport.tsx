import { useEffect, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { SplatRenderer } from "../splat/renderer";
import { useStore } from "../state/store";

export default function Viewport() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<SplatRenderer | null>(null);
  const loadedPathRef = useRef<string | null>(null);
  const firstLoadRef = useRef(true);
  const latestSplatPath = useStore((s) => s.latestSplatPath);
  const setSplatCount = useStore((s) => s.setSplatCount);

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

  return (
    <canvas
      ref={canvasRef}
      className="h-full w-full cursor-grab active:cursor-grabbing"
      style={{ touchAction: "none" }}
    />
  );
}
