# Reel adoption notes — 2026-10-27

Sources: four Instagram reels digested 2026-10-27 plus the owner's "integrate wascape into instasplatter"
request. Full rationale and the cross-project decision table:
`C:\Users\<you>\.dsh\handoffs\2026-10-27-reel-widescope\HANDOFF.md` and `PROJECT-DECISIONS.md`.
Owner-approved browser default: `obscura` v0.2.5 (see `~/.pi/agent/AGENTS.md`).

Status: notes only. Nothing below is implemented yet. This repo is the primary target of the WASCAPE
workstream, so it carries the most detail.

## Verdict

InstaSplatter is a **reconstruction** tool with no delivery layer: it produces a plied scene and shows it
in an orbit viewport. WASCAPE is the delivery layer for exactly that kind of scene, and it is closed
source with its own format (SHLS), streaming runtime and WebGPU renderer, taken in-house in September
2026. So the integration is **architecture plus an export contract**, never imported code: export a
Studio-ready scene, then earn the movement, streaming, editing and performance characteristics that make
a capture feel like a place.

## By source

| Source | Applies | How it would be implemented | Gate |
|---|---|---|---|
| A — obscura browser | no | Tauri WebView app; `docs/E2E-GEO-V010.md` records that headless WebDriver screenshots are not available in the agent env. Browser receipts are not this repo's evidence path | — |
| B — replica parity method | no | Not a port of another product; no upstream to score against | — |
| C — measured render loop | yes (adapt) | A delivery scene carries measurements plus labelled multi-view renders plus a verdict before it is importable: floater count, ground-plane RMS, coverage, PSNR against held-out views. New file: `docs/delivery/rubric.json`; `tools/verify-delivery.ps1` produces the receipt. Measurements cap any "looks photoreal" claim | `npm run smoke` + `tools/verify-delivery.ps1` |
| D — eval sets / guardrails | no | No LLM surface; no external write handle to guard | — |
| E — WASCAPE | yes (primary) | E1–E5 below | `npm run typecheck`, `npm run clippy`, `npm run test:rust`, `npm run smoke`, plus `npm run perf` (new) |

## E1 — Studio-ready export preset

WASCAPE Studio ingests `.ply` and `.splat` and exports "the exact format the engine streams". This repo
already writes PLY, SPZ and web `.splat` (`ROADMAP-V2.md:115`, `[x]`), and already has the up-axis fit
(`ROADMAP-V2.md:100`, deterministic RANSAC plane, `estimate_up_axis`). So the preset is mostly ordering
of work that exists.

- New export profile `delivery-wascape`: ground-plane level, floater clean, colour-grade passthrough,
  subject mask from the selected frames, correct up axis, and a `manifest.json` recording every setting.
- Emit a `.splat` and `.ply` pair plus that manifest, in a directory named after the capture.
- One-time external proof: import the fixture into `studio.wascape.com` and record exactly what happened
  in `docs/INTEROP.md`. If the import fails, E1 is **blocked with the observed error**, not quietly
  dropped, and the preset is documented as unverified.
- Known limit to state in the docs: SPZ export exists but `decode_spz` lives only inside `mod tests`
  (`docs/REALITYLOG-SEAM.md:33`), so a Studio round trip cannot be verified by re-import here yet.

## E2 — Free-roam movement modes

Today: orbit, pan-tracking, zoom-to-cursor, model rotation, camera-frustum gizmos, and a flythrough path
sampler. A search of `src/` for collision/vehicle terms returns zero matches, so there is no walk, no
drive and nothing that collides.

- `walk`: eye height 1.7 m, gravity, step limit, collision against splat-derived occupancy.
- `fly`: free camera, no collision.
- `drive`: rigid body with collision against the same occupancy grid.
- Mode switch leaves orbit untouched; every mode reads the same camera state so the existing numeric
  camera assertions keep applying.

## E3 — Streamed delivery

Whole-file load is the current shape. WASCAPE borrowed HLS: chunked range delivery of a point cloud with
a worker pipeline that keeps the main thread drawing.

- Range-request chunk streaming, three workers (fetch / decode / persist) feeding the WebGL2 renderer.
- Gate on a measurable claim, not a screenshot: first frame rendered after reading less than a set
  fraction of the file, asserted in a test.

## E4 — Studio-like editing

Box and lasso region clean, ground levelling, colour grade, and mask-from-footage. All non-destructive
and revertible, each an operation on the scene graph with a unit test, so the UI stays a view of state.

## E5 — Performance gate

WASCAPE's published bar is 30 FPS on a laptop with no discrete GPU. This machine has an RTX 4060, so the
gate needs both: the measured number here, and one deliberately cap-limited run as a no-dGPU proxy.
`docs/PERF.md` records both and `npm run perf` fails below the recorded floor. Without it, E3 and E4 can
silently destroy the only thing that makes the result feel like a place.

## Rejected, with reason

- **Importing SHLS or reverse engineering the stream.** Closed format, no public spec, no licence.
- **Scraping `wascape.com` scenes or its Studio.** Not ours, and the terms do not permit it.
- **Claiming "WASCAPE-compatible" anywhere before E1's import is recorded in `docs/INTEROP.md`.**
- **Replacing the WebGL2 splat renderer with WebGPU as part of this work.** The geospatial preview layer
  already has a WebGPU path with a CPU fallback (`src/geospatial/preview/capabilities.ts`); the splat
  viewport's engine is a separate decision with its own risk, and E1–E5 do not require it.

## First step

E1's preset behind a flag, with the gate above, then the single manual Studio import recorded in
`docs/INTEROP.md`. Everything else is blocked on knowing whether that import is real.

## Not verified here

- Whether `studio.wascape.com` actually accepts this repo's `.ply`/`.splat` output. That is the point of
  E1's one-time import, and until it is recorded, compatibility is unknown.
- WASCAPE's format, runtime and renderer internals. Only the public site, devlog and the absence of a
  public repository were checked.
- The 30 FPS bar on non-discrete-GPU hardware; only a capped proxy run is proposed, not a real second
  machine.

## Prior backlog, consolidated (2026-10-27)

Older owner requests that belong to this project, with the status the reconciled ledger recorded.
Source: `C:\Users\<you>\.dsh\handoffs\2026-10-27-reel-widescope\PRIOR-BACKLOG-BY-PROJECT.md`
(derived from `dm-context-ledger.md`, the 2026-10-26 run reports and `fullinstadmNOTES.md`).
Statuses are 2026-10-26/27 receipts, not re-verified today: re-run this repo's gate before calling
anything done.

| # | Item | Status |
|---|---|---|
| A1 | "Instasplatter is the main, VR capture is a plugin, geospatial is a plugin; design it all properly" (10-12) | PARTIAL, sequenced (decision D2: prove the round trip before extracting the seam). The seam already exists: `src-tauri/src/pipeline/realitylog.rs` runs hyperscape's `scripts/reconstruct_session.py` and returns a `realitylog-capture-v1` receipt. 543 tracked files, Tauri 2 |
| B14 | Splat/3D capture reels: "implement into anything literally" | VERIFIED candidates: Scan-to-3JS, ABot-Recon, OneCanvas, Mint Playground/MCP, KIRI Engine, SUPRA-Tex, PolyLayout, GeoLibre (see `reelkit\DIGESTS.md`) |
| Goal 2 item 3 | MASt3R-SfM accuracy benchmark | HARNESS RUNNABLE, REAL COMPARISON BLOCKED. `tools/bench/mast3r_sfm_bench.py --selftest` PASS, `--fixture` PASS (1.43 deg recovered from 1.5 injected), `--mast3r` writes a blocked receipt. Three measured preconditions unmet: no capture folder, sidecar `.stub`, 1992 MiB free VRAM against ~6000 needed. `ROADMAP-V2.md:141` stays unchecked |
| V2 roadmap | Unchecked items at `ROADMAP-V2.md:133`, `:139`, `:141`, `:226`, `:247` | OPEN. Phases 3 and 5 were added by owner override; Phase 5 is mostly blocked because Brush is a downloaded release binary driven only over its CLI |

### Prior sessions on this project
- opencode 2026-07-20 - overhaul master plan ("everything that is wrong, not working, not finished")
- opencode 2026-07-20 - fixing the Rust backend sidecars under `tools/sidecars/`
- opencode 2026-08-24 - executed `PHOTOREALISM_MERGE_ROADMAP.md`
- dsh 2026-10-10 - continuation worker for the Antigravity InstaSplatter session

### What this means for the reel work above
E1-E5 sit on top of A1, not beside it: the delivery export, movement modes and streaming are the
"geospatial/VR as plugin" seam seen from the product side, so do not restructure the pipeline while
doing them. B14's verified candidates are the inputs E1's export contract must accept (KIRI and Polycam
`.ply` in particular). The MASt3R benchmark stays blocked and unclaimed; do not let a "camera poses
register live" claim rest on it.
