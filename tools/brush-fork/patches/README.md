# Brush custom patches (apply with build-custom.ps1)

Verified against ArthurBrussee/brush `@3b80985` (Apache-2.0).

| Patch | Purpose | CLI flags |
| --- | --- | --- |
| `01-budgeted-densify.diff` | Adds InstaSplatter train options | `--densify-absgrad-boost`, `--mip-max-scale`, `--transient-mask-weight`, `--appearance-embed-dim` |
| `02-inloop-mip.diff` | AbsGS-style densify sampling boost + keeps mip/mask/appearance flags live | uses the new config fields |

Older design notes (`01-*.md` … `04-*.md`) remain as algorithm briefs for deeper rasterizer/loss work.

## Build on Windows

```powershell
.\tools\brush-fork\build-custom.ps1
```

Requirements: Rust stable, git, VS C++ build tools. Full `brush-app` release can take 20–60+ minutes.

## Auto-detect

`%LOCALAPPDATA%\InstaSplatter\engines\brush-custom\brush_app.exe`

Engine status reports `brushCustom: true`.

## What custom enables

| Capability | Stock Brush | This fork path |
| --- | --- | --- |
| `--max-splats` / mean-noise CLI | Yes | Same |
| `--densify-absgrad-boost` (budgeted AbsGS-style sample tilt) | No | Yes |
| Full in-loop 2D mip / SpotLess mask / appearance nets | No | Flags reserved; algorithm bodies still follow-on |

Do not vendor Inria NC sources. Reimplement against Brush + gsplat (Apache) as oracles.
