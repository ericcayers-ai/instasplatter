# Research stack (v0.3)

Post-2024 Gaussian Splatting and feed-forward SfM / mesh work evaluated for
InstaSplatter. Policy: integrate what improves quality and is license-viable;
special opt-in only for uncertain or NC-licensed paths. Prefer the **newest**
viable release of each component.

## Selected (shipped or wired)

| Project | Version / note | License | Role | Default |
| --- | --- | --- | --- | --- |
| Brush | v0.3.0 CLI; custom override path | Apache-2.0 | Trainer | ON |
| COLMAP | 4.1 patch-match MVS | BSD | Dense init after sparse SfM | ON when CUDA |
| Dense sparse→Gaussian seed | our code | Apache-friendly | Always seed `init.ply` | ON |
| Depth Anything V2 | Small / latest Apache weights | Apache-2.0 | Neural densify sidecar | ON when installed |
| VGGT-1B-Commercial | Jul 2025 commercial CKPT | Meta AUP (no military) | Neural poses/pointmaps | ON when installed + accepted |
| DashGaussian-style progressive | reimplemented | — | Coarse→fine Brush stages | ON |
| Mip-Splatting 3D filter | reimplemented (ref NC) | — | Stage-boundary + bake | ON |
| AbsGS-style opac/scale L1 | via Brush CLI knobs | — | Floater suppression | ON (raised baselines) |
| 2DGS / DN-Splatter / AGS-Mesh meshing | recipe only | Apache (DN) / Apache (2DGS) | Denser TSDF, cleanup, fallback | ON |
| Live splat interpolation | our code | — | Lerp attrs between PLY exports | ON |
| Batch queue | our code | — | Serialize GPU jobs, UI queue | ON |
| Taming-3DGS / FastGS densify ideas | MIT algorithms | MIT | Target for Brush fork | docs / fork |
| SpotLess / WildGaussians | MIT / Apache (verify) | — | Target for Brush fork | docs / fork |

## VGGT-Ω (VGGT-Omega) — newest, not default

| Item | Detail |
| --- | --- |
| Paper | CVPR 2026, Meta + Oxford |
| Code | https://github.com/facebookresearch/vggt-omega |
| Hugging Face | `facebook/VGGT-Omega` (`vggt_omega_1b_512.pt` preferred) |
| License | **CC BY-NC-4.0** (non-commercial) as of May 2026 |
| Decision | **Research opt-in only** (`allowResearchSidecars`). Prefer over VGGT-1B-Commercial / NC when research is enabled. |
| Why not ON by default | No commercial Omega checkpoint yet. VGGT-1B-Commercial remains the ship-default neural SfM path. Switch default to Omega the day Meta publishes a commercial Ω weight. |

## Rejected or special opt-in

| Project | License | Decision |
| --- | --- | --- |
| VGGT-1B (original NC) | CC BY-NC | Research opt-in only |
| MASt3R / DUSt3R | CC BY-NC-SA | Not shipped |
| π³ / Pi3 weights | CC BY-NC (code BSD) | Not shipped |
| SuGaR | mixed / GS-license adjacent | Algorithm reference; Poisson fallback is our own |
| GOF / PGSR / RaDe-GS | often Inria NC | Reference only |
| Difix3D+ | research / gated | Special opt-in later |
| GSFixer / GSFix3D | Apache + **GS NC** components + RAIL++ model | Deferred; commercial use blocked by bundled GS license |
| nvidia Fixer | verify when released | Prefer over Difix when Apache/MIT |
| 3DGS-MCMC densify | Inria NC | Reimplement in Brush fork |
| Analytic-Splatting | verify | Defer |
| CityGaussian / FlashSplat | NC / wrong task | Skip |
| SAM2 masks | Apache | Sidecar later (SpotLess path) |

## Mesh overhaul (v0.3)

| Change | Source inspiration | License stance |
| --- | --- | --- |
| Default resolution 640, render 960 | 2DGS / AGS-Mesh settings | our code |
| Depth confidence floor raised (α≥0.62) | DN-Splatter confidence | our code |
| Laplacian smooth + island cull | standard mesh cleanup | our code |
| Oriented-point TSDF rebuild fallback | DN-Splatter / AGS-Mesh Poisson alternatives | recipe only |
| Quality presets draft / high / max | — | our code |
| UV atlas | — | still deferred |
| Full screened Poisson | — | deferred (octree multigrid) |

## Brush fork path

Stock Brush remains the default download. Drop a custom build at
`%LOCALAPPDATA%/InstaSplatter/engines/brush-custom/brush_app.exe` to override.
See `tools/brush-fork/README.md` for AbsGS densify, true in-loop mip, MCMC
relocate, appearance embeddings, SpotLess masks, and FastGS-style budgeted densify.

## Sidecar install

See `tools/sidecars/README.md`. Base app never depends on PyTorch.
Install paths: `depth-anything-v2`, `vggt-commercial` (+ `ACCEPTED`), `vggt-omega` (research).
