# Neural dense-init sidecars

Drop launchers under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/<name>/`.

InstaSplatter posts a JSON request on stdin:

```json
{
  "imagesDir": "...",
  "workspace": "...",
  "sparseDir": ".../sparse/0",
  "maxPoints": 1200000
}
```

The launcher must print a single absolute path to a point PLY (xyz+rgb) or a
Gaussian init PLY on stdout, then exit 0.

## Priority (v0.3)

1. `vggt-omega` — only if Research sidecars is ON (CC BY-NC; newest / best)
2. `vggt-commercial` — ON when present + `ACCEPTED` (ship default neural path)
3. `depth-anything-v2` — ON when present (Apache-2.0)
4. `vggt-research` — Research sidecars ON only

## depth-anything-v2 (Apache-2.0)

Prefer the newest Small / community ONNX or torch weights compatible with your
launcher. Back-project confident depth into the COLMAP frame.

## vggt-commercial (Meta AUP)

Use gated `facebook/VGGT-1B-Commercial` only. Touch `ACCEPTED` after agreeing
to the Hugging Face terms (no military use).

## vggt-omega (CC BY-NC-4.0) — newest VGGT (VGGT-Ω)

- Code: https://github.com/facebookresearch/vggt-omega
- Weights (gated): https://huggingface.co/facebook/VGGT-Omega  
  Prefer `VGGT-Omega-1B-512` (`vggt_omega_1b_512.pt`); text-aligned 256 variant optional.
- License: **CC BY-NC-4.0** on weights — research / non-commercial only.
- Install as `sidecars/vggt-omega/run.py` (or `run.bat`). Enable Settings →
  **Research sidecars** so Ω is preferred over VGGT-1B-Commercial.
- Ship-default neural path stays `vggt-commercial` until Meta publishes a
  commercial-friendly Ω checkpoint.

## difix / fixer (special opt-in)

Do **not** enable by default. Difix3D+ / GSFixer stacks often mix research-gated
or GS-adjacent NC pieces. When a clean Apache/MIT Fixer lands, drop it under
`sidecars/difix/` and gate behind Research sidecars the same way as Ω.
