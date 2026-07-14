# gsplat CUDA trainer sidecar

Apache-2.0. This is the **NVIDIA quality path** for InstaSplatter: real
`nerfstudio-project/gsplat` training (MCMC densify, AbsGrad/Default strategy,
antialiased raster, appearance embeddings, bilateral grid / PPISP when deps
allow) instead of Brush/wgpu.

## Install

```powershell
# GPU PyTorch first, then:
pip install gsplat

# Optional appearance / PPISP extras used by upstream simple_trainer:
# pip install fused-bilagrid
# (PPISP: follow https://research.nvidia.com/labs/sil/projects/ppisp/)

$dest = "$env:LOCALAPPDATA\InstaSplatter\engines\sidecars\gsplat-train"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item -Recurse tools\sidecars\gsplat-train\* $dest -Force

# Optional: point at cloned gsplat examples for full simple_trainer feature set
# $env:GSPLAT_EXAMPLES = "C:\path\to\gsplat\examples"
```

Without `GSPLAT_EXAMPLES` / vendored `simple_trainer.py`, the launcher falls
back to `train_mini.py` (MCMC or AbsGrad DefaultStrategy, antialiased, opac/scale
regs, live PLY exports). Text models still need COLMAP **text** sparse
(`cameras.txt` / `images.txt` / `points3D.txt`); InstaSplatter can write those
or you run `colmap model_converter --output_type TXT`.

## Defaults wired by the app (compose ON)

| Feature | Default |
| --- | --- |
| Strategy | `mcmc` |
| AbsGrad (when strategy=default) | ON |
| Antialiased raster | ON |
| Appearance embeddings | ON when full trainer |
| Bilateral grid | ON when full trainer |
| Opacity / scale regs | from strictness (AbsGS-style) |

## Detection

Rust looks for `engines/sidecars/gsplat-train/run.bat` (or `run.py`). When CUDA
hardware is present and the trainer setting is Auto, gsplat wins over Brush.
