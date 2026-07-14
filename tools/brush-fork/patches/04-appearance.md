# 04 — Appearance embeddings

**Inspiration:** WildGaussians (MIT renderer ideas; avoid GS-licensed deps).

**Behaviour:** Per-image latent that absorbs exposure / WB drift so geometry
does not overfit photometrics.

**CLI (proposed):** `--appearance-dim N` (0 = off).
