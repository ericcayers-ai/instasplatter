#!/usr/bin/env python3
"""Template Experimental launcher. Copy into a sidecar folder and replace the body.

Exits non-zero — never prints a success path. Keep a `.stub` marker next to this
file until real weights are wired so InstaSplatter does not report the sidecar
as ready.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

NAME = Path(__file__).resolve().parent.name


def main() -> int:
    req = json.loads(sys.stdin.read() or "{}")
    task = (req.get("task") or "densify").lower()
    print(
        f"# {NAME}: stub only — install real NC weights and implement task={task}. "
        f"Delete the .stub marker when ready. Host refuses success from stubs.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
