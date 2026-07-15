# vggt-long (Experimental)

**License:** research/NC
**Tasks:** sfm, densify

Long-sequence VGGT.

Install under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/vggt-long/` with a
`run.bat` / `run.py` that speaks the InstaSplatter JSON protocol.

Keep the `.stub` marker until real weights are wired — InstaSplatter treats
`.stub` as **not ready** and refuses success from template launchers.
Weights are **never** shipped in the NSIS installer.

Protocol: JSON on stdin (`task`) → write COLMAP sparse / print PLY path /
engine-specific output. Delete `.stub` only when the launcher is real.

## Installable adapter (v0.8.1+)

1. Clone the upstream project into `./upstream` (see project URL in RESEARCH-STACK).
2. Install NC weights per upstream LICENSE (Experimental Mode only).
3. Optionally add `run_upstream.py` if demo entrypoints differ.
4. Create `ACCEPTED` after reviewing terms.
5. Dry-run once, then **delete `.stub`** so the host marks the sidecar ready.
6. Copy to `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/vggt-long/` (or use install.ps1).

The launcher fails clearly when weights/upstream are missing — it never invents PLY/poses.

