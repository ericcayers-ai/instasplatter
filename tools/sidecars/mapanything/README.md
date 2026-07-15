# MapAnything (Apache) — Standard pose / densify

Installable adapter. Host routes `task: "sfm"` and `task: "densify"` here when present.

## Install

```powershell
.\tools\sidecars\mapanything\install.ps1
git clone <MapAnything-repo-url> "$env:LOCALAPPDATA\InstaSplatter\engines\sidecars\mapanything\upstream"
# follow upstream README for weights, then:
New-Item -ItemType File "$env:LOCALAPPDATA\InstaSplatter\engines\sidecars\mapanything\ACCEPTED"
```

Optional: drop `run_mapanything.py` / `run_densify.py` next to this launcher for custom entrypoints.

Fails clearly when upstream/weights are missing — never invents COLMAP models or PLY points.
