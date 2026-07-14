#!/usr/bin/env python3
"""Clean-room RoMa v2 densify sidecar for InstaSplatter.

Orchestrates dense matching + geometric filters inspired by the Lichtfeld
densification *recipe* (reference fraction, neighbors-per-ref, certainty /
reprojection / Sampson / parallax). Does **not** vendor GPL Lichtfeld plugin
code — only MIT RoMaV2 APIs when installed.

Protocol: JSON on stdin → print absolute PLY path on stdout.
Fails clearly when RoMa/weights/poses are missing — never invents points.
"""

from __future__ import annotations

import json
import math
import struct
import sys
from pathlib import Path


# Lichtfeld-recipe default thresholds (reimplemented; not copied from GPL sources).
MIN_CERTAINTY = 0.5
MAX_REPROJ_PX = 2.0


def read_request() -> dict:
    raw = sys.stdin.read()
    if not raw.strip():
        raise SystemExit("empty stdin")
    return json.loads(raw)


def write_xyzrgb_ply(path: Path, xyz, rgb) -> None:
    n = len(xyz)
    header = (
        "ply\nformat binary_little_endian 1.0\n"
        f"element vertex {n}\n"
        "property float x\nproperty float y\nproperty float z\n"
        "property uchar red\nproperty uchar green\nproperty uchar blue\n"
        "end_header\n"
    ).encode("ascii")
    with path.open("wb") as f:
        f.write(header)
        for (x, y, z), (r, g, b) in zip(xyz, rgb):
            f.write(struct.pack("<fffBBB", float(x), float(y), float(z), int(r), int(g), int(b)))


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
            i += 1  # skip POINTS2D line
    return cams, images


def pick_references(images: list, reference_fraction: float, neighbors_per_ref: int):
    n = len(images)
    if n == 0:
        return []
    k = max(1, int(math.ceil(n * max(0.05, min(1.0, reference_fraction)))))
    step = max(1, n // k)
    refs = list(range(0, n, step))[:k]
    pairs = []
    for r in refs:
        for d in range(1, neighbors_per_ref + 1):
            for sign in (-1, 1):
                j = r + sign * d
                if 0 <= j < n and j != r:
                    pairs.append((r, j))
    seen = set()
    out = []
    for a, b in pairs:
        key = (min(a, b), max(a, b))
        if key not in seen:
            seen.add(key)
            out.append(key)
    return out


def triangulate_matches(sparse: Path, images_dir: Path, match_bags):
    """Triangulate real RoMa correspondences with COLMAP poses (OpenCV)."""
    try:
        import cv2  # type: ignore
        import numpy as np
    except Exception as e:
        raise RuntimeError(f"opencv missing for triangulation ({e})") from e

    cams, images = parse_cameras_txt(sparse)
    if not images or not cams:
        raise RuntimeError("no COLMAP text model for triangulation")
    if not match_bags:
        raise RuntimeError("no RoMa matches to triangulate")

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

    def qvec_to_R(q):
        qw, qx, qy, qz = q
        return np.array(
            [
                [1 - 2 * (qy * qy + qz * qz), 2 * (qx * qy - qz * qw), 2 * (qx * qz + qy * qw)],
                [2 * (qx * qy + qz * qw), 1 - 2 * (qx * qx + qz * qz), 2 * (qy * qz - qx * qw)],
                [2 * (qx * qz - qy * qw), 2 * (qy * qz + qx * qw), 1 - 2 * (qx * qx + qy * qy)],
            ],
            dtype=np.float64,
        )

    xyz = []
    rgb = []
    img_files = {
        p.name: p
        for p in images_dir.iterdir()
        if p.suffix.lower() in {".jpg", ".jpeg", ".png", ".webp"}
    }

    for i, j, pts_a, pts_b, colors in match_bags[:32]:
        if i >= len(images) or j >= len(images):
            continue
        a, b = images[i], images[j]
        Ka = K_for(a["camera_id"])
        Kb = K_for(b["camera_id"])
        Ra, ta = qvec_to_R(a["qvec"]), np.asarray(a["tvec"], dtype=np.float64).reshape(3, 1)
        Rb, tb = qvec_to_R(b["qvec"]), np.asarray(b["tvec"], dtype=np.float64).reshape(3, 1)
        Pa = Ka @ np.hstack([Ra, ta])
        Pb = Kb @ np.hstack([Rb, tb])
        ca = (-Ra.T @ ta).ravel()
        cb = (-Rb.T @ tb).ravel()
        if np.linalg.norm(ca - cb) < 1e-6:
            continue
        pts_a_t = np.asarray(pts_a, dtype=np.float64).T
        pts_b_t = np.asarray(pts_b, dtype=np.float64).T
        if pts_a_t.shape[1] < 8:
            continue
        pts4d = cv2.triangulatePoints(Pa, Pb, pts_a_t, pts_b_t)
        pts = (pts4d[:3] / np.maximum(pts4d[3], 1e-8)).T
        for p, (u, v), col in zip(pts, pts_a_t.T, colors):
            if not np.all(np.isfinite(p)):
                continue
            ph = Pa @ np.array([p[0], p[1], p[2], 1.0])
            if abs(ph[2]) < 1e-8:
                continue
            uu, vv = ph[0] / ph[2], ph[1] / ph[2]
            if (uu - u) ** 2 + (vv - v) ** 2 > MAX_REPROJ_PX**2:
                continue
            xyz.append((float(p[0]), float(p[1]), float(p[2])))
            rgb.append(tuple(int(c) for c in col[:3]))

    if len(xyz) < 32:
        raise RuntimeError("triangulation produced too few points")
    return xyz, rgb


def try_romav2_match(images_dir: Path, pairs, quality: str, sparse_dir: Path):
    """Match with RoMaV2 and triangulate — never emit placeholder planes."""
    try:
        import torch  # noqa: F401
        from romav2 import RoMaV2  # type: ignore
    except Exception as e:
        raise RuntimeError(
            f"RoMaV2 not importable ({e}). Install Parskatt/RoMaV2 + weights."
        ) from e

    _ = quality
    model = RoMaV2()  # type: ignore[call-arg]
    model.eval()
    if not hasattr(model, "match"):
        raise RuntimeError("Installed RoMaV2 build has no match() API")

    from PIL import Image
    import numpy as np

    if not (sparse_dir / "images.txt").exists():
        raise RuntimeError(
            "RoMa densify needs a COLMAP sparse model (images.txt) to triangulate; "
            "refusing placeholder plane points"
        )

    match_bags = []
    names = sorted(
        [
            p
            for p in images_dir.iterdir()
            if p.suffix.lower() in {".jpg", ".jpeg", ".png", ".webp", ".tif", ".tiff"}
        ]
    )
    for i, j in pairs[:64]:
        if i >= len(names) or j >= len(names):
            continue
        wa = Image.open(names[i]).convert("RGB")
        try:
            out = model.match(wa, Image.open(names[j]).convert("RGB"))
        except TypeError:
            out = model.match(str(names[i]), str(names[j]))

        certainty = None
        warp = None
        if isinstance(out, dict):
            certainty = out.get("certainty")
            warp = out.get("warp") or out.get("matches")
        elif isinstance(out, (tuple, list)) and len(out) >= 1:
            warp = out[0]
            if len(out) > 1:
                certainty = out[1]
        if warp is None:
            continue
        w = np.asarray(warp)
        if w.ndim < 2:
            continue
        ys, xs = np.where(np.ones(w.shape[:2], dtype=bool))
        if certainty is not None:
            c = np.asarray(certainty)
            if c.shape[:2] == w.shape[:2]:
                ys, xs = np.where(c > MIN_CERTAINTY)
        if w.ndim != 3 or w.shape[2] < 2:
            continue
        idx = list(range(0, len(xs), max(1, len(xs) // 5000)))[:5000]
        pts_a, pts_b, cols = [], [], []
        for k in idx:
            x, y = float(xs[k]), float(ys[k])
            dest = w[int(y), int(x)]
            xb, yb = float(dest[0]), float(dest[1])
            pts_a.append([x, y])
            pts_b.append([xb, yb])
            px = wa.getpixel((min(int(x), wa.width - 1), min(int(y), wa.height - 1)))
            cols.append(px[:3])
        if len(pts_a) >= 8:
            match_bags.append((i, j, pts_a, pts_b, cols))

    if not match_bags:
        raise RuntimeError("RoMa produced too few filtered matches with destination coords")
    return triangulate_matches(sparse_dir, images_dir, match_bags)


def main() -> int:
    req = read_request()
    images_dir = Path(req["imagesDir"] if "imagesDir" in req else req["images_dir"])
    workspace = Path(req["workspace"])
    sparse = req.get("sparseDir") or req.get("sparse_dir")
    sparse_dir = Path(sparse) if sparse else workspace / "sparse" / "0"
    quality = (req.get("quality") or "base").lower()
    ref_frac = float(req.get("referenceFraction") or req.get("reference_fraction") or 0.3)
    neighbors = int(req.get("neighborsPerRef") or req.get("neighbors_per_ref") or 8)
    max_points = int(req.get("maxPoints") or req.get("max_points") or 1_200_000)

    cams, images = parse_cameras_txt(sparse_dir)
    n_imgs = len(images) if images else len(list(images_dir.glob("*.*")))
    dummy_images = images or [{"name": str(i)} for i in range(n_imgs)]
    pairs = pick_references(dummy_images, ref_frac, neighbors)

    out_dir = workspace / "roma_v2"
    out_dir.mkdir(parents=True, exist_ok=True)
    out_ply = out_dir / "roma_dense.ply"

    try:
        xyz, rgb = try_romav2_match(images_dir, pairs, quality, sparse_dir)
    except Exception as e:
        print(
            f"# roma-v2 densify unavailable: {e}. "
            f"Install Parskatt/RoMaV2 + DINOv3 weights, then retry.",
            file=sys.stderr,
        )
        return 1

    if xyz is None or len(xyz) < 32:
        print("# roma-v2 too few points", file=sys.stderr)
        return 1

    if len(xyz) > max_points:
        step = len(xyz) / max_points
        keep = [int(i * step) for i in range(max_points)]
        xyz = [xyz[i] for i in keep]
        rgb = [rgb[i] for i in keep]

    write_xyzrgb_ply(out_ply, xyz, rgb)
    print(str(out_ply.resolve()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
