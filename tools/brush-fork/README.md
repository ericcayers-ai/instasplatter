# Custom Brush build

InstaSplatter drives Brush over its CLI today. Training-loop SOTA (AbsGS densify,
true Mip-Splatting 2D filter, MCMC relocate, appearance embeddings, SpotLess masks)
needs a patched Brush binary.

## Override path (auto-detected)

Place a Windows build at:

```
%LOCALAPPDATA%\InstaSplatter\engines\brush-custom\brush_app.exe
```

InstaSplatter prefers this over the stock `engines/brush` download. Engine
status reports `brushCustom: true` when the override is active. Training logs
`Using custom Brush binary (engines/brush-custom).`

## Suggested fork workflow

```powershell
.\tools\brush-fork\build-custom.ps1
# or manually:
git clone https://github.com/ArthurBrussee/brush.git tools/brush-fork/src
cd tools/brush-fork/src
# apply patches under ../patches/ when they land
cargo build --release -p brush-app
New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\InstaSplatter\engines\brush-custom" | Out-Null
Copy-Item target\release\brush_app.exe "$env:LOCALAPPDATA\InstaSplatter\engines\brush-custom\"
```

## Patch priorities (Apache-2.0 / reimplementation only)

See `patches/README.md` for contracts. Order:

1. Budgeted densification (Taming-3DGS / FastGS / AbsGS ideas)
2. In-loop 2D mip low-pass (Mip-Splatting)
3. Stronger opacity/scale L1 defaults
4. Transient mask loss (SpotLess-style)
5. Per-image appearance embeddings (WildGaussians-style)

Do not vendor Inria NC source. Reimplement algorithms against Brush + gsplat
(Apache) as oracles.

Stock Brush already exposes MCMC-style mean noise and AbsGS-like opac/scale L1
via CLI; InstaSplatter wires those without a fork.
