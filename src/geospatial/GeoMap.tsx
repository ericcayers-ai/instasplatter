import { useEffect, useRef } from "react";
import maplibregl, { type Map as MapLibreMap, type StyleSpecification } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import { useStore } from "../state/store";
import { api, type SimEvent } from "../lib/ipc";
import {
  aoiIsValid,
  aoiPolygonGeoJson,
  domainFromAoi,
  networkFeaturesForAoi,
  normalizeAoi,
  type AoiWgs84,
} from "./aoi";
import {
  CARTO_ATTRIBUTION,
  CARTO_DARK_TILES,
  ESRI_WORLD_IMAGERY_ATTRIBUTION,
  ESRI_WORLD_IMAGERY_TILES,
  GEO_MAP_CENTER,
  GEO_MAP_ZOOM,
} from "./defaults";
import {
  FloodPreviewEngine,
  type PreviewRenderArtifacts,
  type ScientificCheckpoint,
} from "./preview";
import type { GeoBasemapMode, GeoViewMode } from "./types";

const FLOOD_RASTER_SRC = "geo-flood-raster";
const FLOOD_RASTER_LAYER = "flood-raster";
const SHORE_SRC = "geo-flood-shore";
const SHORE_LAYER = "flood-shore";
const PARTICLES_SRC = "geo-flood-particles";
const PARTICLES_LAYER = "flood-particles";
const WATERWAYS_SRC = "geo-waterways";
const GAUGES_SRC = "geo-gauges";
const AOI_SRC = "geo-aoi";
const AOI_FILL = "geo-aoi-fill";
const AOI_LINE = "geo-aoi-line";
const AOI_DRAFT_SRC = "geo-aoi-draft";
const AOI_DRAFT_FILL = "geo-aoi-draft-fill";
const AOI_DRAFT_LINE = "geo-aoi-draft-line";

const UI_THROTTLE_MS = 100;

function buildBaseStyle(mode: GeoBasemapMode): StyleSpecification {
  const satellite = mode === "satellite";
  return {
    version: 8,
    name: "InstaSplatter survey",
    glyphs: "https://demotiles.maplibre.org/font/{fontstack}/{range}.pbf",
    sources: {
      "esri-imagery": {
        type: "raster",
        tiles: [ESRI_WORLD_IMAGERY_TILES],
        tileSize: 256,
        attribution: ESRI_WORLD_IMAGERY_ATTRIBUTION,
        maxzoom: 19,
      },
      "carto-dark": {
        type: "raster",
        tiles: [...CARTO_DARK_TILES],
        tileSize: 256,
        attribution: CARTO_ATTRIBUTION,
        maxzoom: 20,
      },
    },
    layers: [
      {
        id: "background",
        type: "background",
        paint: { "background-color": satellite ? "#0a1620" : "#08121D" },
      },
      {
        id: "esri-imagery",
        type: "raster",
        source: "esri-imagery",
        layout: { visibility: satellite ? "visible" : "none" },
        paint: { "raster-opacity": 1 },
      },
      {
        id: "carto-dark",
        type: "raster",
        source: "carto-dark",
        layout: { visibility: satellite ? "none" : "visible" },
        paint: { "raster-opacity": 1 },
      },
    ],
  };
}

function boundsToCoords(bounds: [number, number, number, number]): [
  [number, number],
  [number, number],
  [number, number],
  [number, number],
] {
  const [west, south, east, north] = bounds;
  return [
    [west, north],
    [east, north],
    [east, south],
    [west, south],
  ];
}

function emptyFc(): GeoJSON.FeatureCollection {
  return { type: "FeatureCollection", features: [] };
}

function ensureAoiLayers(map: MapLibreMap) {
  if (!map.getSource(AOI_SRC)) {
    map.addSource(AOI_SRC, { type: "geojson", data: emptyFc() });
  }
  if (!map.getSource(AOI_DRAFT_SRC)) {
    map.addSource(AOI_DRAFT_SRC, { type: "geojson", data: emptyFc() });
  }
  if (!map.getLayer(AOI_FILL)) {
    map.addLayer({
      id: AOI_FILL,
      type: "fill",
      source: AOI_SRC,
      paint: { "fill-color": "#25C6D9", "fill-opacity": 0.08 },
    });
  }
  if (!map.getLayer(AOI_LINE)) {
    map.addLayer({
      id: AOI_LINE,
      type: "line",
      source: AOI_SRC,
      paint: {
        "line-color": "#F1B84B",
        "line-width": 2,
        "line-dasharray": [2, 1.25],
      },
    });
  }
  if (!map.getLayer(AOI_DRAFT_FILL)) {
    map.addLayer({
      id: AOI_DRAFT_FILL,
      type: "fill",
      source: AOI_DRAFT_SRC,
      paint: { "fill-color": "#F1B84B", "fill-opacity": 0.12 },
    });
  }
  if (!map.getLayer(AOI_DRAFT_LINE)) {
    map.addLayer({
      id: AOI_DRAFT_LINE,
      type: "line",
      source: AOI_DRAFT_SRC,
      paint: { "line-color": "#F1B84B", "line-width": 1.5 },
    });
  }
}

function ensureStaticSources(map: MapLibreMap, aoi: AoiWgs84 | null) {
  const net = aoiIsValid(aoi) ? networkFeaturesForAoi(aoi) : null;
  if (!map.getSource(WATERWAYS_SRC)) {
    map.addSource(WATERWAYS_SRC, {
      type: "geojson",
      data: net?.waterways ?? emptyFc(),
    });
  } else {
    (map.getSource(WATERWAYS_SRC) as maplibregl.GeoJSONSource).setData(
      net?.waterways ?? emptyFc(),
    );
  }
  if (!map.getSource(GAUGES_SRC)) {
    map.addSource(GAUGES_SRC, {
      type: "geojson",
      data: net?.gauges ?? emptyFc(),
    });
  } else {
    (map.getSource(GAUGES_SRC) as maplibregl.GeoJSONSource).setData(net?.gauges ?? emptyFc());
  }
  if (!map.getLayer("waterways-line")) {
    map.addLayer({
      id: "waterways-line",
      type: "line",
      source: WATERWAYS_SRC,
      paint: {
        "line-color": "#36516A",
        "line-width": 2.5,
        "line-opacity": 0.95,
      },
    });
  }
  if (!map.getLayer("gauges-circle")) {
    map.addLayer({
      id: "gauges-circle",
      type: "circle",
      source: GAUGES_SRC,
      paint: {
        "circle-radius": 5,
        "circle-color": "#F1B84B",
        "circle-stroke-width": 1.5,
        "circle-stroke-color": "#08121D",
      },
    });
  }
  ensureAoiLayers(map);
}

function syncFloodCanvas(
  canvas: HTMLCanvasElement,
  image: ImageData,
): void {
  if (canvas.width !== image.width || canvas.height !== image.height) {
    canvas.width = image.width;
    canvas.height = image.height;
  }
  const ctx = canvas.getContext("2d");
  if (ctx) ctx.putImageData(image, 0, 0);
}

function ensurePreviewLayers(
  map: MapLibreMap,
  artifacts: PreviewRenderArtifacts,
  canvas: HTMLCanvasElement,
) {
  const coords = boundsToCoords(artifacts.bounds);
  syncFloodCanvas(canvas, artifacts.image);

  if (!map.getSource(FLOOD_RASTER_SRC)) {
    map.addSource(FLOOD_RASTER_SRC, {
      type: "canvas",
      canvas,
      coordinates: coords,
      animate: true,
    });
  } else {
    const src = map.getSource(FLOOD_RASTER_SRC) as maplibregl.CanvasSource;
    src.setCoordinates(coords);
    // animate:true re-reads canvas pixels each frame — no ImageData cast / toDataURL.
  }

  if (!map.getLayer(FLOOD_RASTER_LAYER)) {
    map.addLayer(
      {
        id: FLOOD_RASTER_LAYER,
        type: "raster",
        source: FLOOD_RASTER_SRC,
        paint: {
          "raster-opacity": 0.85,
          "raster-fade-duration": 0,
        },
      },
      "waterways-line",
    );
  }

  if (!map.getSource(SHORE_SRC)) {
    map.addSource(SHORE_SRC, { type: "geojson", data: artifacts.shoreline });
  } else {
    (map.getSource(SHORE_SRC) as maplibregl.GeoJSONSource).setData(artifacts.shoreline);
  }
  if (!map.getLayer(SHORE_LAYER)) {
    map.addLayer({
      id: SHORE_LAYER,
      type: "line",
      source: SHORE_SRC,
      paint: {
        "line-color": "#F1B84B",
        "line-width": 1.25,
        "line-opacity": 0.9,
      },
    });
  }

  if (!map.getSource(PARTICLES_SRC)) {
    map.addSource(PARTICLES_SRC, { type: "geojson", data: artifacts.particles });
  } else {
    (map.getSource(PARTICLES_SRC) as maplibregl.GeoJSONSource).setData(artifacts.particles);
  }
  if (!map.getLayer(PARTICLES_LAYER)) {
    map.addLayer({
      id: PARTICLES_LAYER,
      type: "circle",
      source: PARTICLES_SRC,
      paint: {
        "circle-radius": 1.75,
        "circle-color": "#E8F7FA",
        "circle-opacity": 0.75,
        "circle-stroke-width": 0,
      },
    });
  }
}

function setLayerVis(map: MapLibreMap, id: string, visible: boolean) {
  if (!map.getLayer(id)) return;
  map.setLayoutProperty(id, "visibility", visible ? "visible" : "none");
}

function applyViewMode(map: MapLibreMap, mode: GeoViewMode) {
  if (mode === "3d") {
    map.easeTo({ pitch: 55, bearing: -18, duration: 450 });
  } else {
    map.easeTo({ pitch: 0, bearing: 0, duration: 450 });
  }
}

function fitAoi(map: MapLibreMap, aoi: AoiWgs84) {
  const [west, south, east, north] = normalizeAoi(aoi);
  map.fitBounds(
    [
      [west, south],
      [east, north],
    ],
    { padding: 48, duration: 600, maxZoom: 16 },
  );
}

interface GeoMapProps {
  className?: string;
}

export default function GeoMap({ className }: GeoMapProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const readyRef = useRef(false);
  const engineRef = useRef<FloodPreviewEngine | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const lastUiPush = useRef(0);
  const draftCorner = useRef<[number, number] | null>(null);
  const playingLocal = useRef(false);

  const layers = useStore((s) => s.geoLayers);
  const floodTime = useStore((s) => s.geoFloodTime);
  const floodPlaying = useStore((s) => s.geoFloodPlaying);
  const lowPower = useStore((s) => s.geoFloodLowPower);
  const viewMode = useStore((s) => s.geoViewMode);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const tool = useStore((s) => s.geoTool);
  const basemapMode = useStore((s) => s.geoBasemapMode);
  const aoi = useStore((s) => s.geoAoiWgs84);
  const aoiRevision = useStore((s) => s.geoAoiRevision);
  const setInspectHint = useStore((s) => s.setGeoInspectHint);
  const setGeoPreview = useStore((s) => s.setGeoPreview);
  const setFloodTime = useStore((s) => s.setGeoFloodTime);
  const setFloodPlaying = useStore((s) => s.setGeoFloodPlaying);
  const commitGeoAoi = useStore((s) => s.commitGeoAoi);

  // Init map + preview engine once.
  useEffect(() => {
    const el = containerRef.current;
    if (!el || mapRef.current) return;

    const canvas = document.createElement("canvas");
    canvas.width = 96;
    canvas.height = 72;
    canvasRef.current = canvas;

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

    const mode = useStore.getState().geoBasemapMode;
    const map = new maplibregl.Map({
      container: el,
      style: buildBaseStyle(mode),
      center: GEO_MAP_CENTER,
      zoom: GEO_MAP_ZOOM,
      pitch: 0,
      attributionControl: { compact: true },
      maxPitch: 70,
    });
    map.addControl(new maplibregl.NavigationControl({ visualizePitch: true }), "top-right");
    map.addControl(new maplibregl.ScaleControl({ unit: "metric" }), "bottom-left");

    const pushPreviewThrottled = (artifacts: PreviewRenderArtifacts, force = false) => {
      const now = performance.now();
      if (!force && now - lastUiPush.current < UI_THROTTLE_MS) return;
      lastUiPush.current = now;
      const { frame } = artifacts;
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

      const root = containerRef.current?.closest(".geo-viewport");
      if (root instanceof HTMLElement) {
        root.dataset.waterStyle = useStore.getState().geoWaterStyle;
        root.dataset.hazardClass = String(frame.stats.hazardClass);
        root.dataset.previewValidation = frame.validation;
        root.style.setProperty("--geo-wet", String(frame.stats.wetFraction));
        root.style.setProperty("--geo-depth", String(frame.stats.maxDepthM));
      }
    };

    const applyArtifacts = (artifacts: PreviewRenderArtifacts, forceUi = false) => {
      if (!readyRef.current || !mapRef.current || !canvasRef.current) return;
      if (!engineRef.current?.hasBoundDomain()) {
        setLayerVis(mapRef.current, FLOOD_RASTER_LAYER, false);
        setLayerVis(mapRef.current, SHORE_LAYER, false);
        setLayerVis(mapRef.current, PARTICLES_LAYER, false);
        return;
      }
      ensurePreviewLayers(mapRef.current, artifacts, canvasRef.current);
      pushPreviewThrottled(artifacts, forceUi);
    };

    map.on("load", () => {
      const state = useStore.getState();
      ensureStaticSources(map, state.geoAoiWgs84);
      if (aoiIsValid(state.geoAoiWgs84)) {
        (map.getSource(AOI_SRC) as maplibregl.GeoJSONSource).setData(
          aoiPolygonGeoJson(state.geoAoiWgs84),
        );
        fitAoi(map, state.geoAoiWgs84);
      }
      readyRef.current = true;
      const byId = Object.fromEntries(state.geoLayers.map((l) => [l.id, l]));
      setLayerVis(map, "waterways-line", !!byId.waterways?.visible && aoiIsValid(state.geoAoiWgs84));
      setLayerVis(map, "gauges-circle", !!byId.gauges?.visible && aoiIsValid(state.geoAoiWgs84));
      if (state.geoViewMode === "3d") {
        map.jumpTo({ pitch: 55, bearing: -18 });
      }

      void engine.whenReady().then(() => {
        if (!engine.hasBoundDomain()) return;
        const art = engine.seek(state.geoFloodTime);
        applyArtifacts(art, true);
        setLayerVis(map, FLOOD_RASTER_LAYER, !!byId.flood_depth?.visible || !!byId.flood_hazard?.visible);
        setLayerVis(map, SHORE_LAYER, !!byId.flood_depth?.visible);
        setLayerVis(map, PARTICLES_LAYER, !!byId.flood_velocity?.visible);
      });
    });

    map.on("mousedown", (e) => {
      if (useStore.getState().geoTool !== "drawAoi") return;
      e.preventDefault();
      draftCorner.current = [e.lngLat.lng, e.lngLat.lat];
      map.dragPan.disable();
    });

    map.on("mousemove", (e) => {
      if (useStore.getState().geoTool !== "drawAoi" || !draftCorner.current) return;
      const [lng0, lat0] = draftCorner.current;
      const draft = normalizeAoi([lng0, lat0, e.lngLat.lng, e.lngLat.lat]);
      const src = map.getSource(AOI_DRAFT_SRC) as maplibregl.GeoJSONSource | undefined;
      src?.setData(aoiPolygonGeoJson(draft));
    });

    map.on("mouseup", (e) => {
      if (useStore.getState().geoTool !== "drawAoi" || !draftCorner.current) return;
      const [lng0, lat0] = draftCorner.current;
      draftCorner.current = null;
      map.dragPan.enable();
      const box = normalizeAoi([lng0, lat0, e.lngLat.lng, e.lngLat.lat]);
      const src = map.getSource(AOI_DRAFT_SRC) as maplibregl.GeoJSONSource | undefined;
      src?.setData(emptyFc());
      if (!aoiIsValid(box)) {
        setInspectHint("Drag a larger rectangle for the AOI");
        return;
      }
      void commitGeoAoi(box);
    });

    map.on("click", (e) => {
      const t = useStore.getState().geoTool;
      if (t === "drawAoi") return;
      if (t === "inspect") {
        const eng = engineRef.current;
        const sample = eng?.hasBoundDomain()
          ? eng.sampleAtLngLat(e.lngLat.lng, e.lngLat.lat)
          : null;
        if (sample) {
          setInspectHint(
            `Inspect ${e.lngLat.lat.toFixed(5)}, ${e.lngLat.lng.toFixed(5)} — depth ${sample.depthM.toFixed(2)} m · |v| ${sample.speedMs.toFixed(2)} m/s (${eng?.getBackend() ?? "?"})`,
          );
        } else if (!eng?.hasBoundDomain()) {
          setInspectHint("Draw an AOI first, then inspect flood depth under the cursor");
        } else {
          setInspectHint(
            `Inspect ${e.lngLat.lat.toFixed(5)}, ${e.lngLat.lng.toFixed(5)} — outside flood domain`,
          );
        }
      } else if (t === "measure") {
        setInspectHint("Measure: click two points (stub — distance tool not connected yet)");
      } else if (t === "profile") {
        setInspectHint("Profile: draw a section line (stub — cross-section tool not connected yet)");
      }
    });

    const unsub = engine.subscribe((art) => applyArtifacts(art));
    mapRef.current = map;

    let unlistenSim: (() => void) | undefined;
    void api
      .onSimEvent((ev: SimEvent) => {
        if (ev.kind !== "checkpoint") return;
        if (ev.mode === "preview") return;
        if (ev.maxDepthM == null && ev.wetFraction == null && ev.massM3 == null) return;
        const cp: ScientificCheckpoint = {
          timeS: ev.simTimeHours * 3600,
          maxDepthM: ev.maxDepthM ?? 0,
          wetFraction: ev.wetFraction ?? 0,
          massM3: ev.massM3 ?? 0,
        };
        engineRef.current?.ingestCheckpoint(cp);
      })
      .then((u) => {
        unlistenSim = u;
      });

    return () => {
      unlistenSim?.();
      unsub();
      readyRef.current = false;
      engine.destroy();
      engineRef.current = null;
      setGeoPreview(null);
      map.remove();
      mapRef.current = null;
      canvasRef.current = null;
    };
  }, [setInspectHint, setGeoPreview, commitGeoAoi]);

  // Basemap mode toggle (satellite vs low-bandwidth).
  useEffect(() => {
    const map = mapRef.current;
    if (!map || !readyRef.current) return;
    const satellite = basemapMode === "satellite";
    setLayerVis(map, "esri-imagery", satellite);
    setLayerVis(map, "carto-dark", !satellite);
    const basemap = Object.fromEntries(layers.map((l) => [l.id, l])).basemap;
    const opacity = basemap?.visible ? basemap.opacity : 0;
    if (map.getLayer("esri-imagery")) {
      map.setPaintProperty("esri-imagery", "raster-opacity", satellite ? opacity : 0);
    }
    if (map.getLayer("carto-dark")) {
      map.setPaintProperty("carto-dark", "raster-opacity", satellite ? 0 : opacity);
    }
  }, [basemapMode, layers]);

  // AOI commit → rebind soft-solver + fit + network stubs.
  useEffect(() => {
    const map = mapRef.current;
    const engine = engineRef.current;
    if (!map || !readyRef.current || !engine) return;

    ensureStaticSources(map, aoi);
    if (aoiIsValid(aoi)) {
      (map.getSource(AOI_SRC) as maplibregl.GeoJSONSource)?.setData(aoiPolygonGeoJson(aoi));
      const domain = domainFromAoi(aoi, lowPower);
      const plan = useStore.getState().geoExtentPlan;
      if (plan?.previewCellM && plan.previewCellM > 0) {
        domain.dxM = Math.max(4, plan.previewCellM);
        domain.cols = Math.max(24, Math.round(((aoi[2] - aoi[0]) * 111320 * Math.cos((((aoi[1] + aoi[3]) / 2) * Math.PI) / 180)) / domain.dxM));
        domain.rows = Math.max(16, Math.round(((aoi[3] - aoi[1]) * 111320) / domain.dxM));
      }
      engine.rebindDomain(domain, useStore.getState().geoScenario?.durationHours);
      const art = engine.seek(useStore.getState().geoFloodTime);
      if (canvasRef.current) ensurePreviewLayers(map, art, canvasRef.current);
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
      fitAoi(map, aoi);
      const byId = Object.fromEntries(useStore.getState().geoLayers.map((l) => [l.id, l]));
      setLayerVis(map, FLOOD_RASTER_LAYER, !!byId.flood_depth?.visible || !!byId.flood_hazard?.visible);
      setLayerVis(map, SHORE_LAYER, !!byId.flood_depth?.visible);
      setLayerVis(map, PARTICLES_LAYER, !!byId.flood_velocity?.visible);
      setLayerVis(map, "waterways-line", !!byId.waterways?.visible);
      setLayerVis(map, "gauges-circle", !!byId.gauges?.visible);
    } else {
      (map.getSource(AOI_SRC) as maplibregl.GeoJSONSource)?.setData(emptyFc());
      engine.clearBoundDomain();
      setLayerVis(map, FLOOD_RASTER_LAYER, false);
      setLayerVis(map, SHORE_LAYER, false);
      setLayerVis(map, PARTICLES_LAYER, false);
      setLayerVis(map, "waterways-line", false);
      setLayerVis(map, "gauges-circle", false);
      setGeoPreview(null);
    }
  }, [aoi, aoiRevision, lowPower, setGeoPreview]);

  // Layer visibility / opacity.
  useEffect(() => {
    const map = mapRef.current;
    if (!map || !readyRef.current) return;

    const byId = Object.fromEntries(layers.map((l) => [l.id, l]));
    const basemap = byId.basemap;
    const opacity = basemap?.visible ? basemap.opacity : 0;
    const satellite = useStore.getState().geoBasemapMode === "satellite";
    if (map.getLayer("esri-imagery")) {
      map.setPaintProperty("esri-imagery", "raster-opacity", satellite ? opacity : 0);
    }
    if (map.getLayer("carto-dark")) {
      map.setPaintProperty("carto-dark", "raster-opacity", satellite ? 0 : opacity);
    }

    const hasDomain = !!engineRef.current?.hasBoundDomain();
    const flood = byId.flood_depth;
    const hazard = byId.flood_hazard;
    const floodOn =
      hasDomain && !!(flood?.visible || (waterStyle === "hazard" && hazard?.visible));
    setLayerVis(map, FLOOD_RASTER_LAYER, floodOn || !!(hasDomain && hazard?.visible && waterStyle === "hazard"));
    setLayerVis(
      map,
      SHORE_LAYER,
      hasDomain && !!flood?.visible && (waterStyle === "contour" || waterStyle === "depth"),
    );
    if (flood && map.getLayer(FLOOD_RASTER_LAYER)) {
      map.setPaintProperty(FLOOD_RASTER_LAYER, "raster-opacity", flood.opacity);
    }

    setLayerVis(map, PARTICLES_LAYER, hasDomain && !!byId.flood_velocity?.visible);

    const waterways = byId.waterways;
    setLayerVis(map, "waterways-line", hasDomain && !!waterways?.visible);
    if (waterways && map.getLayer("waterways-line")) {
      map.setPaintProperty("waterways-line", "line-opacity", waterways.opacity);
    }

    const gauges = byId.gauges;
    setLayerVis(map, "gauges-circle", hasDomain && !!gauges?.visible);
  }, [layers, waterStyle]);

  // Scrub / water style → seek preview (throttled while playing to avoid React thrash).
  useEffect(() => {
    if (playingLocal.current) return;
    const engine = engineRef.current;
    const map = mapRef.current;
    if (!engine || !map || !readyRef.current || !engine.hasBoundDomain()) return;
    engine.setWaterStyle(waterStyle);
    engine.setLowPower(lowPower);
    const art = engine.seek(floodTime);
    if (canvasRef.current) ensurePreviewLayers(map, art, canvasRef.current);
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
  }, [floodTime, waterStyle, lowPower, setGeoPreview]);

  // Play loop — advances map directly; throttles React store updates.
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
    const engine = engineRef.current;
    if (!engine?.hasBoundDomain()) {
      setFloodPlaying(false);
      setInspectHint("Draw an AOI before playing the flood preview");
      return;
    }

    playingLocal.current = true;
    let raf = 0;
    let last = performance.now();
    let lastStore = 0;
    let playhead = useStore.getState().geoFloodTime;
    const rate = lowPower ? 800 : 1200;
    const durationS = engine.durationS || (useStore.getState().geoScenario?.durationHours ?? 12) * 3600;

    const tick = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      // Local playhead avoids thrashing React every frame; store updates are throttled.
      playhead = Math.min(1, playhead + (dt * rate) / durationS);
      const art = engine.seek(playhead);
      if (mapRef.current && canvasRef.current) {
        ensurePreviewLayers(mapRef.current, art, canvasRef.current);
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
  }, [floodPlaying, lowPower, setFloodPlaying, setFloodTime, setGeoPreview, setInspectHint]);

  // 2D / 3D.
  useEffect(() => {
    const map = mapRef.current;
    if (!map || !readyRef.current) return;
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (reduce) {
      map.jumpTo({
        pitch: viewMode === "3d" ? 55 : 0,
        bearing: viewMode === "3d" ? -18 : 0,
      });
    } else {
      applyViewMode(map, viewMode);
    }
  }, [viewMode]);

  // Cursor for tools.
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    const canvas = map.getCanvas();
    canvas.style.cursor =
      tool === "pan"
        ? ""
        : tool === "inspect"
          ? "help"
          : tool === "drawAoi"
            ? "crosshair"
            : tool === "measure"
              ? "crosshair"
              : "cell";
  }, [tool]);

  return (
    <div
      ref={containerRef}
      className={`geo-map h-full w-full min-h-0 ${className ?? ""}`}
      role="application"
      aria-label="Geospatial map viewport"
    />
  );
}
