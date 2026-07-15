#!/usr/bin/env python3
"""MapAnything (Apache) pose + densify adapter for InstaSplatter.

Wires an installed MapAnything checkout / pip package. Never invents poses
or points when weights are missing.
"""

from __future__ import annotations

import json
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "_common"))
sys.path.insert(0, str(HERE / "_common"))

from adapter_util import fail, list_images, read_request, require_marker  # noqa: E402

INSTALL = (
    "Clone MapAnything under ./upstream (or pip-install), drop weights, "
    "touch ACCEPTED. See install.ps1."
)


def try_mapanything_sfm(images_dir: Path, workspace: Path) -> Path:
    upstream = HERE / "upstream"
    # Prefer a user-provided script.
    for script in (
        HERE / "run_mapanything.py",
        upstream / "scripts" / "demo_sfm.py",
        upstream / "demo_sfm.py",
        upstream / "run.py",
    ):
        if script.exists():
            import subprocess

            out = workspace / "mapanything_sfm"
            out.mkdir(parents=True, exist_ok=True)
            cmd = [
                sys.executable,
                str(script),
                "--images",
                str(images_dir),
                "--output",
                str(out),
            ]
            proc = subprocess.run(cmd, capture_output=True, text=True)
            if proc.returncode != 0:
                raise RuntimeError(proc.stderr.strip() or proc.stdout.strip() or "sfm failed")
            sparse = out / "sparse" / "0"
            if (sparse / "images.bin").exists() or (sparse / "images.txt").exists():
                dest = workspace / "sparse" / "0"
                if dest.exists():
                    shutil.rmtree(dest)
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copytree(sparse, dest)
                return dest
            raise RuntimeError("MapAnything script did not write COLMAP sparse/0")

    # Pip package heuristic.
    try:
        import mapanything  # type: ignore  # noqa: F401
    except Exception as e:
        raise RuntimeError(f"MapAnything not importable ({e}). {INSTALL}") from e
    raise RuntimeError(
        "mapanything imported but no demo_sfm entrypoint found — "
        "provide upstream/scripts/demo_sfm.py or run_mapanything.py. " + INSTALL
    )


def try_mapanything_densify(images_dir: Path, workspace: Path, sparse: Path, max_points: int) -> Path:
    # Reuse depth densify path if package exposes depth; else require upstream densify script.
    for script in (
        HERE / "run_densify.py",
        HERE / "upstream" / "scripts" / "demo_densify.py",
        HERE / "upstream" / "demo_densify.py",
    ):
        if script.exists():
            import subprocess

            out = workspace / "mapanything"
            out.mkdir(parents=True, exist_ok=True)
            cmd = [
                sys.executable,
                str(script),
                "--images",
                str(images_dir),
                "--sparse",
                str(sparse),
                "--output",
                str(out / "dense.ply"),
                "--max-points",
                str(max_points),
            ]
            proc = subprocess.run(cmd, capture_output=True, text=True)
            if proc.returncode != 0:
                raise RuntimeError(proc.stderr.strip() or "densify failed")
            ply = out / "dense.ply"
            if not ply.exists():
                # Allow script to print path.
                line = next((l.strip() for l in proc.stdout.splitlines() if l.strip().endswith(".ply")), "")
                if line:
                    ply = Path(line)
            if not ply.exists():
                raise RuntimeError("densify script produced no PLY")
            return ply

    raise RuntimeError(
        "No MapAnything densify entrypoint. Provide upstream densify script. " + INSTALL
    )


def main() -> int:
    req = read_request()
    task = (req.get("task") or "densify").lower()
    hint = require_marker(HERE, INSTALL)
    if hint is not None:
        return hint

    images_dir = Path(req.get("imagesDir") or req["images_dir"])
    workspace = Path(req["workspace"])
    sparse = Path(req.get("sparseDir") or req.get("sparse_dir") or (workspace / "sparse" / "0"))
    max_points = int(req.get("maxPoints") or req.get("max_points") or 1_200_000)

    if not list_images(images_dir):
        return fail("no images found")

    try:
        if task in ("sfm", "pose"):
            out = try_mapanything_sfm(images_dir, workspace)
            print(str(out.resolve()))
            return 0
        if task == "densify":
            if not (sparse / "images.txt").exists() and not (sparse / "images.bin").exists():
                return fail("COLMAP sparse model required for densify")
            ply = try_mapanything_densify(images_dir, workspace, sparse, max_points)
            print(str(ply.resolve()))
            return 0
        return fail(f"mapanything unsupported task={task}")
    except Exception as e:
        return fail(f"mapanything unavailable: {e}")


if __name__ == "__main__":
    raise SystemExit(main())
