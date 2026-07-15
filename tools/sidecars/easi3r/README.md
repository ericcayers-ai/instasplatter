# easi3r (Experimental)

**License:** research/NC
**Tasks:** sfm, four_d

Dynamic / 4D — separate path, never fused into static init.ply.

Install under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/easi3r/` with a
`run.bat` / `run.py` that speaks the InstaSplatter JSON protocol.

Keep the `.stub` marker until real weights are wired — InstaSplatter treats
`.stub` as **not ready** and refuses success from template launchers.
Weights are **never** shipped in the NSIS installer.

Protocol: JSON on stdin (`task`) → write COLMAP sparse / print PLY path /
engine-specific output. Delete `.stub` only when the launcher is real.
