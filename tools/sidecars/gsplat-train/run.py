#!/usr/bin/env python3
"""InstaSplatter gsplat trainer sidecar (Apache-2.0 stack).

Reads one JSON object from stdin, trains with nerfstudio gsplat, writes
checkpoint PLYs under workspace/exports, prints the final absolute PLY path.

Install (NVIDIA + CUDA):
  pip install gsplat torch torchvision
  # optional appearance / AA extras used by simple_trainer:
  # pip install fused-bilagrid  (or rely on examples/lib_bilagrid)

Drop this folder at:
  %LOCALAPPDATA%/InstaSplatter/engines/sidecars/gsplat-train/
  (run.bat should invoke this script, or copy run.py alone)

Request schema (camelCase):
  imagesDir, workspace, sparseDir?, maxSteps, maxSplats, maxResolution,
  shDegree, exportEvery, ssimWeight, opacLossWeight, scaleLossWeight,
  strategy ("mcmc"|"default"), absgrad, antialiased, appOpt, postProcessing,
  initPly?, exportDir?
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path


def _log(msg: str) -> None:
    print(msg, file=sys.stderr, flush=True)


def main() -> int:
    raw = sys.stdin.read()
    if not raw.strip():
        _log("empty stdin")
        return 2
    req = json.loads(raw)

    workspace = Path(req["workspace"])
    images = Path(req["imagesDir"])
    sparse = Path(req["sparseDir"]) if req.get("sparseDir") else workspace / "sparse" / "0"
    export_dir = Path(req.get("exportDir") or (workspace / "exports"))
    export_dir.mkdir(parents=True, exist_ok=True)
    result_dir = workspace / "gsplat_run"
    result_dir.mkdir(parents=True, exist_ok=True)

    # Point a COLMAP-style tree at our solved poses + images.
    data_dir = workspace / "gsplat_data"
    if data_dir.exists():
        shutil.rmtree(data_dir, ignore_errors=True)
    data_dir.mkdir(parents=True)
    # Prefer symlink; fall back to junction/copy on Windows.
    img_link = data_dir / "images"
    sparse_link = data_dir / "sparse"
    try:
        img_link.symlink_to(images, target_is_directory=True)
    except OSError:
        shutil.copytree(images, img_link)
    try:
        sparse_link.mkdir(parents=True, exist_ok=True)
        (sparse_link / "0").symlink_to(sparse, target_is_directory=True)
    except OSError:
        shutil.copytree(sparse, sparse_link / "0")

    max_steps = int(req.get("maxSteps") or 30_000)
    max_splats = int(req.get("maxSplats") or 3_000_000)
    sh_degree = int(req.get("shDegree") or 3)
    export_every = max(100, int(req.get("exportEvery") or 500))
    ssim = float(req.get("ssimWeight") or 0.2)
    opac_reg = float(req.get("opacLossWeight") or 0.01)
    scale_reg = float(req.get("scaleLossWeight") or 0.01)
    strategy = (req.get("strategy") or "mcmc").lower()
    if strategy not in ("mcmc", "default"):
        strategy = "mcmc"
    absgrad = bool(req.get("absgrad", True))
    antialiased = bool(req.get("antialiased", True))
    app_opt = bool(req.get("appOpt", True))
    post = req.get("postProcessing")  # None | "bilateral_grid" | "ppisp"
    if post is None and bool(req.get("bilateralGrid", True)):
        post = "bilateral_grid"

    # data_factor ≈ downsample so longest side ~ maxResolution (approx).
    max_res = int(req.get("maxResolution") or 1280)
    data_factor = 1
    # Leave factor 1; gsplat resizes with --data_factor. Cap via factor heuristically.
    if max_res <= 800:
        data_factor = 4
    elif max_res <= 1200:
        data_factor = 2

    ply_steps = list(range(export_every, max_steps + 1, export_every))
    if ply_steps[-1] != max_steps:
        ply_steps.append(max_steps)

    # Prefer an adjacent vendored simple_trainer if present.
    here = Path(__file__).resolve().parent
    trainer = here / "simple_trainer.py"
    examples_root = os.environ.get("GSPLAT_EXAMPLES")
    if not trainer.exists() and examples_root:
        cand = Path(examples_root) / "simple_trainer.py"
        if cand.exists():
            trainer = cand

    if trainer.exists():
        cmd = [
            sys.executable,
            str(trainer),
            strategy,
            "--data_dir",
            str(data_dir),
            "--result_dir",
            str(result_dir),
            "--max_steps",
            str(max_steps),
            "--data_factor",
            str(data_factor),
            "--sh_degree",
            str(sh_degree),
            "--ssim_lambda",
            str(ssim),
            "--opacity_reg",
            str(opac_reg if strategy == "mcmc" else max(opac_reg, 1e-8)),
            "--scale_reg",
            str(scale_reg if strategy == "mcmc" else max(scale_reg, 1e-8)),
            "--disable_viewer",
            "--save_ply",
            "--disable_video",
            "--ply_steps",
            *[str(s) for s in ply_steps],
            "--save_steps",
            *[str(s) for s in ply_steps],
        ]
        if antialiased:
            cmd.append("--antialiased")
        if app_opt:
            cmd += ["--app_opt", "--app_embed_dim", "16"]
        if post:
            cmd += ["--post_processing", post]
        if strategy == "mcmc":
            cmd += ["--strategy.cap_max", str(max_splats)]
        elif absgrad:
            cmd += [
                "--strategy.absgrad",
                "True",
                "--strategy.grow_grad2d",
                "0.0008",
            ]
    else:
        # Minimal fallback train loop bundled beside this launcher.
        mini = here / "train_mini.py"
        if not mini.exists():
            _log(
                "gsplat-train: install nerfstudio gsplat and either set GSPLAT_EXAMPLES "
                "to the examples/ folder or place simple_trainer.py next to run.py. "
                "See README.md."
            )
            return 3
        cmd = [
            sys.executable,
            str(mini),
            "--data_dir",
            str(data_dir),
            "--result_dir",
            str(result_dir),
            "--export_dir",
            str(export_dir),
            "--max_steps",
            str(max_steps),
            "--max_splats",
            str(max_splats),
            "--export_every",
            str(export_every),
            "--sh_degree",
            str(sh_degree),
            "--strategy",
            strategy,
            "--ssim_weight",
            str(ssim),
            "--opacity_reg",
            str(opac_reg),
            "--scale_reg",
            str(scale_reg),
        ]
        if antialiased:
            cmd.append("--antialiased")
        if absgrad:
            cmd.append("--absgrad")
        init_ply = req.get("initPly")
        if init_ply and Path(init_ply).exists():
            cmd += ["--init_ply", init_ply]

    _log("gsplat-train: " + " ".join(cmd))
    env = os.environ.copy()
    env["PYTHONUNBUFFERED"] = "1"

    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env=env,
        cwd=str(trainer.parent if trainer.exists() else here),
    )
    assert proc.stdout is not None
    last_ply: Path | None = None
    for line in proc.stdout:
        line = line.rstrip()
        if not line:
            continue
        print(line, flush=True)
        # Progress parse hints for Rust watcher.
        low = line.lower()
        if "step" in low:
            digits = "".join(ch if ch.isdigit() else " " for ch in line)
            toks = [t for t in digits.split() if t.isdigit()]
            if toks:
                print(f"STEP {toks[0]}", flush=True)
        # Harvest ply exports into workspace/exports as they appear.
        for ply in result_dir.rglob("*.ply"):
            dest = export_dir / f"export_{_iter_from_name(ply)}.ply"
            if not dest.exists() or dest.stat().st_mtime < ply.stat().st_mtime:
                try:
                    shutil.copy2(ply, dest)
                    last_ply = dest
                    print(f"STEP {_iter_from_name(ply)}", flush=True)
                except OSError:
                    pass

    code = proc.wait()
    # Final harvest.
    candidates = sorted(export_dir.glob("export_*.ply"), key=lambda p: p.stat().st_mtime)
    if not candidates:
        candidates = sorted(result_dir.rglob("*.ply"), key=lambda p: p.stat().st_mtime)
    if code != 0 and not candidates:
        _log(f"gsplat-train failed ({code})")
        return code or 1
    final = candidates[-1] if candidates else last_ply
    if final is None:
        _log("gsplat-train produced no PLY")
        return 1
    out = workspace / "result_gsplat.ply"
    shutil.copy2(final, out)
    # Also stage as latest export for the live viewport.
    latest_export = export_dir / f"export_{max_steps}.ply"
    shutil.copy2(final, latest_export)
    print(str(out.resolve()), flush=True)
    return 0


def _iter_from_name(p: Path) -> int:
    stem = p.stem
    for part in stem.replace("-", "_").split("_"):
        if part.isdigit():
            return int(part)
    return int(time.time()) % 1_000_000


if __name__ == "__main__":
    raise SystemExit(main())
