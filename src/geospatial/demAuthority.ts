/**
 * Honest Demo / Live preview / Scientific labels from DEM + run state.
 *
 * - Synthetic DEM → Demo
 * - Real or splat-proxy DEM + soft/HAND → Live preview
 * - ANUGA success with a real (non-proxy) DEM → Scientific
 */

export type DemAuthorityLabel = "Demo" | "Live preview" | "Scientific";

export function demAuthorityLabel(opts: {
  scientificMode?: string | null;
  demSynthetic?: boolean | null;
  demBedSource?: string | null;
}): DemAuthorityLabel {
  const synthetic = opts.demSynthetic === true;
  const proxy = opts.demBedSource === "proxy";
  const realBed = opts.demSynthetic === false && !proxy;

  if (opts.scientificMode === "anuga" && realBed) {
    return "Scientific";
  }
  if (opts.scientificMode === "demo" || synthetic) {
    return "Demo";
  }
  return "Live preview";
}

export function isGeoTiffPath(path: string): boolean {
  const lower = path.toLowerCase();
  return (
    lower.endsWith(".tif") ||
    lower.endsWith(".tiff") ||
    lower.endsWith(".geotiff") ||
    lower.endsWith(".cog")
  );
}
