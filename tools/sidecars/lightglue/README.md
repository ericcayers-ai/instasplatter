# LightGlue matcher (Standard when installed)

When present + ACCEPTED (or weights/upstream), InstaSplatter can prefer LightGlue
matching. Until then, SfM uses COLMAP SIFT with a log notice.

## Install

```powershell
.\tools\sidecars\lightglue\install.ps1
pip install lightglue torch torchvision kornia opencv-python
New-Item -ItemType File "$env:LOCALAPPDATA\InstaSplatter\engines\sidecars\lightglue\ACCEPTED"
```

Writes `workspace/lightglue/pairs.txt` + per-pair match files. Host keeps
COLMAP SfM as the pose solver unless a dedicated match importer is enabled.
