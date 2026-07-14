#!/usr/bin/env python3
"""Stub Experimental pose/densify launcher. Replace with real NC model wiring."""

from __future__ import annotations

import json
import sys
from pathlib import Path

NAME = Path(__file__).resolve().parent.name


def main() -> int:
    req = json.loads(sys.stdin.read() or "{}")
    task = (req.get("task") or "densify").lower()
    workspace = Path(req.get("workspace") or ".")
    print(
        f"# {NAME}: stub only — install real weights and implement {task}. "
        f"Refused unless Experimental Mode is ON on the host.",
        file=sys.stderr,
    )
    if task == "sfm":
        sparse = workspace / "sparse" / "0"
        sparse.mkdir(parents=True, exist_ok=True)
        # Host validates model usability; stub exits non-zero.
        return 2
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
