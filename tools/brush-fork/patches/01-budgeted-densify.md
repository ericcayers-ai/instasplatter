# 01 — Budgeted densification

**Inspiration:** Taming-3DGS, FastGS, AbsGS (algorithms only; MIT/Apache references preferred).

**Behaviour:** Prefer splitting high AbsGrad / contribution Gaussians under a hard
`max_splats` budget. Avoid clone spam that creates floaters.

**CLI (proposed):** `--densify-mode budgeted` (default when custom binary detected).

**InstaSplatter:** Already passes `--max-splats`; no settings change required.
