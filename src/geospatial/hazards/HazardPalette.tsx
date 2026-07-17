/** Hazard palette — Flood (simulate) vs Experimental multi-hazard stubs. */

import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useStore } from "../../state/store";
import {
  HAZARD_NON_CLAIM,
  HAZARD_STUBS,
  type HazardStubDef,
  type HazardStubId,
} from "./stubs";

async function openExternal(url: string) {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

function StubCard({
  stub,
  selected,
  experimentalOn,
  onSelect,
  onRequestExperimental,
}: {
  stub: HazardStubDef;
  selected: boolean;
  experimentalOn: boolean;
  onSelect: () => void;
  onRequestExperimental: () => void;
}) {
  const isSimulate = stub.kind === "simulate";
  const locked = !isSimulate && !experimentalOn;

  return (
    <div
      className={`rounded border px-2 py-1.5 ${
        selected
          ? isSimulate
            ? "border-[var(--color-hydro)]/50 bg-[color-mix(in_srgb,var(--color-hydro)_10%,transparent)]"
            : "border-danger/40 bg-danger/10"
          : "border-edge/70 bg-panel2/40"
      }`}
    >
      <button
        type="button"
        onClick={onSelect}
        className="flex w-full items-start justify-between gap-2 text-left"
        aria-pressed={selected}
      >
        <span className="min-w-0">
          <span className="block text-[11px] font-medium text-ink">{stub.label}</span>
          <span
            className={`mt-0.5 block text-[9px] font-semibold uppercase tracking-wider ${
              isSimulate ? "text-[var(--color-hydro)]" : "text-danger/80"
            }`}
          >
            {isSimulate ? "Simulate" : "Experimental stub"}
          </span>
        </span>
        {locked && (
          <span className="shrink-0 rounded border border-danger/30 px-1 py-0.5 text-[8px] text-danger/80">
            Exp
          </span>
        )}
      </button>

      {selected && (
        <div className="mt-1.5 space-y-1.5 border-t border-edge/50 pt-1.5">
          <p className="text-[10px] leading-snug text-ink-dim">{stub.blurb}</p>
          {locked ? (
            <button
              type="button"
              className="btn btn-danger px-2 py-0.5 text-[10px]"
              onClick={onRequestExperimental}
            >
              Enable Experimental Mode
            </button>
          ) : (
            <div className="flex flex-wrap gap-1">
              {stub.links.map((link) => (
                <button
                  key={link.href + link.label}
                  type="button"
                  className="btn px-1.5 py-0.5 text-[9px]"
                  title={`${link.source}: ${link.href}`}
                  onClick={() => void openExternal(link.href)}
                >
                  {link.source}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Self-contained hazard palette for the geo left rail (or any mount point).
 * Distinguishes flood simulation from Experimental quake/fire/landslide/tsunami stubs.
 */
export default function HazardPalette() {
  const experimentalOn = useStore((s) => !!(s.resolved?.experimentalMode));
  const requestExperimental = useStore((s) => s.requestExperimental);
  const setLayerVisible = useStore((s) => s.setGeoLayerVisible);
  const [selected, setSelected] = useState<HazardStubId>("flood");

  const onSelect = (stub: HazardStubDef) => {
    setSelected(stub.id);
    if (stub.kind === "simulate") {
      // Surface the flood results group; do not invent physics for other hazards.
      setLayerVisible("flood_depth", true);
    }
  };

  const simulate = HAZARD_STUBS.filter((s) => s.kind === "simulate");
  const stubs = HAZARD_STUBS.filter((s) => s.kind === "experimental_stub");

  return (
    <div className="border-b border-edge px-3 py-3" data-testid="hazard-palette">
      <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">
        Hazards
      </div>

      <div className="flex flex-col gap-1.5">
        {simulate.map((stub) => (
          <StubCard
            key={stub.id}
            stub={stub}
            selected={selected === stub.id}
            experimentalOn={experimentalOn}
            onSelect={() => onSelect(stub)}
            onRequestExperimental={requestExperimental}
          />
        ))}
      </div>

      <div className="mb-1.5 mt-3 text-[9px] font-semibold uppercase tracking-wider text-danger/70">
        Layers / Experimental stubs
      </div>
      <div className="flex flex-col gap-1.5">
        {stubs.map((stub) => (
          <StubCard
            key={stub.id}
            stub={stub}
            selected={selected === stub.id}
            experimentalOn={experimentalOn}
            onSelect={() => onSelect(stub)}
            onRequestExperimental={requestExperimental}
          />
        ))}
      </div>

      <p className="mt-2 text-[9px] leading-snug text-ink-dim">{HAZARD_NON_CLAIM}</p>
    </div>
  );
}
