# Neural dense-init, train, and polish sidecars

Drop launchers under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/<name>/`
(or use each folder's `install.ps1`). Dev builds also resolve repo
`tools/sidecars/<name>/` as a fallback.

InstaSplatter posts a JSON request on stdin. Dense-init launchers print a
point/Gaussian PLY path. Pose/`sfm` launchers write a COLMAP sparse model and
print its path (or `OK`). `gsplat-train` prints a trained splat PLY. Polish
launchers print a polished splat path.

**Honesty rule:** launchers fail clearly when weights/upstream are missing —
they never invent points or poses. Host readiness ignores `.stub` and ignores
`ACCEPTED` without a real launcher.

## Dual-mode policy (v0.8.1)

| Mode | Pose | Densify | Polish |
| --- | --- | --- | --- |
| **Standard** (default) | VGGT-Commercial → COLMAP | **RoMa v2** ∧ DA3 ∧ MapAnything ∧ VGGT-C ∧ MVS ∧ sparse | Fixer |
| **Experimental** (ack) | Capture-profile research routing | profile-matched + confidence-fuse | Difix → Fixer |

NC launchers are refused unless Experimental Mode is ON. Experimental folders
keep `.stub` until a weights dry-run succeeds.

## Standard installables (no `.stub` in-repo)

| Sidecar | License | Role |
| --- | --- | --- |
| `roma-v2` | MIT orchestration | Densify (needs RoMaV2 + weights at runtime) |
| `depth-anything-3` | Apache | Preferred monocular densify |
| `depth-anything-v2` | Apache | Legacy densify |
| `mapanything` | Apache | Pose + densify via upstream scripts |
| `lightglue` | Apache-ish matcher | Match pairs for COLMAP routing |
| `vggt-commercial` | Gated commercial | Pose/densify when `ACCEPTED` |
| `fixer` | NVIDIA Open Model | Polish |
| `gsplat-train` | Apache | CUDA trainer |
| `anuga` / `swmm` | Apache / coupling | Geospatial flood |

## Experimental installables (`.stub` until dry-run)

See **[research/README.md](./research/README.md)**. Each folder has a real
adapter that looks for `./upstream` / `run_upstream.py` + marker files.

## Shared helpers

`_common/adapter_util.py` — copied by install scripts into engines trees.
