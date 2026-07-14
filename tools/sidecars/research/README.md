# Experimental research sidecars (NC)

These launchers are **only invoked when Experimental Mode is ON** (license ack).
Do not enable them for commercial redistribution of products built on NC weights.

Install under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/<name>/` with a
`run.bat` / `run.py` that speaks the InstaSplatter JSON protocol.

| Folder | License | Tasks | Notes |
| --- | --- | --- | --- |
| `vggt-omega/` | CC BY-NC-4.0 | `sfm`, `densify` | Preferred Experimental poses |
| `mast3r/` | CC BY-NC-SA | `sfm`, `densify` | MASt3R-SfM |
| `dust3r/` | CC BY-NC-SA | `sfm`, `densify` | Global align fallback |
| `difix/` | research / gated | `polish` | Runs before Fixer in Experimental |
| `vggt-research/` | CC BY-NC | `densify` | Legacy NC VGGT-1B |

## Protocol

Same stdin JSON as other sidecars (`imagesDir`, `workspace`, `sparseDir`, `task`,
`splatPath`, …).

- **`task: "sfm"`** — write COLMAP sparse model to `workspace/sparse/0` (cameras /
  images / points3D text or binary) and print that directory path (or `OK`).
- **`task: "densify"`** — print path to XYZRGB / Gaussian PLY.
- **`task: "polish"`** — read `splatPath`, print polished splat path.

Copy the stub `run.py` / `run.bat` from each subfolder and replace the body with
your local checkpoint wiring. Keep the `.stub` marker file until weights are
wired — InstaSplatter treats `.stub` as "not ready" and never reports success
from template launchers. Weights are **never** shipped in the NSIS installer.
