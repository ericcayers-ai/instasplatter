# Experimental Minecraft schematic export — design

**Status:** approved for autonomous implementation (cloud agent)  
**Release target:** v0.9.1  
**Scope:** Reconstruction suite, Experimental Mode only

## Problem

Users who reconstruct a scene as Gaussian splats have no path into Minecraft builders (WorldEdit, Litematica, Axiom). An experimental export should turn a finished splat into a pasteable region without leaving the app.

## Research summary

| Format | Extension | Notes |
|---|---|---|
| Legacy MCEdit | `.schematic` | Numeric block IDs; obsolete after 1.13 |
| **Sponge Schematic v2** | **`.schem`** | Palette + varint `BlockData`; WorldEdit / FAWE / most converters |
| Sponge Schematic v3 | `.schem` | Nested `Blocks` container; newer, less universal |
| Litematica | `.litematic` | Mod-specific multi-region; not needed for v1 |

**Choice: Sponge v2 `.schem`.** Widest paste tooling, flat NBT root named `Schematic`, Gzip-compressed. Modern `DataVersion` (1.21.x) so block names resolve correctly.

**Voxelization source: Gaussian splat PLY** (always present after a successful recon). Mesh TSDF is slower and optional; skip for this release.

**Colour strategy: nearest Minecraft concrete** (16 colours). Concrete reads cleaner than wool for photogrammetry-style builds and is vanilla-only.

**Sizing:** Fit the robust AABB (95th-percentile radius, same spirit as splat export) into a max axis of **128 blocks** by default (clamp 16–256). Surface stamps from opaque Gaussians — reconstructions are already shell-like, so no hollow pass.

## Approaches considered

1. **Splat → occupancy grid → Sponge v2** *(recommended)*  
   Pros: fast, native Rust, matches existing export bake (viewport rotation). Cons: soft surfaces become blocky (inherent).

2. **Mesh TSDF → voxels → schem**  
   Pros: solid volumes. Cons: depends on mesh extract; slow; Experimental export would feel like a second mesh job.

3. **External CLI (Mineways / Cubical)**  
   Pros: mature. Cons: Windows-only deps, GPL/NC risk, not local-first zero-config.

## Architecture

```
PLY splat (+ optional 3×3 rotation)
  → colour from SH DC, opacity filter
  → robust bounds + metres→blocks scale
  → weighted RGB accumulators per cell
  → nearest concrete palette index
  → Sponge v2 NBT (fastnbt) + Gzip → .schem
```

### Components

| Unit | Responsibility |
|---|---|
| `splat/schematic.rs` | Voxelize, palette map, encode varints, write `.schem` |
| `lib.rs` IPC `export_minecraft_schematic` | Gate on Experimental Mode; reuse splat rotation bake |
| TitleBar + store action | Experimental-only “Export schematic” button + save dialog |
| About / RESEARCH-STACK / README | Document experimental export |

### Options (IPC / UI)

- `maxExtent` (default 128) — longest axis in blocks  
- `opacityMin` (default 0.1) — ignore faint Gaussians  
- Viewport `rotation` / project `model_rotation` — same bake as splat export  

### Error handling

- Empty cloud / no occupied cells → clear error  
- Dimensions exceeding `u16` → reject before write  
- Experimental Mode off → command refuses with message to enable Experimental  

### Testing

- Varint encode/decode round-trip  
- Palette nearest-neighbour for known RGB  
- Tiny synthetic cloud → valid Gzip NBT with expected Width/Height/Length, air+concrete palette, non-empty BlockData  
- IPC gate unit-level via export options validation  

## Non-goals

- Litematica / Structure Block NBT / Bedrock `.mcstructure`  
- Entities, biomes, block entities  
- Geospatial DEM→terrain schematic (future)  
- Standard Mode exposure  

## Success criteria

1. With Experimental ON and a finished splat, user can save a `.schem` that WorldEdit-class tools accept.  
2. `cargo test` and `npx tsc --noEmit` green.  
3. Documented as experimental; Standard path unchanged.  
