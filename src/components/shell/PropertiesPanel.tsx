import { useStore } from "../../state/store";
import type { Settings } from "../../lib/ipc";

const PRESETS = [
  { id: null, label: "Auto" },
  { id: "draft", label: "Draft" },
  { id: "eco", label: "Eco" },
  { id: "balanced", label: "Balanced" },
  { id: "high", label: "High" },
  { id: "max", label: "Max" },
];

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
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

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="border-b border-edge px-3 py-3">
      <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">{title}</div>
      <div className="divide-y divide-edge/60">{children}</div>
    </div>
  );
}

export default function PropertiesPanel() {
  const open = useStore((s) => s.rightPanelOpen);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);
  const settings = useStore((s) => s.settings);
  const resolved = useStore((s) => s.resolved);
  const jobSettingsSnapshot = useStore((s) => s.jobSettingsSnapshot);
  const screen = useStore((s) => s.screen);
  const resultPath = useStore((s) => s.resultPath);
  const jobError = useStore((s) => s.jobError);
  const updateSettings = useStore((s) => s.updateSettings);

  if (!open) return null;
  const set = (patch: Partial<Settings>) => void updateSettings(patch);

  const running = screen === "processing" && !resultPath && !jobError;
  const changedSinceStart =
    running &&
    jobSettingsSnapshot &&
    resolved &&
    JSON.stringify(jobSettingsSnapshot) !== JSON.stringify(resolved);

  return (
    <div className="flex w-72 shrink-0 flex-col overflow-y-auto border-l border-edge bg-panel">
      <div className="flex items-center justify-between border-b border-edge px-3 py-2">
        <div className="text-xs font-semibold">Settings</div>
        <div className="flex items-center gap-2">
          <button
            onClick={() =>
              set({
                preset: null, maxFrames: null, maxResolution: null,
                blurRejectFraction: null, matcher: null, siftGpu: null,
                totalSteps: null, maxSplats: null, shDegree: null,
                refineEvery: null, ssimWeight: null, exportEvery: null,
                strictness: null, keepIntermediates: null,
                progressiveResolution: null, mipFilter: null, liveInit: null,
                denseInit: null, useNeuralInit: null, allowResearchSidecars: null,
                postPolish: null,
                exportFormat: null,
              })
            }
            className="text-[10px] text-ink-dim hover:text-ink"
          >
            Reset all
          </button>
          <button onClick={() => setRightPanelOpen(false)} className="btn px-1.5 py-0.5 text-[10px]">
            ✕
          </button>
        </div>
      </div>

      {changedSinceStart && (
        <div className="border-b border-edge bg-accent2/10 px-3 py-2 text-[10px] text-ink-dim">
          This run already started. Changes below apply the next time you start or resume a job.
        </div>
      )}

      <div className="grid grid-cols-3 gap-1.5 p-3">
        {PRESETS.map((p) => {
          const active = (settings.preset ?? null) === p.id;
          return (
            <button
              key={p.label}
              onClick={() => set({ preset: p.id })}
              className={`btn justify-center py-1.5 text-[11px] ${active ? "btn-active" : ""}`}
            >
              {p.label}
              {p.id === null && resolved && <span className="ml-1 text-accent">({resolved.preset})</span>}
            </button>
          );
        })}
      </div>

      <Section title="Input">
        <Row label="Max frames" hint="Frames used for reconstruction">
          <AutoNumber value={settings.maxFrames} autoValue={resolved?.maxFrames} min={10} max={2000} step={10} onChange={(v) => set({ maxFrames: v })} />
        </Row>
        <Row label="Max resolution" hint="Longest image edge in pixels">
          <AutoNumber value={settings.maxResolution} autoValue={resolved?.maxResolution} min={480} max={3840} step={80} onChange={(v) => set({ maxResolution: v })} />
        </Row>
        <Row label="Blur rejection" hint="Fraction of blurriest frames dropped">
          <AutoNumber value={settings.blurRejectFraction} autoValue={resolved?.blurRejectFraction} min={0} max={0.9} step={0.05} onChange={(v) => set({ blurRejectFraction: v })} />
        </Row>
      </Section>

      <Section title="Camera solving">
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
          hint="Register cameras one at a time instead of solving them all first. Falls back to the batch solver if it loses confidence."
        >
          <BoolSelect value={settings.liveInit} onChange={(v) => set({ liveInit: v })} />
        </Row>
        <Row label="GPU feature extraction" hint="Requires NVIDIA CUDA">
          <BoolSelect value={settings.siftGpu} onChange={(v) => set({ siftGpu: v })} />
        </Row>
      </Section>

      <Section title="Training">
        <Row label="Total steps" hint="More steps means a sharper result and a longer run">
          <AutoNumber value={settings.totalSteps} autoValue={resolved?.totalSteps} min={500} max={100000} step={500} onChange={(v) => set({ totalSteps: v })} />
        </Row>
        <Row label="Max splats" hint="Hard cap on Gaussian count (VRAM)">
          <AutoNumber value={settings.maxSplats} autoValue={resolved?.maxSplats} min={100000} max={20000000} step={100000} onChange={(v) => set({ maxSplats: v })} />
        </Row>
        <Row label="SH degree" hint="Color detail (view-dependent effects)">
          <AutoNumber value={settings.shDegree} autoValue={resolved?.shDegree} min={0} max={3} step={1} onChange={(v) => set({ shDegree: v })} />
        </Row>
        <Row label="Refine every" hint="Densification cadence in steps">
          <AutoNumber value={settings.refineEvery} autoValue={resolved?.refineEvery} min={50} max={1000} step={50} onChange={(v) => set({ refineEvery: v })} />
        </Row>
        <Row label="SSIM weight" hint="Structural vs. photometric loss balance">
          <AutoNumber value={settings.ssimWeight} autoValue={resolved?.ssimWeight} min={0} max={1} step={0.05} onChange={(v) => set({ ssimWeight: v })} />
        </Row>
        <Row
          label="Progressive resolution"
          hint="Train at reduced resolution first and raise it on a schedule. On by default."
        >
          <BoolSelect value={settings.progressiveResolution} onChange={(v) => set({ progressiveResolution: v })} />
        </Row>
        <Row
          label="Mip-Splatting filter"
          hint="Bound each Gaussian to the sampling rate of the cameras that saw it. On by default."
        >
          <BoolSelect value={settings.mipFilter} onChange={(v) => set({ mipFilter: v })} />
        </Row>
        <Row
          label="Dense init"
          hint="Compose COLMAP MVS with any installed neural densifier and the sparse cloud. On by default."
        >
          <BoolSelect value={settings.denseInit} onChange={(v) => set({ denseInit: v })} />
        </Row>
        <Row
          label="Neural densifiers"
          hint="AND with MVS when present (DAV2 / VGGT-Commercial). Research ON prefers VGGT-Ω."
        >
          <BoolSelect value={settings.useNeuralInit} onChange={(v) => set({ useNeuralInit: v })} />
        </Row>
        <Row
          label="Research sidecars"
          hint="Allow non-commercial VGGT-Ω / Difix. Off by default; licensing risk."
        >
          <BoolSelect value={settings.allowResearchSidecars} onChange={(v) => set({ allowResearchSidecars: v })} />
        </Row>
        <Row
          label="Post polish"
          hint="Run NVIDIA Fixer after training when installed (commercial Open Model). Difix only with Research ON."
        >
          <BoolSelect value={settings.postPolish} onChange={(v) => set({ postPolish: v })} />
        </Row>
        <Row label="Live update every" hint="Steps between Brush checkpoints; the viewport interpolates between them">
          <AutoNumber value={settings.exportEvery} autoValue={resolved?.exportEvery} min={100} max={5000} step={100} onChange={(v) => set({ exportEvery: v })} />
        </Row>
      </Section>

      <Section title="Cleanliness">
        <Row label="Clean vs. detailed" hint="Higher means stronger floater suppression; lower keeps fine detail">
          <button
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
      </Section>

      <Section title="Output">
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
          <BoolSelect value={settings.keepIntermediates} onChange={(v) => set({ keepIntermediates: v })} onLabel="Keep" offLabel="Clean up" />
        </Row>
      </Section>

      <div className="p-3 text-center text-[10px] text-ink-dim">
        Every setting defaults to Auto, resolved from your hardware profile at job start.
      </div>
    </div>
  );
}
