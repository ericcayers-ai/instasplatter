import { useEffect, useRef } from "react";
import maplibregl, { type Map as MapLibreMap, type StyleSpecification } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import { useStore } from "../state/store";
import { api, type SimEvent } from "../lib/ipc";
import { GEO_MAP_CENTER, GEO_MAP_ZOOM } from "./defaults";
import { placeholderGauges, placeholderWaterways } from "./floodPreview";
import {
  FloodPreviewEngine,
  type PreviewRenderArtifacts,
  type ScientificCheckpoint,
} from "./preview";
import type { GeoViewMode } from "./types";

const FLOOD_RASTER_SRC = "geo-flood-raster";
const FLOOD_RASTER_LAYER = "flood-raster";
const SHORE_SRC = "geo-flood-shore";
const SHORE_LAYER = "flood-shore";
const PARTICLES_SRC = "geo-flood-particles";
const PARTICLES_LAYER = "flood-particles";
const WATERWAYS_SRC = "geo-waterways";
const GAUGES_SRC = "geo-gauges";

/** Offline-friendly dark basemap using public OSM raster tiles via a MapLibre style. */
const BASE_STYLE: StyleSpecification = {
  version: 8,
  name: "InstaSplatter survey",
  glyphs: "https://demotiles.maplibre.org/font/{fontstack}/{range}.pbf",
  sources: {
    "osm-raster": {
      type: "raster",
      tiles: [
        "https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
        "https://b.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
        "https://c.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
      ],
      tileSize: 256,
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OSM</a> &copy; CARTO',
    },
  },
  layers: [
    {
      id: "background",
      type: "background",
      paint: { "background-color": "#08121D" },
    },
    {
      id: "osm-raster",
      type: "raster",
      source: "osm-raster",
      paint: { "raster-opacity": 1 },
    },
  ],
};

function imageDataToCanvas(image: ImageData): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = image.width;
  canvas.height = image.height;
  const ctx = canvas.getContext("2d");
  if (ctx) ctx.putImageData(image, 0, 0);
  return canvas;
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

function ensureStaticSources(map: MapLibreMap) {
  if (!map.getSource(WATERWAYS_SRC)) {
    map.addSource(WATERWAYS_SRC, { type: "geojson", data: placeholderWaterways() });
  }
  if (!map.getSource(GAUGES_SRC)) {
    map.addSource(GAUGES_SRC, { type: "geojson", data: placeholderGauges() });
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
}

function ensurePreviewLayers(map: MapLibreMap, artifacts: PreviewRenderArtifacts) {
  const coords = boundsToCoords(artifacts.bounds);

  if (!map.getSource(FLOOD_RASTER_SRC)) {
    const canvas = imageDataToCanvas(artifacts.image);
    map.addSource(FLOOD_RASTER_SRC, {
      type: "image",
      url: canvas.toDataURL(),
      coordinates: coords,
    });
  } else {
    const src = map.getSource(FLOOD_RASTER_SRC) as maplibregl.ImageSource;
    // ImageData is accepted by MapLibre updateImage — avoids toDataURL each frame.
    src.updateImage({ url: artifacts.image as unknown as string, coordinates: coords });
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

interface GeoMapProps {
  className?: string;
}

export default function GeoMap({ className }: GeoMapProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const readyRef = useRef(false);
  const engineRef = useRef<FloodPreviewEngine | null>(null);
  const canvasScratch = useRef<HTMLCanvasElement | null>(null);

  const layers = useStore((s) => s.geoLayers);
  const floodTime = useStore((s) => s.geoFloodTime);
  const floodPlaying = useStore((s) => s.geoFloodPlaying);
  const lowPower = useStore((s) => s.geoFloodLowPower);
  const viewMode = useStore((s) => s.geoViewMode);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const tool = useStore((s) => s.geoTool);
  const setInspectHint = useStore((s) => s.setGeoInspectHint);
  const setGeoPreview = useStore((s) => s.setGeoPreview);
  const setFloodTime = useStore((s) => s.setGeoFloodTime);
  const setFloodPlaying = useStore((s) => s.setGeoFloodPlaying);

  // Init map + preview engine once.
  useEffect(() => {
    const el = containerRef.current;
    if (!el || mapRef.current) return;

    const reducedMotion = !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    const low = useStore.getState().geoFloodLowPower;
    const engine = new FloodPreviewEngine({
      lowPower: low,
      reducedMotion,
      waterStyle: useStore.getState().geoWaterStyle,
    });
    engineRef.current = engine;

    const map = new maplibregl.Map({
      container: el,
      style: BASE_STYLE,
      center: GEO_MAP_CENTER,
      zoom: GEO_MAP_ZOOM,
      pitch: 0,
      attributionControl: { compact: true },
      maxPitch: 70,
    });
    map.addControl(new maplibregl.NavigationControl({ visualizePitch: true }), "top-right");
    map.addControl(new maplibregl.ScaleControl({ unit: "metric" }), "bottom-left");

    const pushPreview = (artifacts: PreviewRenderArtifacts) => {
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

    const applyArtifacts = (artifacts: PreviewRenderArtifacts) => {
      if (!readyRef.current || !mapRef.current) return;
      ensurePreviewLayers(mapRef.current, artifacts);
      pushPreview(artifacts);
    };

    map.on("load", () => {
      ensureStaticSources(map);
      readyRef.current = true;
      const state = useStore.getState();
      const byId = Object.fromEntries(state.geoLayers.map((l) => [l.id, l]));
      if (byId.basemap && map.getLayer("osm-raster")) {
        map.setPaintProperty(
          "osm-raster",
          "raster-opacity",
          byId.basemap.visible ? byId.basemap.opacity : 0,
        );
      }
      setLayerVis(map, "waterways-line", !!byId.waterways?.visible);
      setLayerVis(map, "gauges-circle", !!byId.gauges?.visible);
      if (state.geoViewMode === "3d") {
        map.jumpTo({ pitch: 55, bearing: -18 });
      }

      void engine.whenReady().then(() => {
        const art = engine.seek(state.geoFloodTime);
        applyArtifacts(art);
        setLayerVis(map, FLOOD_RASTER_LAYER, !!byId.flood_depth?.visible || !!byId.flood_hazard?.visible);
        setLayerVis(map, SHORE_LAYER, !!byId.flood_depth?.visible);
        setLayerVis(map, PARTICLES_LAYER, !!byId.flood_velocity?.visible);
      });
    });

    map.on("click", (e) => {
      const t = useStore.getState().geoTool;
      const preview = useStore.getState().geoPreview;
      if (t === "inspect") {
        const depth = preview?.maxDepthM;
        setInspectHint(
          depth != null
            ? `Inspect ${e.lngLat.lat.toFixed(5)}, ${e.lngLat.lng.toFixed(5)} — preview max depth ${depth.toFixed(2)} m (${preview?.backend ?? "?"})`
            : `Inspect ${e.lngLat.lat.toFixed(5)}, ${e.lngLat.lng.toFixed(5)} — flood sample pending`,
        );
      } else if (t === "measure") {
        setInspectHint("Measure: click two points (stub — distance tool not connected yet)");
      } else if (t === "profile") {
        setInspectHint("Profile: draw a section line (stub — cross-section tool not connected yet)");
      }
    });

    const unsub = engine.subscribe((art) => applyArtifacts(art));
    mapRef.current = map;

    let unlistenSim: (() => void) | undefined;
    void api.onSimEvent((ev: SimEvent) => {
      if (ev.kind !== "checkpoint") return;
      // Only scientific / demo ANUGA-side streams feed comparison.
      if (ev.mode === "preview") return;
      if (ev.maxDepthM == null && ev.wetFraction == null && ev.massM3 == null) return;
      const cp: ScientificCheckpoint = {
        timeS: ev.simTimeHours * 3600,
        maxDepthM: ev.maxDepthM ?? 0,
        wetFraction: ev.wetFraction ?? 0,
        massM3: ev.massM3 ?? 0,
      };
      engineRef.current?.ingestCheckpoint(cp);
    }).then((u) => {
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
      canvasScratch.current = null;
    };
  }, [setInspectHint, setGeoPreview]);

  // Layer visibility / opacity.
  useEffect(() => {
    const map = mapRef.current;
    if (!map || !readyRef.current) return;

    const byId = Object.fromEntries(layers.map((l) => [l.id, l]));
    const basemap = byId.basemap;
    if (basemap && map.getLayer("osm-raster")) {
      map.setPaintProperty("osm-raster", "raster-opacity", basemap.visible ? basemap.opacity : 0);
    }

    const flood = byId.flood_depth;
    const hazard = byId.flood_hazard;
    const floodOn = !!(flood?.visible || (waterStyle === "hazard" && hazard?.visible));
    setLayerVis(map, FLOOD_RASTER_LAYER, floodOn || !!(hazard?.visible && waterStyle === "hazard"));
    setLayerVis(map, SHORE_LAYER, !!flood?.visible && (waterStyle === "contour" || waterStyle === "depth"));
    if (flood && map.getLayer(FLOOD_RASTER_LAYER)) {
      map.setPaintProperty(FLOOD_RASTER_LAYER, "raster-opacity", flood.opacity);
    }

    setLayerVis(map, PARTICLES_LAYER, !!byId.flood_velocity?.visible);

    const waterways = byId.waterways;
    setLayerVis(map, "waterways-line", !!waterways?.visible);
    if (waterways && map.getLayer("waterways-line")) {
      map.setPaintProperty("waterways-line", "line-opacity", waterways.opacity);
    }

    const gauges = byId.gauges;
    setLayerVis(map, "gauges-circle", !!gauges?.visible);
  }, [layers, waterStyle]);

  // Scrub / water style → seek preview.
  useEffect(() => {
    const engine = engineRef.current;
    const map = mapRef.current;
    if (!engine || !map || !readyRef.current) return;
    engine.setWaterStyle(waterStyle);
    engine.setLowPower(lowPower);
    const art = engine.seek(floodTime);
    ensurePreviewLayers(map, art);
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

  // Play loop — advances normalised time; preview seek interpolates display frames.
  useEffect(() => {
    if (!floodPlaying) return;
    const reduced = !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (reduced) {
      setFloodPlaying(false);
      return;
    }
    let raf = 0;
    let last = performance.now();
    // Full scenario in ~36s wall time (12 h → 36 s ⇒ 1200×).
    const rate = lowPower ? 800 : 1200;
    const durationS = (useStore.getState().geoScenario?.durationHours ?? 12) * 3600;

    const tick = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const cur = useStore.getState().geoFloodTime;
      const next = cur + (dt * rate) / durationS;
      if (next >= 1) {
        setFloodTime(1);
        setFloodPlaying(false);
        return;
      }
      setFloodTime(next);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [floodPlaying, lowPower, setFloodPlaying, setFloodTime]);

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
      tool === "pan" ? "" : tool === "inspect" ? "help" : tool === "measure" ? "crosshair" : "cell";
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
