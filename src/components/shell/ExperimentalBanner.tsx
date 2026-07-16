/** Persistent banner when Experimental Mode is active. */

import { useStore } from "../../state/store";

export default function ExperimentalBanner() {
  const resolved = useStore((s) => s.resolved);
  const pipelineChips = useStore((s) => s.pipelineChips);
  const engineStatus = useStore((s) => s.engineStatus);
  const suite = useStore((s) => s.suite);
  const stages = useStore((s) => s.stages);
  const scientific = useStore((s) => s.geoScientificRun);
  const on = !!(resolved?.experimentalMode);

  if (!on) return null;

  const chips: string[] = [];
  if (pipelineChips.cameras) chips.push(pipelineChips.cameras);
  if (pipelineChips.init) chips.push(pipelineChips.init);
  if (pipelineChips.polish) chips.push(pipelineChips.polish);
  if (pipelineChips.trainer) chips.push(pipelineChips.trainer);
  if (pipelineChips.flood) chips.push(pipelineChips.flood);
  if (pipelineChips.export) chips.push(pipelineChips.export);

  const activeStage = stages.find((s) => s.state === "active");
  if (suite === "reconstruction" && activeStage && activeStage.progress > 0) {
    chips.unshift(`${activeStage.label} ${Math.round(activeStage.progress * 100)}%`);
  }
  if (suite === "geospatial" && scientific?.state === "running") {
    chips.unshift(`Flood ${Math.round(scientific.progress * 100)}%`);
  }

  // Fallback: installed research engines when a job has not reported yet.
  if (chips.length === 0 && engineStatus) {
    const ready: string[] = [];
    if (engineStatus.vggtOmega) ready.push("VGGT-Ω");
    if (engineStatus.mast3r) ready.push("MASt3R");
    if (engineStatus.dust3r) ready.push("DUSt3R");
    if (engineStatus.vggtCommercial) ready.push("VGGT-C");
    if (engineStatus.romaV2) ready.push("RoMa");
    if (engineStatus.difix) ready.push("Difix");
    if (engineStatus.fixer) ready.push("Fixer");
    if (ready.length) chips.push(`Ready: ${ready.join(" · ")}`);
  }

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-danger/35 bg-danger/10 px-3 py-1.5 text-[11px]">
      <span className="font-semibold tracking-wide text-danger">
        Experimental Mode — NC research stack
      </span>
      {chips.map((c) => (
        <span
          key={c}
          className="rounded border border-danger/30 bg-panel/60 px-1.5 py-0.5 text-[10px] text-ink"
        >
          {c.replace(/^(Cameras|Init|Polish|Trainer|Flood|Export):\s*/i, "")}
        </span>
      ))}
    </div>
  );
}
