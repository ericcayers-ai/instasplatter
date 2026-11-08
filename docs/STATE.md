# instasplatter — state

Updated 2026-11-07. Keep under 60 lines.

## Gate

`npm run smoke:static` · `npx tsc --noEmit` · `cd src-tauri && cargo test --locked`

**`cargo test --locked` is RED at HEAD `7f53550` and was red before this run's
changes**: `tests/renderer_boundary.rs:135` asserts the literal
`connect-src 'self' https:` while `tauri.conf.json:27` declares
`connect-src 'self' http://ipc.localhost https:` — the config gained the Tauri
IPC origin and the assertion was never updated. Fix the assertion (check the
directive's sources, not a substring), not the CSP. See
`docs/evidence/untangle-2026-11-07.md`.

## Verified at update time

- branch `feat-sat-splat`, HEAD `7f53550`, working tree **deliberately uncommitted** (owner reviews).
- Last green gate record (2026-11-03): `npx tsc --noEmit` OK · `npm run lint` OK · `npm run build` OK ·
  `cargo test --locked` 346 passed / 0 failed / 3 ignored. Not re-run 2026-11-07 (cargo was off-limits
  after the disk interruptions; `src-tauri/target` was deleted).
- Smokes (last run 2026-11-03, all pass): `src/splat/frustum.smoke.ts`,
  `src/geospatial/precision.smoke.ts`, `src/geospatial/hazards/floodValidation.smoke.ts` (27/27),
  `src/geospatial/pointSplatFloodVps.smoke.ts`.
- Bench receipt: `docs/PERF.md`. UI receipt: `docs/assets/verify/vps-panel/`.

## New 2026-11-07: SOG v2 reader

`src-tauri/src/splat/sog.rs` (`read_sog` / `read_sog_bytes` → `SogCloud`), exported
through `splat_contract`. Measured against a real `splat-transform v3.4.2` archive
(113 085 splats) and its source PLY: 113 085/113 085 positions pair within 0.05,
opacity = `sigmoid(logit)` verified, scales/DC = `codebook[byte]` verified to the
codebook step. **Quaternions are not decoded correctly** (no permutation is a unit
quaternion; mean |dot| 0.5393) — orientation is an open item. Numbers, fixtures and
probe output: `docs/evidence/sog-reader-2026-11-07.md`.

## Open, ranked

1. SOG: quaternion encoding unresolved; no streamed-SOG (`lod-meta.json`) reader; no caller yet —
   nothing dispatches `.sog` (`splat::ply::read_ply` has 14 call sites).
2. 3D Tiles + `KHR_gaussian_splatting` writer is still a stub (`realitylog_library.rs`).
3. VPS anchor/descriptor export from a finished reconstruction (the localiser has no in-app input).
4. Tiled ENU frames beyond ~33 km (precision work stops at "you must tile").
5. Screen-space LOD inside a chunk (culling is whole-chunk only).
6. Flood provenance (coverage, nodata, mass error, depression fill) computed but not shown in the UI.
7. ANUGA stays a flat-bed scaffold; no external cross-validation of the in-app solver.
8. Version drift: `package.json` / `Cargo.toml` 0.10.0 vs docs 0.10.1.

## How to resume

Run the gate and fix the `renderer_boundary` assertion first (it is the only known
red test). Then `docs/evidence/sog-reader-2026-11-07.md` §"What the owner runs" to
confirm the SOG numbers in one command, and `docs/AUDIT-2026-11-03.md` for the rest.
