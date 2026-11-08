/**
 * Thin CesiumJS Globe wrapper for the geospatial suite.
 *
 * Standard: blank ion, Esri/OSM XYZ imagery, ellipsoid or local DEM terrain.
 * Experimental: ion World Terrain only when a user token is stored.
 * Never MapLibre→canvas-drape→Cesium.
 */

import { useEffect, useRef, useState } from "react";
import { Color, Entity, ImageryLayer, Rectangle, Viewer } from "cesium";
import { applySunDrivenLighting } from "./sunLight";
import { useStore } from "../../state/store";
import { api } from "../../lib/ipc";
import { aoiIsValid, coarsenDomainDxFromPlan, domainFromAoi, type AoiWgs84 } from "../aoi";
import { FloodPreviewEngine } from "../preview";
import { PreviewBadge } from "../PreviewBadge";
import { captureGlobeLookAt, flyGlobeToContext } from "./cameraSync";
import { applyFloodOverlay, applyDemReliefOverlay, clearFloodOverlay, clearDemReliefOverlay, scheduleIdleWork } from "./floodOverlay";
import { createGlobeImageryProvider, globeImageryAttribution } from "./imagery";
import { applyCesiumIonPolicy, blankCesiumIon } from "./ion";
import {
  getDemHeightmap,
  getLocalDemTerrainUrl,
  resolveGlobeTerrain,
  subscribeLocalDemTerrainUrl,
} from "./terrain";
import { demAuthorityLabel } from "../demAuthority";

const UI_THROTTLE_MS = 100;
const FLOOD_OVERLAY_MS = 180;
const SUN_QUARTER_HOUR_MS = 15 * 60 * 1000;
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
  const demCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const baseLayerRef = useRef<ImageryLayer | null>(null);
  const readyRef = useRef(false);
  const lastFloodPush = useRef(0);
  const floodTimer = useRef<number | null>(null);
  const demReliefCancel = useRef<(() => void) | null>(null);
  const playingLocal = useRef(false);

  const aoi = useStore((s) => s.geoAoiWgs84);
  const aoiRevision = useStore((s) => s.geoAoiRevision);
  const demRevision = useStore((s) => s.geoDemRevision);
  const demSample = useStore((s) => s.geoDemSample);
  const demProduct = useStore((s) => s.geoDemProduct);
  const workspace = useStore((s) => s.workspace);
  const floodTime = useStore((s) => s.geoFloodTime);
  const sunHourUtc = useStore((s) => s.geoSunHourUtc);
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
  const setGeoSunHourUtc = useStore((s) => s.setGeoSunHourUtc);
  const [sunClockNow, setSunClockNow] = useState(() => new Date());
  const displayedSunTimeMs = (() => {
    if (sunHourUtc == null) {
      return Math.round(sunClockNow.getTime() / SUN_QUARTER_HOUR_MS) * SUN_QUARTER_HOUR_MS;
    }
    const manualTime = new Date(sunClockNow);
    manualTime.setUTCHours(
      Math.floor(sunHourUtc),
      Math.round((sunHourUtc % 1) * 60),
      0,
      0,
    );
    return manualTime.getTime();
  })();
  const displayedSunTime = new Date(displayedSunTimeMs);
  const displayedSunHourUtc =
    sunHourUtc ??
    displayedSunTime.getUTCHours() + displayedSunTime.getUTCMinutes() / 60;
  const displayedSunHour = Math.floor(displayedSunHourUtc);
  const displayedSunMinute = Math.round((displayedSunHourUtc - displayedSunHour) * 60);
  const setTool = useStore((s) => s.setGeoTool);
  const setInspectHint = useStore((s) => s.setGeoInspectHint);

  useEffect(() => {
    let timerId: number | undefined;
    const scheduleNextClockUpdate = () => {
      const nowMs = Date.now();
      const now = new Date(nowMs);
      const nextUpdateMs =
        sunHourUtc == null
          ? (Math.round(nowMs / SUN_QUARTER_HOUR_MS) + 0.5) * SUN_QUARTER_HOUR_MS
          : Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + 1);
      timerId = window.setTimeout(() => {
        setSunClockNow(new Date());
        scheduleNextClockUpdate();
      }, Math.max(1, nextUpdateMs - nowMs));
    };

    scheduleNextClockUpdate();
    return () => {
      if (timerId !== undefined) window.clearTimeout(timerId);
    };
  }, [sunHourUtc]);

  const [attribution, setAttribution] = useState(globeImageryAttribution("satellite"));
  const [terrainLabel, setTerrainLabel] = useState("Ellipsoid");
  const [globeError, setGlobeError] = useState<string | null>(null);

  const floodVisible =
    !!layers.find((l) => l.id === "flood_depth")?.visible ||
    !!layers.find((l) => l.id === "flood_hazard")?.visible;
  const floodOpacity = layers.find((l) => l.id === "flood_depth")?.opacity ?? 0.72;
  const demVisible = !!layers.find((l) => l.id === "dtm")?.visible;
  const demOpacity = layers.find((l) => l.id === "dtm")?.opacity ?? 0.9;

  const authority = demAuthorityLabel({
    scientificMode: scientific?.mode,
    demSynthetic: demProduct?.synthetic,
    demBedSource: demProduct?.bedSource,
  });

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
      // eslint-disable-next-line react-hooks/set-state-in-effect -- error handling in init effect
      setGlobeError(err instanceof Error ? err.message : String(err));
      return;
    }

    // Photorealism merge (Item 3): the atmosphere is ON by default and the
    // globe is lit by the astronomically-correct sun direction (Item 1).
    // baseColor stays dark as the under-terrain / ocean fallback.
    viewer.scene.globe.baseColor = Color.fromCssColorString("#0a1620");
    viewer.scene.fog.enabled = true;
    viewer.scene.fog.density = 2.0e-4;

    const imagery = createGlobeImageryProvider(useStore.getState().geoBasemapMode);
    baseLayerRef.current = viewer.imageryLayers.addImageryProvider(imagery);
    setAttribution(globeImageryAttribution(useStore.getState().geoBasemapMode));

    const floodCanvas = document.createElement("canvas");
    floodCanvasRef.current = floodCanvas;
    const demCanvas = document.createElement("canvas");
    demCanvasRef.current = demCanvas;

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
    // Sun-driven lighting (Item 3): resolve via Rust astronomy command.
    {
      const aoiNow = initialAoi;
      const lat = aoiIsValid(aoiNow) ? (aoiNow![1] + aoiNow![3]) / 2 : 20.0;
      const lon = aoiIsValid(aoiNow) ? (aoiNow![0] + aoiNow![2]) / 2 : 0.0;
      void applySunDrivenLighting(viewer, lat, lon);
    }
    readyRef.current = true;
    flyGlobeToContext(viewer, initialAoi, { duration: 0 });
    ensureAoiEntity(viewer, initialAoi);

    void resolveGlobeTerrain({
      experimental: !!useStore.getState().resolved?.experimentalMode,
      terrainUrlOverride: getLocalDemTerrainUrl(),
      heightmapOverride: getDemHeightmap(),
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
        heightmapOverride: getDemHeightmap(),
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
      demReliefCancel.current?.();
      demReliefCancel.current = null;
      captureGlobeLookAt(viewer);
      engine.destroy();
      engineRef.current = null;
      floodCanvasRef.current = null;
      demCanvasRef.current = null;
      baseLayerRef.current = null;
      if (!viewer.isDestroyed()) {
        clearFloodOverlay(viewer);
        clearDemReliefOverlay(viewer);
        viewer.destroy();
      }
      viewerRef.current = null;
      blankCesiumIon();
      setGeoPreview(null);
    };
    // Mount once for Viewer lifetime.
  }, [setGeoPreview]);

  // Re-aim lighting from the same displayed timestamp used by the sun control.
  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || viewer.isDestroyed()) return;
    const aoiNow = useStore.getState().geoAoiWgs84;
    const lat = aoiIsValid(aoiNow) ? (aoiNow![1] + aoiNow![3]) / 2 : 20.0;
    const lon = aoiIsValid(aoiNow) ? (aoiNow![0] + aoiNow![2]) / 2 : 0.0;
    const when = new Date(displayedSunTimeMs);
    void applySunDrivenLighting(viewer, lat, lon, when);
  }, [displayedSunTimeMs, aoiRevision]);

  // Experimental / ion / terrain policy when Experimental toggles.
  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || !readyRef.current) return;
    applyCesiumIonPolicy(experimental);
    void resolveGlobeTerrain({
      experimental,
      terrainUrlOverride: getLocalDemTerrainUrl(),
      heightmapOverride: getDemHeightmap(),
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

    let cancelCatchUp: (() => void) | undefined;
    if (aoiIsValid(aoi)) {
      const plan = useStore.getState().geoExtentPlan;
      const domain = coarsenDomainDxFromPlan(domainFromAoi(aoi, lowPower), plan?.previewCellM);
      engine.rebindDomain(domain, useStore.getState().geoScenario?.durationHours);
      const pushArt = (art: ReturnType<FloodPreviewEngine["seek"]>) => {
        if (floodCanvasRef.current && viewerRef.current) {
          void applyFloodOverlay(
            viewerRef.current,
            floodCanvasRef.current,
            art,
            floodVisible,
            floodOpacity,
          );
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
      };
      cancelCatchUp = engine.seekZeroThenCatchUp(() => useStore.getState().geoFloodTime, pushArt);
    } else {
      engine.clearBoundDomain();
      clearFloodOverlay(viewer);
      clearDemReliefOverlay(viewer);
      setGeoPreview(null);
    }
    return () => cancelCatchUp?.();
  }, [aoi, aoiRevision, lowPower, floodVisible, floodOpacity, setGeoPreview]);

  // DEM samples → soft/HAND bed on Globe (GeoTIFF path; heightmap terrain via store).
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
    if (!engine.hasBoundDomain() || !viewer || !floodCanvasRef.current) return;
    const pushArt = (art: ReturnType<FloodPreviewEngine["seek"]>) => {
      if (floodCanvasRef.current && viewerRef.current) {
        void applyFloodOverlay(
          viewerRef.current,
          floodCanvasRef.current,
          art,
          floodVisible,
          floodOpacity,
        );
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
    };
    return engine.seekZeroThenCatchUp(() => useStore.getState().geoFloodTime, pushArt);
  }, [demSample, demRevision, floodVisible, floodOpacity, setGeoPreview]);

  // DEM relief SingleTile drape (elevation tint + hillshade) — idle/async, capped grid.
  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || !readyRef.current) return;

    demReliefCancel.current?.();
    demReliefCancel.current = null;

    if (!demSample || !demVisible) {
      clearDemReliefOverlay(viewer);
      return;
    }

    const grid = {
      cols: demSample.cols,
      rows: demSample.rows,
      bounds: demSample.bounds,
      z: demSample.z,
    };

    const sampleTime = new Date(displayedSunTimeMs);
    const hourUtc = displayedSunHourUtc;
    let stale = false;
    demReliefCancel.current = scheduleIdleWork(() => {
      if (!viewerRef.current || viewerRef.current.isDestroyed() || !demCanvasRef.current) {
        return;
      }
      void (async () => {
        let shade: number[] | undefined;
        if (workspace) {
          try {
            const solarGrid = await api.sampleGeoDemHillshade(
              workspace,
              grid.cols,
              grid.rows,
              {
                year: sampleTime.getUTCFullYear(),
                month: sampleTime.getUTCMonth() + 1,
                day: sampleTime.getUTCDate(),
                hourUtc,
              },
              aoi,
            );
            const sameBounds = solarGrid.bounds.every(
              (value, index) => Math.abs(value - grid.bounds[index]) < 1e-7,
            );
            if (
              sameBounds &&
              solarGrid.cols === grid.cols &&
              solarGrid.rows === grid.rows &&
              solarGrid.shade.length === grid.cols * grid.rows &&
              solarGrid.shade.every((value) => Number.isFinite(value) && value >= 0 && value <= 1)
            ) {
              shade = solarGrid.shade;
            }
          } catch {
            // The static NW hillshade remains available when no staged DEM IPC is present.
          }
        }
        if (
          stale ||
          !viewerRef.current ||
          viewerRef.current.isDestroyed() ||
          !demCanvasRef.current
        ) {
          return;
        }
        await applyDemReliefOverlay(
          viewerRef.current,
          demCanvasRef.current,
          grid,
          demVisible,
          demOpacity,
          shade,
        );
      })();
    }, 160);

    return () => {
      stale = true;
      demReliefCancel.current?.();
      demReliefCancel.current = null;
    };
  }, [
    demSample,
    demRevision,
    demVisible,
    demOpacity,
    displayedSunHourUtc,
    displayedSunTimeMs,
    workspace,
    aoi,
  ]);

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
    <div className="relative h-full min-h-0 w-full overflow-hidden bg-bg">
      <div ref={containerRef} className="absolute inset-0" data-testid="cesium-globe" />

      <div className="pointer-events-none absolute right-3 top-12 z-10 flex flex-col items-end gap-1.5">
        <div className="pointer-events-auto rounded border border-edge bg-panel/90 px-2 py-1 backdrop-blur-sm">
          <label className="flex items-center gap-2 text-[10px] text-ink-dim">
            <span>Sun&nbsp;UTC</span>
            <input
              aria-label="Sun hour (UTC)"
              type="range"
              min={0}
              max={23.75}
              step={0.25}
              value={displayedSunHourUtc}
              onChange={(e) => setGeoSunHourUtc(Number(e.target.value))}
              className="h-1 w-28 accent-[var(--color-accent,#4aa3ff)]"
            />
            <span className="tabular-nums">
              {String(displayedSunHour).padStart(2, "0")}:{String(displayedSunMinute).padStart(2, "0")}
            </span>
          </label>
        </div>
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
          <button
            type="button"
            className="pointer-events-auto rounded border border-[var(--color-gauge)]/40 bg-panel/90 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--color-gauge)] backdrop-blur-sm"
            onClick={() => {
              setTool("drawAoi");
              useStore.getState().setGeoViewMode("2d");
            }}
          >
            {aoiIsValid(aoi) ? "Edit AOI in 2D" : "Draw AOI in 2D"}
          </button>
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
