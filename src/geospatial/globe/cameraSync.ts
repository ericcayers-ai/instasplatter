/**
 * Lightweight camera / AOI look-at bridge between MapLibre and Cesium.
 * Shared truth is AOI bbox; optional last look-at helps when switching views.
 */

import type { Viewer } from "cesium";
import { Cartesian3, Math as CesiumMath, Rectangle } from "cesium";
import { aoiCenter, aoiIsValid, type AoiWgs84 } from "../aoi";
import { GEO_MAP_CENTER, GEO_MAP_ZOOM } from "../defaults";

export type SharedGeoLookAt = {
  longitude: number;
  latitude: number;
  /** Camera height above ellipsoid (m); approximate from MapLibre zoom when set. */
  heightM: number;
};

let lastLookAt: SharedGeoLookAt | null = null;

export function setSharedGeoLookAt(pose: SharedGeoLookAt | null): void {
  lastLookAt = pose;
}

export function getSharedGeoLookAt(): SharedGeoLookAt | null {
  return lastLookAt;
}

/** Rough MapLibre zoom → Cesium camera height (metres). */
export function heightFromMapZoom(zoom: number, latDeg: number): number {
  const metresPerPixel =
    (156543.03392 * Math.cos((latDeg * Math.PI) / 180)) / Math.pow(2, zoom);
  // Assume ~960px viewport height; frame a few screenfuls of ground.
  return Math.max(800, metresPerPixel * 960 * 1.2);
}

export function lookAtFromAoi(aoi: AoiWgs84): SharedGeoLookAt {
  const [lon, lat] = aoiCenter(aoi);
  const span = Math.max(aoi[2] - aoi[0], aoi[3] - aoi[1]);
  // Heuristic height so the AOI fills most of the view.
  const heightM = Math.max(2_000, span * 111_320 * 1.8);
  return { longitude: lon, latitude: lat, heightM };
}

/** Fly Cesium camera to AOI rectangle, else last look-at, else world cold-start. */
export function flyGlobeToContext(
  viewer: Viewer,
  aoi: AoiWgs84 | null | undefined,
  opts?: { duration?: number },
): void {
  const duration = opts?.duration ?? 0.6;
  if (aoiIsValid(aoi)) {
    const rect = Rectangle.fromDegrees(aoi[0], aoi[1], aoi[2], aoi[3]);
    void viewer.camera.flyTo({
      destination: rect,
      duration,
    });
    setSharedGeoLookAt(lookAtFromAoi(aoi));
    return;
  }

  const pose = lastLookAt;
  if (pose) {
    viewer.camera.flyTo({
      destination: Cartesian3.fromDegrees(pose.longitude, pose.latitude, pose.heightM),
      duration,
    });
    return;
  }

  const [lon, lat] = GEO_MAP_CENTER;
  viewer.camera.setView({
    destination: Cartesian3.fromDegrees(
      lon,
      lat,
      heightFromMapZoom(GEO_MAP_ZOOM, lat),
    ),
    orientation: {
      heading: 0,
      pitch: CesiumMath.toRadians(-45),
      roll: 0,
    },
  });
}

/** Capture current Cesium camera as a shared look-at (call before unmount). */
export function captureGlobeLookAt(viewer: Viewer): void {
  const carto = viewer.camera.positionCartographic;
  setSharedGeoLookAt({
    longitude: CesiumMath.toDegrees(carto.longitude),
    latitude: CesiumMath.toDegrees(carto.latitude),
    heightM: carto.height,
  });
}
