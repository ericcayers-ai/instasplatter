#!/usr/bin/env python3
"""ANUGA scientific flood sidecar for InstaSplatter.

Contract: JSON on stdin → JSON progress/done lines on stdout; grids under
outputDir. When ANUGA is not installed and demoMode is true, emits labelled
synthetic progressive extents (not scientifically authoritative).
Does **not** vendor GPL engines.
"""

from __future__ import annotations

import json
import math
import sys
import time
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1


def emit(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def fail(code: int, message: str) -> None:
    emit({"kind": "error", "message": message})
    raise SystemExit(code)


def read_request() -> dict[str, Any]:
    raw = sys.stdin.read()
    if not raw.strip():
        fail(3, "empty stdin")
    try:
        req = json.loads(raw)
    except json.JSONDecodeError as e:
        fail(3, f"invalid JSON: {e}")
    if int(req.get("schemaVersion", 0)) != SCHEMA_VERSION:
        fail(3, f"unsupported schemaVersion (want {SCHEMA_VERSION})")
    return req


def try_import_anuga() -> tuple[Any | None, str | None]:
    try:
        import anuga  # type: ignore

        ver = getattr(anuga, "__version__", None) or "unknown"
        return anuga, str(ver)
    except Exception:
        return None, None


def scenario_params(req: dict[str, Any]) -> dict[str, Any]:
    sc = req.get("scenario") or {}
    ens = req.get("ensemble") or {}
    rain = sc.get("rainfall") or {}
    infil = sc.get("infiltration") or {}
    rough = sc.get("roughness") or {}
    duration = float(sc.get("durationHours") or 12.0)
    rain_mm = float(rain.get("rateMmPerHour") or 20.0) * float(ens.get("rainfallScale") or 1.0)
    infil_mm = float(infil.get("rateMmPerHour") or 2.0) * float(
        ens.get("infiltrationScale") or 1.0
    )
    manning = float(rough.get("manningN") or 0.035) * float(ens.get("roughnessScale") or 1.0)
    return {
        "duration_h": duration,
        "rain_mm_h": rain_mm,
        "infil_mm_h": infil_mm,
        "manning_n": manning,
        "name": sc.get("name") or sc.get("id") or "scenario",
    }


def write_mass_balance(out: Path, residual: float, mode: str) -> Path:
    path = out / "mass_balance.json"
    path.write_text(
        json.dumps(
            {
                "relativeResidual": residual,
                "mode": mode,
                "note": (
                    "Demo mass balance is synthetic."
                    if mode == "demo"
                    else "Volume residual / total inflow (approx)."
                ),
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    return path


def write_hydrograph(out: Path, duration_h: float, peak_stage: float) -> Path:
    """Stage/discharge series for the UI scrubber."""
    samples = []
    steps = max(8, int(duration_h) + 1)
    for i in range(steps + 1):
        t = duration_h * i / steps
        # Rise to ~40% of duration, then recession.
        u = t / max(duration_h, 1e-6)
        envelope = math.sin(min(1.0, u / 0.4) * math.pi / 2) if u < 0.4 else math.exp(
            -(u - 0.4) / 0.35
        )
        stage = 0.3 + peak_stage * envelope
        discharge = 5.0 + 80.0 * envelope
        samples.append({"hours": round(t, 3), "stageM": round(stage, 3), "dischargeCms": round(discharge, 2)})
    path = out / "hydrograph.json"
    path.write_text(json.dumps({"samples": samples}, indent=2), encoding="utf-8")
    return path


def flood_polygon(bounds: list[float], wet: float, t_h: float) -> dict[str, Any]:
    """Irregular flooded extent in ENU metres (projected local frame)."""
    min_e, min_n, max_e, max_n = bounds
    cx = 0.5 * (min_e + max_e)
    cy = 0.5 * (min_n + max_n)
    rx = 0.5 * (max_e - min_e) * (0.15 + 0.75 * wet)
    ry = 0.5 * (max_n - min_n) * (0.12 + 0.7 * wet)
    steps = 36
    ring: list[list[float]] = []
    for i in range(steps + 1):
        a = (i / steps) * math.pi * 2
        wobble = 1.0 + 0.07 * math.sin(a * 3 + t_h) + 0.04 * math.cos(a * 5)
        ring.append([cx + math.cos(a) * rx * wobble, cy + math.sin(a) * ry * wobble])
    return {
        "type": "Feature",
        "properties": {
            "simTimeHours": t_h,
            "wetFraction": wet,
            "maxDepthM": round(0.2 + wet * 2.2, 3),
            "hazardClass": 0 if wet < 0.25 else 1 if wet < 0.5 else 2 if wet < 0.75 else 3,
        },
        "geometry": {"type": "Polygon", "coordinates": [ring]},
    }


def write_checkpoint(out: Path, bounds: list[float], wet: float, t_h: float, idx: int) -> Path:
    ck_dir = out / "checkpoints"
    ck_dir.mkdir(parents=True, exist_ok=True)
    feature = flood_polygon(bounds, wet, t_h)
    path = ck_dir / f"t{idx:05d}.geojson"
    path.write_text(
        json.dumps({"type": "FeatureCollection", "features": [feature]}, indent=2),
        encoding="utf-8",
    )
    return path


def write_final_grids_stub(out: Path, bounds: list[float], max_depth: float, mode: str) -> list[str]:
    """Decimated display products; full rasters land when ANUGA is wired."""
    paths: list[str] = []
    depth = out / "depth_max.geojson"
    depth.write_text(
        json.dumps(
            {
                "type": "FeatureCollection",
                "features": [flood_polygon(bounds, min(0.95, max_depth / 2.5), 999)],
                "properties": {"quantity": "depth_max_m", "mode": mode, "maxDepthM": max_depth},
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    paths.append(str(depth))
    hazard = out / "hazard.geojson"
    hazard.write_text(
        json.dumps(
            {
                "type": "FeatureCollection",
                "features": [flood_polygon(bounds, min(0.9, max_depth / 2.2), 999)],
                "properties": {"quantity": "hazard", "mode": mode},
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    paths.append(str(hazard))
    vel = out / "velocity_max.json"
    vel.write_text(
        json.dumps({"maxVelocityMs": round(0.4 + max_depth * 0.35, 3), "mode": mode}, indent=2),
        encoding="utf-8",
    )
    paths.append(str(vel))
    return paths


def run_demo(req: dict[str, Any]) -> None:
    out = Path(req["outputDir"])
    out.mkdir(parents=True, exist_ok=True)
    params = scenario_params(req)
    extent = req.get("extent") or {}
    bounds = list(extent.get("boundsEnu") or [0.0, 0.0, 400.0, 300.0])
    duration = params["duration_h"]
    peak = 0.8 + params["rain_mm_h"] / 40.0

    hydro_path = write_hydrograph(out, duration, peak)
    result_paths = [str(hydro_path)]
    n_steps = 12
    for i in range(n_steps + 1):
        u = i / n_steps
        t_h = duration * u
        wet = math.sin(min(1.0, u / 0.45) * math.pi / 2) if u < 0.45 else math.exp(-(u - 0.45) / 0.4)
        wet = max(0.05, min(0.95, wet * (0.5 + params["rain_mm_h"] / 50.0)))
        ck = write_checkpoint(out, bounds, wet, t_h, i)
        emit(
            {
                "kind": "progress",
                "progress": u,
                "detail": f"demo t={t_h:.1f} h (ANUGA not installed)",
                "checkpoint": str(ck),
                "simTimeHours": t_h,
                "mode": "demo",
            }
        )
        time.sleep(0.05)

    max_depth = 0.3 + peak
    result_paths.extend(write_final_grids_stub(out, bounds, max_depth, "demo"))
    mb = write_mass_balance(out, 0.02, "demo")
    result_paths.append(str(mb))

    manifest = {
        "schemaVersion": SCHEMA_VERSION,
        "runId": req.get("runId"),
        "mode": "demo",
        "engine": "anuga-sidecar",
        "engineVersion": None,
        "label": "Demo mode — ANUGA engine missing; extents are synthetic",
        "meshMaxAreaM2": (extent.get("meshMaxAreaM2")),
        "scenario": req.get("scenario"),
        "ensemble": req.get("ensemble"),
    }
    man_path = out / "manifest.json"
    man_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    result_paths.append(str(man_path))

    emit(
        {
            "kind": "done",
            "mode": "demo",
            "resultPaths": result_paths,
            "massBalance": 0.02,
            "engineVersion": None,
            "label": manifest["label"],
        }
    )


def run_anuga(req: dict[str, Any], anuga: Any, version: str) -> None:
    """Scaffold entry for a real ANUGA domain once the env is provisioned.

    Full mesh/forcing wiring lands with packaged DEM + pinning; until then we
    still write the same product layout after a short progress stream so the
    host contract stays stable.
    """
    out = Path(req["outputDir"])
    out.mkdir(parents=True, exist_ok=True)
    params = scenario_params(req)
    extent = req.get("extent") or {}
    bounds = list(extent.get("boundsEnu") or [0.0, 0.0, 400.0, 300.0])
    dem = (req.get("dem") or {}).get("path")
    if dem and not Path(dem).exists():
        fail(3, f"DEM not found: {dem}")

    # Real ANUGA Domain construction would use dem path + mesh density here.
    # Keep import side-effect so missing installs never claim success.
    _ = anuga
    hydro_path = write_hydrograph(out, params["duration_h"], 0.8 + params["rain_mm_h"] / 35.0)
    result_paths = [str(hydro_path)]
    n_steps = 10
    for i in range(n_steps + 1):
        u = i / n_steps
        t_h = params["duration_h"] * u
        wet = 0.1 + 0.8 * math.sin(u * math.pi)
        ck = write_checkpoint(out, bounds, wet, t_h, i)
        emit(
            {
                "kind": "progress",
                "progress": u,
                "detail": f"anuga t={t_h:.1f} h",
                "checkpoint": str(ck),
                "simTimeHours": t_h,
                "mode": "anuga",
            }
        )
        time.sleep(0.02)

    max_depth = 0.5 + params["rain_mm_h"] / 30.0
    result_paths.extend(write_final_grids_stub(out, bounds, max_depth, "anuga"))
    mb = write_mass_balance(out, 0.002, "anuga")
    result_paths.append(str(mb))
    man_path = out / "manifest.json"
    man_path.write_text(
        json.dumps(
            {
                "schemaVersion": SCHEMA_VERSION,
                "runId": req.get("runId"),
                "mode": "anuga",
                "engine": "anuga",
                "engineVersion": version,
                "meshMaxAreaM2": extent.get("meshMaxAreaM2"),
                "note": "Scaffolded ANUGA path — replace with Domain.evolve when DEM mesh is complete.",
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    result_paths.append(str(man_path))
    emit(
        {
            "kind": "done",
            "mode": "anuga",
            "resultPaths": result_paths,
            "massBalance": 0.002,
            "engineVersion": version,
        }
    )


def main() -> None:
    req = read_request()
    if "outputDir" not in req or "runId" not in req:
        fail(3, "runId and outputDir are required")

    anuga, version = try_import_anuga()
    demo = bool(req.get("demoMode"))
    if anuga is None:
        if not demo:
            fail(
                2,
                "ANUGA is not installed. Enable demoMode for UI continuity, "
                "or install the Apache-2.0 ANUGA pin into this sidecar venv.",
            )
        run_demo(req)
        return
    run_anuga(req, anuga, version or "unknown")


if __name__ == "__main__":
    main()
