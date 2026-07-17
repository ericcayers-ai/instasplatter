/**
 * Thin CesiumJS Globe wrapper for the geospatial suite.
 *
 * Standard: blank ion, Esri/OSM XYZ imagery, ellipsoid or local DEM terrain.
 * Experimental: ion World Terrain only when a user token is stored.
 * Never MapLibre→canvas-drape→Cesium.
 */

import { useEffect, useRef, useState } from "react";
import { Color, Entity, ImageryLayer, Rectangle, Viewer } from "cesium";
import { useStore } from "../../state/store";
import { aoiIsValid, domainFromAoi, type AoiWgs84 } from "../aoi";
import { FloodPreviewEngine } from "../preview";
import { PreviewBadge } from "../PreviewBadge";
import { captureGlobeLookAt, flyGlobeToContext } from "./cameraSync";
import { applyFloodOverlay, clearFloodOverlay } from "./floodOverlay";
import { createGlobeImageryProvider, globeImageryAttribution } from "./imagery";
import { applyCesiumIonPolicy, blankCesiumIon } from "./ion";
import {
  getLocalDemTerrainUrl,
  resolveGlobeTerrain,
  subscribeLocalDemTerrainUrl,
} from "./terrain";

const UI_THROTTLE_MS = 100;
const FLOOD_OVERLAY_MS = 180;
const AOI_ENTITY_ID = "instasplatter-aoi";

function ensureAoiEntity(viewer: Viewer, aoi: AoiWgs84 | null | undefined) {
  const existing = viewer.entities.getById(AOI_ENTITY_ID);
  if (existing) viewer.entities.remove(existing);
  if (!aoiIsValid(aoi)) return;
  viewer.entities.add(
    new Entity({
      id: AOI_ENTITY_ID,
      rectangle: {
        coordinates: Rectangle.fromDegrees(aoi[0], aoi[1], aoi[2], aoi[3]),
        material: Color.fromCssColorString("#25C6D9").withAlpha(0.08),
        outline: true,
        outlineColor: Color.fromCssColorString("#F1B84B"),
        outlineWidth: 2,
        height: 0,
      },
    }),
  );
}

export default function CesiumGlobe() {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewerRef = useRef<Viewer | null>(null);
  const engineRef = useRef<FloodPreviewEngine | null>(null);
  const floodCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const baseLayerRef = useRef<ImageryLayer | null>(null);
  const readyRef = useRef(false);
  const lastFloodPush = useRef(0);
  const floodTimer = useRef<number | null>(null);
  const playingLocal = useRef(false);

  const aoi = useStore((s) => s.geoAoiWgs84);
  const aoiRevision = useStore((s) => s.geoAoiRevision);
  const demRevision = useStore((s) => s.geoDemRevision);
  const demSample = useStore((s) => s.geoDemSample);
  const demProduct = useStore((s) => s.geoDemProduct);
  const floodTime = useStore((s) => s.geoFloodTime);
  const floodPlaying = useStore((s) => s.geoFloodPlaying);
  const lowPower = useStore((s) => s.geoFloodLowPower);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const basemapMode = useStore((s) => s.geoBasemapMode);
  const layers = useStore((s) => s.geoLayers);
  const preview = useStore((s) => s.geoPreview);
  const scientific = useStore((s) => s.geoScientificRun);
  const experimental = useStore((s) => !!s.resolved?.experimentalMode);
  const setGeoPreview = useStore((s) => s.setGeoPreview);
  const setFloodTime = useStore((s) => s.setGeoFloodTime);
  const setFloodPlaying = useStore((s) => s.setGeoFloodPlaying);
  const setTool = useStore((s) => s.setGeoTool);
  const setInspectHint = useStore((s) => s.setGeoInspectHint);

  const [attribution, setAttribution] = useState(globeImageryAttribution("satellite"));
  const [terrainLabel, setTerrainLabel] = useState("Ellipsoid");
  const [globeError, setGlobeError] = useState<string | null>(null);

  const floodVisible =
    !!layers.find((l) => l.id === "flood_depth")?.visible ||
    !!layers.find((l) => l.id === "flood_hazard")?.visible;
  const floodOpacity = layers.find((l) => l.id === "flood_depth")?.opacity ?? 0.72;

  const authority =
    scientific?.mode === "anuga" && !demProduct?.synthetic
      ? "Scientific"
      : scientific?.mode === "demo" || demProduct?.synthetic
        ? "Demo"
        : "Live preview";

  // Create Viewer once.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    blankCesiumIon();
    applyCesiumIonPolicy(!!useStore.getState().resolved?.experimentalMode);

    const creditHost = document.createElement("div");
    creditHost.className = "geo-cesium-credits";
    creditHost.style.cssText =
      "position:absolute;left:8px;bottom:8px;z-index:2;max-width:42%;font-size:10px;opacity:0.85;pointer-events:auto;";
    el.appendChild(creditHost);

    let viewer: Viewer;
    try {
      viewer = new Viewer(el, {
        animation: false,
        timeline: false,
        fullscreenButton: false,
        geocoder: false,
        homeButton: false,
        infoBox: false,
        baseLayerPicker: false,
        navigationHelpButton: false,
        selectionIndicator: false,
        sceneModePicker: false,
        baseLayer: false,
        requestRenderMode: true,
        maximumRenderTimeChange: Number.POSITIVE_INFINITY,
        creditContainer: creditHost,
      });
    } catch (err) {
      setGlobeError(err instanceof Error ? err.message : String(err));
      return;
    }

    viewer.scene.globe.baseColor = Color.fromCssColorString("#0a1620");
    viewer.scene.backgroundColor = Color.fromCssColorString("#0a0c10");
    viewer.scene.fog.enabled = false;

    const imagery = createGlobeImageryProvider(useStore.getState().geoBasemapMode);
    baseLayerRef.current = viewer.imageryLayers.addImageryProvider(imagery);
    setAttribution(globeImageryAttribution(useStore.getState().geoBasemapMode));

    const floodCanvas = document.createElement("canvas");
    floodCanvasRef.current = floodCanvas;

    const reducedMotion = !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    const low = useStore.getState().geoFloodLowPower;
    const initialAoi = useStore.getState().geoAoiWgs84;
    const engine = new FloodPreviewEngine({
      lowPower: low,
      reducedMotion,
      waterStyle: useStore.getState().geoWaterStyle,
      domain: aoiIsValid(initialAoi) ? domainFromAoi(initialAoi, low) : undefined,
      durationHours: useStore.getState().geoScenario?.durationHours,
    });
    engineRef.current = engine;

    viewerRef.current = viewer;
    readyRef.current = true;
    flyGlobeToContext(viewer, initialAoi, { duration: 0 });
    ensureAoiEntity(viewer, initialAoi);

    void resolveGlobeTerrain({
      experimental: !!useStore.getState().resolved?.experimentalMode,
      terrainUrlOverride: getLocalDemTerrainUrl(),
    }).then((t) => {
      if (viewer.isDestroyed()) return;
      viewer.terrainProvider = t.provider;
      setTerrainLabel(t.detail);
      viewer.scene.requestRender();
    });

    const unsubTerrain = subscribeLocalDemTerrainUrl(() => {
      void resolveGlobeTerrain({
        experimental: !!useStore.getState().resolved?.experimentalMode,
        terrainUrlOverride: getLocalDemTerrainUrl(),
      }).then((t) => {
        if (viewer.isDestroyed()) return;
        viewer.terrainProvider = t.provider;
        setTerrainLabel(t.detail);
        viewer.scene.requestRender();
      });
    });

    void engine.whenReady().then(() => {
      if (!engine.hasBoundDomain() || !floodCanvasRef.current) return;
      const art = engine.seek(useStore.getState().geoFloodTime);
      void applyFloodOverlay(
        viewer,
        floodCanvasRef.current,
        art,
        !!useStore.getState().geoLayers.find((l) => l.id === "flood_depth")?.visible ||
          !!useStore.getState().geoLayers.find((l) => l.id === "flood_hazard")?.visible,
        useStore.getState().geoLayers.find((l) => l.id === "flood_depth")?.opacity ?? 0.72,
      );
      setGeoPreview({
        backend: art.frame.backend,
        validation: art.frame.validation,
        maxDepthM: art.frame.stats.maxDepthM,
        wetFraction: art.frame.stats.wetFraction,
        massM3: art.frame.stats.massM3,
        maxSpeedMs: art.frame.stats.maxSpeedMs,
        hazardClass: art.frame.stats.hazardClass,
        cfl: art.frame.stats.cfl,
      });
    });

    return () => {
      unsubTerrain();
      readyRef.current = false;
      if (floodTimer.current != null) window.clearTimeout(floodTimer.current);
      captureGlobeLookAt(viewer);
      engine.destroy();
      engineRef.current = null;
      floodCanvasRef.current = null;
      baseLayerRef.current = null;
      if (!viewer.isDestroyed()) {
        clearFloodOverlay(viewer);
        viewer.destroy();
      }
      viewerRef.current = null;
      blankCesiumIon();
      setGeoPreview(null);
    };
    // Mount once for Viewer lifetime.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [setGeoPreview]);

  // Experimental / ion / terrain policy when Experimental toggles.
  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || !readyRef.current) return;
    applyCesiumIonPolicy(experimental);
    void resolveGlobeTerrain({
      experimental,
      terrainUrlOverride: getLocalDemTerrainUrl(),
    }).then((t) => {
      if (viewer.isDestroyed()) return;
      viewer.terrainProvider = t.provider;
      setTerrainLabel(t.detail);
      viewer.scene.requestRender();
    });
  }, [experimental]);

  // Basemap swap.
  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || !readyRef.current) return;
    if (baseLayerRef.current) {
      viewer.imageryLayers.remove(baseLayerRef.current, true);
      baseLayerRef.current = null;
    }
    const provider = createGlobeImageryProvider(basemapMode);
    baseLayerRef.current = viewer.imageryLayers.addImageryProvider(provider, 0);
    setAttribution(globeImageryAttribution(basemapMode));
    viewer.scene.requestRender();
  }, [basemapMode]);

  // AOI → camera + entity + soft domain.
  useEffect(() => {
    const viewer = viewerRef.current;
    const engine = engineRef.current;
    if (!viewer || !readyRef.current || !engine) return;

    ensureAoiEntity(viewer, aoi);
    flyGlobeToContext(viewer, aoi);

    if (aoiIsValid(aoi)) {
      const domain = domainFromAoi(aoi, lowPower);
      const plan = useStore.getState().geoExtentPlan;
      if (plan?.previewCellM && plan.previewCellM > 0) {
        domain.dxM = Math.max(4, plan.previewCellM);
        domain.cols = Math.max(
          24,
          Math.round(
            ((aoi[2] - aoi[0]) *
              111320 *
              Math.cos((((aoi[1] + aoi[3]) / 2) * Math.PI) / 180)) /
              domain.dxM,
          ),
        );
        domain.rows = Math.max(16, Math.round(((aoi[3] - aoi[1]) * 111320) / domain.dxM));
      }
      engine.rebindDomain(domain, useStore.getState().geoScenario?.durationHours);
      const art = engine.seek(useStore.getState().geoFloodTime);
      if (floodCanvasRef.current) {
        void applyFloodOverlay(viewer, floodCanvasRef.current, art, floodVisible, floodOpacity);
      }
      setGeoPreview({
        backend: art.frame.backend,
        validation: art.frame.validation,
        maxDepthM: art.frame.stats.maxDepthM,
        wetFraction: art.frame.stats.wetFraction,
        massM3: art.frame.stats.massM3,
        maxSpeedMs: art.frame.stats.maxSpeedMs,
        hazardClass: art.frame.stats.hazardClass,
        cfl: art.frame.stats.cfl,
      });
    } else {
      engine.clearBoundDomain();
      clearFloodOverlay(viewer);
      setGeoPreview(null);
    }
  }, [aoi, aoiRevision, lowPower, floodVisible, floodOpacity, setGeoPreview]);

  // DEM samples → soft/HAND bed on Globe (GeoTIFF path; terrain tiles optional).
  useEffect(() => {
    const engine = engineRef.current;
    const viewer = viewerRef.current;
    if (!engine || !demSample) return;
    engine.setDemBed({
      z: demSample.z,
      cols: demSample.cols,
      rows: demSample.rows,
      bedSource: (demSample.bedSource as "real" | "synthetic" | "proxy") || "synthetic",
    });
    if (engine.hasBoundDomain() && viewer && floodCanvasRef.current) {
      const art = engine.seek(useStore.getState().geoFloodTime);
      void applyFloodOverlay(viewer, floodCanvasRef.current, art, floodVisible, floodOpacity);
      setGeoPreview({
        backend: art.frame.backend,
        validation: art.frame.validation,
        maxDepthM: art.frame.stats.maxDepthM,
        wetFraction: art.frame.stats.wetFraction,
        massM3: art.frame.stats.massM3,
        maxSpeedMs: art.frame.stats.maxSpeedMs,
        hazardClass: art.frame.stats.hazardClass,
        cfl: art.frame.stats.cfl,
      });
    }
  }, [demSample, demRevision, floodVisible, floodOpacity, setGeoPreview]);

  // Scrub / style → flood overlay (throttled).
  useEffect(() => {
    const viewer = viewerRef.current;
    const engine = engineRef.current;
    if (!viewer || !readyRef.current || !engine || playingLocal.current) return;

    engine.setWaterStyle(waterStyle);
    engine.setLowPower(lowPower);

    const push = () => {
      if (!engine.hasBoundDomain() || !floodCanvasRef.current) {
        clearFloodOverlay(viewer);
        return;
      }
      const art = engine.seek(floodTime);
      void applyFloodOverlay(viewer, floodCanvasRef.current, art, floodVisible, floodOpacity);
      const now = performance.now();
      if (now - lastFloodPush.current >= UI_THROTTLE_MS) {
        lastFloodPush.current = now;
        setGeoPreview({
          backend: art.frame.backend,
          validation: art.frame.validation,
          maxDepthM: art.frame.stats.maxDepthM,
          wetFraction: art.frame.stats.wetFraction,
          massM3: art.frame.stats.massM3,
          maxSpeedMs: art.frame.stats.maxSpeedMs,
          hazardClass: art.frame.stats.hazardClass,
          cfl: art.frame.stats.cfl,
        });
      }
    };

    if (floodTimer.current != null) window.clearTimeout(floodTimer.current);
    floodTimer.current = window.setTimeout(push, FLOOD_OVERLAY_MS);
    return () => {
      if (floodTimer.current != null) window.clearTimeout(floodTimer.current);
    };
  }, [floodTime, waterStyle, lowPower, floodVisible, floodOpacity, setGeoPreview]);

  // Play loop.
  useEffect(() => {
    if (!floodPlaying) {
      playingLocal.current = false;
      return;
    }
    const reduced = !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (reduced) {
      setFloodPlaying(false);
      return;
    }
    const viewer = viewerRef.current;
    const engine = engineRef.current;
    if (!viewer || !engine?.hasBoundDomain()) {
      setFloodPlaying(false);
      setInspectHint("Draw an AOI before playing the flood preview");
      return;
    }

    playingLocal.current = true;
    let raf = 0;
    let last = performance.now();
    let lastStore = 0;
    let lastOverlay = 0;
    let playhead = useStore.getState().geoFloodTime;
    const rate = lowPower ? 800 : 1200;
    const durationS = engine.durationS || (useStore.getState().geoScenario?.durationHours ?? 12) * 3600;

    const tick = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      playhead = Math.min(1, playhead + (dt * rate) / durationS);
      const art = engine.seek(playhead);
      if (floodCanvasRef.current && now - lastOverlay >= FLOOD_OVERLAY_MS) {
        lastOverlay = now;
        void applyFloodOverlay(viewer, floodCanvasRef.current, art, floodVisible, floodOpacity);
      }
      if (now - lastStore >= UI_THROTTLE_MS || playhead >= 1) {
        lastStore = now;
        setFloodTime(playhead);
        setGeoPreview({
          backend: art.frame.backend,
          validation: art.frame.validation,
          maxDepthM: art.frame.stats.maxDepthM,
          wetFraction: art.frame.stats.wetFraction,
          massM3: art.frame.stats.massM3,
          maxSpeedMs: art.frame.stats.maxSpeedMs,
          hazardClass: art.frame.stats.hazardClass,
          cfl: art.frame.stats.cfl,
        });
      }
      if (playhead >= 1) {
        setFloodTime(1);
        setFloodPlaying(false);
        playingLocal.current = false;
        return;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      playingLocal.current = false;
    };
  }, [
    floodPlaying,
    lowPower,
    floodVisible,
    floodOpacity,
    setFloodPlaying,
    setFloodTime,
    setGeoPreview,
    setInspectHint,
  ]);

  return (
    <div className="relative h-full min-h-0 w-full overflow-hidden bg-[#0a0c10]">
      <div ref={containerRef} className="absolute inset-0" data-testid="cesium-globe" />

      <div className="pointer-events-none absolute right-3 top-12 z-10 flex flex-col items-end gap-1.5">
        <div className="pointer-events-auto">
          <PreviewBadge validation={preview?.validation ?? "live"} backend={preview?.backend} />
        </div>
        <div className="pointer-events-none flex flex-wrap justify-end gap-1">
          <span
            className="rounded border border-edge bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-ink-dim backdrop-blur-sm"
            title="Flood result authority for the current domain"
          >
            {authority}
          </span>
          <span
            className="rounded border border-edge bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--color-hydro)] backdrop-blur-sm"
            title={terrainLabel}
          >
            Globe
          </span>
          {aoiIsValid(aoi) ? (
            <span className="rounded border border-[var(--color-hydro)]/35 bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--color-hydro)] backdrop-blur-sm">
              AOI bound
            </span>
          ) : (
            <button
              type="button"
              className="pointer-events-auto rounded border border-[var(--color-gauge)]/40 bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--color-gauge)] backdrop-blur-sm"
              onClick={() => {
                setTool("drawAoi");
                useStore.getState().setGeoViewMode("2d");
              }}
            >
              Draw AOI in 2D
            </button>
          )}
        </div>
      </div>

      <div className="pointer-events-none absolute bottom-3 left-3 z-10 max-w-md rounded border border-edge bg-panel/90 px-2.5 py-1.5 text-[10px] leading-snug text-ink-dim backdrop-blur-sm">
        <div className="text-ink">{attribution}</div>
        <div className="mt-0.5">{terrainLabel} · ion blanked on Standard</div>
        {globeError && (
          <div className="mt-0.5 text-[var(--color-hazard,#e55d78)]">{globeError}</div>
        )}
      </div>
    </div>
  );
}
