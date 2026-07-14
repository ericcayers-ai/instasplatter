# Brush fork patch contracts

These are **design stubs**, not applied diffs yet. Each patch must build against
upstream ArthurBrussee/brush (Apache-2.0) without vendoring Inria NC code.

| Patch file | Goal | Upstream touch points (approx.) |
| --- | --- | --- |
| `01-budgeted-densify.md` | Cap + prioritise splits (Taming/FastGS/AbsGS) | densify/refine in `brush-train` |
| `02-inloop-mip.md` | 2D mip low-pass during raster | `brush-render` / splat rasterizer |
| `03-mask-loss.md` | SpotLess-style transient weights | loss map in `brush-loss` |
| `04-appearance.md` | Per-image appearance embeddings | train step + SH coeffs |

When a patch lands as a real `.diff`, list it here and document the Brush CLI
flag InstaSplatter should pass (compose with existing progressive/mip/losses).
