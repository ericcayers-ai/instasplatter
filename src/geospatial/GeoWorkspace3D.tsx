import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useStore } from "../state/store";
import { api } from "../lib/ipc";
import { SplatRenderer } from "../splat/renderer";
import { axisAngle, mat3Mul } from "../splat/camera";
import { FloodPreviewEngine } from "./preview";
import { aoiIsValid, domainFromAoi } from "./aoi";
import { buildSatelliteCanvas } from "./imageryTiles";
import { GeoEnuScene } from "./workspace3d/GeoEnuScene";
import { PreviewBadge } from "./PreviewBadge";
import {
  axisAngleRotation,
  cloneModelTransform,
  identityModelTransform,
  mat3ToRotation,
  mulRot,
  normalizeModelTransform,
  rotationToMat3,
  type GizmoMode,
  type ModelTransform,
} from "./modelTransform";
import type { Vec3 } from "./enu";

function applyTfToSplat(splat: SplatRenderer, tf: ModelTransform) {
  splat.setModelRotation(rotationToMat3(tf.rotation));
  splat.setModelTranslation(tf.translation);
  splat.setModelScale(tf.scale);
}

/**
 * Primary Geospatial 3D workspace: ENU terrain + depth water + georegistered splat gizmos.
 * Terrain, water, and splats share one WebGL2 context and depth buffer so water can
 * occlude underwater Gaussians (approximate: Gaussian billboards still expand in screen space).
 */
export default function GeoWorkspace3D() {
  const sceneCanvasRef = useRef<HTMLCanvasElement>(null);
  const sceneRef = useRef<GeoEnuScene | null>(null);
  const splatRef = useRef<SplatRenderer | null>(null);
  const engineRef = useRef<FloodPreviewEngine | null>(null);
  const transformRef = useRef<ModelTransform>(identityModelTransform());
  const dragRef = useRef<{ kind: "orbit" | "pan" | "gizmo"; x: number; y: number } | null>(null);
  const loadedSplat = useRef<string | null>(null);
  const persistTimer = useRef<number | null>(null);

  const aoi = useStore((s) => s.geoAoiWgs84);
  const aoiRevision = useStore((s) => s.geoAoiRevision);
  const floodTime = useStore((s) => s.geoFloodTime);
  const floodPlaying = useStore((s) => s.geoFloodPlaying);
  const lowPower = useStore((s) => s.geoFloodLowPower);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const preview = useStore((s) => s.geoPreview);
  const scientific = useStore((s) => s.geoScientificRun);
  const workspace = useStore((s) => s.workspace);
  const latestSplatPath = useStore((s) => s.latestSplatPath);
  const resultPath = useStore((s) => s.resultPath);
  const layers = useStore((s) => s.geoLayers);
  const setGeoPreview = useStore((s) => s.setGeoPreview);
  const setFloodTime = useStore((s) => s.setGeoFloodTime);
  const setFloodPlaying = useStore((s) => s.setGeoFloodPlaying);
  const setInspectHint = useStore((s) => s.setGeoInspectHint);
  const setTool = useStore((s) => s.setGeoTool);
  const setViewMode = useStore((s) => s.setGeoViewMode);
  const tool = useStore((s) => s.geoTool);
  const storeTransform = useStore((s) => s.geoModelTransform);
  const setStoreTransform = useStore((s) => s.setGeoModelTransform);
  const persistTransform = useStore((s) => s.persistGeoModelTransform);

  const [gizmoMode, setGizmoMode] = useState<GizmoMode>("translate");
  const [attribution, setAttribution] = useState("Esri World Imagery");
  const [splatStatus, setSplatStatus] = useState("No splat loaded");
  const reducedMotion =
    typeof window !== "undefined" &&
    !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

  const splatVisible = layers.find((l) => l.id === "splat")?.visible !== false;
  const floodVisible =
    !!layers.find((l) => l.id === "flood_depth")?.visible ||
    !!layers.find((l) => l.id === "flood_hazard")?.visible;

  useEffect(() => {
    const sceneEl = sceneCanvasRef.current;
    if (!sceneEl) return;

    const scene = new GeoEnuScene(sceneEl);
    scene.setReducedMotion(reducedMotion);
    scene.setLowPower(useStore.getState().geoFloodLowPower);
    sceneRef.current = scene;

    let splat: SplatRenderer | null = null;
    try {
      splat = new SplatRenderer(sceneEl, {
        gl: scene.context,
        manageLoop: false,
        clearOnFrame: false,
        depthTest: true,
      });
      // Share the ENU camera object so orbit/pan/zoom stay locked.
      splat.camera = scene.camera;
      splat.showFrustums = false;
      splat.showCameraPath = false;
      splat.showSparse = false;
      splat.showDense = false;
      splat.showMesh = false;
      splat.showSplat = useStore.getState().geoLayers.find((l) => l.id === "splat")?.visible !== false;
      splatRef.current = splat;
      scene.attachSplat(splat);
    } catch {
      setSplatStatus("WebGL2 splat overlay unavailable");
    }

    const engine = new FloodPreviewEngine({
      lowPower: useStore.getState().geoFloodLowPower,
      reducedMotion,
      waterStyle: useStore.getState().geoWaterStyle,
      domain: (() => {
        const box = useStore.getState().geoAoiWgs84;
        return aoiIsValid(box)
          ? domainFromAoi(box, useStore.getState().geoFloodLowPower)
          : undefined;
      })(),
      durationHours: useStore.getState().geoScenario?.durationHours,
    });
    engineRef.current = engine;

    const tf = normalizeModelTransform(useStore.getState().geoModelTransform);
    transformRef.current = tf;
    scene.setModelTransform(tf, [0, 0, 0]);
    if (splat) applyTfToSplat(splat, tf);

    return () => {
      scene.attachSplat(null);
      scene.dispose();
      splat?.dispose();
      engine.destroy();
      splatRef.current = null;
      sceneRef.current = null;
      engineRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const splat = splatRef.current;
    if (splat) splat.showSplat = splatVisible;
  }, [splatVisible]);

  useEffect(() => {
    const scene = sceneRef.current;
    const engine = engineRef.current;
    if (!scene || !engine) return;
    scene.setAoi(aoi);
    scene.setLowPower(lowPower);

    if (aoiIsValid(aoi)) {
      engine.rebindDomain(
        domainFromAoi(aoi, lowPower),
        useStore.getState().geoScenario?.durationHours,
      );
    } else {
      engine.clearBoundDomain();
      scene.setWaterGrid(null);
      setGeoPreview(null);
    }

    const ac = new AbortController();
    void buildSatelliteCanvas(aoi, { lowPower, signal: ac.signal }).then((pack) => {
      if (ac.signal.aborted) return;
      scene.setTerrainImage(pack.canvas);
      setAttribution(pack.attribution);
    });
    return () => ac.abort();
  }, [aoi, aoiRevision, lowPower, setGeoPreview]);

  useEffect(() => {
    if (!workspace) return;
    let stale = false;
    (async () => {
      try {
        const ref = await api.getGeoReference(workspace);
        if (!stale && ref?.localOrigin) {
          setInspectHint(
            `ENU origin ${ref.localOrigin[0].toFixed(5)}°, ${ref.localOrigin[1].toFixed(5)}° · ${ref.scaleStatus ?? "unknown"} scale`,
          );
        }
      } catch {
        /* optional */
      }
      try {
        const tf = await api.getModelTransform(workspace);
        if (stale || !tf) return;
        const norm = normalizeModelTransform(tf);
        transformRef.current = norm;
        setStoreTransform(norm);
        sceneRef.current?.setModelTransform(norm);
        if (splatRef.current) applyTfToSplat(splatRef.current, norm);
      } catch {
        /* older projects */
      }
    })();
    return () => {
      stale = true;
    };
  }, [workspace, setInspectHint, setStoreTransform]);

  useEffect(() => {
    const norm = normalizeModelTransform(storeTransform);
    transformRef.current = norm;
    sceneRef.current?.setModelTransform(norm);
    if (splatRef.current) applyTfToSplat(splatRef.current, norm);
  }, [storeTransform]);

  useEffect(() => {
    const splat = splatRef.current;
    const path = resultPath ?? latestSplatPath;
    if (!splat || !path || loadedSplat.current === path) return;
    loadedSplat.current = path;
    let stale = false;
    (async () => {
      try {
        const resp = await fetch(convertFileSrc(path));
        const buf = await resp.arrayBuffer();
        if (stale) return;
        splat.loadPly(buf, true, false);
        setSplatStatus(`Splat · ${path.split(/[/\\]/).pop()}`);
        useStore.setState((s) => ({
          geoLayers: s.geoLayers.map((l) =>
            l.id === "splat"
              ? { ...l, visible: true, placeholder: false, status: "ready" as const }
              : l,
          ),
        }));
      } catch (err) {
        setSplatStatus(`Splat load failed: ${String(err)}`);
      }
    })();
    return () => {
      stale = true;
    };
  }, [resultPath, latestSplatPath]);

  useEffect(() => {
    const engine = engineRef.current;
    const scene = sceneRef.current;
    if (!engine || !scene) return;
    engine.setWaterStyle(waterStyle);
    engine.setLowPower(lowPower);
    scene.setLowPower(lowPower);
    scene.setReducedMotion(reducedMotion);

    if (!engine.hasBoundDomain() || !floodVisible) {
      scene.setWaterGrid(null);
      return;
    }

    void engine.whenReady().then(() => {
      if (!engineRef.current?.hasBoundDomain()) return;
      const art = engine.seek(floodTime);
      const { frame } = art;
      setGeoPreview({
        backend: frame.backend,
        validation: frame.validation,
        maxDepthM: frame.stats.maxDepthM,
        wetFraction: frame.stats.wetFraction,
        massM3: frame.stats.massM3,
        maxSpeedMs: frame.stats.maxSpeedMs,
        hazardClass: frame.stats.hazardClass,
        cfl: frame.stats.cfl,
      });
      scene.setWaterGrid({
        h: frame.h,
        u: frame.u,
        v: frame.v,
        zBed: engine.getBedZ(),
        cols: frame.cols,
        rows: frame.rows,
        style: waterStyle,
        maxDepthRef: Math.max(0.5, frame.stats.maxDepthM, 2),
        surfaceBiasM: 0.25,
      });
    });
  }, [floodTime, waterStyle, lowPower, floodVisible, reducedMotion, setGeoPreview, aoiRevision]);

  useEffect(() => {
    if (!floodPlaying) return;
    if (reducedMotion) {
      setInspectHint("Reduced motion — scrub the timeline instead of playing");
      setFloodPlaying(false);
      return;
    }
    const engine = engineRef.current;
    if (!engine?.hasBoundDomain()) {
      setFloodPlaying(false);
      return;
    }
    let raf = 0;
    let last = performance.now();
    const rate = lowPower ? 800 : 1200;
    const tick = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const art = engine.advancePlay(dt * rate);
      const t01 = art.frame.stats.timeS / Math.max(1, engine.durationS);
      setFloodTime(t01);
      if (t01 >= 1) {
        setFloodPlaying(false);
        return;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [floodPlaying, lowPower, reducedMotion, setFloodPlaying, setFloodTime, setInspectHint]);

  useEffect(() => {
    if (tool === "drawAoi") {
      setInspectHint("Switching to 2D satellite to draw AOI");
      setViewMode("2d");
    }
  }, [tool, setViewMode, setInspectHint]);

  const schedulePersist = (tf: ModelTransform) => {
    setStoreTransform(cloneModelTransform(tf));
    if (persistTimer.current) window.clearTimeout(persistTimer.current);
    persistTimer.current = window.setTimeout(() => {
      void persistTransform();
    }, 400);
  };

  const onPointerDown = (e: React.PointerEvent) => {
    const el = sceneCanvasRef.current;
    if (!el) return;
    el.setPointerCapture(e.pointerId);
    if (e.button === 1 || e.button === 2 || (e.button === 0 && e.shiftKey)) {
      dragRef.current = { kind: "pan", x: e.clientX, y: e.clientY };
    } else if (e.button === 0 && e.altKey) {
      dragRef.current = { kind: "gizmo", x: e.clientX, y: e.clientY };
    } else {
      dragRef.current = { kind: "orbit", x: e.clientX, y: e.clientY };
    }
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const drag = dragRef.current;
    const scene = sceneRef.current;
    if (!drag || !scene) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    drag.x = e.clientX;
    drag.y = e.clientY;
    const h = sceneCanvasRef.current?.clientHeight ?? 1;

    if (drag.kind === "orbit") {
      scene.orbit(dx, dy);
      return;
    }
    if (drag.kind === "pan") {
      scene.pan(dx, dy, h);
      return;
    }

    const tf = cloneModelTransform(transformRef.current);
    const axis: Vec3 = e.ctrlKey && e.shiftKey ? [0, 0, 1] : e.ctrlKey ? [0, 1, 0] : [1, 0, 0];
    const sens = scene.camera.distance * 0.0015;
    if (gizmoMode === "translate") {
      tf.translation[0] += axis[0] * dx * sens;
      tf.translation[1] += axis[1] * -dy * sens;
      tf.translation[2] += axis[2] * -dy * sens;
    } else if (gizmoMode === "rotate") {
      tf.rotation = mulRot(axisAngleRotation(axis, (dx - dy) * 0.005), tf.rotation);
    } else {
      const factor = 1 + (dx - dy) * 0.004;
      if (e.shiftKey) {
        tf.scale = [
          Math.max(1e-3, tf.scale[0] * factor),
          Math.max(1e-3, tf.scale[1] * factor),
          Math.max(1e-3, tf.scale[2] * factor),
        ];
      } else {
        const i = axis[0] ? 0 : axis[1] ? 1 : 2;
        tf.scale[i] = Math.max(1e-3, tf.scale[i] * factor);
      }
    }
    transformRef.current = tf;
    scene.setModelTransform(tf);
    if (splatRef.current) applyTfToSplat(splatRef.current, tf);
    schedulePersist(tf);
  };

  const onPointerUp = (e: React.PointerEvent) => {
    dragRef.current = null;
    try {
      sceneCanvasRef.current?.releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
  };

  const onWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const scene = sceneRef.current;
    const el = sceneCanvasRef.current;
    if (!scene || !el) return;
    const rect = el.getBoundingClientRect();
    scene.zoom(
      e.deltaY > 0 ? 1.08 : 1 / 1.08,
      e.clientX - rect.left,
      e.clientY - rect.top,
      rect.width,
      rect.height,
    );
  };

  const nudge = (axis: Vec3, amount: number) => {
    const tf = cloneModelTransform(transformRef.current);
    if (gizmoMode === "translate") {
      tf.translation[0] += axis[0] * amount;
      tf.translation[1] += axis[1] * amount;
      tf.translation[2] += axis[2] * amount;
    } else if (gizmoMode === "rotate") {
      tf.rotation = mulRot(axisAngleRotation(axis, amount * 0.05), tf.rotation);
    } else {
      const i = axis[0] ? 0 : axis[1] ? 1 : 2;
      tf.scale[i] = Math.max(1e-3, tf.scale[i] * (1 + amount * 0.05));
    }
    transformRef.current = tf;
    sceneRef.current?.setModelTransform(tf);
    if (splatRef.current) applyTfToSplat(splatRef.current, tf);
    schedulePersist(tf);
  };

  const resetTransform = () => {
    const tf = identityModelTransform();
    transformRef.current = tf;
    sceneRef.current?.setModelTransform(tf);
    splatRef.current?.resetModelRotation();
    schedulePersist(tf);
  };

  const rotateSplat90 = (axis: Vec3) => {
    const splat = splatRef.current;
    if (!splat) return;
    splat.rotateModel(axis, Math.PI / 2);
    const tf = cloneModelTransform(transformRef.current);
    tf.rotation = mat3ToRotation(
      mat3Mul(axisAngle(axis, Math.PI / 2), rotationToMat3(tf.rotation)),
    );
    transformRef.current = tf;
    sceneRef.current?.setModelTransform(tf);
    applyTfToSplat(splat, tf);
    schedulePersist(tf);
  };

  const authority =
    scientific?.mode === "anuga"
      ? "Scientific"
      : scientific?.mode === "demo"
        ? "Demo"
        : "Live preview";

  return (
    <div className="geo-workspace-3d relative h-full w-full min-h-0" data-water-style={waterStyle}>
      <canvas
        ref={sceneCanvasRef}
        className="absolute inset-0 z-0 h-full w-full touch-none"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onWheel={onWheel}
        onContextMenu={(e) => e.preventDefault()}
        aria-label="Geospatial 3D ENU workspace"
      />

      <div className="pointer-events-none absolute right-3 top-12 z-10 flex flex-col items-end gap-1.5">
        <div className="pointer-events-auto">
          <PreviewBadge validation={preview?.validation ?? "live"} backend={preview?.backend} />
        </div>
        <span className="rounded border border-edge bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-ink-dim backdrop-blur-sm">
          {authority}
        </span>
        {!aoiIsValid(aoi) && (
          <button
            type="button"
            className="pointer-events-auto rounded border border-[var(--color-gauge)]/40 bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--color-gauge)] backdrop-blur-sm"
            onClick={() => {
              setTool("drawAoi");
              setViewMode("2d");
            }}
          >
            Draw AOI in 2D
          </button>
        )}
      </div>

      <div className="pointer-events-none absolute bottom-3 left-3 z-10 flex max-w-md flex-col gap-2">
        <div className="pointer-events-auto flex flex-wrap items-center gap-1 rounded border border-edge bg-panel/90 p-1 shadow-sm backdrop-blur-sm">
          {(
            [
              { id: "translate" as const, label: "Move" },
              { id: "rotate" as const, label: "Rotate" },
              { id: "scale" as const, label: "Scale" },
            ] as const
          ).map((m) => (
            <button
              key={m.id}
              type="button"
              className={`px-2 py-1 text-[11px] ${gizmoMode === m.id ? "btn-active rounded" : "text-ink-dim"}`}
              aria-pressed={gizmoMode === m.id}
              onClick={() => setGizmoMode(m.id)}
            >
              {m.label}
            </button>
          ))}
          <span className="mx-1 h-4 w-px bg-edge" />
          <button type="button" className="btn px-2 py-0.5 text-[10px]" onClick={() => nudge([1, 0, 0], 1)} title="Nudge +X (east)">
            +X
          </button>
          <button type="button" className="btn px-2 py-0.5 text-[10px]" onClick={() => nudge([0, 1, 0], 1)} title="Nudge +Y (north)">
            +Y
          </button>
          <button type="button" className="btn px-2 py-0.5 text-[10px]" onClick={() => nudge([0, 0, 1], 1)} title="Nudge +Z (up)">
            +Z
          </button>
          <button
            type="button"
            className="btn px-2 py-0.5 text-[10px]"
            onClick={() => rotateSplat90([0, 0, 1])}
            title="Rotate splat 90° about up"
          >
            90°↑
          </button>
          <button type="button" className="btn px-2 py-0.5 text-[10px]" onClick={resetTransform} title="Reset model transform">
            Reset
          </button>
        </div>
        <div className="rounded border border-edge bg-panel/85 px-2 py-1 text-[10px] text-ink-dim backdrop-blur-sm">
          <div>{splatStatus}</div>
          <div className="mt-0.5 opacity-80">
            Drag orbit · Shift-drag pan · Alt-drag gizmo · Wheel zoom ·{" "}
            {gizmoMode === "scale" ? "Shift=uniform scale" : "Ctrl=Y · Ctrl+Shift=Z"}
          </div>
          <div className="mt-0.5 opacity-70">
            Shared WebGL depth: water occludes underwater Gaussians (billboard approx).
          </div>
          <div className="mt-0.5 opacity-70">{attribution}</div>
        </div>
      </div>
    </div>
  );
}
