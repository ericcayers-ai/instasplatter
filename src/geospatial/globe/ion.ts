/**
 * Cesium ion policy for InstaSplatter Standard vs Experimental paths.
 *
 * Standard: blank token, no geocoder, no ion base layers / World Terrain —
 * assert zero traffic to api.cesium.com.
 * Experimental: optional user-supplied token may enable World Terrain only.
 */

import { Ion } from "cesium";

export const CESIUM_ION_TOKEN_STORAGE_KEY = "instasplatter.cesiumIonToken";

/** Blank the library default evaluation token before any Viewer is created. */
export function blankCesiumIon(): void {
  Ion.defaultAccessToken = "";
}

/**
 * Read a user ion token from localStorage. Caller must gate on Experimental mode
 * before using this for World Terrain.
 */
export function readStoredIonToken(): string | null {
  try {
    const raw = localStorage.getItem(CESIUM_ION_TOKEN_STORAGE_KEY);
    const token = raw?.trim();
    return token ? token : null;
  } catch {
    return null;
  }
}

export function writeStoredIonToken(token: string | null): void {
  try {
    if (!token?.trim()) {
      localStorage.removeItem(CESIUM_ION_TOKEN_STORAGE_KEY);
    } else {
      localStorage.setItem(CESIUM_ION_TOKEN_STORAGE_KEY, token.trim());
    }
  } catch {
    // ignore quota / private mode
  }
}

/**
 * Apply ion policy: always blank first; only restore a user token when
 * Experimental is on and a token is stored.
 */
export function applyCesiumIonPolicy(experimental: boolean): {
  ionEnabled: boolean;
  token: string | null;
} {
  blankCesiumIon();
  if (!experimental) {
    return { ionEnabled: false, token: null };
  }
  const token = readStoredIonToken();
  if (token) {
    Ion.defaultAccessToken = token;
    return { ionEnabled: true, token };
  }
  return { ionEnabled: false, token: null };
}
