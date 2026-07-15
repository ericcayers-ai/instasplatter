# Local and CI-adjacent smoke harness for InstaSplatter

Documents the multi-vendor HW matrix and runs whatever local smoke is possible
without requiring every GPU vendor in one session.

## Hardware matrix (manual / lab)

| Vendor | GPU class | Expected trainer | Smoke target |
| --- | --- | --- | --- |
| NVIDIA | RTX 40xx / 30xx | gsplat (if installed) or Brush | Draft 48 frames, PLY export |
| AMD | RX 7000 | Brush (wgpu) | Draft 32 frames |
| Intel | Arc / iGPU | Brush (wgpu) eco/draft | Draft 24 frames, honest ETA |
| Apple | (future) | Brush (wgpu) | not shipped on Windows |

Adversarial captures: blurry handheld, lighting change, person walking, mixed focal.
These need real footage — not automated in this environment.

## Local automated smoke (this repo)

```powershell
# Unit + type + frontend build
.\tools\smoke-local.ps1

# Optional: headless batch when engines + sample video exist
.\tools\smoke-local.ps1 -WithDevBatch -VideoPath "D:\captures\sample.mp4"
```

`smoke-local.ps1` runs:

1. `cargo test` (src-tauri)
2. `npx tsc --noEmit`
3. `npm run build`
4. Sidecar adapter protocol checks (python `--help` / dry stdin fail-clearly)
5. Optional `INSTASPLATTER_DEV` batch enqueue when `-WithDevBatch` is set

Full `npm run tauri build` is release-gated separately (slow).

## E2E path (documented)

See [docs/SMOKE-TEST.md](../docs/SMOKE-TEST.md) and [docs/E2E-VERIFICATION.md](../docs/E2E-VERIFICATION.md).
