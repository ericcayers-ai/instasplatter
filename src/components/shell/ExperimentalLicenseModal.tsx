/** First-enable modal for Experimental Mode (NC research stack). */

import { useStore } from "../../state/store";

export default function ExperimentalLicenseModal() {
  const open = useStore((s) => s.experimentalModalOpen);
  const acceptExperimental = useStore((s) => s.acceptExperimental);
  const declineExperimental = useStore((s) => s.declineExperimental);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg/70 p-4 backdrop-blur-[2px]">
      <div
        role="dialog"
        aria-labelledby="exp-license-title"
        className="max-w-md rounded border border-danger/40 bg-panel2 p-5 shadow-lg"
      >
        <h2 id="exp-license-title" className="font-display text-base font-bold text-danger">
          Experimental Mode — research / personal only
        </h2>
        <p className="mt-3 text-xs leading-relaxed text-ink-dim">
          Experimental Mode unlocks non-commercial (NC) research models including VGGT-Ω,
          MASt3R, DUSt3R, Difix, and related Sidecars. Those weights are licensed for{" "}
          <span className="text-ink">personal and research use</span>, not commercial redistribution
          of derivative products without separate rights.
        </p>
        <p className="mt-2 text-xs leading-relaxed text-ink-dim">
          Standard Mode stays the default and uses only commercially redistributable solvers
          (VGGT-Commercial, RoMa v2 densify, COLMAP, DAV2, Fixer, Brush/gsplat).
        </p>
        <ul className="mt-3 list-inside list-disc text-[11px] text-ink-dim">
          <li>NC checkpoints are never bundled in the installer — you install sidecars yourself.</li>
          <li>Turning Experimental ON force-opens research gates and Max quality floors.</li>
          <li>You can turn it off anytime from the TitleBar toggle.</li>
        </ul>
        <div className="mt-5 flex justify-end gap-2">
          <button onClick={declineExperimental} className="btn">
            Stay on Standard
          </button>
          <button onClick={() => void acceptExperimental()} className="btn btn-danger">
            I understand — enable Experimental
          </button>
        </div>
      </div>
    </div>
  );
}
