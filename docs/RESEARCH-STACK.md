# Research stack (v0.6+) — dual mode

Evidence-first inventory: **[PAPER-SWEEP-2024+.md](./PAPER-SWEEP-2024+.md)**.

InstaSplatter runs as **two stacks** behind one Experimental Mode toggle.

| | **Standard Mode** (default) | **Experimental Mode** (license ack) |
|---|---|---|
| Intent | Best quality that stays commercially redistributable | Absolute max quality; personal/research; NC OK after ack |
| Cameras | **VGGT-1B-Commercial first** → light COLMAP BA/fallback | **Capture-profile routing** (not blind merge): static Ω/MASt3R/DUSt3R/Pi3X; long video StreamVGGT/VGGT-Long/MASt3R-SLAM/SLAM3R; dynamic Ω/MonST3R/Easi3R (+ separate 4D); large aerial CityGaussian/Urban-GS/Horizon |
| Dense init | **RoMa v2 densify** ∧ DA3 ∧ COLMAP MVS ∧ sparse/VGGT | Profile-matched research densifiers, then **confidence-fuse** (schema v2 + gates) |
| Surface / 4D | Stock mesh path | Separate adapters (`gs-2d`, GOF/PGSR/…, MonST3R/Easi3R) |
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
  → RoMa v2 densify ∧ DA3/VGGT-C ∧ COLMAP MVS ∧ sparse → init.ply
  → gsplat (CUDA) or Brush: progressive ∧ mip ∧ AbsGS opac/scale ∧ MCMC
  → Fixer polish when installed
  → Live PLY lerp viewport → Mesh export
```

## Experimental Mode pipeline

```
Video → frame gate → detect CaptureProfile
  → profile-matched research pose hypotheses (scored)
  → canonical COLMAP/ENU + validation gates (reject before fusion)
  → confidence-fuse densifiers (schema v2) — never raw concatenate
  → optional 4D / large-scene / surface adapters on separate paths
  → Difix then Fixer
  → gsplat Max (MCMC+AbsGrad+AA+appearance+bilagrid) or Brush Max
  → Live PLY lerp viewport → Mesh export
```

Routing table lives in `src-tauri/src/pipeline/experimental.rs`.

## License map (hard constraints)

| Component | License | Standard | Experimental |
|---|---|---|---|
| VGGT-1B-Commercial | Meta AUP (commercial) | Primary SfM | Fallback in chain |
| VGGT-Ω | CC BY-NC-4.0 | Never | Preferred static SfM |
| MASt3R / DUSt3R / Pi3X | CC BY-NC(-SA) | Never | Static unordered |
| StreamVGGT / VGGT-Long / SLAM* | research/NC | Never | Long video |
| MonST3R / Easi3R | research/NC | Never | Dynamic + separate 4D |
| CityGaussian / Urban-GS / Horizon | research/NC | Never | Large aerial adapters |
| 2DGS / GOF / SuGaR / … | mix | Never (vendored) | Surface adapters |
| RoMa v2 code | **MIT** | Densify sidecar | Densify (`precise`) |
| Lichtfeld Densification Plugin | **GPL-3.0** | **Do not copy** — recipe only | Same |
| Difix3D+ | Research/gated | Off | Preferred polish (then Fixer) |
| NVIDIA Fixer | NVIDIA Open Model | ON when installed | ON when installed |
| COLMAP / DA3 / gsplat / Brush | BSD / Apache / Apache | ON | ON |
| TRITON / Wflow / GeoClaw | permissive (verify) | Never | External hydro install |
| SFINCS / HiPIMS / BG_Flood / Itzï | **GPL** | Never | External plugin only |

## Sidecar install

See `tools/sidecars/README.md` and `tools/sidecars/research/README.md`.

## Hydro experimental adapters

Registry + GPL external-plugin protocol: `src-tauri/src/geospatial/hydro.rs`,
docs in `tools/sidecars/hydro-plugins/README.md`. ANUGA/SWMM/WebGPU remain
separate Standard/preview todos — not implemented here.
