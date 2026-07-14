# RoMa v2 densify sidecar (Standard + Experimental)

Clean-room densification following the **Lichtfeld densify recipe** (reference
fraction + neighbors-per-ref, certainty / reprojection / Sampson / parallax
filters) while calling **MIT** [RoMaV2](https://github.com/Parskatt/RoMaV2) APIs.

**Do not copy** the GPL-3.0 Lichtfeld Densification Plugin sources into this
tree. Install this launcher under:

`%LOCALAPPDATA%/InstaSplatter/engines/sidecars/roma-v2/`

## Files

| File | Role |
| --- | --- |
| `run.py` | densify launcher (stdin JSON → stdout PLY path) |
| `run.bat` | Windows wrapper |
| `requirements.txt` | suggested pip deps |

## Install

```powershell
$dir = "$env:LOCALAPPDATA\InstaSplatter\engines\sidecars\roma-v2"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
Copy-Item tools\sidecars\roma-v2\* $dir -Recurse -Force
cd $dir
python -m venv .venv
.\.venv\Scripts\pip install -r requirements.txt
# Clone / install RoMaV2 separately, then place DINOv3 weights per RoMa docs.
# Meta DINOv3 weights: review Meta's custom license before commercial redistrib.
```

## Protocol

Stdin JSON (camelCase):

```json
{
  "imagesDir": "...",
  "workspace": "...",
  "sparseDir": ".../sparse/0",
  "maxPoints": 1200000,
  "task": "densify",
  "quality": "base",
  "referenceFraction": 0.3,
  "neighborsPerRef": 8
}
```

`quality`: `fast` | `base` | `high` | `precise` (Experimental forces `precise`).

Stdout: absolute path to an XYZRGB (or Gaussian) PLY. Lines starting with `#` are ignored.
