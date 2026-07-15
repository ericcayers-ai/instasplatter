import type { GeoWaterStyle } from "../types";
import { sampleHydrograph } from "../floodPreview";
import { PLACEHOLDER_SCENARIO } from "../defaults";
import { detectPreviewCapabilities } from "./capabilities";
import { compareAgainstCheckpoint, nearestCheckpoint } from "./compare";
import {
  advanceParticles,
  extractShorelineRings,
  particlesToGeoJson,
  rasterizeDepth,
  ringsToLngLat,
  seedParticles,
  type Particle,
} from "./raster";
import {
  buildSyntheticBed,
  cloneState,
  computeStats,
  copyState,
  createEmptyState,
  lerpStates,
  resolveDomain,
  softStep,
} from "./softSolver";
import type {
  PreviewBackend,
  PreviewCapabilities,
  PreviewCompareReport,
  PreviewDisplayFrame,
  PreviewDomain,
  PreviewEngineOptions,
  PreviewForcing,
  PreviewStats,
  PreviewValidationState,
  ScientificCheckpoint,
  GridState,
} from "./types";
import { createGpuAccelerator, type GpuAccelerator } from "./webgpuSolver";

export interface PreviewRenderArtifacts {
  image: ImageData;
  shoreline: GeoJSON.FeatureCollection;
  particles: GeoJSON.FeatureCollection;
  bounds: [number, number, number, number];
  frame: PreviewDisplayFrame;
}

type Listener = (artifacts: PreviewRenderArtifacts) => void;

/**
 * Live flood preview: CFL soft-step physics, display-frame interpolation,
 * scientific checkpoint comparison hooks, and MapLibre-ready rasters.
 */
export class FloodPreviewEngine {
  readonly domain: PreviewDomain;
  readonly capabilities: PreviewCapabilities;
  readonly durationS: number;
  readonly cfl: number;

  private backend: PreviewBackend = "cpu";
  private accelerator: GpuAccelerator | null = null;
  private waterStyle: GeoWaterStyle = "depth";
  private lowPower: boolean;
  private reducedMotion: boolean;

  private state: GridState;
  private prev: GridState;
  private curr: GridState;
  private display: GridState;
  private lastStats: PreviewStats;
  private lastDt = 1;
  private lastCfl = 0.4;

  private checkpoints: ScientificCheckpoint[] = [];
  private lastCompare: PreviewCompareReport | null = null;
  private validation: PreviewValidationState = "live";

  private particles: Particle[] = [];
  private image: ImageData | null = null;
  private listeners = new Set<Listener>();

  /** Keyframe cache for scrub seeks (sparse). */
  private keyframes: GridState[] = [];
  private keyframeEveryS: number;

  private ready = false;
  private initPromise: Promise<void>;

  constructor(opts: PreviewEngineOptions = {}) {
    this.lowPower = !!opts.lowPower;
    this.capabilities = detectPreviewCapabilities({ lowPower: this.lowPower });
    this.reducedMotion = opts.reducedMotion ?? this.capabilities.reducedMotion;
    this.domain = resolveDomain(opts.domain, this.lowPower);
    this.durationS = (opts.durationHours ?? PLACEHOLDER_SCENARIO.durationHours) * 3600;
    this.cfl = opts.cfl ?? (this.lowPower ? 0.25 : 0.4);
    this.waterStyle = opts.waterStyle ?? "depth";
    this.keyframeEveryS = this.lowPower ? 1800 : 900;

    const { cols, rows, dxM } = this.domain;
    this.state = createEmptyState(cols, rows, dxM, 0);
    this.state.z = buildSyntheticBed(cols, rows);
    this.prev = cloneState(this.state);
    this.curr = cloneState(this.state);
    this.display = cloneState(this.state);
    this.lastStats = computeStats(this.state, 0, this.cfl);

    this.initPromise = this.initAccelerator();
  }

  private async initAccelerator(): Promise<void> {
    this.accelerator = await createGpuAccelerator(this.capabilities.preferredBackend);
    this.backend = this.accelerator.kind;
    this.ready = true;
    this.captureKeyframe();
    this.emit();
  }

  whenReady(): Promise<void> {
    return this.initPromise;
  }

  getBackend(): PreviewBackend {
    return this.backend;
  }

  getValidation(): PreviewValidationState {
    return this.validation;
  }

  getDomainBounds(): [number, number, number, number] {
    return this.domain.bounds;
  }

  setWaterStyle(style: GeoWaterStyle): void {
    this.waterStyle = style;
    this.emit();
  }

  setLowPower(on: boolean): void {
    this.lowPower = on;
  }

  setScientificCheckpoints(list: ScientificCheckpoint[]): void {
    this.checkpoints = list.slice().sort((a, b) => a.timeS - b.timeS);
    this.validation = list.length ? "comparing" : "live";
    this.runCompare();
    this.emit();
  }

  /** Inject or clear comparison checkpoints (protocol shared with ANUGA runs). */
  ingestCheckpoint(cp: ScientificCheckpoint): void {
    this.checkpoints.push(cp);
    this.checkpoints.sort((a, b) => a.timeS - b.timeS);
    this.validation = "comparing";
    this.runCompare();
    this.emit();
  }

  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  destroy(): void {
    this.listeners.clear();
    this.accelerator?.destroy();
    this.accelerator = null;
  }

  /**
   * Seek display / physics to normalised scenario time 0–1.
   * Physics steps forward from the nearest keyframe when needed.
   */
  seek(t01: number): PreviewRenderArtifacts {
    const targetS = Math.max(0, Math.min(1, t01)) * this.durationS;
    this.ensureAt(targetS);
    // Display interpolates between prev/curr physics brackets.
    const span = Math.max(1e-6, this.curr.timeS - this.prev.timeS);
    const alpha = Math.max(0, Math.min(1, (targetS - this.prev.timeS) / span));
    lerpStates(this.prev, this.curr, alpha, this.display);
    this.display.timeS = targetS;
    this.lastStats = computeStats(this.display, this.lastDt, this.lastCfl);
    this.runCompare();
    return this.emit();
  }

  /** Advance one display tick while playing (scenario seconds). */
  advancePlay(scenarioDtS: number): PreviewRenderArtifacts {
    const t01 = Math.min(1, (this.display.timeS + scenarioDtS) / this.durationS);
    return this.seek(t01);
  }

  sampleForcing(t01: number): PreviewForcing {
    const { stageM, dischargeCms } = sampleHydrograph(t01);
    const peak = 2.35;
    const intensity = peak > 0 ? Math.min(1.4, stageM / peak) : 0;
    return {
      rainfallMmHr: 2 + intensity * 38,
      inflowCms: Math.max(0, dischargeCms * 0.35),
      manningN: 0.035,
      infiltrationMmHr: 1.5 + (1 - intensity) * 2,
    };
  }

  private captureKeyframe(): void {
    this.keyframes.push(cloneState(this.state));
    // Cap memory.
    if (this.keyframes.length > 48) {
      this.keyframes = this.keyframes.filter((_, i) => i % 2 === 0);
    }
  }

  private nearestKeyframe(targetS: number): GridState {
    let best = this.keyframes[0] ?? this.state;
    let bestD = Math.abs(best.timeS - targetS);
    for (const kf of this.keyframes) {
      if (kf.timeS > targetS + 1e-6) break;
      const d = targetS - kf.timeS;
      if (d >= 0 && d <= bestD) {
        best = kf;
        bestD = d;
      }
    }
    // Prefer a keyframe before the target.
    const before = this.keyframes.filter((k) => k.timeS <= targetS);
    if (before.length) return before[before.length - 1];
    return best;
  }

  private ensureAt(targetS: number): void {
    if (targetS < this.state.timeS - 1e-3) {
      const kf = this.nearestKeyframe(targetS);
      copyState(this.state, kf);
    }

    const maxDt = this.lowPower ? 90 : 45;
    let guard = 8000;
    let sinceKeyframe = this.state.timeS % this.keyframeEveryS;

    while (this.state.timeS < targetS - 1e-6 && guard-- > 0) {
      copyState(this.prev, this.curr);
      const t01 = this.state.timeS / this.durationS;
      const forcing = this.sampleForcing(t01);
      const remain = targetS - this.state.timeS;
      const { dtS, cfl } = softStep(this.state, forcing, this.cfl, Math.min(maxDt, remain));
      this.lastDt = dtS;
      this.lastCfl = cfl;
      copyState(this.curr, this.state);
      sinceKeyframe += dtS;
      if (sinceKeyframe >= this.keyframeEveryS) {
        this.captureKeyframe();
        sinceKeyframe = 0;
      }
    }

    // Bracket for display interpolation when we overshoot slightly.
    if (this.curr.timeS < this.prev.timeS) {
      copyState(this.prev, this.curr);
    }
  }

  private runCompare(): void {
    if (!this.checkpoints.length) {
      this.validation = "live";
      this.lastCompare = null;
      return;
    }
    const cp = nearestCheckpoint(this.checkpoints, this.display.timeS);
    if (!cp) {
      this.validation = "live";
      this.lastCompare = null;
      return;
    }
    this.lastCompare = compareAgainstCheckpoint(
      {
        maxDepthM: this.lastStats.maxDepthM,
        wetFraction: this.lastStats.wetFraction,
        massM3: this.lastStats.massM3,
        h: this.display.h,
        cols: this.display.cols,
        rows: this.display.rows,
      },
      cp,
    );
    this.validation = this.lastCompare.withinTolerance ? "validated" : "diverged";
    // Stay in comparing until within a reasonable time window of a checkpoint.
    if (Math.abs(cp.timeS - this.display.timeS) > 1800) {
      this.validation = "comparing";
    }
  }

  private buildArtifacts(): PreviewRenderArtifacts {
    const maxRef = Math.max(0.5, this.lastStats.maxDepthM, 1.2);
    this.image = rasterizeDepth(
      this.display.h,
      this.display.cols,
      this.display.rows,
      this.waterStyle,
      maxRef,
      this.image ?? undefined,
    );

    const rings = extractShorelineRings(this.display.h, this.display.cols, this.display.rows);
    const shoreline = ringsToLngLat(rings, this.domain.bounds);

    const wantParticles =
      !this.reducedMotion &&
      !this.lowPower &&
      this.waterStyle !== "contour";
    if (wantParticles) {
      const budget = this.backend === "cpu" ? 48 : 96;
      this.particles = seedParticles(
        this.display.h,
        this.display.u,
        this.display.v,
        this.display.cols,
        this.display.rows,
        budget,
        this.particles,
      );
      // Particle advection uses a small display-scaled step.
      this.particles = advanceParticles(
        this.particles,
        this.display.h,
        this.display.u,
        this.display.v,
        this.display.cols,
        this.display.rows,
        Math.min(120, this.lastDt),
        this.display.dx,
      );
    } else {
      this.particles = [];
    }
    const particles = particlesToGeoJson(this.particles, this.domain.bounds);

    const span = Math.max(1e-6, this.curr.timeS - this.prev.timeS);
    const alpha = Math.max(0, Math.min(1, (this.display.timeS - this.prev.timeS) / span));

    const frame: PreviewDisplayFrame = {
      h: this.display.h,
      u: this.display.u,
      v: this.display.v,
      cols: this.display.cols,
      rows: this.display.rows,
      alpha,
      stats: this.lastStats,
      backend: this.backend,
      validation: this.validation,
      compare: this.lastCompare,
    };

    return {
      image: this.image,
      shoreline,
      particles,
      bounds: this.domain.bounds,
      frame,
    };
  }

  private emit(): PreviewRenderArtifacts {
    const artifacts = this.buildArtifacts();
    for (const fn of this.listeners) fn(artifacts);
    return artifacts;
  }
}

export function validationBadgeLabel(v: PreviewValidationState): string {
  switch (v) {
    case "validated":
      return "Validated vs ANUGA";
    case "diverged":
      return "Preview diverged";
    case "comparing":
      return "Comparing…";
    default:
      return "Live preview";
  }
}
