# 03 — Transient mask loss

**Inspiration:** SpotLessSplats (Apache-derived algorithm).

**Behaviour:** Down-weight pixels that look distractor-like across views (feature
space clustering or learned mask). Optional SAM2 class masks via sidecar.

**CLI (proposed):** `--transient-mask auto|off` + optional `--mask-dir`.
