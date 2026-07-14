# Neural dense-init and polish sidecars

Drop launchers under `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/<name>/`.

InstaSplatter posts a JSON request on stdin:

```json
{
  "imagesDir": "...",
  "workspace": "...",
  "sparseDir": ".../sparse/0",
  "maxPoints": 1200000,
  "splatPath": ".../result.ply"
}
```

`splatPath` is set for polish sidecars (Fixer / Difix). Dense-init launchers
may ignore it.

The launcher must print a single absolute path on stdout (point PLY for densify,
or polished Gaussian PLY for Fixer), then exit 0.

## Priority (v0.3.1) — densify (compose with MVS)

Neural points are **merged** with COLMAP MVS and sparse COLMAP points. They are
not alternatives.

1. `vggt-omega` — only if Research sidecars is ON (CC BY-NC; newest / best)
2. `vggt-commercial` — ON when present + `ACCEPTED` (ship default neural path)
3. `depth-anything-v2` — ON when present (Apache-2.0)
4. `vggt-research` — Research sidecars ON only

Plus always: COLMAP patch-match MVS when CUDA COLMAP is available.

## Polish (post train)

1. `fixer` — **default ON when installed**. NVIDIA Open Model License (commercial OK).
   HF: https://huggingface.co/nvidia/Fixer  
   Code: https://github.com/nv-tlabs/Fixer
2. `difix` — Research sidecars ON only (Difix3D+ research / gated). Prefer Fixer.

Setting: `postPolish` (default true; no-op until a launcher exists).

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

## fixer (NVIDIA Open Model — commercial OK)

Example layout:

```
engines/sidecars/fixer/run.bat
engines/sidecars/fixer/...  # weights / venv as needed
```

Read `splatPath` + cameras under `workspace`, enhance novel/artifact views,
optionally re-distill or write a cleaned Gaussian PLY. Print the absolute path
of the polished PLY (may overwrite `splatPath`).

Until the launcher exists, `postPolish` is a silent no-op.
