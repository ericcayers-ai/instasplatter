# mast3r-slam (Experimental)

**License:** CC BY-NC-SA
**Tasks:** sfm

MASt3R-SLAM long-video poses.

Install under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/mast3r-slam/` with a
`run.bat` / `run.py` that speaks the InstaSplatter JSON protocol.

Keep the `.stub` marker until real weights are wired — InstaSplatter treats
`.stub` as **not ready** and refuses success from template launchers.
Weights are **never** shipped in the NSIS installer.

Protocol: JSON on stdin (`task`) → write COLMAP sparse / print PLY path /
engine-specific output. Delete `.stub` only when the launcher is real.
