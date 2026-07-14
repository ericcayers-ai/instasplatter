# Research stack (v0.5.1) — dual mode

Evidence-first inventory: **[PAPER-SWEEP-2024+.md](./PAPER-SWEEP-2024+.md)**.

InstaSplatter runs as **two stacks** behind one Experimental Mode toggle.

| | **Standard Mode** (default) | **Experimental Mode** (license ack) |
|---|---|---|
| Intent | Best quality that stays commercially redistributable | Absolute max quality; personal/research; NC OK after ack |
| Cameras | **VGGT-1B-Commercial first** → light COLMAP BA/fallback | **VGGT-Ω → MASt3R → DUSt3R → VGGT-C → COLMAP** |
| Dense init | **RoMa v2 densify** ∧ DAV2 ∧ COLMAP MVS ∧ sparse/VGGT | Same **plus** Ω/MASt3R/DUSt3R pointmaps — merge everything |
| Polish | NVIDIA Fixer when installed | **Difix then Fixer** |
| Trainer | gsplat Auto on CUDA else Brush | Force gsplat Max + strategies; Brush Max if no gsplat |
| Caps | Balanced→High floors when neural stack present | Force Max-tier frames/res/steps/splats |
| UI | Quiet chrome | TitleBar Experimental ON + NC banner + solver chips |

Weights / PyTorch sidecars stay **user-installed** under
`%LOCALAPPDATA%/InstaSplatter/engines/sidecars/`. Base NSIS installer stays lean
(COLMAP + Brush only).

## Standard Mode pipeline

```
Video → frame gate
  → VGGT-Commercial poses (if ACCEPTED) → optional COLMAP BA
      else COLMAP SfM (or live-init → COLMAP)
  → RoMa v2 densify ∧ DAV2/VGGT-C ∧ COLMAP MVS ∧ sparse → init.ply
  → gsplat (CUDA) or Brush: progressive ∧ mip ∧ AbsGS opac/scale ∧ MCMC
  → Fixer polish when installed
  → Live PLY lerp viewport → Mesh export
```

## Experimental Mode pipeline

```
Video → frame gate
  → VGGT-Ω → MASt3R-SfM → DUSt3R → VGGT-C → COLMAP  (first usable success)
  → Merge ALL densifiers (Ω/MASt3R/DUSt3R + RoMa precise + DAV2 + MVS + sparse)
  → Difix then Fixer
  → gsplat Max (MCMC+AbsGrad+AA+appearance+bilagrid) or Brush Max
  → Live PLY lerp viewport → Mesh export
```

## License map (hard constraints)

| Component | License | Standard | Experimental |
|---|---|---|---|
| VGGT-1B-Commercial | Meta AUP (commercial) | Primary SfM | Fallback in chain |
| VGGT-Ω | CC BY-NC-4.0 | Never | Preferred SfM |
| MASt3R / DUSt3R | CC BY-NC-SA | Never | Pose + dense merge |
| RoMa v2 code | **MIT** ([Parskatt/RoMaV2](https://github.com/Parskatt/RoMaV2)) | Densify sidecar | Densify (`precise`) |
| DINOv3 (RoMa backbone) | Meta custom — review for redistrib | User-install weights | Same |
| Lichtfeld Densification Plugin | **GPL-3.0** | **Do not copy** — recipe only | Same |
| Difix3D+ | Research/gated | Off | Preferred polish (then Fixer) |
| NVIDIA Fixer | NVIDIA Open Model | ON when installed | ON when installed |
| COLMAP / DAV2 / gsplat / Brush | BSD / Apache / Apache | ON | ON |

Inspiration for densify behavior:
[shadygm/Lichtfeld-Densification-Plugin](https://github.com/shadygm/Lichtfeld-Densification-Plugin)
(RoMa matching, certainty/reproj/Sampson/parallax filters, reference-fraction +
neighbors-per-ref). Our path is clean-room Apache-friendly orchestration + MIT RoMa APIs.

## Selected (shipped or wired)

| Project | Version / note | License | Role | Default | Code proof |
| --- | --- | --- | --- | --- | --- |
| Brush | v0.3.0 CLI; `brush-custom` override | Apache-2.0 | Trainer (MCMC-style stock) | ON | `engines.rs::brush_exe` |
| COLMAP | 4.1 patch-match MVS | BSD | SfM fallback + dense MVS | ON when CUDA | `dense.rs`, `colmap.rs` |
| RoMa v2 densify | clean-room recipe | MIT densifier | Dense matches → init.ply | ON when installed | `roma-v2`, `try_roma_densify` |
| Depth Anything V2 | Small / latest Apache | Apache-2.0 | Neural densify sidecar | ON when installed | `sidecars.rs` |
| VGGT-1B-Commercial | Jul 2025 commercial CKPT | Meta AUP | Primary Standard poses | ON when installed + ACCEPTED | `try_neural_poses` |
| DashGaussian-style progressive | reimplemented | — | Coarse→fine Brush stages | ON | `plan_stages` |
| Mip-Splatting 3D filter | reimplemented (ref NC) | — | Stage-boundary + bake | ON | `mipfilter.rs` |
| AbsGS-style opac/scale L1 | via Brush CLI knobs | — | Floater suppression | ON | `opac_loss_weight` |
| NVIDIA Fixer | HF `nvidia/Fixer` | NVIDIA Open Model | Post-train polish | ON when installed | `try_polish` |
| Live splat interpolation | our code | — | Lerp attrs between PLY exports | ON | `src/splat/worker.ts` |
| Batch queue | our code | — | Serialize GPU jobs, UI queue | ON | queue / UI |
| Experimental Mode UI | our code | — | Toggle + NC modal + banner | OFF | TitleBar / ExperimentalBanner |

## VGGT-Ω / MASt3R / DUSt3R — Experimental only

| Item | Detail |
| --- | --- |
| VGGT-Ω | CC BY-NC-4.0; preferred Experimental SfM (`vggt-omega`) |
| MASt3R / DUSt3R | CC BY-NC-SA; Experimental pose + densify merge |
| Gate | `experimentalMode` + `experimentalLicenseAcked` (forces `allowResearchSidecars`; flag alone never unlocks NC) |
| Why not ON by default | NC licenses block commercial redistribution |

## Rejected for Standard (available Experimental where noted)

| Project | License | Decision |
| --- | --- | --- |
| VGGT-1B (original NC) | CC BY-NC | Experimental densify only |
| MASt3R / DUSt3R | CC BY-NC-SA | Experimental only |
| π³ / Pi3 weights | CC BY-NC (code BSD) | Not shipped |
| Lichtfeld densify plugin | GPL-3.0 | Recipe reimplementation only — **never vendor** |
| Difix3D+ research weights | research / gated | Experimental polish |
| GSFixer / GSFix3D | Apache + **GS NC** + RAIL++ | Deferred / rejected for commercial bundling |

## Sidecar install

See `tools/sidecars/README.md`. Base app never depends on PyTorch.

| Folder | When used |
| --- | --- |
| `roma-v2` | Standard densify (MIT); Experimental too (`precise`) |
| `depth-anything-v2` | Neural densify (Apache) |
| `vggt-commercial` (+ `ACCEPTED`) | Standard primary poses + densify |
| `vggt-omega` / `mast3r` / `dust3r` | Experimental only |
| `fixer` | Default polish when present |
| `difix` | Experimental polish (before Fixer) |
| `gsplat-train` | Default trainer on NVIDIA when installed |

## gsplat parity (nerfstudio-project/gsplat, Apache-2.0)

| gsplat feature | InstaSplatter status |
| --- | --- |
| CUDA rasterization library | Via `gsplat-train` sidecar |
| MCMC densify | **ON** default strategy |
| AbsGrad densify | **ON** when strategy=default |
| Antialiased / mip raster | **ON** |
| Appearance embeddings | **ON** when full trainer available |
| Bilateral grid | **ON** |

**Trainer Auto:** CUDA + `gsplat-train` → gsplat; else Brush.
Experimental forces Max floors and prefers gsplat with all strategies ON.
