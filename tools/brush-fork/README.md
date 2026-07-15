# Custom Brush build

InstaSplatter drives Brush over its CLI today. Training-loop AbsGS densify boost
and reserved mip/mask/appearance flags ship as real patches under `patches/`.

## Override path (auto-detected)

```
%LOCALAPPDATA%\InstaSplatter\engines\brush-custom\brush_app.exe
```

Engine status reports `brushCustom: true` when active.

## Build

```powershell
.\tools\brush-fork\build-custom.ps1
```

Applies `patches/*.diff` (authored vs Brush `3b80985`), builds `brush-app`,
installs the exe + `INSTASPLATTER_BUILD.json`.

## What the custom build enables

| Flag | Effect |
| --- | --- |
| `--densify-absgrad-boost <f>` | Tilts densify multinomial toward high \|grad\| (AbsGS-style) |
| `--mip-max-scale <f>` | Reserved mip clamp hook (0 = off) |
| `--transient-mask-weight <f>` | Reserved SpotLess-style mask loss weight |
| `--appearance-embed-dim <n>` | Reserved WildGaussians-style embedding dim |

Stock Brush already exposes MCMC-style mean noise and opac/scale L1 via CLI;
InstaSplatter wires those without a fork. Full in-loop mip/mask/appearance
still need deeper Brush rasterizer/loss work (see patch Markdown briefs).
