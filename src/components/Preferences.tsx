import { useStore } from "../state/store";
import type { Settings } from "../lib/ipc";

const PRESETS = [
  { id: null, label: "Auto", hint: "Picked for your hardware" },
  { id: "draft", label: "Draft", hint: "Fastest preview" },
  { id: "eco", label: "Eco", hint: "Laptop friendly" },
  { id: "balanced", label: "Balanced", hint: "Good default" },
  { id: "high", label: "High", hint: "More detail" },
  { id: "max", label: "Max", hint: "Archival quality" },
];

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 py-2.5">
      <div className="min-w-0">
        <div className="text-sm">{label}</div>
        {hint && <div className="text-xs text-ink-dim">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
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
        className={`rounded-full px-3 py-1 text-xs transition ${
          isAuto ? "bg-accent/15 text-accent" : "border border-edge text-ink-dim hover:text-ink"
        }`}
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
        className={`w-24 rounded-lg border border-edge bg-panel2 px-2.5 py-1 text-right text-sm tabular-nums outline-none focus:border-accent/60 ${isAuto ? "opacity-40" : ""}`}
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
      className="rounded-lg border border-edge bg-panel2 px-2.5 py-1.5 text-sm outline-none focus:border-accent/60"
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

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="rounded-2xl border border-edge bg-panel p-5">
      <div className="mb-2 text-xs font-semibold uppercase tracking-widest text-ink-dim">
        {title}
      </div>
      <div className="divide-y divide-edge/50">{children}</div>
    </div>
  );
}

export default function Preferences() {
  const open = useStore((s) => s.prefsOpen);
  const openPrefs = useStore((s) => s.openPrefs);
  const settings = useStore((s) => s.settings);
  const resolved = useStore((s) => s.resolved);
  const updateSettings = useStore((s) => s.updateSettings);

  if (!open) return null;
  const set = (patch: Partial<Settings>) => void updateSettings(patch);

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={() => openPrefs(false)}
    >
      <div
        className="float-in flex max-h-[85vh] w-[640px] flex-col gap-4 overflow-y-auto rounded-3xl border border-edge bg-bg p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <div className="text-lg font-semibold">Preferences</div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => set({
                preset: null, maxFrames: null, maxResolution: null,
                blurRejectFraction: null, matcher: null, siftGpu: null,
                totalSteps: null, maxSplats: null, shDegree: null,
                refineEvery: null, ssimWeight: null, exportEvery: null,
                strictness: null, keepIntermediates: null,
                progressiveResolution: null, mipFilter: null, liveInit: null,
                exportFormat: null,
              })}
              className="text-xs text-ink-dim transition hover:text-ink"
            >
              Reset all to Auto
            </button>
            <button
              onClick={() => openPrefs(false)}
              className="rounded-full border border-edge px-3 py-1 text-sm text-ink-dim transition hover:text-ink"
            >
              ✕
            </button>
          </div>
        </div>

        {/* Presets */}
        <div className="grid grid-cols-3 gap-2">
          {PRESETS.map((p) => {
            const active = (settings.preset ?? null) === p.id;
            return (
              <button
                key={p.label}
                onClick={() => set({ preset: p.id })}
                className={`rounded-2xl border p-3 text-left transition ${
                  active
                    ? "border-accent/70 bg-accent/10"
                    : "border-edge bg-panel hover:border-accent/30"
                }`}
              >
                <div className="text-sm font-medium">
                  {p.label}
                  {p.id === null && resolved && (
                    <span className="ml-1 text-xs text-accent">({resolved.preset})</span>
                  )}
                </div>
                <div className="text-xs text-ink-dim">{p.hint}</div>
              </button>
            );
          })}
        </div>

        <Section title="Input">
          <Row label="Max frames" hint="Frames used for reconstruction">
            <AutoNumber value={settings.maxFrames} autoValue={resolved?.maxFrames} min={10} max={2000} step={10} onChange={(v) => set({ maxFrames: v })} />
          </Row>
          <Row label="Max resolution" hint="Longest image edge in px">
            <AutoNumber value={settings.maxResolution} autoValue={resolved?.maxResolution} min={480} max={3840} step={80} onChange={(v) => set({ maxResolution: v })} />
          </Row>
          <Row label="Blur rejection" hint="Fraction of blurriest frames dropped">
            <AutoNumber value={settings.blurRejectFraction} autoValue={resolved?.blurRejectFraction} min={0} max={0.9} step={0.05} onChange={(v) => set({ blurRejectFraction: v })} />
          </Row>
        </Section>

        <Section title="Camera solving">
          <Row label="Matcher" hint="Sequential suits video; exhaustive suits unordered photos">
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
            <AutoSelect
              value={settings.liveInit == null ? null : settings.liveInit ? "on" : "off"}
              options={[
                { id: "on", label: "On" },
                { id: "off", label: "Off" },
              ]}
              onChange={(v) => set({ liveInit: v == null ? null : v === "on" })}
            />
          </Row>
          <Row label="GPU feature extraction" hint="Requires NVIDIA CUDA">
            <AutoSelect
              value={settings.siftGpu == null ? null : settings.siftGpu ? "on" : "off"}
              options={[
                { id: "on", label: "On" },
                { id: "off", label: "Off" },
              ]}
              onChange={(v) => set({ siftGpu: v == null ? null : v === "on" })}
            />
          </Row>
        </Section>

        <Section title="Training">
          <Row label="Total steps" hint="More steps → sharper result, longer time">
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
            hint="Train at reduced resolution first and raise it on a schedule. Faster, at the cost of restarting the optimiser at each step."
          >
            <AutoSelect
              value={settings.progressiveResolution == null ? null : settings.progressiveResolution ? "on" : "off"}
              options={[
                { id: "on", label: "On" },
                { id: "off", label: "Off" },
              ]}
              onChange={(v) => set({ progressiveResolution: v == null ? null : v === "on" })}
            />
          </Row>
          <Row
            label="Mip-Splatting filter"
            hint="Bound each Gaussian to the sampling rate of the cameras that saw it. Reduces aliasing and oversized blobs."
          >
            <AutoSelect
              value={settings.mipFilter == null ? null : settings.mipFilter ? "on" : "off"}
              options={[
                { id: "on", label: "On" },
                { id: "off", label: "Off" },
              ]}
              onChange={(v) => set({ mipFilter: v == null ? null : v === "on" })}
            />
          </Row>
          <Row label="Live update every" hint="Steps between viewport refreshes">
            <AutoNumber value={settings.exportEvery} autoValue={resolved?.exportEvery} min={100} max={5000} step={100} onChange={(v) => set({ exportEvery: v })} />
          </Row>
        </Section>

        <Section title="Cleanliness">
          <Row
            label="Clean ↔ Detailed"
            hint="Higher = stronger floater suppression; lower = keep fine detail"
          >
            <button
              onClick={() => set({ strictness: settings.strictness == null ? 0.5 : null })}
              className={`rounded-full px-3 py-1 text-xs transition ${
                settings.strictness == null
                  ? "bg-accent/15 text-accent"
                  : "border border-edge text-ink-dim hover:text-ink"
              }`}
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
              className={`w-40 ${settings.strictness == null ? "opacity-40" : ""}`}
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
          <Row label="Keep intermediates" hint="Keep frames + COLMAP database on disk">
            <AutoSelect
              value={settings.keepIntermediates == null ? null : settings.keepIntermediates ? "on" : "off"}
              options={[
                { id: "on", label: "Keep" },
                { id: "off", label: "Clean up" },
              ]}
              onChange={(v) => set({ keepIntermediates: v == null ? null : v === "on" })}
            />
          </Row>
        </Section>

        <div className="text-center text-[11px] text-ink-dim">
          Every setting defaults to <b>Auto</b> — resolved from your hardware profile at job start.
        </div>
      </div>
    </div>
  );
}
