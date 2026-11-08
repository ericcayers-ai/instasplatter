# Orchestrate run — instasplatter (2026-11-07)

Order: `[[ORDER-ID: instasplatter-20261107]]`, priority "a real SOG reader with a
fixture and a measured parse; no weight downloads, no GPU".

Branch `feat-sat-splat`, HEAD `7f53550`, 157 dirty paths before this run (owner
reviews the tree; nothing was committed).

## Obsolescence check (§1) — not obsoleting

`sunset-gs → instasplatter` is the recorded successor direction, and no
`CONSOLIDATION`/`ARCHIVE` note exists in the tree (`grep -ril` over `docs/*.md`
and `ROADMAP-V2.md` returns only forward-looking text). No
`ARCHIVE-DECISION-*.md` was written.

## Re-verification of the six ranked items (§1b, one command each)

| # | item | state before this run | verdict |
|---|---|---|---|
| 1 | no SOG reader; SPZ decode only in `mod tests`; `KHR_gaussian_splatting` writer stub | `docs/REALITYLOG-SEAM.md:34` "zero SOG support anywhere in this repo"; `Format::parse("sog") == None` asserted in `realitylog_library.rs:352` | **partly closed** — reader written and measured (below); seam still PLY-only, no Streamed SOG, no glTF writer |
| 2 | VPS anchor/descriptor export is external-JSON only | untouched | open |
| 3 | tiled ENU frames break beyond ~33 km | untouched | open |
| 4 | no screen-space LOD inside a chunk | untouched | open |
| 5 | flood provenance not surfaced in UI | untouched | open |
| 6 | ANUGA flat-bed scaffold | untouched | open |
| — | version 0.10.0 (package.json) vs 0.10.1 (docs) | `grep '"version"' package.json` → `0.10.0`; `src-tauri/Cargo.toml` → `0.10.0` | open, cosmetic |

Nothing was dropped as already-closed.

## Gate, measured before any edit (§2)

```
cargo test --locked   →  error: test failed, to rerun pass `--test renderer_boundary`
test the_csp_admits_no_remote_script_origin ... FAILED
test result: FAILED. 2 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 2.18s
```

The gate is **red at HEAD**, before this run's changes, and the cause is a stale
assertion (`renderer_boundary.rs:135` requires the literal `connect-src 'self'
https:`, the config now says `connect-src 'self' http://ipc.localhost https:`).
Detail and the fix shape: `docs/evidence/untangle-2026-11-07.md`.

`npm run smoke:static`, `npx tsc --noEmit` and a second `cargo test` were **not**
run after the edits: the orchestrator forbade `cargo` for the rest of the run
(disk 8 GB free; each `src-tauri/target` regrowth was ~1.8 GB in 6 minutes) and
`src-tauri/target` was deleted between attempts. The Rust changes are therefore
compile-evidenced only by the probe runs, not gate-verified.

## What was implemented

New: `src-tauri/src/splat/sog.rs` — a SOG v2 reader (ZIP → `meta.json` + lossless
WebP attribute images → `SplatCloud`), `pub mod sog;` in `splat/mod.rs`, and a
`splat_contract` re-export in `lib.rs`. No new dependency (`zip` and `image` with
the `webp` feature were already direct dependencies), no download, no GPU.

Measurement, fixtures, verbatim probe output, and the exact owner command:
`docs/evidence/sog-reader-2026-11-07.md`. Headline numbers:

- 113 085 / 113 085 decoded positions pair with the source PLY within 0.05
  (the writer reorders splats, so this is a set match, not an index match).
- Positions are signed-log encoded: decoding them linearly puts the same archive
  in a ±4.5 box instead of the source's −89.9 … 81.4.
- Opacity is `sigmoid(logit)` in the 8-bit alpha channel; the four measured round
  trips reproduce to 0.02 logit and saturate beyond ±20.
- Scales and DC are `codebook[byte]` with no further transform; deltas are the
  codebook step (~0.02 near zero) except where the source value is outside the
  256-entry codebook.
- **Quaternions are not solved**: no channel permutation yields a unit
  quaternion (best still 0.98 unit error, mean |dot| 0.5393 on the paired splat,
  11/2000 exact membership). Orientation is an open item, stated in the reader's
  own doc so it cannot be mistaken for a finished path.

## Token/disk discipline note

The probe loop (`cargo run --release --example sog_probe`) was run eight times to
resolve the format; each release rebuild regrew `target`. That was the right
research move and the wrong disk move — the reader's remaining unknowns (the
quaternion encoding) should be resolved with one instrumented build, not eight.

## Scratchpad

`C:/Users/<you>/.dsh/scratch/orchestrate-2026-11-07/progress/instasplatter.md`.
