#!/usr/bin/env python3
"""Shared helpers for InstaSplatter sidecar adapters.

All Standard / Experimental launchers should fail clearly when weights or
upstream packages are missing — never invent PLY / sparse outputs.
"""

from __future__ import annotations

import json
import struct
import sys
from pathlib import Path
from typing import Any, Iterable, Optional


IMAGE_EXTS = {".jpg", ".jpeg", ".png", ".webp", ".tif", ".tiff", ".bmp"}


def read_request() -> dict[str, Any]:
    raw = sys.stdin.read()
    if not raw.strip():
        raise SystemExit("empty stdin")
    return json.loads(raw)


def fail(msg: str, code: int = 1) -> int:
    print(f"# {msg}", file=sys.stderr)
    return code


def sidecar_root() -> Path:
    # run.py lives in tools/sidecars/<name>/ or engines/.../sidecars/<name>/
    return Path(__file__).resolve().parent.parent


def list_images(images_dir: Path) -> list[Path]:
    return sorted(
        p
        for p in images_dir.iterdir()
        if p.is_file() and p.suffix.lower() in IMAGE_EXTS
    )


def write_xyzrgb_ply(path: Path, xyz: Iterable[tuple[float, float, float]], rgb: Iterable[tuple[int, int, int]]) -> None:
    xyz_l = list(xyz)
    rgb_l = list(rgb)
    n = len(xyz_l)
    header = (
        "ply\nformat binary_little_endian 1.0\n"
        f"element vertex {n}\n"
        "property float x\nproperty float y\nproperty float z\n"
        "property uchar red\nproperty uchar green\nproperty uchar blue\n"
        "end_header\n"
    ).encode("ascii")
    with path.open("wb") as f:
        f.write(header)
        for (x, y, z), (r, g, b) in zip(xyz_l, rgb_l):
            f.write(struct.pack("<fffBBB", float(x), float(y), float(z), int(r), int(g), int(b)))


def marker_ready(root: Path) -> bool:
    """True when the user accepted terms / dropped weights / cloned upstream."""
    return any(
        (root / name).exists()
        for name in (
            "ACCEPTED",
            "weights.onnx",
            "weights.pt",
            "weights.pth",
            "checkpoint.pt",
            "checkpoint.pth",
            "model.pt",
            "model.onnx",
            "upstream",
            "repo",
        )
    )


def require_marker(root: Path, install_hint: str) -> Optional[int]:
    if marker_ready(root):
        return None
    return fail(
        f"weights/upstream not installed under {root}. {install_hint}",
        code=2,
    )


def parse_cameras_txt(sparse: Path):
    """Minimal COLMAP cameras.txt / images.txt reader."""
    cams = {}
    cam_path = sparse / "cameras.txt"
    img_path = sparse / "images.txt"
    if not cam_path.exists() or not img_path.exists():
        return None, None
    for line in cam_path.read_text(encoding="utf-8", errors="ignore").splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        cams[int(parts[0])] = parts
    images = []
    lines = img_path.read_text(encoding="utf-8", errors="ignore").splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        i += 1
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) < 10:
            continue
        images.append(
            {
                "id": int(parts[0]),
                "qvec": list(map(float, parts[1:5])),
                "tvec": list(map(float, parts[5:8])),
                "camera_id": int(parts[8]),
                "name": parts[9],
            }
        )
        if i < len(lines) and not lines[i].startswith("#"):
            i += 1
    return cams, images


def qvec_to_R(q):
    import numpy as np

    qw, qx, qy, qz = q
    return np.array(
        [
            [1 - 2 * (qy * qy + qz * qz), 2 * (qx * qy - qz * qw), 2 * (qx * qz + qy * qw)],
            [2 * (qx * qy + qz * qw), 1 - 2 * (qx * qx + qz * qz), 2 * (qy * qz - qx * qw)],
            [2 * (qx * qz - qy * qw), 2 * (qy * qz + qx * qw), 1 - 2 * (qx * qx + qy * qy)],
        ],
        dtype=np.float64,
    )


def densify_from_monocular_depth(
    images_dir: Path,
    sparse: Path,
    depth_fn,
    max_points: int,
    stride: int = 8,
):
    """Back-project monocular depth with COLMAP poses → XYZRGB.

    depth_fn(image_path) -> HxW float depth (metres arbitrary, relative OK).
    """
    try:
        import numpy as np
        from PIL import Image
    except Exception as e:
        raise RuntimeError(f"numpy/Pillow required ({e})") from e

    cams, images = parse_cameras_txt(sparse)
    if not images or not cams:
        raise RuntimeError("COLMAP sparse text model required for densify")

    def K_for(cam_id: int):
        parts = cams[cam_id]
        model = parts[1]
        w, h = float(parts[2]), float(parts[3])
        params = list(map(float, parts[4:]))
        if model in ("PINHOLE", "OPENCV", "SIMPLE_PINHOLE", "SIMPLE_RADIAL"):
            if model.startswith("SIMPLE"):
                f, cx, cy = params[0], params[1], params[2]
                return np.array([[f, 0, cx], [0, f, cy], [0, 0, 1]], dtype=np.float64)
            fx, fy, cx, cy = params[0], params[1], params[2], params[3]
            return np.array([[fx, 0, cx], [0, fy, cy], [0, 0, 1]], dtype=np.float64)
        f = params[0]
        return np.array([[f, 0, w / 2], [0, f, h / 2], [0, 0, 1]], dtype=np.float64)

    name_to_path = {p.name: p for p in list_images(images_dir)}
    xyz, rgb = [], []
    for im in images:
        path = name_to_path.get(im["name"])
        if path is None:
            continue
        depth = np.asarray(depth_fn(path), dtype=np.float64)
        if depth.ndim != 2 or depth.size < 16:
            continue
        K = K_for(im["camera_id"])
        R = qvec_to_R(im["qvec"])
        t = np.asarray(im["tvec"], dtype=np.float64).reshape(3, 1)
        rgb_img = np.asarray(Image.open(path).convert("RGB"))
        h, w = depth.shape
        ys = range(0, h, stride)
        xs = range(0, w, stride)
        for y in ys:
            for x in xs:
                z = float(depth[y, x])
                if not np.isfinite(z) or z <= 1e-4:
                    continue
                px = np.array([x, y, 1.0], dtype=np.float64)
                ray = np.linalg.inv(K) @ px
                Xc = ray * z
                Xw = (R.T @ (Xc.reshape(3, 1) - t)).ravel()
                yy = min(y, rgb_img.shape[0] - 1)
                xx = min(x, rgb_img.shape[1] - 1)
                col = rgb_img[yy, xx]
                xyz.append((float(Xw[0]), float(Xw[1]), float(Xw[2])))
                rgb.append((int(col[0]), int(col[1]), int(col[2])))
                if len(xyz) >= max_points:
                    return xyz, rgb
    if len(xyz) < 32:
        raise RuntimeError("depth densify produced too few points")
    return xyz, rgb
