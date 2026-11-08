# RealityLog seam — Capture Source contract

**Status:** implemented (migration steps M1–M8 of the design at
`continuation-2026-10-11/MERGE-instasplatter-hyperscape-DESIGN.md`).
**Date:** 2026-10-11

InstaSplatter is the surviving application. `hyperscape-lite` / RealityLog stays
whole **at its own path** and is consumed as a *by-path Capture Source*: the host
invokes it as a subprocess and pulls its artefacts over HTTP. Nothing is
vendored, submoduled or copied; the source keeps its tree, its history and its
licence, and the dependency runs **one way** — instasplatter → hyperscape-lite.

| Edge | Direction | Mechanism |
| --- | --- | --- |
| A | source → host | `GET /library`, then `GET /captures/<id>/<file>` on the source's own capture-library service (`src/pipeline/realitylog_library.rs`) |
| B | host → source | `python <root>/scripts/reconstruct_session.py …` with every engine path injected (`src/pipeline/realitylog.rs`) |
| C | host → source | engine paths are *injected*, so the source never resolves an asset of its own |

## The crossing format is PLY, and nothing else

The seam carries the **artefact**, not the renderer. InstaSplatter keeps its
hand-written WebGL2 renderer (`src/splat/renderer.ts`); the source's three.js +
Spark `viewer/` is deliberately not ported, because the host's CSP
(`src-tauri/tauri.conf.json`, `script-src 'self' 'unsafe-eval'`) admits neither
its inline import map nor its `cdn.jsdelivr.net` module fetches.

Only 3DGS binary PLY crosses, because it is the only format both sides already
read natively. Everything else is refused rather than half-read:

| Format | Source writes it? | Host reads it? | Verdict |
| --- | --- | --- | --- |
| **3DGS PLY** | yes (Brush `splat_<steps>.ply`) | yes (`splat/ply.rs::read_ply_bytes`) | **the seam's transport** |
| SPZ | yes | **no** — `decode_spz` exists only inside `mod tests` (`splat/export.rs`) | refused; promoting it out of `mod tests` is a separate, separately gated change |
| SOG | yes (`scripts/export_sog.py`, `splat-transform` 3.4.2) | **no — zero SOG support anywhere in this repo** | refused |
| glTF `KHR_gaussian_splatting` | no | writer is a stub (`geospatial/exports.rs`) | refused |
| `.ksplat` | no | zero support | refused |

`realitylog_library::select_ply` is the single place that decides this, and
`entry.splat_format` must be `ply` for an entry to be pulled.

## Renderer boundary

`src/splat/renderer.ts` is the **only** in-app renderer, and the source's
`viewer/` is deliberately not ported. `src-tauri/tests/renderer_boundary.rs`
enforces that rather than trusting it: it fails if any file under `src/` or
`index.html` loads a script module from a remote origin, if the CSP's
`script-src` gains a remote host or `'unsafe-inline'`, if a `viewer/` file
appears in the host tree, or if anything imports `three` or
`@sparkjsdev/spark`. Its ignored companion (`built_assets_carry_no_remote_script_origin`)
runs the same check over a built `dist/`.

## What the host requires of the source

* `scripts/reconstruct_session.py` must exist on disk. It is **untracked in the
  source's git index** at the time of this merge, so a clone does not receive
  the reconstruction chain; the adapter fails closed with
  *"the reconstruction chain is untracked in the source checkout"* rather than
  reporting a generic missing file.
* `MQ3DRECON_EXE` must point at the third repo's `mq3drecon` executable
  (`Programs/mq3drecon`), which this merge neither vendors nor installs.
  `REALITYLOG_ROOT` (or `--realitylog-root`, or the configured path) selects the
  checkout.
* The source's `git status --porcelain` is byte-identical before and after a
  seam run. The host only ever reads the checkout or executes it as a
  subprocess, so a second writer in that tree cannot be disturbed by this seam.

## Trainer cadence

Both repos drive the same Brush v0.3.0 archive, and both had their own copy of
the numbers. The seam injects exactly one of them — the training length — through
the source's existing `--steps`, so there is one authority at run time:

| Number | Source's value | Host's lever |
| --- | --- | --- |
| `TOTAL_STEPS` | 7000 | **injected** as `--steps` |
| `EVAL_SPLIT_EVERY` | 10 | the source's |
| `EVAL_EVERY` | 1000 | the source's |
| `EXPORT_EVERY_BATCH` | 2000 | the source's |
| `EXPORT_EVERY_LIVE` | 1000 | the source's — *live* path only |

**The `EXPORT_EVERY` divergence is unexplained, and is not to be "fixed".**
`scripts/brush_defaults.py` in the source records that the batch path exports
twice as rarely as the live `pc_receiver` path with "no receipt, comment or
commit message" saying why, and explicitly says not to cite it as intentional.
The host therefore injects no `--export-every`: it records the divergence
(`realitylog::SOURCE_EXPORT_EVERY_BATCH` vs `SOURCE_EXPORT_EVERY_LIVE`) and the
source's `--steps` is the only cadence lever it touches.

`src-tauri/tests/` verifies this two ways: hermetically against a recorded
transcript of the real chain, and (ignored, needs `REALITYLOG_ROOT`) by parsing
`scripts/brush_defaults.py` and `scripts/tests/test_reconstruct_session.py` in
the source and asserting the host's recorded constants equal the source's own.

## Live evidence (2026-10-11)

* Source `d2c8c0a`, host `7f53550`; source porcelain hash
  `71a6fa3bc7e0abd1386db3e2d47cd275a78d4477` (588 entries) before **and** after
  the seam ran its real dry-run gate.
* Real `--dry-run` against the source checkout: exit 0, 7 steps, matching the
  stage list in `reconstruct_session.py`'s own docstring.
* Live library pull from the source's own `python -m pc_receiver.capture_library`:
  14,362,858 bytes, sha256 `dbbdd096…`, **60,853 splats** — the same count the
  source's own PLY reader reports for those bytes.
