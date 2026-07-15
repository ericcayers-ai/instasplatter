# Depth Anything 3 (Apache-2.0) — Standard densify

Preferred monocular-depth densifier when installed. DAV2 is the legacy fallback.

## Protocol

JSON on stdin (`task: "densify"`) → absolute XYZRGB PLY path on stdout.

Requires a COLMAP sparse text model (`images.txt` / `cameras.txt`) to back-project depth.

## Install

```powershell
.\tools\sidecars\depth-anything-3\install.ps1
# then:
#   pip install torch opencv-python pillow
#   drop weights.pt or weights.onnx into the engines folder
#   New-Item ACCEPTED
```

Fails clearly (exit ≠ 0, stderr `# …`) when weights / runtime / sparse poses are missing — never invents points.

Delete `.stub` only when this launcher is present (done in-repo); engines copy is ready when `ACCEPTED` or weights exist.
