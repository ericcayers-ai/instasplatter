# Experimental research sidecars (NC)

These launchers are **only invoked when Experimental Mode is ON** (license ack),
except `gs-2d` (Apache) which still lives under the experimental surface path.
Do not enable NC weights for commercial redistribution.

Install under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/<name>/` with a
`run.bat` / `run.py` that speaks the InstaSplatter JSON protocol.

## Capture-aware routing (not blind merge)

| Profile | Pose / adapters | Notes |
| --- | --- | --- |
| Static unordered | `vggt-omega`, `mast3r`, `dust3r`, `pi3x` | Scored → COLMAP/ENU → fuse |
| Long video | `stream-vggt`, `vggt-long`, `mast3r-slam`, `slam3r` | SLAM / streaming family |
| Dynamic | `vggt-omega`, `monst3r`, `easi3r` | 4D stays on a **separate** path |
| Large aerial/urban | `city-gaussian`, `urban-gs`, `horizon-gs` | Partition/LOD products |
| Surface/mesh | `gs-2d`, `gof`, `pgsr`, `rade-gs`, `sugar`, `milo` | Separate adapters |

Every candidate must pass canonical-frame alignment + validation gates before
fusion (`ExperimentalValidationGate` + schema v2). Keep `.stub` until wired —
the host refuses stub success.

| Folder | License | Tasks | Notes |
| --- | --- | --- | --- |
| `vggt-omega/` | CC BY-NC-4.0 | `sfm`, `densify` | Preferred Experimental poses |
| `mast3r/` | CC BY-NC-SA | `sfm`, `densify` | MASt3R-SfM |
| `dust3r/` | CC BY-NC-SA | `sfm`, `densify` | Global align fallback |
| `pi3x/` | CC BY-NC | `sfm`, `densify` | Static unordered |
| `stream-vggt/` | research/NC | `sfm`, `densify` | Long video |
| `vggt-long/` | research/NC | `sfm`, `densify` | Long sequence |
| `mast3r-slam/` | CC BY-NC-SA | `sfm` | Long-video SLAM |
| `slam3r/` | research/NC | `sfm` | SLAM3R |
| `monst3r/` | research/NC | `sfm`, `four_d` | Dynamic — not fused into init.ply |
| `easi3r/` | research/NC | `sfm`, `four_d` | Dynamic — not fused into init.ply |
| `city-gaussian/` | research/NC | `large_scene` | Partition / LOD |
| `urban-gs/` | research/NC | `large_scene` | Urban aerial |
| `horizon-gs/` | research/NC | `large_scene` | Horizon LOD |
| `gs-2d/` | Apache-2.0 | `surface_mesh` | 2DGS surface path |
| `gof/` / `pgsr/` / `rade-gs/` | NC | `surface_mesh` | Surface adapters |
| `sugar/` / `milo/` | NC / GS-adj | `surface_mesh` | Mesh adapters |
| `difix/` | research / gated | `polish` | Runs before Fixer in Experimental |
| `vggt-research/` | CC BY-NC | `densify` | Legacy NC VGGT-1B |

Hydro experimental / GPL plugins: see **[hydro-plugins/README.md](./hydro-plugins/README.md)**.

## Protocol

Same stdin JSON as other sidecars (`imagesDir`, `workspace`, `sparseDir`, `task`,
`splatPath`, …).

- **`task: "sfm"`** — write COLMAP sparse model to `workspace/sparse/0` and print that directory path (or `OK`).
- **`task: "densify"`** — print path to XYZRGB / Gaussian PLY.
- **`task: "four_d"`** — dynamic/4D product under `workspace/four_d/` (never merged into static densify).
- **`task: "large_scene"`** / **`surface_mesh`** — engine-specific outputs under their own folders.
- **`task: "polish"`** — read `splatPath`, print polished splat path.

Copy the stub `run.py` from each subfolder and replace the body with your local
checkpoint wiring. Keep the `.stub` marker file until weights are wired —
InstaSplatter treats `.stub` as "not ready" and never reports success from
template launchers. Weights are **never** shipped in the NSIS installer.
