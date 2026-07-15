import { useEffect, useRef } from "react";
import maplibregl, { type Map as MapLibreMap, type StyleSpecification } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import { useStore } from "../state/store";
import { GEO_MAP_CENTER, GEO_MAP_ZOOM } from "./defaults";
import {
  floodSnapshotFromTime,
  placeholderFloodPolygon,
  placeholderGauges,
  placeholderWaterways,
} from "./floodPreview";
import type { GeoViewMode, GeoWaterStyle } from "./types";

const FLOOD_SRC = "geo-flood-extent";
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

function floodFillColor(style: GeoWaterStyle, hazardClass: number): string {
  if (style === "hazard") {
    const colors = ["#25C6D933", "#F1B84B66", "#E55D7899", "#E55D78CC"];
    return colors[Math.min(3, Math.max(0, hazardClass))];
  }
  if (style === "contour") {
    return "#25C6D922";
  }
  // Depth: darker Hydro Cyan as water deepens via opacity, base colour fixed.
  return "#25C6D9";
}

function ensureSources(map: MapLibreMap) {
  if (!map.getSource(FLOOD_SRC)) {
    map.addSource(FLOOD_SRC, {
      type: "geojson",
      data: placeholderFloodPolygon(0.1),
    });
  }
  if (!map.getSource(WATERWAYS_SRC)) {
    map.addSource(WATERWAYS_SRC, { type: "geojson", data: placeholderWaterways() });
  }
  if (!map.getSource(GAUGES_SRC)) {
    map.addSource(GAUGES_SRC, { type: "geojson", data: placeholderGauges() });
  }

  if (!map.getLayer("flood-fill")) {
    map.addLayer({
      id: "flood-fill",
      type: "fill",
      source: FLOOD_SRC,
      paint: {
        "fill-color": "#25C6D9",
        "fill-opacity": 0.45,
      },
    });
  }
  if (!map.getLayer("flood-outline")) {
    map.addLayer({
      id: "flood-outline",
      type: "line",
      source: FLOOD_SRC,
      paint: {
        "line-color": "#25C6D9",
        "line-width": 1.5,
        "line-opacity": 0.85,
      },
    });
  }
  if (!map.getLayer("flood-contour")) {
    map.addLayer({
      id: "flood-contour",
      type: "line",
      source: FLOOD_SRC,
      layout: { visibility: "none" },
      paint: {
        "line-color": "#F1B84B",
        "line-width": 1,
        "line-dasharray": [2, 2],
        "line-opacity": 0.9,
      },
    });
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

  const layers = useStore((s) => s.geoLayers);
  const floodTime = useStore((s) => s.geoFloodTime);
  const viewMode = useStore((s) => s.geoViewMode);
  const waterStyle = useStore((s) => s.geoWaterStyle);
  const tool = useStore((s) => s.geoTool);
  const setInspectHint = useStore((s) => s.setGeoInspectHint);

  // Init map once.
  useEffect(() => {
    const el = containerRef.current;
    if (!el || mapRef.current) return;

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

    map.on("load", () => {
      ensureSources(map);
      readyRef.current = true;
      const state = useStore.getState();
      const snap = floodSnapshotFromTime(state.geoFloodTime);
      const src = map.getSource(FLOOD_SRC) as maplibregl.GeoJSONSource | undefined;
      src?.setData(placeholderFloodPolygon(snap.wetFraction));
      // Re-apply visibility from the store (effects may have run before load).
      const byId = Object.fromEntries(state.geoLayers.map((l) => [l.id, l]));
      if (byId.basemap && map.getLayer("osm-raster")) {
        map.setPaintProperty(
          "osm-raster",
          "raster-opacity",
          byId.basemap.visible ? byId.basemap.opacity : 0,
        );
      }
      setLayerVis(map, "flood-fill", !!byId.flood_depth?.visible || !!byId.flood_hazard?.visible);
      setLayerVis(map, "flood-outline", !!byId.flood_depth?.visible);
      setLayerVis(
        map,
        "flood-contour",
        state.geoWaterStyle === "contour" && !!byId.flood_depth?.visible,
      );
      setLayerVis(map, "waterways-line", !!byId.waterways?.visible);
      setLayerVis(map, "gauges-circle", !!byId.gauges?.visible);
      if (state.geoViewMode === "3d") {
        map.jumpTo({ pitch: 55, bearing: -18 });
      }
    });

    map.on("click", (e) => {
      const t = useStore.getState().geoTool;
      if (t === "inspect") {
        setInspectHint(
          `Inspect ${e.lngLat.lat.toFixed(5)}, ${e.lngLat.lng.toFixed(5)} — flood sample pending solver`,
        );
      } else if (t === "measure") {
        setInspectHint("Measure: click two points (stub — distance tool not connected yet)");
      } else if (t === "profile") {
        setInspectHint("Profile: draw a section line (stub — cross-section tool not connected yet)");
      }
    });

    mapRef.current = map;
    return () => {
      readyRef.current = false;
      map.remove();
      mapRef.current = null;
    };
  }, [setInspectHint]);

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
    setLayerVis(map, "flood-fill", floodOn || !!(hazard?.visible && waterStyle === "hazard"));
    setLayerVis(map, "flood-outline", floodOn);
    setLayerVis(map, "flood-contour", waterStyle === "contour" && !!flood?.visible);

    const waterways = byId.waterways;
    setLayerVis(map, "waterways-line", !!waterways?.visible);
    if (waterways && map.getLayer("waterways-line")) {
      map.setPaintProperty("waterways-line", "line-opacity", waterways.opacity);
    }

    const gauges = byId.gauges;
    setLayerVis(map, "gauges-circle", !!gauges?.visible);
  }, [layers, waterStyle]);

  // Flood time → geometry + paints.
  useEffect(() => {
    const map = mapRef.current;
    if (!map || !readyRef.current) return;
    const snap = floodSnapshotFromTime(floodTime);
    const src = map.getSource(FLOOD_SRC) as maplibregl.GeoJSONSource | undefined;
    src?.setData(placeholderFloodPolygon(snap.wetFraction));

    const fill = floodFillColor(waterStyle, snap.hazardClass);
    const depthOpacity = Math.min(0.82, 0.22 + snap.wetFraction * 0.55);
    if (map.getLayer("flood-fill")) {
      if (waterStyle === "depth") {
        map.setPaintProperty("flood-fill", "fill-color", "#25C6D9");
        map.setPaintProperty("flood-fill", "fill-opacity", depthOpacity);
      } else if (waterStyle === "hazard") {
        map.setPaintProperty("flood-fill", "fill-color", fill);
        map.setPaintProperty("flood-fill", "fill-opacity", 0.7);
      } else {
        map.setPaintProperty("flood-fill", "fill-color", "#25C6D9");
        map.setPaintProperty("flood-fill", "fill-opacity", 0.18);
      }
    }
    if (map.getLayer("flood-outline")) {
      map.setPaintProperty(
        "flood-outline",
        "line-color",
        waterStyle === "hazard" ? "#E55D78" : "#25C6D9",
      );
    }

    // Drive CSS water mode on the map shell for legend/styling hooks.
    const root = containerRef.current?.closest(".geo-viewport");
    if (root instanceof HTMLElement) {
      root.dataset.waterStyle = waterStyle;
      root.dataset.hazardClass = String(snap.hazardClass);
      root.style.setProperty("--geo-wet", String(snap.wetFraction));
      root.style.setProperty("--geo-depth", String(snap.maxDepthM));
    }
  }, [floodTime, waterStyle]);

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
