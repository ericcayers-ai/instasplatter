/** About / implementations panel — Standard vs Experimental stacks, licenses, docs. */

import { openUrl } from "@tauri-apps/plugin-opener";
import { useStore } from "../../state/store";

const DOCS = {
  researchStack:
    "https://github.com/ericcayers-ai/instasplatter/blob/main/docs/RESEARCH-STACK.md",
  paperSweep:
    "https://github.com/ericcayers-ai/instasplatter/blob/main/docs/PAPER-SWEEP-2024+.md",
  license: "https://github.com/ericcayers-ai/instasplatter/blob/main/LICENSE",
};

async function openDoc(url: string) {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

function AttrRow({ name, license, note }: { name: string; license: string; note?: string }) {
  return (
    <div className="flex flex-col gap-0.5 border-b border-edge/50 py-2 last:border-0">
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-xs text-ink">{name}</span>
        <span className="shrink-0 font-mono text-[10px] text-ink-dim">{license}</span>
      </div>
      {note && <div className="text-[10px] leading-snug text-ink-dim">{note}</div>}
    </div>
  );
}

export default function AboutPanel() {
  const open = useStore((s) => s.aboutOpen);
  const setAboutOpen = useStore((s) => s.setAboutOpen);
  const engineStatus = useStore((s) => s.engineStatus);
  const floodEngine = useStore((s) => s.geoFloodEngine);
  const experimentalOn = useStore((s) => !!(s.resolved?.experimentalMode));

  if (!open) return null;

  const sidecarReady = (on: boolean | undefined) => (on ? "installed" : "not installed");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg/70 p-4 backdrop-blur-[2px]">
      <div
        role="dialog"
        aria-labelledby="about-title"
        className="flex max-h-[min(90vh,40rem)] w-full max-w-lg flex-col overflow-hidden rounded border border-edge bg-panel2 shadow-lg"
      >
        <div className="flex items-center justify-between border-b border-edge px-5 py-3">
          <h2 id="about-title" className="font-display text-base font-bold tracking-tight">
            About InstaSplatter
          </h2>
          <button
            type="button"
            className="btn px-2 py-0.5 text-[11px]"
            onClick={() => setAboutOpen(false)}
          >
            Close
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4 text-xs leading-relaxed text-ink-dim">
          <p>
            Dual-suite app: <span className="text-ink">Reconstruction</span> (Gaussian splat) and{" "}
            <span className="text-ink">Geospatial</span> (flood / ENU). Apache-2.0 product shell with
            opt-in research sidecars.
          </p>

          <h3 className="mt-4 text-[10px] font-semibold uppercase tracking-wider text-ink">
            Standard vs Experimental
          </h3>
          <div className="mt-2 grid gap-2 sm:grid-cols-2">
            <div className="rounded border border-edge bg-panel/80 p-3">
              <div className="text-[11px] font-semibold text-accent">Standard (default)</div>
              <p className="mt-1.5 text-[10px] leading-snug">
                Commercially redistributable path: COLMAP, RoMa densify, VGGT-Commercial, DA3,
                Fixer, Brush / gsplat, ANUGA / SWMM + labelled preview.
              </p>
            </div>
            <div
              className={`rounded border p-3 ${
                experimentalOn
                  ? "border-danger/50 bg-danger/10"
                  : "border-edge bg-panel/80"
              }`}
            >
              <div className={`text-[11px] font-semibold ${experimentalOn ? "text-danger" : "text-ink"}`}>
                Experimental {experimentalOn ? "(ON)" : ""}
              </div>
              <p className="mt-1.5 text-[10px] leading-snug">
                NC research stack after TitleBar ack: VGGT-Ω, MASt3R, DUSt3R, Difix, profiled pose
                chains, experimental hydro adapters behind promotion gates.
              </p>
            </div>
          </div>

          <h3 className="mt-4 text-[10px] font-semibold uppercase tracking-wider text-ink">
            Geospatial engines
          </h3>
          <div className="mt-1 divide-y divide-edge/50">
            <AttrRow
              name="ANUGA"
              license="Apache-2.0"
              note={`Scientific shallow-water — ${floodEngine?.anugaReady ? "launcher found" : "not installed (demo fallback)"}`}
            />
            <AttrRow
              name="EPA SWMM"
              license="Public domain"
              note={`Network exchange — ${floodEngine?.swmmReady ? "scaffold ready" : "scaffold"}`}
            />
            <AttrRow
              name="Soft preview"
              license="Apache-2.0"
              note="WebGPU/CPU depth–velocity — always “Live preview” until compare gates pass"
            />
          </div>

          <h3 className="mt-4 text-[10px] font-semibold uppercase tracking-wider text-ink">
            Reconstruction sidecars
          </h3>
          <div className="mt-1 divide-y divide-edge/50">
            <AttrRow name="COLMAP" license="BSD" note={sidecarReady(engineStatus?.colmap)} />
            <AttrRow
              name="Brush"
              license="Apache-2.0"
              note={`${sidecarReady(engineStatus?.brush)}${engineStatus?.brushCustom ? " · custom fork detected" : ""}`}
            />
            <AttrRow name="gsplat" license="Apache-2.0" note={sidecarReady(engineStatus?.gsplat)} />
            <AttrRow name="RoMa v2" license="MIT" note={sidecarReady(engineStatus?.romaV2)} />
            <AttrRow
              name="VGGT-Commercial"
              license="Meta AUP"
              note={sidecarReady(engineStatus?.vggtCommercial)}
            />
            <AttrRow name="NVIDIA Fixer" license="Open Model" note={sidecarReady(engineStatus?.fixer)} />
            <AttrRow
              name="VGGT-Ω / MASt3R / Difix"
              license="NC / research"
              note={`Experimental only — Ω ${sidecarReady(engineStatus?.vggtOmega)}, MASt3R ${sidecarReady(engineStatus?.mast3r)}, Difix ${sidecarReady(engineStatus?.difix)}`}
            />
          </div>

          <h3 className="mt-4 text-[10px] font-semibold uppercase tracking-wider text-ink">
            Imagery & attribution
          </h3>
          <p className="mt-1.5 text-[10px] leading-snug">
            Esri World Imagery tiles require attribution when used:{" "}
            <span className="text-ink">
              Tiles © Esri — Source: Esri, Maxar, Earthstar Geographics, and the GIS User Community
            </span>
            . Carto/OSM remain a low-bandwidth fallback.
          </p>

          <h3 className="mt-4 text-[10px] font-semibold uppercase tracking-wider text-ink">
            Documentation
          </h3>
          <div className="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              className="btn px-2.5 py-1 text-[11px]"
              onClick={() => void openDoc(DOCS.researchStack)}
            >
              RESEARCH-STACK.md
            </button>
            <button
              type="button"
              className="btn px-2.5 py-1 text-[11px]"
              onClick={() => void openDoc(DOCS.paperSweep)}
            >
              PAPER-SWEEP
            </button>
            <button
              type="button"
              className="btn px-2.5 py-1 text-[11px]"
              onClick={() => void openDoc(DOCS.license)}
            >
              Apache-2.0 LICENSE
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
