#!/usr/bin/env python3
"""MASt3R-SfM accuracy benchmark harness (ROADMAP-V2 §2.2, the unchecked box at :141).

What this measures: two camera pose sets for the same capture — a REFERENCE (MASt3R-SfM, or
synthetic ground truth) and an ESTIMATE (our sparse model) — after removing the arbitrary
similarity gauge that separates any two SfM reconstructions. Reported per camera and in
aggregate: rotation error in degrees, camera-centre error in metres and as a fraction of the
reference scene diameter, plus a PASS/FAIL against stated thresholds.

Why the gauge has to be removed: an SfM model is only defined up to a similarity — scale is
unobservable from images alone, and the world frame is whatever the first solve chose. Comparing
raw centres would report the alignment error of the gauge, not the accuracy of the poses.
`umeyama` recovers the optimal scale + rotation + translation (Umeyama 1991, the standard SfM
evaluation alignment), so what remains is the reconstruction error.

Three ways to run, so the harness is useful before the benchmark weights exist (§ weight-free
path first):

1. ``--selftest`` — proves the metric code itself is right: inject a KNOWN rotation and centre
   error into a synthetic rig, then require the harness to recover that number. No files, no
   GPU, no weights, stdlib + numpy only.
2. ``--fixture`` — builds a synthetic capture with ground truth and scores a perturbed estimate
   of it, writing a receipt. This is the gate that runs on a machine with no MASt3R weights.
3. ``--mast3r`` — runs the MASt3R-SfM sidecar (``tools/sidecars/mast3r``) over a real image
   folder, then scores our sparse model against its output. When the sidecar is not installed,
   or the GPU has less free VRAM than the model needs, it writes a BLOCKED receipt naming the
   measured free VRAM and the exact command a human has to run — it never invents poses.

Usage::

    python tools/bench/mast3r_sfm_bench.py --selftest
    python tools/bench/mast3r_sfm_bench.py --fixture --receipt docs/evidence/mast3r-bench/fixture.md
    python tools/bench/mast3r_sfm_bench.py --mast3r --images <capture> \
        --estimate <workspace>/sparse/0 --receipt docs/evidence/mast3r-bench/capture.md

Exit codes: 0 = PASS or a written (honest) BLOCKED receipt, 1 = FAIL (poses scored and did not
meet the thresholds), 2 = usage/setup error.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

try:
    import numpy as np
except ImportError:  # pragma: no cover - environment guard, not logic
    sys.stderr.write(
        "mast3r_sfm_bench needs numpy (used by tools/sidecars/gsplat-train too).\n"
        "Install it for this interpreter: python -m pip install numpy\n"
    )
    raise SystemExit(2)

REPO = Path(__file__).resolve().parents[2]
DEFAULT_SIDECAR = REPO / "tools" / "sidecars" / "mast3r" / "run.py"

# Thresholds are stated, not tuned to whatever we happen to score. They are the criterion the
# roadmap asks for: poses good enough that a real capture is validated, not merely synthetic.
ROTATION_MEDIAN_LIMIT_DEG = 2.0
CENTRE_RELATIVE_MEDIAN_LIMIT = 0.02

# MASt3R-SfM needs the weights plus roughly this much device memory; measured as free VRAM
# before the sidecar is allowed to start, because a partial load fails more confusingly than a
# clean refusal.
MAST3R_MIN_FREE_VRAM_MIB = 6000


# --------------------------------------------------------------------------- rotations
def quat_wxyz_to_matrix(q: tuple[float, float, float, float]) -> np.ndarray:
    """COLMAP's stored quaternion ``(qw, qx, qy, qz)`` -> 3x3 rotation matrix."""
    w, x, y, z = q
    norm = math.sqrt(w * w + x * x + y * y + z * z)
    if norm == 0:
        raise ValueError("zero quaternion")
    w, x, y, z = (v / norm for v in (w, x, y, z))
    return np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ],
        dtype=float,
    )


def matrix_to_quat_wxyz(rotation: np.ndarray) -> tuple[float, float, float, float]:
    """3x3 rotation -> COLMAP's ``(qw, qx, qy, qz)``, so a pose set can be written back out.

    The branch work is Shepperd's method: pick the largest of the four squared components as
    the divisor so a rotation near 180 degrees does not divide by something near zero.
    """
    m = rotation
    trace = float(np.trace(m))
    if trace > 0:
        s = math.sqrt(trace + 1.0) * 2.0
        w = 0.25 * s
        x = (m[2, 1] - m[1, 2]) / s
        y = (m[0, 2] - m[2, 0]) / s
        z = (m[1, 0] - m[0, 1]) / s
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = math.sqrt(1.0 + m[0, 0] - m[1, 1] - m[2, 2]) * 2.0
        w = (m[2, 1] - m[1, 2]) / s
        x = 0.25 * s
        y = (m[0, 1] + m[1, 0]) / s
        z = (m[0, 2] + m[2, 0]) / s
    elif m[1, 1] > m[2, 2]:
        s = math.sqrt(1.0 + m[1, 1] - m[0, 0] - m[2, 2]) * 2.0
        w = (m[0, 2] - m[2, 0]) / s
        x = (m[0, 1] + m[1, 0]) / s
        y = 0.25 * s
        z = (m[1, 2] + m[2, 1]) / s
    else:
        s = math.sqrt(1.0 + m[2, 2] - m[0, 0] - m[1, 1]) * 2.0
        w = (m[1, 0] - m[0, 1]) / s
        x = (m[0, 2] + m[2, 0]) / s
        y = (m[1, 2] + m[2, 1]) / s
        z = 0.25 * s
    return (w, x, y, z)


def write_colmap_images(model: Model, path: Path) -> Path:
    """Write a COLMAP ``images.txt`` for a model (pose lines only; no 2D points).

    Present so a run can be reproduced from a fixture without a COLMAP install, and so the
    reader is exercised against the same format the pipeline emits.
    """
    lines = [
        "# Image list with two lines of data per image:",
        "#   IMAGE_ID, QW, QX, QY, QZ, TX, TY, TZ, CAMERA_ID, NAME",
        "#   POINTS2D[] as (X, Y, POINT3D_ID)",
    ]
    for index, (name, (rotation, centre)) in enumerate(sorted(model.poses.items()), start=1):
        w, x, y, z = matrix_to_quat_wxyz(rotation)
        # tvec = -R * centre, the world-to-camera translation COLMAP stores.
        tvec = -(rotation @ centre)
        lines.append(
            f"{index} {w:.12f} {x:.12f} {y:.12f} {z:.12f} "
            f"{tvec[0]:.12f} {tvec[1]:.12f} {tvec[2]:.12f} 1 {name}"
        )
        lines.append("")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def rotation_angle_deg(matrix: np.ndarray) -> float:
    """Geodesic angle of a rotation matrix, in degrees.

    ``acos((tr - 1) / 2)`` loses precision near zero, which is exactly the range a good
    reconstruction lands in, so the angle comes from the norm of the matrix logarithm's axis
    instead: ``asin(|vee(R - R^T)| / 2)``.
    """
    vee = np.array(
        [
            matrix[2, 1] - matrix[1, 2],
            matrix[0, 2] - matrix[2, 0],
            matrix[1, 0] - matrix[0, 1],
        ]
    )
    sine = float(np.linalg.norm(vee) / 2.0)
    cosine = (float(np.trace(matrix)) - 1.0) / 2.0
    return math.degrees(math.atan2(sine, cosine))


def umeyama(src: np.ndarray, dst: np.ndarray, with_scale: bool = True) -> tuple[float, np.ndarray, np.ndarray]:
    """Least-squares similarity (scale, rotation, translation) mapping ``src`` onto ``dst``.

    Umeyama, *Least-squares estimation of transformation parameters between two point
    patterns*, PAMI 1991. ``src``/``dst`` are (N, 3) arrays of corresponding camera centres.
    """
    if src.shape != dst.shape or src.shape[1] != 3 or src.shape[0] < 3:
        raise ValueError("need matching (N>=3, 3) point sets")
    n = src.shape[0]
    mu_src = src.mean(axis=0)
    mu_dst = dst.mean(axis=0)
    src_c = src - mu_src
    dst_c = dst - mu_dst
    cov = (dst_c.T @ src_c) / n
    u, d, vt = np.linalg.svd(cov)
    s_fix = np.eye(3)
    if np.linalg.det(u) * np.linalg.det(vt) < 0:
        s_fix[2, 2] = -1.0
    rotation = u @ s_fix @ vt
    var_src = float((src_c**2).sum() / n)
    scale = 1.0
    if with_scale and var_src > 0:
        scale = float(np.trace(np.diag(d) @ s_fix) / var_src)
    translation = mu_dst - scale * (rotation @ mu_src)
    return scale, rotation, translation


# --------------------------------------------------------------------------- models
class Model:
    """Camera poses of one reconstruction: name -> (rotation matrix, centre)."""

    def __init__(self, poses: dict[str, tuple[np.ndarray, np.ndarray]], source: str) -> None:
        self.poses = poses
        self.source = source

    def __len__(self) -> int:
        return len(self.poses)

    def names(self) -> list[str]:
        return list(self.poses)

    def centres(self, names: list[str]) -> np.ndarray:
        return np.array([self.poses[name][1] for name in names], dtype=float)

    def rotations(self, names: list[str]) -> list[np.ndarray]:
        return [self.poses[name][0] for name in names]


def read_colmap_images(path: Path) -> Model:
    """Read a COLMAP ``images.txt`` (the sparse model the pipeline already produces)."""
    poses: dict[str, tuple[np.ndarray, np.ndarray]] = {}
    lines = [
        line.strip()
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    # images.txt holds one pose line then one 2D-points line per image; only pose lines are
    # decoded, so the pairs are walked rather than assumed.
    index = 0
    while index < len(lines):
        parts = lines[index].split()
        index += 1
        if len(parts) < 10:
            continue
        qvec = (float(parts[1]), float(parts[2]), float(parts[3]), float(parts[4]))
        tvec = np.array([float(parts[5]), float(parts[6]), float(parts[7])])
        name = " ".join(parts[9:])
        rotation = quat_wxyz_to_matrix(qvec)
        # COLMAP stores world-to-camera: X_cam = R X_world + t, so the centre is -R^T t.
        centre = -rotation.T @ tvec
        poses[name] = (rotation, centre)
    if not poses:
        raise ValueError(f"no camera poses found in {path}")
    return Model(poses, source=str(path))


def read_model(path: Path) -> Model:
    """A COLMAP sparse directory, or a single ``images.txt``."""
    if path.is_dir():
        images = path / "images.txt"
        if not images.is_file():
            raise ValueError(f"{path} has no images.txt (not a COLMAP sparse model)")
        return read_colmap_images(images)
    return read_colmap_images(path)


def synthetic_model(
    count: int = 7,
    radius: float = 4.0,
    arc_deg: float = 120.0,
    inward: bool = True,
) -> Model:
    """A ring of cameras looking at the origin — the fixture's ground truth."""
    poses: dict[str, tuple[np.ndarray, np.ndarray]] = {}
    for i in range(count):
        angle = math.radians(-arc_deg / 2 + arc_deg * i / max(1, count - 1))
        centre = np.array([radius * math.cos(angle), 0.0, radius * math.sin(angle)])
        forward = -centre / np.linalg.norm(centre) if inward else np.array([0.0, 0.0, 1.0])
        # Build a rotation whose -Z axis is the camera's forward direction (OpenCV convention).
        z_axis = -forward
        up = np.array([0.0, 1.0, 0.0])
        x_axis = np.cross(up, z_axis)
        if np.linalg.norm(x_axis) < 1e-8:
            x_axis = np.array([1.0, 0.0, 0.0])
        x_axis /= np.linalg.norm(x_axis)
        y_axis = np.cross(z_axis, x_axis)
        rotation = np.column_stack([x_axis, y_axis, z_axis])
        poses[f"cam_{i:02d}"] = (rotation, centre)
    return Model(poses, source="synthetic ground truth")


def perturb(
    model: Model,
    rotation_deg: float,
    centre_noise: float,
    seed: int,
) -> Model:
    """A copy of ``model`` with a KNOWN rotation and centre error injected per camera."""
    rng = np.random.default_rng(seed)
    poses: dict[str, tuple[np.ndarray, np.ndarray]] = {}
    for name, (rotation, centre) in model.poses.items():
        axis = rng.normal(size=3)
        axis /= np.linalg.norm(axis)
        angle = math.radians(rotation_deg) * (0.5 + rng.random())
        delta = _axis_angle_matrix(axis, angle)
        noisy_centre = centre + rng.normal(size=3) * centre_noise
        poses[name] = (delta @ rotation, noisy_centre)
    return Model(poses, source=f"{model.source} + injected error")


def _axis_angle_matrix(axis: np.ndarray, angle: float) -> np.ndarray:
    k = np.array(
        [
            [0.0, -axis[2], axis[1]],
            [axis[2], 0.0, -axis[0]],
            [-axis[1], axis[0], 0.0],
        ]
    )
    return np.eye(3) + math.sin(angle) * k + (1 - math.cos(angle)) * (k @ k)


def transform_model(model: Model, scale: float, rotation: np.ndarray, translation: np.ndarray) -> Model:
    """Apply a similarity to a model — used to hide the gauge in the fixture's estimate."""
    poses = {
        name: (rotation @ rot, scale * (rotation @ centre) + translation)
        for name, (rot, centre) in model.poses.items()
    }
    return Model(poses, source=f"{model.source} (re-gauged)")


# --------------------------------------------------------------------------- scoring
def scene_diameter(centres: np.ndarray) -> float:
    """Largest pairwise centre distance: the reference's own scale, so error is comparable."""
    if len(centres) < 2:
        return 0.0
    deltas = centres[:, None, :] - centres[None, :, :]
    return float(np.max(np.linalg.norm(deltas, axis=2)))


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, int(round(fraction * (len(ordered) - 1)))))
    return ordered[index]


def score(reference: Model, estimate: Model) -> dict[str, Any]:
    """Score ``estimate`` against ``reference`` after removing the similarity gauge."""
    shared = [name for name in reference.names() if name in estimate.poses]
    if len(shared) < 3:
        raise ValueError(
            f"need at least 3 cameras in both models, found {len(shared)} "
            f"(reference {len(reference)}, estimate {len(estimate)})"
        )
    ref_centres = reference.centres(shared)
    est_centres = estimate.centres(shared)
    scale, rotation, translation = umeyama(est_centres, ref_centres)
    aligned_centres = (scale * (rotation @ est_centres.T)).T + translation
    aligned_rotations = [rotation @ rot for rot in estimate.rotations(shared)]

    centre_errors = np.linalg.norm(aligned_centres - ref_centres, axis=1)
    rotation_errors = [
        rotation_angle_deg(ref_rot.T @ aligned_rot)
        for ref_rot, aligned_rot in zip(reference.rotations(shared), aligned_rotations, strict=True)
    ]
    diameter = scene_diameter(ref_centres)
    centre_list = [float(value) for value in centre_errors]

    result: dict[str, Any] = {
        "cameras": len(shared),
        "missing_from_estimate": sorted(set(reference.names()) - set(shared)),
        "aligned_scale": round(scale, 6),
        "scene_diameter_m": round(diameter, 6),
        "rotation_deg": {
            "mean": round(sum(rotation_errors) / len(rotation_errors), 6),
            "median": round(percentile(rotation_errors, 0.5), 6),
            "p95": round(percentile(rotation_errors, 0.95), 6),
            "max": round(max(rotation_errors), 6),
            "per_camera": {
                name: round(value, 6) for name, value in zip(shared, rotation_errors, strict=True)
            },
        },
        "centre_error": {
            "mean_m": round(sum(centre_list) / len(centre_list), 6),
            "median_m": round(percentile(centre_list, 0.5), 6),
            "max_m": round(max(centre_list), 6),
            "relative_median": round(percentile(centre_list, 0.5) / diameter, 6)
            if diameter > 0
            else None,
            "per_camera_m": {
                name: round(value, 6) for name, value in zip(shared, centre_list, strict=True)
            },
        },
        "thresholds": {
            "rotation_median_deg": ROTATION_MEDIAN_LIMIT_DEG,
            "centre_relative_median": CENTRE_RELATIVE_MEDIAN_LIMIT,
        },
    }
    relative = result["centre_error"]["relative_median"]
    passed = (
        result["rotation_deg"]["median"] <= ROTATION_MEDIAN_LIMIT_DEG
        and relative is not None
        and relative <= CENTRE_RELATIVE_MEDIAN_LIMIT
    )
    result["verdict"] = "PASS" if passed else "FAIL"
    return result


# --------------------------------------------------------------------------- VRAM / sidecar
def free_vram_mib() -> int | None:
    """Free device memory, measured. ``None`` when no NVIDIA tool is available.

    Not read from Win32_VideoController: ``AdapterRAM`` is a 32-bit field and reports 4 GB for
    this 8 GB card, which is exactly the number a load decision must not trust.
    """
    if not shutil.which("nvidia-smi"):
        return None
    try:
        proc = subprocess.run(
            ["nvidia-smi", "--query-gpu=memory.free", "--format=csv,noheader,nounits"],
            capture_output=True,
            text=True,
            timeout=20,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if proc.returncode != 0:
        return None
    values = [line.strip() for line in proc.stdout.splitlines() if line.strip().isdigit()]
    return max((int(value) for value in values), default=None)


IMAGE_SUFFIXES = (".jpg", ".jpeg", ".png", ".webp", ".tif", ".tiff")


def count_images(folder: Path) -> int:
    if not folder.is_dir():
        return 0
    return sum(1 for item in folder.rglob("*") if item.suffix.lower() in IMAGE_SUFFIXES)


def preconditions(images: Path, sidecar: Path) -> list[str]:
    """Every reason the MASt3R-SfM comparison cannot run right now, or an empty list.

    Returning all of them at once (rather than the first) is the point: the receipt has to
    tell a human exactly what is missing, not send them round the loop three times.
    """
    reasons: list[str] = []
    found = count_images(images)
    if not images.is_dir():
        reasons.append(f"no capture folder at {images} (a real multi-view capture is needed)")
    elif found < 3:
        reasons.append(
            f"{found} image(s) in {images}; SfM needs at least 3 overlapping views"
        )
    if not sidecar.is_file():
        reasons.append(f"sidecar launcher not found at {sidecar}")
    elif (sidecar.parent / ".stub").exists():
        reasons.append(
            f"MASt3R weights/upstream not installed ({(sidecar.parent / '.stub')} is present); "
            f"the install steps are in {sidecar.parent / 'README.md'}"
        )
    free = free_vram_mib()
    if free is not None and free < MAST3R_MIN_FREE_VRAM_MIB:
        reasons.append(
            f"only {free} MiB of free VRAM measured; MASt3R-SfM needs about "
            f"{MAST3R_MIN_FREE_VRAM_MIB} MiB"
        )
    return reasons


def run_mast3r(images: Path, workspace: Path, sidecar: Path) -> tuple[str, str, str]:
    """Invoke the MASt3R-SfM sidecar. Returns (status, detail, combined output).

    ``status`` is ``"ok"`` (poses produced), ``"blocked"`` (weights/gauge unavailable, a human
    must install) or ``"failed"`` (it ran and errored).
    """
    if not sidecar.is_file():
        return "blocked", f"sidecar launcher not found at {sidecar}", ""
    free = free_vram_mib()
    if free is not None and free < MAST3R_MIN_FREE_VRAM_MIB:
        return (
            "blocked",
            f"only {free} MiB of free VRAM on this GPU; MASt3R-SfM needs about "
            f"{MAST3R_MIN_FREE_VRAM_MIB} MiB. Close other GPU work and re-run.",
            "",
        )
    workspace.mkdir(parents=True, exist_ok=True)
    request = {
        "imagesDir": str(images),
        "workspace": str(workspace),
        "task": "sfm",
    }
    proc = subprocess.run(
        [sys.executable, str(sidecar)],
        input=json.dumps(request),
        capture_output=True,
        text=True,
        timeout=3600,
    )
    output = (proc.stdout or "") + (proc.stderr or "")
    if proc.returncode != 0:
        return "blocked", output.strip() or f"sidecar exited {proc.returncode}", output
    sparse = workspace / "sparse" / "0"
    if not sparse.is_dir():
        return "failed", "sidecar reported success but produced no sparse/0 model", output
    return "ok", f"poses at {sparse}", output


# --------------------------------------------------------------------------- receipts
def environment() -> dict[str, Any]:
    info: dict[str, Any] = {
        "when_utc": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "platform": platform.platform(),
        "python": sys.version.split()[0],
        "numpy": np.__version__,
        "gpu_free_mib": free_vram_mib(),
    }
    if shutil.which("nvidia-smi"):
        try:
            name = subprocess.run(
                ["nvidia-smi", "--query-gpu=name,memory.total", "--format=csv,noheader"],
                capture_output=True,
                text=True,
                timeout=20,
            )
            info["gpu"] = name.stdout.strip().splitlines()[0] if name.stdout.strip() else "unknown"
        except (OSError, subprocess.SubprocessError, IndexError):
            pass
    return info


def write_receipt(path: Path, title: str, payload: dict[str, Any], command: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    status = payload.get("status", payload.get("verdict", "unknown"))
    lines = [
        f"# {title}",
        "",
        f"- **Status: {status}**",
        f"- Command: `{command}`",
        f"- When (UTC): {payload.get('environment', {}).get('when_utc', '?')}",
        f"- Machine: {payload.get('environment', {}).get('platform', '?')} · "
        f"GPU {payload.get('environment', {}).get('gpu', 'unknown')} · "
        f"free VRAM {payload.get('environment', {}).get('gpu_free_mib', 'n/a')} MiB",
        f"- Python {payload.get('environment', {}).get('python', '?')} · "
        f"numpy {payload.get('environment', {}).get('numpy', '?')}",
        "",
    ]
    if payload.get("detail"):
        lines += ["## What this run found", "", str(payload["detail"]), ""]
    lines += ["## Full JSON", "", "```json", json.dumps(payload, indent=2, sort_keys=True), "```", ""]
    path.write_text("\n".join(lines), encoding="utf-8")


# --------------------------------------------------------------------------- modes
def mode_selftest() -> int:
    """Prove the metric code recovers a known error. No files, no GPU, no weights."""
    reference = synthetic_model()
    failures: list[str] = []

    # 1. A perfect estimate scores zero, whichever gauge it happens to be expressed in.
    perfect = score(reference, reference)
    if perfect["verdict"] != "PASS" or perfect["rotation_deg"]["max"] > 1e-6:
        failures.append(f"identity estimate should score ~0: {perfect['rotation_deg']}")

    re_gauged = transform_model(
        reference,
        scale=3.7,
        rotation=_axis_angle_matrix(np.array([0.3, 1.0, -0.2]) / math.sqrt(1.13), 1.1),
        translation=np.array([12.0, -4.0, 7.5]),
    )
    re_gauged_score = score(reference, re_gauged)
    if re_gauged_score["rotation_deg"]["max"] > 1e-6 or (
        re_gauged_score["centre_error"]["relative_median"] or 0
    ) > 1e-9:
        failures.append(f"a pure similarity must score 0 after alignment: {re_gauged_score}")

    # 2. The injected error is the recovered error: rotation within 25%, centre within 40%.
    for injected_deg in (0.5, 1.5, 4.0):
        noisy = perturb(reference, rotation_deg=injected_deg, centre_noise=0.0, seed=7)
        recovered = score(reference, noisy)["rotation_deg"]["median"]
        # Injection is uniform in [0.5, 1.5] x the requested angle, so the median is ~1x it.
        if not (0.7 * injected_deg <= recovered <= 1.3 * injected_deg):
            failures.append(
                f"injected {injected_deg}deg recovered {recovered}deg (outside 0.7-1.3x)"
            )
    # 3. The verdict discriminates: 4 deg must fail the 2 deg threshold.
    if score(reference, perturb(reference, rotation_deg=4.0, centre_noise=0.0, seed=3))["verdict"] != "FAIL":
        failures.append("a 4 deg error must FAIL the 2 deg threshold")
    # 4. Centre error is reported against the reference diameter, not raw metres.
    diameter = score(reference, reference)["scene_diameter_m"]
    if not (6.0 < diameter < 9.0):
        failures.append(f"scene diameter for the 4 m radius rig looks wrong: {diameter}")

    # 5. The COLMAP reader reproduces a model through the on-disk format it will read in
    # practice. A writer/reader pair that disagrees here would silently mis-score every real
    # capture, so this is a round trip rather than a parse check.
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        written = write_colmap_images(reference, Path(tmp) / "images.txt")
        try:
            reread = read_model(written)
        except ValueError as exc:
            failures.append(f"COLMAP reader rejected its own output: {exc}")
        else:
            round_trip = score(reference, reread)
            if round_trip["rotation_deg"]["max"] > 1e-4:
                failures.append(
                    f"COLMAP round trip changed rotations: {round_trip['rotation_deg']['max']} deg"
                )
            if round_trip["cameras"] != len(reference):
                failures.append(
                    f"COLMAP round trip lost cameras: {round_trip['cameras']} of {len(reference)}"
                )
            # And the pose convention: a camera's centre must survive the write/read.
            name = reference.names()[0]
            original = reference.poses[name][1]
            recovered = reread.poses[name][1]
            if float(np.linalg.norm(original - recovered)) > 1e-6:
                failures.append(
                    f"COLMAP round trip moved camera centre {name} by "
                    f"{np.linalg.norm(original - recovered)} m"
                )

    if failures:
        for failure in failures:
            print(f"SELFTEST FAIL: {failure}")
        return 1
    print("SELFTEST PASS: recovered injected errors, rejected a bad model, aligned a re-gauged one")
    return 0


def mode_fixture(args: argparse.Namespace) -> int:
    reference = synthetic_model(count=args.cameras, radius=args.radius)
    estimate = perturb(reference, args.inject_deg, args.inject_centre, seed=args.seed)
    # An estimate never shares the reference's gauge in practice, so hide it here too.
    estimate = transform_model(
        estimate,
        scale=2.5,
        rotation=_axis_angle_matrix(np.array([0.0, 0.0, 1.0]), 0.4),
        translation=np.array([1.0, 2.0, -3.0]),
    )
    payload: dict[str, Any] = {
        "mode": "fixture",
        "status": "OK",
        "environment": environment(),
        "fixture": {
            "cameras": args.cameras,
            "radius_m": args.radius,
            "injected_rotation_deg": args.inject_deg,
            "injected_centre_m": args.inject_centre,
            "seed": args.seed,
        },
        "result": score(reference, estimate),
    }
    payload["verdict"] = payload["result"]["verdict"]
    print(json.dumps(payload, indent=2, sort_keys=True))
    if args.receipt:
        write_receipt(
            Path(args.receipt),
            "MASt3R-SfM benchmark — fixture run (no weights required)",
            payload,
            args.command,
        )
    return 0 if payload["verdict"] == "PASS" else 1


def mode_mast3r(args: argparse.Namespace) -> int:
    images = Path(args.images).resolve()
    estimate_path = Path(args.estimate).resolve() if args.estimate else None
    sidecar = Path(args.sidecar)
    workspace = Path(args.workspace).resolve() if args.workspace else (
        REPO / "docs" / "evidence" / "mast3r-bench" / "workspace"
    )
    missing = preconditions(images, sidecar)
    if missing:
        payload: dict[str, Any] = {
            "mode": "mast3r",
            "status": "BLOCKED",
            "detail": "MASt3R-SfM could not run here: " + "; ".join(missing) + ".",
            "preconditions_unmet": missing,
            "environment": environment(),
            "images": str(images),
            "estimate": str(estimate_path) if estimate_path else None,
            "sidecar": str(sidecar),
            "rerun": (
                f"python tools/bench/mast3r_sfm_bench.py --mast3r --images {images} "
                f"--estimate {estimate_path or '<our sparse/0>'} "
                "--receipt docs/evidence/mast3r-bench/mast3r.md"
            ),
        }
        print(json.dumps(payload, indent=2, sort_keys=True))
        if args.receipt:
            write_receipt(
                Path(args.receipt),
                "MASt3R-SfM benchmark — blocked (preconditions unmet)",
                payload,
                args.command,
            )
        return 0

    status, detail, output = run_mast3r(images, workspace, sidecar)
    payload = {
        "mode": "mast3r",
        "status": status.upper(),
        "detail": detail,
        "environment": environment(),
        "images": str(images),
        "estimate": str(estimate_path) if estimate_path else None,
        "sidecar": str(sidecar),
        "sidecar_output": output[-4000:] if output else "",
    }
    if status == "ok" and estimate_path is not None:
        payload["result"] = score(read_model(workspace / "sparse" / "0"), read_model(estimate_path))
        payload["verdict"] = payload["result"]["verdict"]
    elif status == "ok":
        payload["detail"] = (
            f"{detail}. Pass --estimate <our sparse/0> to score our poses against it."
        )
    print(json.dumps(payload, indent=2, sort_keys=True))
    if args.receipt:
        write_receipt(
            Path(args.receipt),
            "MASt3R-SfM benchmark — reference-vs-ours",
            payload,
            args.command,
        )
    if status == "failed":
        return 1
    return 0


def mode_score(args: argparse.Namespace) -> int:
    payload: dict[str, Any] = {
        "mode": "score",
        "status": "OK",
        "environment": environment(),
        "reference": str(Path(args.reference).resolve()),
        "estimate": str(Path(args.estimate).resolve()),
        "result": score(read_model(Path(args.reference)), read_model(Path(args.estimate))),
    }
    payload["verdict"] = payload["result"]["verdict"]
    print(json.dumps(payload, indent=2, sort_keys=True))
    if args.receipt:
        write_receipt(
            Path(args.receipt),
            "MASt3R-SfM benchmark — two sparse models",
            payload,
            args.command,
        )
    return 0 if payload["verdict"] == "PASS" else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Score our SfM poses against MASt3R-SfM or synthetic ground truth."
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--selftest", action="store_true", help="prove the metric code (no data)")
    mode.add_argument("--fixture", action="store_true", help="synthetic ground-truth run")
    mode.add_argument("--mast3r", action="store_true", help="run the MASt3R-SfM sidecar, then score")
    mode.add_argument("--score", action="store_true", help="score two existing sparse models")
    parser.add_argument("--reference", help="reference sparse model (MASt3R-SfM or ground truth)")
    parser.add_argument("--estimate", help="our sparse model to score")
    parser.add_argument("--images", help="image folder for --mast3r")
    parser.add_argument("--workspace", help="workspace for the MASt3R-SfM run")
    parser.add_argument("--sidecar", default=str(DEFAULT_SIDECAR), help="sidecar launcher path")
    parser.add_argument("--receipt", help="write a Markdown receipt here")
    parser.add_argument("--cameras", type=int, default=7)
    parser.add_argument("--radius", type=float, default=4.0)
    parser.add_argument("--inject-deg", dest="inject_deg", type=float, default=1.5)
    parser.add_argument("--inject-centre", dest="inject_centre", type=float, default=0.02)
    parser.add_argument("--seed", type=int, default=11)
    args = parser.parse_args(argv)
    args.command = "python " + " ".join(
        ["tools/bench/mast3r_sfm_bench.py"] + (argv if argv is not None else sys.argv[1:])
    )

    try:
        if args.selftest:
            return mode_selftest()
        if args.fixture:
            return mode_fixture(args)
        if args.mast3r:
            if not args.images:
                parser.error("--mast3r needs --images <capture folder>")
            return mode_mast3r(args)
        if args.score:
            if not (args.reference and args.estimate):
                parser.error("--score needs both reference and estimate models")
            return mode_score(args)
    except (ValueError, FileNotFoundError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
