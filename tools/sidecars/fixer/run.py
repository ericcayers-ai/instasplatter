#!/usr/bin/env python3
"""NVIDIA Fixer polish adapter (commercial Open Model when licensed)."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "_common"))
sys.path.insert(0, str(HERE / "_common"))

from adapter_util import fail, read_request, require_marker  # noqa: E402

INSTALL = (
    "Install NVIDIA Fixer weights/runtime under this folder or ./upstream, "
    "touch ACCEPTED. See install.ps1."
)


def main() -> int:
    req = read_request()
    task = (req.get("task") or "polish").lower()
    if task != "polish":
        return fail(f"fixer unsupported task={task}")
    hint = require_marker(HERE, INSTALL)
    if hint is not None:
        return hint
    splat = req.get("splatPath") or req.get("splat_path")
    if not splat or not Path(splat).exists():
        return fail("splatPath required for polish")
    workspace = Path(req["workspace"])
    out = workspace / "fixer"
    out.mkdir(parents=True, exist_ok=True)
    out_ply = out / "polished.ply"
    for script in (
        HERE / "run_fixer.py",
        HERE / "upstream" / "infer.py",
        HERE / "upstream" / "demo.py",
    ):
        if not script.exists():
            continue
        cmd = [
            sys.executable,
            str(script),
            "--input",
            str(splat),
            "--output",
            str(out_ply),
        ]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            return fail(proc.stderr.strip() or "fixer failed")
        if out_ply.exists():
            print(str(out_ply.resolve()))
            return 0
        line = next(
            (l.strip() for l in proc.stdout.splitlines() if l.strip().endswith(".ply")),
            "",
        )
        if line:
            print(line)
            return 0
        return fail("fixer script produced no PLY")
    return fail("No Fixer entrypoint (run_fixer.py / upstream). " + INSTALL, 2)


if __name__ == "__main__":
    raise SystemExit(main())
