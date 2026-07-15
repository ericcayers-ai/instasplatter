# gs-2d (Experimental)

**License:** Apache-2.0
**Tasks:** surface_mesh

2DGS surface/mesh experimental adapter (separate from densify fusion).

Install under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/gs-2d/` with a
`run.bat` / `run.py` that speaks the InstaSplatter JSON protocol.

Keep the `.stub` marker until real weights are wired — InstaSplatter treats
`.stub` as **not ready** and refuses success from template launchers.
Weights are **never** shipped in the NSIS installer.

Protocol: JSON on stdin (`task`) → write COLMAP sparse / print PLY path /
engine-specific output. Delete `.stub` only when the launcher is real.
