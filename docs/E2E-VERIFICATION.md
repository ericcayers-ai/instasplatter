# E2E verification path (v0.8.1+)

## What can be automated locally

| Step | Command | Gate |
| --- | --- | --- |
| Unit tests | `cd src-tauri && cargo test` | Required before push |
| Typecheck | `npx tsc --noEmit` | Required |
| Frontend build | `npm run build` | Required |
| Sidecar fail-clearly | `.\tools\smoke-local.ps1` | Required |
| NSIS installer | `npm run tauri build` | Required for release |
| Headless ingest batch | `INSTASPLATTER_DEV=1` + `batch.txt` | Optional when sample media exists |

## What needs real engines + GPU

1. COLMAP SfM on a known image set (installer engines download).
2. Brush or gsplat train to first checkpoint + PLY.
3. Viewport orbit + suite switch Reconstruction ↔ Geospatial.
4. Flood demo mode (no ANUGA) vs labelled scientific when ANUGA+DEM evolve.

Documented sample clips: [SMOKE-TEST.md](./SMOKE-TEST.md).

## Multi-vendor matrix

See [tools/HW-MATRIX.md](../tools/HW-MATRIX.md). Cross-vendor adversarial capture runs are **external lab work** — harness scripts document the matrix; they cannot invent GPUs.
