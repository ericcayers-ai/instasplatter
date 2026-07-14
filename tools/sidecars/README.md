# Neural dense-init, train, and polish sidecars

Drop launchers under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/<name>/`.

InstaSplatter posts a JSON request on stdin (schema varies slightly by role).
Dense-init launchers print a point/Gaussian PLY path. `gsplat-train` prints a
trained splat PLY. Polish launchers print a polished splat path.

## Densify (compose with MVS)

Neural points are **merged** with COLMAP MVS and sparse COLMAP points.

1. `vggt-omega` — Research ON only (CC BY-NC)
2. `vggt-commercial` — ON when present + `ACCEPTED`
3. `depth-anything-v2` — ON when present (Apache-2.0)
4. `vggt-research` — Research ON only

## Trainer — gsplat (Apache-2.0)

See **[gsplat-train/README.md](./gsplat-train/README.md)**. When installed and
the machine has CUDA, Auto trainer selection prefers gsplat over Brush.

## Polish

1. `fixer` — default ON when installed (NVIDIA Open Model, commercial OK)
2. `difix` — Research ON only

`postPolish` defaults true (no-op until a launcher exists).
