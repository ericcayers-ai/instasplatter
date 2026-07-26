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

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5 py-2">
      <div className="flex items-center justify-between gap-3">
        <div className="text-xs">{label}</div>
        <div className="flex shrink-0 items-center gap-1.5">{children}</div>
      </div>
      {hint && <div className="text-[10px] leading-snug text-ink-dim">{hint}</div>}
    </div>
  );
}

function AutoNumber({
  value,
  autoValue,
  min,
  max,
  step,
  onChange,
}: {
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
  value,
  options,
  onChange,
}: {
  value: string | null | undefined;
  options: { id: string | null; label: string }[];
  onChange: (v: string | null) => void;
}) {
  return (
    <select
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
  value,
  onChange,
  onLabel = "On",
  offLabel = "Off",
}: {
  value: boolean | null | undefined;
  onChange: (v: boolean | null) => void;
  onLabel?: string;
  offLabel?: string;
}) {
  return (
    <AutoSelect
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
          <Row label="Max frames" hint="Frames used for reconstruction">
            <AutoNumber
              value={settings.maxFrames}
              autoValue={resolved?.maxFrames}
              min={10}
              max={2000}
              step={10}
              onChange={(v) => set({ maxFrames: v })}
            />
          </Row>
          <Row label="Max resolution" hint="Longest image edge in pixels">
            <AutoNumber
              value={settings.maxResolution}
              autoValue={resolved?.maxResolution}
              min={480}
              max={3840}
              step={80}
              onChange={(v) => set({ maxResolution: v })}
            />
          </Row>
          <Row label="Blur rejection" hint="Fraction of blurriest frames dropped">
            <AutoNumber
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
          <Row label="Matcher" hint="Sequential suits video, exhaustive suits unordered photos">
            <AutoSelect
              value={settings.matcher}
              options={[
                { id: "sequential", label: "Sequential" },
                { id: "exhaustive", label: "Exhaustive" },
              ]}
              onChange={(v) => set({ matcher: v })}
            />
          </Row>
          <Row
            label="Live camera tracking"
            hint={
              resolved?.liveInit
                ? "On for the next run. Cameras lock one at a time; the batch solver takes over if tracking fails."
                : "Off by default (Experimental). Register cameras one at a time instead of solving them all first."
            }
          >
            <BoolSelect value={settings.liveInit} onChange={(v) => set({ liveInit: v })} />
          </Row>
          {resolved?.liveInit && (
            <div className="mb-1 text-[10px] leading-snug text-accent">
              Live tracking will run on the next job. Status appears here and in Cameras once frames lock.
            </div>
          )}
          <Row
            label="GPU feature extraction"
            hint={
              hasCuda && colmapCuda
                ? "CUDA COLMAP is ready — SIFT runs on the GPU."
                : hasCuda
                  ? "NVIDIA GPU found, but COLMAP is CPU-only. Click Install engines to upgrade to CUDA COLMAP."
                  : "Requires NVIDIA + working nvidia-smi. Without CUDA, features run on CPU (Brush still uses Vulkan/DX12)."
            }
          >
            <BoolSelect value={settings.siftGpu} onChange={(v) => set({ siftGpu: v })} />
          </Row>
        </CollapsibleSection>

        <CollapsibleSection title="Training" defaultOpen>
          <Row label="Total steps" hint="More steps means a sharper result and a longer run">
            <AutoNumber
              value={settings.totalSteps}
              autoValue={resolved?.totalSteps}
              min={500}
              max={100000}
              step={500}
              onChange={(v) => set({ totalSteps: v })}
            />
          </Row>
          <Row label="Max splats" hint="Hard cap on Gaussian count (VRAM)">
            <AutoNumber
              value={settings.maxSplats}
              autoValue={resolved?.maxSplats}
              min={100000}
              max={20000000}
              step={100000}
              onChange={(v) => set({ maxSplats: v })}
            />
          </Row>
          <Row label="SH degree" hint="Color detail (view-dependent effects)">
            <AutoNumber
              value={settings.shDegree}
              autoValue={resolved?.shDegree}
              min={0}
              max={3}
              step={1}
              onChange={(v) => set({ shDegree: v })}
            />
          </Row>
          <Row label="Refine every" hint="Densification cadence in steps">
            <AutoNumber
              value={settings.refineEvery}
              autoValue={resolved?.refineEvery}
              min={50}
              max={1000}
              step={50}
              onChange={(v) => set({ refineEvery: v })}
            />
          </Row>
          <Row label="SSIM weight" hint="Structural vs. photometric loss balance">
            <AutoNumber
              value={settings.ssimWeight}
              autoValue={resolved?.ssimWeight}
              min={0}
              max={1}
              step={0.05}
              onChange={(v) => set({ ssimWeight: v })}
            />
          </Row>
          <Row
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
              value={settings.trainer ?? null}
              options={[
                { id: "brush", label: "Brush (wgpu)" },
                { id: "gsplat", label: "gsplat (CUDA)" },
              ]}
              onChange={(v) => set({ trainer: v })}
            />
          </Row>
          <Row label="Live update every" hint="Steps between Brush checkpoints">
            <AutoNumber
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
          <Row label="Clean vs. detailed" hint="Higher means stronger floater suppression">
            <button
              type="button"
              onClick={() => set({ strictness: settings.strictness == null ? 0.5 : null })}
              className={`btn px-2 py-0.5 text-[10px] ${settings.strictness == null ? "btn-active" : ""}`}
            >
              Auto
            </button>
            <input
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
          <Row label="Export format" hint="Offered first in the export dialog">
            <AutoSelect
              value={settings.exportFormat ?? null}
              options={[
                { id: "ply", label: "PLY" },
                { id: "splat", label: "Splat" },
                { id: "spz", label: "SPZ" },
              ]}
              onChange={(v) => set({ exportFormat: v })}
            />
          </Row>
          <Row label="Keep intermediates" hint="Keep frames and the COLMAP database on disk">
            <BoolSelect
              value={settings.keepIntermediates}
              onChange={(v) => set({ keepIntermediates: v })}
              onLabel="Keep"
              offLabel="Clean up"
            />
          </Row>
        </CollapsibleSection>

        <CollapsibleSection title="Geospatial" defaultOpen={false}>
          <Row label="Default suite" hint="Preferred suite when the app starts.">
            <AutoSelect
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
          <Row label="gsplat strategy" hint="MCMC (floater-resistant) or Default + AbsGrad.">
            <AutoSelect
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
          <Row label="gsplat antialiased" hint="In-loop mip-style AA. On by default.">
            <BoolSelect value={settings.gsplatAntialiased} onChange={(v) => set({ gsplatAntialiased: v })} />
          </Row>
          <Row label="gsplat appearance" hint="Per-image appearance embeddings. On by default.">
            <BoolSelect value={settings.gsplatAppearance} onChange={(v) => set({ gsplatAppearance: v })} />
          </Row>
          <Row label="gsplat bilateral grid" hint="View-dependent color correction. On by default.">
            <BoolSelect
              value={settings.gsplatBilateralGrid}
              onChange={(v) => set({ gsplatBilateralGrid: v })}
            />
          </Row>
          <Row label="Progressive resolution" hint="Train at reduced resolution first. On by default.">
            <BoolSelect
              value={settings.progressiveResolution}
              onChange={(v) => set({ progressiveResolution: v })}
            />
          </Row>
          <Row label="Mip-Splatting filter" hint="Bound Gaussians to camera sampling rate. On by default.">
            <BoolSelect value={settings.mipFilter} onChange={(v) => set({ mipFilter: v })} />
          </Row>
          <Row label="Dense init" hint="Compose COLMAP MVS with neural densifiers. On by default.">
            <BoolSelect value={settings.denseInit} onChange={(v) => set({ denseInit: v })} />
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
          <Row label="Neural densifiers" hint="AND with MVS / RoMa when present.">
            <BoolSelect value={settings.useNeuralInit} onChange={(v) => set({ useNeuralInit: v })} />
          </Row>
          <Row label="Post polish" hint="NVIDIA Fixer when installed.">
            <BoolSelect value={settings.postPolish} onChange={(v) => set({ postPolish: v })} />
          </Row>
        </CollapsibleSection>
      </div>
    </div>
  );
}
