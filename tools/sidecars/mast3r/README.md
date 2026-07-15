# mast3r (Experimental / NC)

See [../research/README.md](../research/README.md).

Copy to `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/mast3r/` and wire your local checkpoint.
Host refuses this sidecar unless **Experimental Mode** is ON.

## Installable adapter (v0.8.1+)

1. Clone the upstream project into `./upstream` (see project URL in RESEARCH-STACK).
2. Install NC weights per upstream LICENSE (Experimental Mode only).
3. Optionally add `run_upstream.py` if demo entrypoints differ.
4. Create `ACCEPTED` after reviewing terms.
5. Dry-run once, then **delete `.stub`** so the host marks the sidecar ready.
6. Copy to `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/mast3r/` (or use install.ps1).

The launcher fails clearly when weights/upstream are missing — it never invents PLY/poses.

