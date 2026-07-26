import type { PresetName, Settings } from "./ipc";

/** Knobs a quality preset owns. Cleared to Auto when the user picks a preset so
 * the UI and pipeline re-resolve from that preset instead of keeping stale
 * manual overrides. */
export const PRESET_OWNED_KEYS = [
  "maxFrames",
  "maxResolution",
  "totalSteps",
  "maxSplats",
  "shDegree",
  "exportEvery",
  "refineEvery",
] as const satisfies readonly (keyof Settings)[];

export type PresetOwnedKey = (typeof PRESET_OWNED_KEYS)[number];

/**
 * Patch that applies a quality preset and clears every knob the preset expands
 * into. Advanced / Experimental flags are left alone.
 */
export function applyPresetPatch(preset: PresetName | null): Partial<Settings> {
  const patch: Partial<Settings> = { preset };
  for (const key of PRESET_OWNED_KEYS) {
    patch[key] = null;
  }
  return patch;
}

/** Fields Reset-all clears — kept in one place so preset apply stays narrower. */
export function resetAllSettingsPatch(): Partial<Settings> {
  return {
    ...applyPresetPatch(null),
    blurRejectFraction: null,
    matcher: null,
    siftGpu: null,
    ssimWeight: null,
    strictness: null,
    keepIntermediates: null,
    progressiveResolution: null,
    mipFilter: null,
    liveInit: null,
    denseInit: null,
    useNeuralInit: null,
    allowResearchSidecars: null,
    experimentalMode: null,
    experimentalLicenseAcked: null,
    postPolish: null,
    trainer: null,
    gsplatStrategy: null,
    gsplatAbsgrad: null,
    gsplatAntialiased: null,
    gsplatAppearance: null,
    gsplatBilateralGrid: null,
    exportFormat: null,
    defaultSuite: null,
  };
}
