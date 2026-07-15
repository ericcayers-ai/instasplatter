#!/usr/bin/env python3
"""EPA SWMM network-coupling scaffold for InstaSplatter.

Writes a clear coupling contract under outputDir. Does not bundle SWMM;
replace the stub body once an EPA SWMM binary / pyswmm pin is installed.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1


def emit(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def fail(code: int, message: str) -> None:
    emit({"kind": "error", "message": message})
    raise SystemExit(code)


def main() -> None:
    raw = sys.stdin.read()
    if not raw.strip():
        fail(3, "empty stdin")
    try:
        req = json.loads(raw)
    except json.JSONDecodeError as e:
        fail(3, f"invalid JSON: {e}")
    if int(req.get("schemaVersion", 0)) != SCHEMA_VERSION:
        fail(3, f"unsupported schemaVersion (want {SCHEMA_VERSION})")

    out = Path(req.get("outputDir") or ".")
    out.mkdir(parents=True, exist_ok=True)
    demo = bool(req.get("demoMode", True))
    network = req.get("networkPath")

    # Detection hook — real installs place swmm5 / pyswmm here.
    swmm_ready = (Path(__file__).parent / "ACCEPTED").exists()
    if not swmm_ready and not demo:
        fail(2, "EPA SWMM not installed (no ACCEPTED marker / binary).")

    contract = {
        "schemaVersion": SCHEMA_VERSION,
        "runId": req.get("runId"),
        "mode": "stub" if not swmm_ready else "swmm",
        "networkPath": network,
        "surfaceExchange": req.get("surfaceExchange"),
        "coupling": {
            "in": ["node_inflow_cms", "surface_depth_m"],
            "out": ["outfall_cms", "surcharge_depth_m"],
        },
        "message": (
            "SWMM launcher scaffold — install EPA SWMM and delete stub behaviour."
            if not swmm_ready
            else "SWMM ready marker present; wire pyswmm evolve loop."
        ),
    }
    path = out / "swmm_coupling.json"
    path.write_text(json.dumps(contract, indent=2), encoding="utf-8")
    emit(
        {
            "kind": "done",
            "mode": contract["mode"],
            "couplingPaths": [str(path)],
            "message": contract["message"],
        }
    )


if __name__ == "__main__":
    main()
