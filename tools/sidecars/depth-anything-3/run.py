#!/usr/bin/env python3
"""Depth Anything 3 (Apache-2.0) densify sidecar for InstaSplatter.

Protocol: JSON stdin (task densify) → absolute XYZRGB PLY path on stdout.
Fails clearly when torch / DA3 package / weights / COLMAP sparse are missing.
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
    "See install.ps1 / README: pip install depth-anything-3 (or depth_anything_3) "
    "and place checkpoint/weights under this folder, then touch ACCEPTED."
)


def load_depth_model(root: Path):
    try:
        import torch  # noqa: F401
    except Exception as e:
        raise RuntimeError(f"PyTorch required ({e})") from e

    # Prefer explicit local checkpoint paths.
    ckpt_candidates = [
        root / "weights.pt",
        root / "weights.pth",
        root / "checkpoint.pt",
        root / "checkpoint.pth",
        root / "model.pt",
        root / "weights.onnx",
    ]
    ckpt = next((p for p in ckpt_candidates if p.exists()), None)

    # ONNX path
    if ckpt is not None and ckpt.suffix.lower() == ".onnx":
        try:
            import numpy as np
            import onnxruntime as ort  # type: ignore
            from PIL import Image
        except Exception as e:
            raise RuntimeError(f"onnxruntime/Pillow required for ONNX DA3 ({e})") from e
        sess = ort.InferenceSession(str(ckpt), providers=["CUDAExecutionProvider", "CPUExecutionProvider"])

        def depth_fn(path: Path):
            im = Image.open(path).convert("RGB")
            arr = np.asarray(im).astype("float32") / 255.0
            # NCHW resize to 518 as a common DA family size; ORT model may differ.
            import cv2  # type: ignore

            h, w = arr.shape[:2]
            resized = cv2.resize(arr, (518, 518), interpolation=cv2.INTER_AREA)
            inp = resized.transpose(2, 0, 1)[None]
            name = sess.get_inputs()[0].name
            out = sess.run(None, {name: inp})[0]
            d = np.asarray(out).squeeze().astype("float64")
            d = cv2.resize(d, (w, h), interpolation=cv2.INTER_LINEAR)
            # Relative depth → positive metric-ish via percentile scale.
            med = float(np.median(d[d > 0])) if np.any(d > 0) else 1.0
            return np.maximum(d / max(med, 1e-6), 1e-4)

        return depth_fn

    # Torch package path (depth_anything_3 / depth_anything3 / transformers).
    model = None
    err = None
    for import_name, factory in (
        ("depth_anything_3", lambda m: m.DepthAnything3.from_pretrained(str(ckpt) if ckpt else "depth-anything/DA3-BASE")),
        ("depth_anything3", lambda m: m.DepthAnything3.from_pretrained(str(ckpt) if ckpt else "depth-anything/DA3-BASE")),
    ):
        try:
            mod = __import__(import_name)
            model = factory(mod)
            break
        except Exception as e:
            err = e
    if model is None:
        try:
            from transformers import AutoModelForDepthEstimation, AutoImageProcessor  # type: ignore
            import torch
            from PIL import Image
            import numpy as np

            name = "depth-anything/Depth-Anything-V2-Small-hf" if ckpt is None else str(ckpt)
            processor = AutoImageProcessor.from_pretrained(name)
            hf_model = AutoModelForDepthEstimation.from_pretrained(name)
            hf_model.eval()

            def depth_fn(path: Path):
                im = Image.open(path).convert("RGB")
                inputs = processor(images=im, return_tensors="pt")
                with torch.no_grad():
                    out = hf_model(**inputs)
                d = out.predicted_depth.squeeze().cpu().numpy().astype("float64")
                d = np.maximum(d, 1e-4)
                # Resize to image
                import cv2  # type: ignore

                return cv2.resize(d, im.size, interpolation=cv2.INTER_LINEAR)

            return depth_fn
        except Exception as e:
            raise RuntimeError(
                f"Could not load Depth Anything 3/V2 runtime ({err}; hf fallback: {e}). {INSTALL}"
            ) from e

    import numpy as np
    from PIL import Image
    import torch

    model.eval()

    def depth_fn(path: Path):
        im = Image.open(path).convert("RGB")
        # Heuristic API surface across DA3 builds.
        if hasattr(model, "infer_image"):
            d = model.infer_image(im)
        elif hasattr(model, "predict"):
            d = model.predict(im)
        else:
            t = torch.from_numpy(np.asarray(im).astype("float32") / 255.0).permute(2, 0, 1)[None]
            with torch.no_grad():
                out = model(t)
            d = out[0] if isinstance(out, (tuple, list)) else out
            if hasattr(d, "detach"):
                d = d.detach().cpu().numpy()
        d = np.asarray(d).squeeze().astype("float64")
        if d.shape[:2] != (im.height, im.width):
            import cv2  # type: ignore

            d = cv2.resize(d, (im.width, im.height), interpolation=cv2.INTER_LINEAR)
        return np.maximum(d, 1e-4)

    return depth_fn


def main() -> int:
    req = read_request()
    task = (req.get("task") or "densify").lower()
    if task not in ("densify", "depth"):
        return fail(f"depth-anything-3 unsupported task={task}")

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
        return fail("COLMAP sparse model (images.txt) required")

    try:
        depth_fn = load_depth_model(HERE)
        xyz, rgb = densify_from_monocular_depth(images_dir, sparse, depth_fn, max_points)
    except Exception as e:
        return fail(f"depth-anything-3 densify unavailable: {e}. {INSTALL}")

    out_dir = workspace / "depth_anything_3"
    out_dir.mkdir(parents=True, exist_ok=True)
    out_ply = out_dir / "da3_dense.ply"
    write_xyzrgb_ply(out_ply, xyz, rgb)
    print(str(out_ply.resolve()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
