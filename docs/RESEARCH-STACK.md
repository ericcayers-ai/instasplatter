# Research stack (v0.3.1)

Evidence-first inventory: **[PAPER-SWEEP-2024+.md](./PAPER-SWEEP-2024+.md)** (40+ entries, greppable status, license, integration points).

Post-2024 Gaussian Splatting and feed-forward SfM / mesh work evaluated for
InstaSplatter. Policy: integrate what improves quality and is license-viable;
special opt-in only for uncertain or NC-licensed paths. Prefer the **newest**
viable release of each component. Features **compose (AND)**; they are not
mutually exclusive "pick one model" toggles unless technically required.

## Composed default pipeline

```
Video → frame gate
  → COLMAP (or live init)  + optional VGGT-Ω if Research ON
  → Neural densify (DAV2 / VGGT-commercial) ∧ COLMAP MVS ∧ sparse seed → init.ply
  → Brush: progressive ∧ mip bake ∧ AbsGS opac/scale L1 ∧ MCMC mean-noise knobs
  → [brush-custom] in-loop mip / SpotLess / appearance when override binary present
  → Fixer polish when installed (NVIDIA Open Model, commercial OK)
  → Live PLY lerp viewport
  → Mesh: dense TSDF ∧ smooth ∧ island cull ∧ oriented-point fallback
```

## Selected (shipped or wired)

| Project | Version / note | License | Role | Default | Code proof |
| --- | --- | --- | --- | --- | --- |
| Brush | v0.3.0 CLI; `brush-custom` override | Apache-2.0 | Trainer (MCMC-style stock) | ON | `engines.rs::brush_exe` |
| COLMAP | 4.1 patch-match MVS | BSD | SfM + dense init (composes with neural) | ON when CUDA | `dense.rs::densify_after_sfm` |
| Dense sparse→Gaussian seed | our code | Apache-friendly | Always seed `init.ply` | ON | `points_to_gaussians` |
| Depth Anything V2 | Small / latest Apache | Apache-2.0 | Neural densify sidecar | ON when installed | `sidecars.rs` |
| VGGT-1B-Commercial | Jul 2025 commercial CKPT | Meta AUP | Neural poses/pointmaps | ON when installed + ACCEPTED | `vggt-commercial` |
| DashGaussian-style progressive | reimplemented | — | Coarse→fine Brush stages | ON | `plan_stages` |
| Mip-Splatting 3D filter | reimplemented (ref NC) | — | Stage-boundary + bake | ON | `mipfilter.rs` |
| AbsGS-style opac/scale L1 | via Brush CLI knobs | — | Floater suppression | ON | `opac_loss_weight` |
| 3DGS-MCMC knobs | Brush stock + noise weight | Inria ref NC | Mean noise / splat cap | ON | `mean_noise_weight`, `max_splats` |
| 2DGS / DN-Splatter / AGS-Mesh | recipe only | Apache (DN/2DGS) | Denser TSDF, cleanup, fallback | ON | `mesh/mod.rs` |
| NVIDIA Fixer | HF `nvidia/Fixer` | NVIDIA Open Model | Post-train polish sidecar | ON when installed | `post_polish`, `try_polish` |
| Live splat interpolation | our code | — | Lerp attrs between PLY exports | ON | `src/splat/worker.ts` |
| Batch queue | our code | — | Serialize GPU jobs, UI queue | ON | queue / UI |

## Counts (maintain with PAPER-SWEEP)

| Bucket | Count (approx.) |
| --- | --- |
| Inventories in PAPER-SWEEP | **45** |
| wired-default-ON / integrated | **~22** |
| opt-in (Research / special) | **~6** |
| deferred (fork / later) | **~12** |
| rejected | **~5** |

## VGGT-Ω (VGGT-Omega) — newest, not default

| Item | Detail |
| --- | --- |
| Paper | CVPR 2026 Oral, Meta + Oxford, arXiv:2605.15195 |
| Code | https://github.com/facebookresearch/vggt-omega |
| Hugging Face | `facebook/VGGT-Omega` (`vggt_omega_1b_512.pt` preferred) |
| License | **CC BY-NC-4.0** (non-commercial) as of May 2026 |
| Decision | **Research opt-in only** (`allowResearchSidecars`). Prefer over VGGT-1B-Commercial when research is enabled. |
| Why not ON by default | No commercial Omega checkpoint yet. |

## Rejected or special opt-in

| Project | License | Decision |
| --- | --- | --- |
| VGGT-1B (original NC) | CC BY-NC | Research opt-in only |
| MASt3R / DUSt3R | CC BY-NC-SA | Not shipped |
| π³ / Pi3 weights | CC BY-NC (code BSD) | Not shipped |
| SuGaR | mixed / GS-license adjacent | Algorithm reference; oriented-point fallback is our own |
| GOF / PGSR / RaDe-GS | often Inria NC | Reference only |
| Difix3D+ research weights | research / gated | Research sidecar `difix` (prefer Fixer for commercial) |
| GSFixer / GSFix3D | Apache + **GS NC** + RAIL++ | Deferred / rejected for commercial bundling |
| CityGaussian / FlashSplat | NC / wrong task | Skip |
| SAM2 masks | Apache | Sidecar later (SpotLess path) |

## Mesh overhaul

| Change | Source inspiration | License stance |
| --- | --- | --- |
| Default resolution 768, render 1024 | 2DGS / AGS-Mesh | our code |
| Depth confidence floor (α≥0.62) | DN-Splatter | our code |
| Laplacian smooth + island cull | standard cleanup | our code |
| Oriented-point TSDF rebuild fallback | DN-Splatter / AGS-Mesh | recipe only |
| Quality presets draft / high / max | — | our code |
| UV atlas | — | still deferred |
| Full screened Poisson | — | deferred |

## Brush fork path

Stock Brush remains the default download. Drop a custom build at
`%LOCALAPPDATA%/InstaSplatter/engines/brush-custom/brush_app.exe` to override
(auto-detected; see engine status `brushCustom`).

See `tools/brush-fork/README.md` and `tools/brush-fork/patches/` for AbsGS densify,
true in-loop mip, MCMC relocate, appearance embeddings, SpotLess masks, and
FastGS-style budgeted densify.

## Sidecar install

See `tools/sidecars/README.md`. Base app never depends on PyTorch.

| Folder | When used |
| --- | --- |
| `depth-anything-v2` | Default neural densify (Apache) |
| `vggt-commercial` (+ `ACCEPTED`) | Default neural SfM densify |
| `vggt-omega` | Research ON only |
| `fixer` | Default polish when present (NVIDIA Open Model) |
| `difix` | Research ON only |
