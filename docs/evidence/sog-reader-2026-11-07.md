# SOG reader — implementation and measurement (2026-11-07)

Closes ranked open item 1 of `docs/STATE.md` ("Delivery formats: SOG/Streamed SOG
reader…"), the hole `docs/AUDIT-2026-11-03.md` §8 and `docs/REALITYLOG-SEAM.md`
line 34 both name: the host could not read the container its own capture source
exports.

## What was added

| file | change |
|---|---|
| `src-tauri/src/splat/sog.rs` | new: SOG v2 reader — ZIP → `meta.json` + lossless-WebP attribute images → `SplatCloud` |
| `src-tauri/src/splat/mod.rs` | `pub mod sog;` |
| `src-tauri/src/lib.rs` | `splat_contract` re-exports `sog` next to `ply` (the surface `tests/` drives) |

No dependency was added: `zip` and `image` (webp feature) were already direct
dependencies. Nothing was downloaded; no GPU was used.

## Fixtures (both real, both outside this repo)

- archive: `C:/Users/<you>/OneDrive/Desktop/Programs/hyperscape-lite/docs/evidence/p6.3-splat_7000-20260920T093656Z.sog`
  (4,154,294 bytes, `{"version":2,"asset":{"generator":"splat-transform v3.4.2"},"count":113085}`,
  8 ZIP members: `meta.json`, `means_l.webp`, `means_u.webp`, `quats.webp`, `scales.webp`,
  `sh0.webp`, `shN_centroids.webp`, `shN_labels.webp`; attribute images are 340×336 VP8L)
- source PLY: `C:/Users/<you>/OneDrive/Desktop/Programs/hyperscape-lite/gs_run/raw/brush_out/splat_7000.ply`
  (26,689,611 bytes, `element vertex 113085`, Brush export, SH degree 3)

## Measured: positions need the signed-logarithmic decode

First decode attempt, plain 16-bit linear dequantization between `means.mins`
and `means.maxs` (probe output, verbatim):

```
counts sog=113085 ply=113085
sog bounds [-4.509715, -4.408096, -3.6344154] .. [4.4120007, 4.3936048, 4.343295]
ply bounds [-89.8959, -81.11297, -36.879704] .. [81.43422, 79.94253, 75.96076]
sorted-by-x max|delta|=85.3862
```

`sign(t)·(exp(|t|)−1)` — the archive's `mins`/`maxs` are in signed-log space —
puts the same decode on the source's box:

```
counts sog=113085 ply=113085 sh_bands=3
sog bounds [-89.89592, -81.11296, -36.8797] .. [81.43422, 79.93163, 75.96072]
ply bounds [-89.8959, -81.11297, -36.879704] .. [81.43422, 79.94253, 75.96076]
```

`bounds` matching is necessary, not sufficient: the writer **reorders** splats, so
each decoded position was paired against the source PLY by position (grid, cell
0.1, tolerance 0.05):

```
position pairing: 113085/113085 matched, 0 unmatched
  pair0 sog=[-25.815434, -25.053978, -15.240782] ply=[-25.814417, -25.051107, -15.239634]
```

113 085 / 113 085 splats land on a source splat within 0.05. Splat `i` is texel
`(i % width, i / width)` of every attribute image — confirmed by that pairing on
a **non-square** 340×336 image, so row-major is measured, not assumed.

## Measured: opacity is `sigmoid(logit)` in the 8-bit alpha channel

```
opacity pairs (ply_logit -> reader_logit -> a_byte):
  -3.286 -> -3.308 -> 9
  -3.270 -> -3.308 -> 9
  -3.280 -> -3.308 -> 9
  -4.027 -> -4.139 -> 4
  -2.580 -> -2.639 -> 17
  -3.055 -> -3.099 -> 11
opacity: sigmoid-inverse max|logit delta|=18.1511
```

The four 0.01-level round trips are the assertion in
`sog.rs::the_opacity_channel_inverts_to_the_measured_logits`. The 18.1511 maximum
is saturation only: an 8-bit channel pinned at 0 or 255 cannot represent a logit
beyond ±20, so the reader clamps there (`opacity_logit(0) = -20.0`,
`opacity_logit(255) = 20.0`, both asserted).

## Measured: scales and DC resolve to the nearest codebook entry

```
scales axis 0: max|delta|=10.3094 exact=3674
scales axis 1: max|delta|=13.7546 exact=3708
scales axis 2: max|delta|=10.8090 exact=3740
scale+dc sample (ply_log | reader_log | ply_dc -> reader_dc):
  [-3.1707299, 0.7832705, 1.7982318] | [-3.1538439, 0.8019789, 1.8155928] | [-0.3263006, -0.7369674, -1.155632] -> [-0.33484158, -0.7320022, -1.1580502]
  [1.6081246, -1.2016796, 0.80583686] | [1.6136173, -1.1922011, 0.8019789] | [0.15813239, -0.2845839, -0.6943664] -> [0.16105771, -0.28301868, -0.6983767]
  [2.2517507, -0.98911023, 0.2755213] | [2.3208034, -1.0119511, 0.27555314] | [0.5454968, 0.048190262, -0.6499275] -> [0.5462905, 0.043755777, -0.64832413]
```

Per-splat deltas are the codebook step (~0.02 near zero); the 10.3–13.8 maxima are
source values outside the 256-entry codebook, which clamps. Both `scales` and
`sh0` are a single byte → `codebook[label]`, with no further transform (the
`symlog` alternative was measured and is wrong by 4.6e6 / 190 respectively).

## Not measured: quaternions

```
  ply_raw [-0.2217 0.1991 -0.7962 0.5266] unit [-0.222 0.199 -0.796 0.527] | reader [0.310 -0.286 -0.749 0.992] rnorm=1.313
quat membership: 11/2000 reader quats exist in the PLY quat set
quat mean |dot| with paired PLY splat = 0.5393
quats: SOG channel k = PLY[[3, 0, 1, 2]] (0=w,1=x,2=y,3=z), max|unit delta|=0.98258
```

`PLY[[3,0,1,2]]` is the *best of all 24 channel permutations*, and it still misses
by 0.98 in unit space; the decoded four-tuple is not a unit quaternion under any
permutation (norms 1.086…1.425). So the quaternion encoding is **not**
`(w,x,y,z)`-times-`byte/255*2−1`, and this session could not establish it.
The reader therefore returns the archive's channels as stored and says so in its
module doc; rendering orientation from them is an open item, not a finished path.
The ignored end-to-end test asserts the observed norm band (0.9…1.6) so the gap
cannot silently close or widen unnoticed.

## What the owner runs (one command)

```powershell
cd src-tauri
$env:SOG_ARCHIVE="C:/Users/<you>/OneDrive/Desktop/Programs/hyperscape-lite/docs/evidence/p6.3-splat_7000-20260920T093656Z.sog"
$env:SOG_REFERENCE_PLY="C:/Users/<you>/OneDrive/Desktop/Programs/hyperscape-lite/gs_run/raw/brush_out/splat_7000.ply"
cargo test --locked sog -- --ignored --nocapture
```

`reproduces_the_ply_it_was_exported_from` asserts: 113 085 splats decoded,
`shN.bands == 3`, 113 085/113 085 positions paired within 0.05, the six decoded
bounds above to ±0.02, opacity strictly inside (0,1), quat norms 0.9…1.6.

## What is NOT proven

- No `cargo test` was run in this session: the orchestrator forbade cargo after
  the third disk interruption (`src-tauri/target` grows ~1.8 GB per build and the
  volume is at 8 GB free). Compile evidence is indirect — the reader built and
  ran inside probes 7–15 (`cargo run --release --example sog_probe`), whose
  output is quoted above; `src-tauri/target` was then deleted by the orchestrator
  and the probe example removed.
- No streamed SOG (`lod-meta.json` + per-LOD chunks) reader: this is the bundled
  single-file container only.
- `shN` (palette higher-order SH) is not decoded — reported via
  `SogCloud::sh_bands_in_archive`, not silently dropped.
- The reader has no caller yet: `splat::ply::read_ply` still has 14 call sites
  and none dispatches on extension. Wiring `.sog` into one entry point is the
  next action, deliberately left out of this change.
