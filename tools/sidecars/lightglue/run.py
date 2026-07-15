#!/usr/bin/env python3
"""LightGlue matcher adapter for InstaSplatter.

When SuperPoint/ALIKED + LightGlue are installed, writes COLMAP-compatible
match pairs under workspace for the host / COLMAP import path.
Fails clearly when packages or weights are missing — never fakes matches.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "_common"))
sys.path.insert(0, str(HERE / "_common"))

from adapter_util import fail, list_images, read_request, require_marker  # noqa: E402

INSTALL = (
    "pip install lightglue opencv-python torch torchvision kornia; "
    "or clone cvg/LightGlue under ./upstream and touch ACCEPTED. See install.ps1."
)


def run_lightglue(images: list[Path], workspace: Path, max_pairs: int = 64) -> Path:
    try:
        import torch
        from lightglue import LightGlue, SuperPoint  # type: ignore
        from lightglue.utils import load_image, rbd  # type: ignore
    except Exception as e:
        # Try upstream checkout.
        upstream = HERE / "upstream"
        if (upstream / "lightglue").exists():
            sys.path.insert(0, str(upstream))
            from lightglue import LightGlue, SuperPoint  # type: ignore
            from lightglue.utils import load_image, rbd  # type: ignore
            import torch
        else:
            raise RuntimeError(f"LightGlue not importable ({e}). {INSTALL}") from e

    device = "cuda" if torch.cuda.is_available() else "cpu"
    extractor = SuperPoint(max_num_keypoints=2048).eval().to(device)
    matcher = LightGlue(features="superpoint").eval().to(device)

    out = workspace / "lightglue"
    out.mkdir(parents=True, exist_ok=True)
    pairs_path = out / "pairs.txt"
    matches_dir = out / "matches"
    matches_dir.mkdir(exist_ok=True)

    n = len(images)
    pair_lines = []
    # Sequential + stride pairs (COLMAP-friendly).
    for i in range(n):
        for d in (1, 2, 4, 8):
            j = i + d
            if j < n:
                pair_lines.append((i, j))
    pair_lines = pair_lines[:max_pairs]

    written = 0
    with pairs_path.open("w", encoding="utf-8") as pf:
        for i, j in pair_lines:
            try:
                feats0 = extractor.extract(load_image(str(images[i])).to(device))
                feats1 = extractor.extract(load_image(str(images[j])).to(device))
                matches01 = matcher({"image0": feats0, "image1": feats1})
                feats0, feats1, matches01 = [rbd(x) for x in (feats0, feats1, matches01)]
                kpts0 = feats0["keypoints"].cpu().numpy()
                kpts1 = feats1["keypoints"].cpu().numpy()
                m = matches01["matches"].cpu().numpy()
                if m.ndim != 2 or m.shape[0] < 16:
                    continue
                match_file = matches_dir / f"{images[i].stem}__{images[j].stem}.txt"
                with match_file.open("w", encoding="utf-8") as mf:
                    for a, b in m:
                        mf.write(
                            f"{kpts0[int(a)][0]} {kpts0[int(a)][1]} "
                            f"{kpts1[int(b)][0]} {kpts1[int(b)][1]}\n"
                        )
                pf.write(f"{images[i].name} {images[j].name}\n")
                written += 1
            except Exception:
                continue

    if written < 1:
        raise RuntimeError("LightGlue produced no usable pairs")

    # Marker file the host can detect; COLMAP import still uses SIFT unless
    # a dedicated importer is added — this adapter validates the engine path.
    ok = out / "OK"
    ok.write_text(f"pairs={written}\n", encoding="utf-8")
    # Print path so invoke_launcher treats success; host pose path still COLMAP.
    print(str(ok.resolve()))
    return ok


def main() -> int:
    req = read_request()
    task = (req.get("task") or "match").lower()
    if task not in ("match", "sfm", "densify"):
        return fail(f"lightglue unsupported task={task}")

    hint = require_marker(HERE, INSTALL)
    if hint is not None:
        return hint

    images_dir = Path(req.get("imagesDir") or req["images_dir"])
    workspace = Path(req["workspace"])
    images = list_images(images_dir)
    if len(images) < 2:
        return fail("need at least 2 images")

    try:
        run_lightglue(images, workspace)
        return 0
    except Exception as e:
        return fail(f"lightglue unavailable: {e}")


if __name__ == "__main__":
    raise SystemExit(main())
