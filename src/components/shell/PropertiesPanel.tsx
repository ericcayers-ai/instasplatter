import type { ReactNode } from "react";
import { useStore } from "../../state/store";
import type { PresetName, Settings } from "../../lib/ipc";
import { applyPresetPatch, resetAllSettingsPatch } from "../../lib/presets";
import CollapsibleSection from "./CollapsibleSection";

const PRESETS: { id: PresetName | null; label: string }[] = [
  { id: null, label: "Auto" },
  { id: "draft", label: "Draft" },
  { id: "eco", label: "Eco" },
  { id: "balanced", label: "Balanced" },
  { id: "high", label: "High" },
  { id: "max", label: "Max" },
];

function Row({
  label,
  controlId,
  hint,
  children,
}: {
  label: string;
  controlId: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5 py-2">
      <div className="flex items-center justify-between gap-3">
        <label htmlFor={controlId} className="text-xs">
          {label}
        </label>
        <div className="flex shrink-0 items-center gap-1.5">{children}</div>
      </div>
      {hint && <div className="text-[10px] leading-snug text-ink-dim">{hint}</div>}
    </div>
  );
}

function AutoNumber({
  id,
  value,
  autoValue,
  min,
  max,
  step,
  onChange,
}: {
  id: string;
  value: number | null | undefined;
  autoValue: number | undefined;
  min: number;
  max: number;
  step: number;
  onChange: (v: number | null) => void;
}) {
  const isAuto = value == null;
  return (
    <>
      <button
        type="button"
        onClick={() => onChange(isAuto ? (autoValue ?? min) : null)}
        className={`btn px-2 py-0.5 text-[10px] ${isAuto ? "btn-active" : ""}`}
      >
        Auto
      </button>
      <input
        id={id}
        type="number"
        disabled={isAuto}
        min={min}
        max={max}
        step={step}
        value={isAuto ? (autoValue ?? "") : value}
        onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}
        className={`w-20 px-2 py-0.5 text-right text-xs tabular-nums outline-none ${isAuto ? "opacity-40" : ""}`}
      />
    </>
  );
}

function AutoSelect({
  id,
  value,
  options,
  onChange,
}: {
  id: string;
  value: string | null | undefined;
  options: { id: string | null; label: string }[];
  onChange: (v: string | null) => void;
}) {
  return (
    <select
      id={id}
      value={value ?? "__auto__"}
      onChange={(e) => onChange(e.target.value === "__auto__" ? null : e.target.value)}
      className="px-2 py-0.5 text-xs outline-none"
    >
      <option value="__auto__">Auto</option>
      {options
        .filter((o) => o.id !== null)
        .map((o) => (
          <option key={o.id} value={o.id!}>
            {o.label}
          </option>
        ))}
    </select>
  );
}

function BoolSelect({
  id,
  value,
  onChange,
  onLabel = "On",
  offLabel = "Off",
}: {
  id: string;
  value: boolean | null | undefined;
  onChange: (v: boolean | null) => void;
  onLabel?: string;
  offLabel?: string;
}) {
  return (
    <AutoSelect
      id={id}
      value={value == null ? null : value ? "on" : "off"}
      options={[
        { id: "on", label: onLabel },
        { id: "off", label: offLabel },
      ]}
      onChange={(v) => onChange(v == null ? null : v === "on")}
    />
  );
}

/** Reconstruction settings body — nest under left RECONSTRUCTION dock. */
export function ReconstructionSettings() {
  const settings = useStore((s) => s.settings);
  const resolved = useStore((s) => s.resolved);
  const profile = useStore((s) => s.profile);
  const engineStatus = useStore((s) => s.engineStatus);
  const jobSettingsSnapshot = useStore((s) => s.jobSettingsSnapshot);
  const screen = useStore((s) => s.screen);
  const resultPath = useStore((s) => s.resultPath);
  const jobError = useStore((s) => s.jobError);
  const updateSettings = useStore((s) => s.updateSettings);
  const experimentalOn = !!(resolved?.experimentalMode);

  const set = (patch: Partial<Settings>) => void updateSettings(patch);

  const running = screen === "processing" && !resultPath && !jobError;
  const changedSinceStart =
    running &&
    jobSettingsSnapshot &&
    resolved &&
    JSON.stringify(jobSettingsSnapshot) !== JSON.stringify(resolved);

  const hasCuda = !!profile?.has_cuda;
  const colmapCuda = !!engineStatus?.colmapCuda;

  return (
    <div className="flex flex-col">
      <div className="mb-2 flex items-center justify-between gap-2">
        <span className="text-[10px] text-ink-dim">
          Knobs <span className="kbd">⌃,</span>
        </span>
        <button
          type="button"
          onClick={() => set(resetAllSettingsPatch())}
          className="btn btn-ghost-teal px-1.5 py-0.5 text-[10px]"
        >
          Reset all
        </button>
      </div>

      {changedSinceStart && (
        <div className="mb-2 rounded border border-accent2/30 bg-accent2/10 px-2 py-1.5 text-[10px] text-ink-dim">
          This run already started. Changes apply the next time you start or resume.
        </div>
      )}

      <div className="mb-2 grid grid-cols-3 gap-1">
        {PRESETS.map((p) => {
          const active = (settings.preset ?? null) === p.id;
          const resolvedMatches =
            p.id === null
              ? settings.preset == null
              : resolved?.preset === p.id && settings.preset === p.id;
          return (
            <button
              key={p.label}
              type="button"
              onClick={() => set(applyPresetPatch(p.id))}
              className={`btn justify-center py-1 text-[10px] ${active ? "btn-active" : "btn-ghost-teal"}`}
            >
              {p.label}
              {p.id === null && resolved && (
                <span className="ml-0.5 text-accent">({resolved.preset})</span>
              )}
              {p.id !== null && active && resolved && !resolvedMatches && (
                <span className="ml-0.5 text-accent">→{resolved.preset}</span>
              )}
            </button>
          );
        })}
      </div>

      {experimentalOn && (
        <div className="mb-2 rounded border border-danger/30 bg-danger/10 px-2 py-1.5 text-[10px] leading-snug text-danger/90">
          Experimental Mode floors quality to Max. Your preset choice is kept; Auto knobs follow Max until you leave Experimental.
        </div>
      )}

      {!experimentalOn && settings.preset != null && resolved && settings.preset !== resolved.preset && (
        <div className="mb-2 rounded border border-accent/30 bg-accent/10 px-2 py-1.5 text-[10px] leading-snug text-ink-dim">
          Preset {settings.preset} resolves as <span className="text-ink">{resolved.preset}</span> with the installed neural stack.
        </div>
      )}

      <div className="-mx-3">
        <CollapsibleSection title="Input" defaultOpen>
          <Row controlId="setting-max-frames" label="Max frames" hint="Frames used for reconstruction">
            <AutoNumber
              id="setting-max-frames"
              value={settings.maxFrames}
              autoValue={resolved?.maxFrames}
              min={10}
              max={2000}
              step={10}
              onChange={(v) => set({ maxFrames: v })}
            />
          </Row>
          <Row controlId="setting-max-resolution" label="Max resolution" hint="Longest image edge in pixels">
            <AutoNumber
              id="setting-max-resolution"
              value={settings.maxResolution}
              autoValue={resolved?.maxResolution}
              min={480}
              max={3840}
              step={80}
              onChange={(v) => set({ maxResolution: v })}
            />
          </Row>
          <Row controlId="setting-blur-rejection" label="Blur rejection" hint="Fraction of blurriest frames dropped">
            <AutoNumber
              id="setting-blur-rejection"
              value={settings.blurRejectFraction}
              autoValue={resolved?.blurRejectFraction}
              min={0}
              max={0.9}
              step={0.05}
              onChange={(v) => set({ blurRejectFraction: v })}
            />
          </Row>
        </CollapsibleSection>

        <CollapsibleSection title="Cameras" defaultOpen>
          <Row controlId="setting-matcher" label="Matcher" hint="Sequential suits video, exhaustive suits unordered photos">
            <AutoSelect
              id="setting-matcher"
              value={settings.matcher}
              options={[
                { id: "sequential", label: "Sequential" },
                { id: "exhaustive", label: "Exhaustive" },
              ]}
              onChange={(v) => set({ matcher: v })}
            />
          </Row>
          <Row
            controlId="setting-live-camera-tracking"
            label="Live camera tracking"
            hint={
              resolved?.liveInit
                ? "On for the next run. Cameras lock one at a time; the batch solver takes over if tracking fails."
                : "Off by default (Experimental). Register cameras one at a time instead of solving them all first."
            }
          >
            <BoolSelect id="setting-live-camera-tracking" value={settings.liveInit} onChange={(v) => set({ liveInit: v })} />
          </Row>
          {resolved?.liveInit && (
            <div className="mb-1 text-[10px] leading-snug text-accent">
              Live tracking will run on the next job. Status appears here and in Cameras once frames lock.
            </div>
          )}
          <Row
            controlId="setting-gpu-feature-extraction"
            label="GPU feature extraction"
            hint={
              hasCuda && colmapCuda
                ? "CUDA COLMAP is ready — SIFT runs on the GPU."
                : hasCuda
                  ? "NVIDIA GPU found, but COLMAP is CPU-only. Click Install engines to upgrade to CUDA COLMAP."
                  : "Requires NVIDIA + working nvidia-smi. Without CUDA, features run on CPU (Brush still uses Vulkan/DX12)."
            }
          >
            <BoolSelect id="setting-gpu-feature-extraction" value={settings.siftGpu} onChange={(v) => set({ siftGpu: v })} />
          </Row>
        </CollapsibleSection>

        <CollapsibleSection title="Training" defaultOpen>
          <Row controlId="setting-total-steps" label="Total steps" hint="More steps means a sharper result and a longer run">
            <AutoNumber
              id="setting-total-steps"
              value={settings.totalSteps}
              autoValue={resolved?.totalSteps}
              min={500}
              max={100000}
              step={500}
              onChange={(v) => set({ totalSteps: v })}
            />
          </Row>
          <Row controlId="setting-max-splats" label="Max splats" hint="Hard cap on Gaussian count (VRAM)">
            <AutoNumber
              id="setting-max-splats"
              value={settings.maxSplats}
              autoValue={resolved?.maxSplats}
              min={100000}
              max={20000000}
              step={100000}
              onChange={(v) => set({ maxSplats: v })}
            />
          </Row>
          <Row controlId="setting-sh-degree" label="SH degree" hint="Color detail (view-dependent effects)">
            <AutoNumber
              id="setting-sh-degree"
              value={settings.shDegree}
              autoValue={resolved?.shDegree}
              min={0}
              max={3}
              step={1}
              onChange={(v) => set({ shDegree: v })}
            />
          </Row>
          <Row controlId="setting-refine-every" label="Refine every" hint="Densification cadence in steps">
            <AutoNumber
              id="setting-refine-every"
              value={settings.refineEvery}
              autoValue={resolved?.refineEvery}
              min={50}
              max={1000}
              step={50}
              onChange={(v) => set({ refineEvery: v })}
            />
          </Row>
          <Row controlId="setting-ssim-weight" label="SSIM weight" hint="Structural vs. photometric loss balance">
            <AutoNumber
              id="setting-ssim-weight"
              value={settings.ssimWeight}
              autoValue={resolved?.ssimWeight}
              min={0}
              max={1}
              step={0.05}
              onChange={(v) => set({ ssimWeight: v })}
            />
          </Row>
          <Row
            controlId="setting-trainer"
            label="Trainer"
            hint={
              hasCuda
                ? engineStatus?.gsplat
                  ? "Auto prefers gsplat (CUDA) when installed; Brush is the portable wgpu fallback."
                  : "CUDA GPU ready. Install gsplat-train sidecar for the CUDA trainer, or keep Auto → Brush (wgpu)."
                : "No CUDA — Auto uses Brush (Vulkan/Metal/DX12). gsplat requires NVIDIA + the sidecar."
            }
          >
            <AutoSelect
              id="setting-trainer"
              value={settings.trainer ?? null}
              options={[
                { id: "brush", label: "Brush (wgpu)" },
                { id: "gsplat", label: "gsplat (CUDA)" },
              ]}
              onChange={(v) => set({ trainer: v })}
            />
          </Row>
          <Row controlId="setting-live-update-every" label="Live update every" hint="Steps between Brush checkpoints">
            <AutoNumber
              id="setting-live-update-every"
              value={settings.exportEvery}
              autoValue={resolved?.exportEvery}
              min={100}
              max={5000}
              step={100}
              onChange={(v) => set({ exportEvery: v })}
            />
          </Row>
        </CollapsibleSection>

        <CollapsibleSection title="Cleanliness" defaultOpen={false}>
          <Row controlId="setting-clean-vs-detailed" label="Clean vs. detailed" hint="Higher means stronger floater suppression">
            <button
              type="button"
              onClick={() => set({ strictness: settings.strictness == null ? 0.5 : null })}
              className={`btn px-2 py-0.5 text-[10px] ${settings.strictness == null ? "btn-active" : ""}`}
            >
              Auto
            </button>
            <input
              id="setting-clean-vs-detailed"
              type="range"
              min={0}
              max={1}
              step={0.05}
              disabled={settings.strictness == null}
              value={settings.strictness ?? resolved?.strictness ?? 0.5}
              onChange={(e) => set({ strictness: Number(e.target.value) })}
              className={`w-28 ${settings.strictness == null ? "opacity-40" : ""}`}
            />
          </Row>
        </CollapsibleSection>

        <CollapsibleSection title="Output" defaultOpen={false}>
          <Row controlId="setting-export-format" label="Export format" hint="Offered first in the export dialog">
            <AutoSelect
              id="setting-export-format"
              value={settings.exportFormat ?? null}
              options={[
                { id: "ply", label: "PLY" },
                { id: "splat", label: "Splat" },
                { id: "spz", label: "SPZ" },
              ]}
              onChange={(v) => set({ exportFormat: v })}
            />
          </Row>
          <Row controlId="setting-keep-intermediates" label="Keep intermediates" hint="Keep frames and the COLMAP database on disk">
            <BoolSelect
              id="setting-keep-intermediates"
              value={settings.keepIntermediates}
              onChange={(v) => set({ keepIntermediates: v })}
              onLabel="Keep"
              offLabel="Clean up"
            />
          </Row>
        </CollapsibleSection>

        <CollapsibleSection title="Geospatial" defaultOpen={false}>
          <Row controlId="setting-default-suite" label="Default suite" hint="Preferred suite when the app starts.">
            <AutoSelect
              id="setting-default-suite"
              value={settings.defaultSuite ?? null}
              options={[
                { id: "reconstruction", label: "Reconstruction" },
                { id: "geospatial", label: "Geospatial" },
              ]}
              onChange={(v) => set({ defaultSuite: v as "reconstruction" | "geospatial" | null })}
            />
          </Row>
          <div className="py-2 text-[10px] leading-snug text-ink-dim">
            AOI, basemap, and flood controls live in the Geospatial suite right dock.
          </div>
        </CollapsibleSection>

        <CollapsibleSection title="Advanced" defaultOpen={false}>
          <Row controlId="setting-gsplat-strategy" label="gsplat strategy" hint="MCMC (floater-resistant) or Default + AbsGrad.">
            <AutoSelect
              id="setting-gsplat-strategy"
              value={settings.gsplatStrategy ?? null}
              options={[
                { id: "mcmc", label: "MCMC" },
                { id: "default", label: "Default + AbsGrad" },
              ]}
              onChange={(v) =>
                set({
                  gsplatStrategy: v,
                  gsplatAbsgrad: v === "default" ? true : false,
                })
              }
            />
          </Row>
          <Row controlId="setting-gsplat-antialiased" label="gsplat antialiased" hint="In-loop mip-style AA. On by default.">
            <BoolSelect id="setting-gsplat-antialiased" value={settings.gsplatAntialiased} onChange={(v) => set({ gsplatAntialiased: v })} />
          </Row>
          <Row controlId="setting-gsplat-appearance" label="gsplat appearance" hint="Per-image appearance embeddings. On by default.">
            <BoolSelect id="setting-gsplat-appearance" value={settings.gsplatAppearance} onChange={(v) => set({ gsplatAppearance: v })} />
          </Row>
          <Row controlId="setting-gsplat-bilateral-grid" label="gsplat bilateral grid" hint="View-dependent color correction. On by default.">
            <BoolSelect
              id="setting-gsplat-bilateral-grid"
              value={settings.gsplatBilateralGrid}
              onChange={(v) => set({ gsplatBilateralGrid: v })}
            />
          </Row>
          <Row controlId="setting-progressive-resolution" label="Progressive resolution" hint="Train at reduced resolution first. On by default.">
            <BoolSelect
              id="setting-progressive-resolution"
              value={settings.progressiveResolution}
              onChange={(v) => set({ progressiveResolution: v })}
            />
          </Row>
          <Row controlId="setting-mip-splatting-filter" label="Mip-Splatting filter" hint="Bound Gaussians to camera sampling rate. On by default.">
            <BoolSelect id="setting-mip-splatting-filter" value={settings.mipFilter} onChange={(v) => set({ mipFilter: v })} />
          </Row>
          <Row controlId="setting-dense-init" label="Dense init" hint="Compose COLMAP MVS with neural densifiers. On by default.">
            <BoolSelect id="setting-dense-init" value={settings.denseInit} onChange={(v) => set({ denseInit: v })} />
          </Row>
        </CollapsibleSection>

        <CollapsibleSection
          title="Experimental engines"
          defaultOpen={false}
          tone="danger"
          badge={experimentalOn ? "ON" : "off"}
        >
          <div className="py-2 text-[10px] leading-snug text-ink-dim">
            Toggle Experimental Mode from the TitleBar (requires a one-time NC license ack).
          </div>
          <Row controlId="setting-neural-densifiers" label="Neural densifiers" hint="AND with MVS / RoMa when present.">
            <BoolSelect id="setting-neural-densifiers" value={settings.useNeuralInit} onChange={(v) => set({ useNeuralInit: v })} />
          </Row>
          <Row controlId="setting-post-polish" label="Post polish" hint="NVIDIA Fixer when installed.">
            <BoolSelect id="setting-post-polish" value={settings.postPolish} onChange={(v) => set({ postPolish: v })} />
          </Row>
        </CollapsibleSection>
      </div>
    </div>
  );
}
