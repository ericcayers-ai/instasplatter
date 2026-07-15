#!/usr/bin/env python3
"""Depth Anything V2 (Apache-2.0) densify — legacy Standard densifier.

Prefer depth-anything-3 when available. Same protocol as DA3.
"""

from __future__ import annotations

import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "_common"))
sys.path.insert(0, str(HERE / "_common"))

from adapter_util import (  # noqa: E402
    densify_from_monocular_depth,
    fail,
    list_images,
    read_request,
    require_marker,
    write_xyzrgb_ply,
)

INSTALL = (
    "pip install torch transformers opencv-python pillow; "
    "place weights or rely on HF Depth-Anything-V2-Small-hf; touch ACCEPTED. See install.ps1."
)


def load_depth_model(root: Path):
    try:
        import torch
        from transformers import AutoModelForDepthEstimation, AutoImageProcessor  # type: ignore
        from PIL import Image
        import numpy as np
        import cv2  # type: ignore
    except Exception as e:
        raise RuntimeError(f"torch/transformers/opencv required ({e})") from e

    ckpt = next(
        (
            p
            for p in (
                root / "weights.pt",
                root / "weights.pth",
                root / "weights.onnx",
                root / "model.pt",
            )
            if p.exists()
        ),
        None,
    )
    name = str(ckpt) if ckpt and ckpt.suffix != ".onnx" else "depth-anything/Depth-Anything-V2-Small-hf"
    if ckpt is not None and ckpt.suffix.lower() == ".onnx":
        import onnxruntime as ort  # type: ignore

        sess = ort.InferenceSession(str(ckpt), providers=["CUDAExecutionProvider", "CPUExecutionProvider"])

        def depth_fn(path: Path):
            im = Image.open(path).convert("RGB")
            arr = np.asarray(im).astype("float32") / 255.0
            h, w = arr.shape[:2]
            resized = cv2.resize(arr, (518, 518), interpolation=cv2.INTER_AREA)
            inp = resized.transpose(2, 0, 1)[None]
            out = sess.run(None, {sess.get_inputs()[0].name: inp})[0]
            d = np.asarray(out).squeeze().astype("float64")
            return cv2.resize(np.maximum(d, 1e-4), (w, h), interpolation=cv2.INTER_LINEAR)

        return depth_fn

    processor = AutoImageProcessor.from_pretrained(name)
    model = AutoModelForDepthEstimation.from_pretrained(name)
    model.eval()

    def depth_fn(path: Path):
        im = Image.open(path).convert("RGB")
        inputs = processor(images=im, return_tensors="pt")
        with torch.no_grad():
            out = model(**inputs)
        d = out.predicted_depth.squeeze().cpu().numpy().astype("float64")
        return cv2.resize(np.maximum(d, 1e-4), im.size, interpolation=cv2.INTER_LINEAR)

    return depth_fn


def main() -> int:
    req = read_request()
    task = (req.get("task") or "densify").lower()
    if task not in ("densify", "depth"):
        return fail(f"depth-anything-v2 unsupported task={task}")
    hint = require_marker(HERE, INSTALL)
    if hint is not None:
        return hint
    images_dir = Path(req.get("imagesDir") or req["images_dir"])
    workspace = Path(req["workspace"])
    sparse = Path(req.get("sparseDir") or req.get("sparse_dir") or (workspace / "sparse" / "0"))
    max_points = int(req.get("maxPoints") or req.get("max_points") or 1_200_000)
    if not list_images(images_dir):
        return fail("no images found")
    if not (sparse / "images.txt").exists():
        return fail("COLMAP sparse model required")
    try:
        xyz, rgb = densify_from_monocular_depth(
            images_dir, sparse, load_depth_model(HERE), max_points
        )
    except Exception as e:
        return fail(f"depth-anything-v2 densify unavailable: {e}. {INSTALL}")
    out_dir = workspace / "depth_anything_v2"
    out_dir.mkdir(parents=True, exist_ok=True)
    out_ply = out_dir / "dav2_dense.ply"
    write_xyzrgb_ply(out_ply, xyz, rgb)
    print(str(out_ply.resolve()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
