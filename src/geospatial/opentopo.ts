/**
 * OpenTopography API key persistence (mirrors Cesium ion token pattern).
 * Used when fetching GlobalDEM COP30 clips for flood DEM staging.
 */

export const OPENTOPO_API_KEY_STORAGE_KEY = "instasplatter.openTopoApiKey";

export function readStoredOpenTopoApiKey(): string | null {
  try {
    const raw = localStorage.getItem(OPENTOPO_API_KEY_STORAGE_KEY);
    const key = raw?.trim();
    return key ? key : null;
  } catch {
    return null;
  }
}

export function writeStoredOpenTopoApiKey(key: string | null): void {
  try {
    if (!key?.trim()) {
      localStorage.removeItem(OPENTOPO_API_KEY_STORAGE_KEY);
    } else {
      localStorage.setItem(OPENTOPO_API_KEY_STORAGE_KEY, key.trim());
    }
  } catch {
    // ignore quota / private mode
  }
}
