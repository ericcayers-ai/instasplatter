# 02 — In-loop 2D mip

**Inspiration:** Mip-Splatting (reimplement; do not vendor Inria rasterizer).

**Behaviour:** Apply 2D low-pass during forward raster so the stage-boundary
PLY filter in InstaSplatter becomes a belt-and-suspenders bake only.

**CLI (proposed):** `--mip-filter` (InstaSplatter can pass when `mipFilter` is ON).
