#!/usr/bin/env python3
"""Experimental (NC) installable adapter for horizon-gs.

Host refuses this sidecar unless Experimental Mode is ON.
Keep the `.stub` marker until upstream + weights/ACCEPTED are present; delete
`.stub` only after a successful dry-run produces real artifacts.

Protocol: JSON stdin → PLY path / sparse path / OK on stdout.
Fails clearly when weights / upstream are missing — never invents outputs.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
NAME = HERE.name
sys.path.insert(0, str(HERE.parent / "_common"))
sys.path.insert(0, str(HERE / "_common"))

from adapter_util import fail, list_images, marker_ready, read_request  # noqa: E402

INSTALL = (
    "Clone upstream under ./upstream (see README), install NC weights per "
    "upstream LICENSE, touch ACCEPTED, then delete .stub after a dry-run. "
    "Optional: provide run_upstream.py next to this launcher."
)

UPSTREAM_SCRIPTS = (
    "run_upstream.py",
    "demo.py",
    "demo_colmap.py",
    "infer.py",
    "run.py",
    "scripts/demo.py",
    "scripts/demo_colmap.py",
    "scripts/infer.py",
)


def find_script() -> Path | None:
    for rel in UPSTREAM_SCRIPTS:
        for base in (HERE, HERE / "upstream", HERE / "repo"):
            p = base / rel if rel != "run.py" or base != HERE else None
            # Never recurse into our own run.py
            if rel == "run.py" and base == HERE:
                continue
            cand = base / rel
            if cand.exists():
                return cand
    return None


def run_script(script: Path, images_dir: Path, workspace: Path, task: str, splat: str | None) -> int:
    out = workspace / NAME.replace("-", "_")
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
    if splat:
        cmd.extend(["--splat", splat])
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        return fail(proc.stderr.strip() or proc.stdout.strip() or f"{NAME} upstream failed")
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
        print("OK")
        return 0
    if task == "polish" and splat:
        for p in out.rglob("*.ply"):
            print(str(p.resolve()))
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
    return fail(f"{NAME} upstream produced no output artifacts")


def main() -> int:
    req = read_request()
    task = (req.get("task") or "densify").lower()
    if not marker_ready(HERE):
        return fail(
            f"{NAME}: weights/upstream not installed. {INSTALL}",
            2,
        )
    images_dir = Path(req.get("imagesDir") or req.get("images_dir") or ".")
    workspace = Path(req.get("workspace") or ".")
    splat = req.get("splatPath") or req.get("splat_path")
    if task != "polish" and images_dir.exists() and not list_images(images_dir):
        # Some surface adapters only need splat input.
        if not splat:
            return fail("no images found")

    script = find_script()
    if script is None:
        return fail(
            f"{NAME}: no upstream entrypoint found under ./upstream or run_upstream.py. {INSTALL}",
            2,
        )
    try:
        return run_script(script, images_dir, workspace, task, splat)
    except Exception as e:
        return fail(f"{NAME} unavailable: {e}")


if __name__ == "__main__":
    raise SystemExit(main())
