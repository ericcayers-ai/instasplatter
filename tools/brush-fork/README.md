# Custom Brush build

InstaSplatter drives Brush over its CLI today. Training-loop SOTA (AbsGS densify,
true Mip-Splatting 2D filter, MCMC relocate, appearance embeddings, SpotLess masks)
needs a patched Brush binary.

## Override path

Place a Windows build at:

```
%LOCALAPPDATA%\InstaSplatter\engines\brush-custom\brush_app.exe
```

InstaSplatter prefers this over the stock `engines/brush` download.

## Suggested fork workflow

```bash
git clone https://github.com/ArthurBrussee/brush.git brush-fork
cd brush-fork
# apply patches under patches/ when they land
cargo build --release -p brush-app
copy target\release\brush_app.exe %LOCALAPPDATA%\InstaSplatter\engines\brush-custom\
```

## Patch priorities (Apache-2.0 / reimplementation only)

1. Budgeted densification (Taming-3DGS / FastGS / AbsGS ideas)
2. In-loop 2D mip low-pass (Mip-Splatting)
3. Stronger opacity/scale L1 defaults
4. Transient mask loss (SpotLess-style)
5. Per-image appearance embeddings (WildGaussians-style)

Do not vendor Inria NC source. Reimplement algorithms against Brush + gsplat
(Apache) as oracles.
