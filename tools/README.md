# InstaSplatter Tools

## Development

| Script | Purpose |
|--------|---------|
| `smoke-local.ps1` | Windows smoke harness — runs cargo test, tsc, checks sidecar adapters fail-clearly on empty input |
| `upgrade_experimental_adapters.py` | Regenerates `run.py`/`run.bat`/`install.ps1` for 20 experimental sidecar adapters |
| `bench/mast3r_sfm_bench.py` | MASt3R-SfM accuracy benchmark: scores our poses against a reference after gauge alignment; runs weight-free on synthetic ground truth. See `bench/README.md` |

## Brush Fork

`brush-fork/` contains patches for custom Brush builds. See `brush-fork/README.md`.

- `build-custom.ps1` — build script
- `patches/` — `.diff` files for densify boost, mip filter, mask loss, appearance embeddings

## Sidecars

`sidecars/` contains installable adapter launchers for external engines:

| Adapter | Status | License |
|---------|--------|---------|
| `anuga/` | Real | GPL-2.0 (external) |
| `swmm/` | Real | EPA (external) |
| `depth-anything-{v2,v3}/` | Real | Apache-2.0 |
| `lightglue/` | Real | Apache-2.0 |
| `roma-v2/` | Real | MIT |
| `mapanything/` | Real | Apache-2.0 |
| `vggt-commercial/` | Real | Application-gated |
| `fixer/` | Real | NVIDIA OML |
| `gsplat-train/` | Real | Apache-2.0 |
| `*.stub` folders | Placeholder | Not ready |

See also: `sidecars/README.md`, `sidecars/research/README.md`.

## Hardware Matrix

`HW-MATRIX.md` documents GPU vendor support and test status. Manual verification required per-vendor.

## References

- `_refs/` — local upstream code clones for search/reference (gitignored)
- `docs/RESEARCH-STACK.md` — paper and license audit
- `docs/PAPER-SWEEP-2024+.md` — research evidence sweep
