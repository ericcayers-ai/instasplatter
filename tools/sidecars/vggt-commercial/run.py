#!/usr/bin/env python3
"""VGGT-1B-Commercial pose/densify adapter (gated commercial weights).

Requires ACCEPTED + weights. NC research checkpoints use Experimental sidecars.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "_common"))
sys.path.insert(0, str(HERE / "_common"))

from adapter_util import fail, list_images, read_request, require_marker  # noqa: E402

INSTALL = (
    "Obtain VGGT-1B-Commercial weights, place under this folder or ./upstream, "
    "touch ACCEPTED. See install.ps1 / Meta commercial terms."
)


def main() -> int:
    req = read_request()
    task = (req.get("task") or "densify").lower()
    hint = require_marker(HERE, INSTALL)
    if hint is not None:
        return hint
    if not (HERE / "ACCEPTED").exists():
        return fail("VGGT-Commercial requires ACCEPTED (commercial terms). " + INSTALL, 2)

    images_dir = Path(req.get("imagesDir") or req["images_dir"])
    workspace = Path(req["workspace"])
    if not list_images(images_dir):
        return fail("no images")

    for script in (
        HERE / "run_vggt.py",
        HERE / "upstream" / "demo_colmap.py",
        HERE / "upstream" / "demo.py",
    ):
        if not script.exists():
            continue
        out = workspace / "vggt_commercial"
        out.mkdir(parents=True, exist_ok=True)
        cmd = [
            sys.executable,
            str(script),
            "--images",
            str(images_dir),
            "--output",
            str(out),
            "--task",
            task,
        ]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            return fail(proc.stderr.strip() or "vggt failed")
        if task in ("sfm", "pose"):
            sparse = out / "sparse" / "0"
            if sparse.exists():
                dest = workspace / "sparse" / "0"
                if dest.exists():
                    shutil.rmtree(dest)
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copytree(sparse, dest)
                print(str(dest.resolve()))
                return 0
        for p in out.rglob("*.ply"):
            print(str(p.resolve()))
            return 0
        line = next(
            (l.strip() for l in proc.stdout.splitlines() if l.strip() and not l.startswith("#")),
            "",
        )
        if line:
            print(line)
            return 0
        return fail("VGGT script produced no sparse/PLY output")

    return fail("No VGGT entrypoint (run_vggt.py or upstream/demo). " + INSTALL, 2)


if __name__ == "__main__":
    raise SystemExit(main())
