# Neural dense-init, train, and polish sidecars

Drop launchers under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/<name>/`.

InstaSplatter posts a JSON request on stdin (schema varies slightly by role).
Dense-init launchers print a point/Gaussian PLY path. Pose/`sfm` launchers write
a COLMAP sparse model and print its path (or `OK`). `gsplat-train` prints a
trained splat PLY. Polish launchers print a polished splat path.

## Dual-mode policy (v0.5)

| Mode | Pose | Densify | Polish |
| --- | --- | --- | --- |
| **Standard** (default) | VGGT-Commercial → COLMAP | **RoMa v2** ∧ DA3 ∧ VGGT-C ∧ MVS ∧ sparse | Fixer |
| **Experimental** (ack) | **Capture-profile** research routing (not blind merge) | profile-matched + confidence-fuse | Difix → Fixer |

NC launchers are refused unless Experimental Mode is ON.

## Densify (compose with MVS — never early-return)

1. `roma-v2` — **[roma-v2/](./roma-v2/)** MIT densify (Lichtfeld *recipe*, not GPL plugin)
2. Profile-matched Experimental adapters — see **[research/README.md](./research/README.md)**
3. `vggt-commercial` — ON when present + `ACCEPTED`
4. `depth-anything-3` — preferred Apache densify (DAV2 legacy fallback)
5. `vggt-research` — Experimental only

## Pose (`task: "sfm"`)

Same folder names; write `workspace/sparse/0` COLMAP model. Experimental
routing picks candidates by capture profile (`LongVideo`, `DynamicScene`, …).

## Trainer — gsplat (Apache-2.0)

See **[gsplat-train/README.md](./gsplat-train/README.md)**. When installed and
the machine has CUDA, Auto trainer selection prefers gsplat over Brush.

## Polish

1. `difix` — Experimental only (runs first)
2. `fixer` — default ON when installed (NVIDIA Open Model, commercial OK)

`postPolish` defaults true (no-op until a launcher exists).

## Research stubs

See **[research/README.md](./research/README.md)** (Ω, MASt3R, Pi3X, StreamVGGT,
MonST3R, CityGaussian, surface adapters, …). Hydro plugins:
**[hydro-plugins/README.md](./hydro-plugins/README.md)**.
