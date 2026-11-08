# Pre-publication audit — 2026-11-09

Scope: the whole repository, as it was about to be made public: secrets and PII,
licensing, the security of every Tauri command and process spawn, dependency
advisories, and user-facing stubs. Written so a reviewer can re-run each check.

What was **not** run here, and why: `cargo test`, `cargo clippy` and `npm install`
are blocked on this machine by a disk floor (~4 GB free; a cold Tauri debug build
needs ~6–8 GB). Every local gate that does not compile Rust *was* run, and the
Rust gate runs in CI (`.github/workflows/ci.yml`) on every push to `main`. The
receipts section below separates measured results from claims.

## Method

| Sweep | Instrument | Result |
|---|---|---|
| Secret / PII | ripgrep over all 616 publish-set files (543 tracked + 73 untracked-not-ignored), 29 pattern classes | no credentials, no keys, no tokens, no `.env`, no private URLs |
| Unicode smuggling | `cybersafe_scan.py` + a code-point pass over tracked files | 1 finding (fixed, below) |
| Licensing | 388 npm packages + 559 crates resolved from `package-lock.json`, `Cargo.lock` and the vendored registry; SPDX expressions evaluated, not string-matched | 0 non-permissive packages that ship |
| Security | read of all 57 `#[tauri::command]`s, all 17 `Command::new` sites, both ZIP readers, every `reqwest` call site, CSP and capabilities | 2 HIGH, 5 MED, 9 LOW/INFO (below) |
| Stubs / gaps | exhaustive literal sweep of `src/` and `src-tauri/src/` plus module-reachability | 7 user-facing, 9 internal |
| Dependencies | `npm audit` | 19 advisories → 11 after `npm audit fix` |

## Fixed in this pass

1. **`delete_project` could delete any directory.** `Path::starts_with` is component-wise
   on the literal path, so `…\jobs\..\..\..\Documents` passed the prefix test and reached
   `remove_dir_all`. Both sides are now canonicalised and any `..` component is rejected
   (`lib.rs`), with a second check inside `pipeline::discard_workspace`.
2. **`export_diagnostics` wrote attacker-chosen content to an arbitrary path**, which is a
   Startup-folder persistence primitive. Every export command now goes through
   `guard_export_dest`: absolute, no `..`, existing parent, extension allowlist where the
   format is fixed, and refusal inside `SystemRoot`, `ProgramFiles`, `ProgramData`,
   `APPDATA` and `%LOCALAPPDATA%\Microsoft\Windows`.
3. **SOG archive reader was an unbounded decompression.** `read_to_end` with no budget and
   an unchecked WebP decode. Now: declared-size check before inflating, `Read::take` as a
   second line of defence (8 MiB per entry), and a 16 M-texel cap before `to_rgba8`.
4. **Exported `scientific_vs_preview.withinTolerance` was always `true`** — it compared
   hardcoded constants with themselves. It is now `null` with `"status": "not-measured"`,
   which is what the export actually knows.
5. **Raster provenance stopped at the process boundary.** The caller's warning notes were
   only in the manifest, never in the `.tif.json` that travels with the raster. They are
   now written into the sidecar, together with an explicit `"synthetic": true` flag for
   extent-derived fields.
6. **The flow-path "stub" line is no longer emitted.** A drawn diagonal in a
   delivery-named GeoJSON reads as a hydrologic result.
7. **CSP** now closes `object-src`, `base-uri` and `frame-ancestors`; capabilities drop the
   unused `opener:allow-reveal-item-in-dir`.
8. **OpenTopography API keys are redacted from error text** (`catalog.rs`), so
   `export_diagnostics` cannot publish a user's key.
9. **Unclamped grid sizes** in `sample_geo_dem`, `sample_geo_dem_hillshade` and
   `generate_sat_splat` are now bounded (≤ 4096 a side) and the sat-splat AOI is validated.
10. **Two permanently dead toolbar buttons** (Measure, Profile — "coming soon", `disabled`)
    are removed with their store branches, rather than shipped as gaps.
11. **Sidecar `.stub` markers restored** for the 15 adapters whose marker had been dropped
    while their launcher still has no upstream: `is_stub_sidecar` now reports them as
    unwired again.
12. **Documents scrubbed** of the build machine's username (25 lines, 11 files) and of one
    captured egress IP; the U+200B zero-width space in `ROADMAP.md:105` is gone.
13. **Agent/editor scaffolding untracked** (`.cursor/`, `.agents/`, `CLAUDE.md`) and
    gitignored, so a public clone is the app, not one developer's toolchain.
14. **`tests/renderer_boundary.rs`: the stale CSP assertion is fixed** (it demanded the
    exact string `connect-src 'self' https:`, which the app's own `http://ipc.localhost`
    origin had already made false) and two guards were added: the CSP hardening directives,
    and a scan that fails if the renderer ever gains an HTML/code injection sink.

## Residual, with the evidence that scopes it

| Item | Status | Evidence |
|---|---|---|
| `maplibre-gl@5.24` — GHSA-jrc7-96c5-q579 (critical, `DOM.sanitize` XSS bypass) | **Open.** Not reachable in this codebase: the app never calls `Popup#setHTML`, and `rg 'setHTML\|innerHTML\|dangerouslySetInnerHTML' src/` returns nothing. Now enforced by `the_renderer_has_no_html_or_code_injection_sink`. The fix is a major upgrade to `maplibre-gl@6` (ESM-only, WebGL2, `setWorkerUrl`), which needs `npm install` — blocked on the disk floor. | `npm audit --json`; the test above |
| 10 moderate advisories | **Open, dev-only.** All sit in the `eslint` / `@typescript-eslint` chain, none in the shipped bundle. `npm audit fix --force` would downgrade `@eslint/js` to 9.x. | `npm audit --json`, `dev: true` on every entry |
| `'unsafe-eval'` in `script-src` | **Kept deliberately.** No `eval`/`new Function` exists in `src/`, but Cesium's renderer path is unverified without a live webview, so removing it risks a blank globe. Add the three directives that cannot break script execution instead (done). | `rg 'eval\(\|new Function' src/` → none |
| `read_text_file` reads any path the user can read | **Accepted.** It is the import flow's file picker; it caps at 64 MiB. Post-webview-compromise exfiltration, with no in-app XSS sink to reach it. | `lib.rs`, `read_text_file` |
| Screenshots not pixel-reviewed | **Open.** The two settings/log captures (`R06`, `R07`) were checked for embedded text chunks, not visually. No image was available to an automated reader in this pass. | `grep -a` over the PNGs |
| 173 crates with unknown licence | **Scoped.** All are outside the `x86_64-pc-windows-msvc` target graph (GTK/macOS/Android/wasm). They matter only if a Linux/macOS bundle is produced, which `tauri.conf.json` declares. | `cargo metadata --offline --filter-platform x86_64-pc-windows-msvc` |
| Rust gate (test / clippy / build) | **Not run locally** — disk floor. Runs in CI on push. | `.github/workflows/ci.yml` |

## Licensing summary

Apache-2.0 application. 388 npm packages: 376 permissive, 12 flagged (all `lightningcss`
MPL-2.0 + platform binaries, build-only, absent from `dist/`). 367 crates in the Windows
graph: 5 MPL-2.0, unmodified, attributed in `NOTICE` and `THIRDPARTY.md`. Nothing GPL,
AGPL, SSPL or non-commercial is compiled or bundled. `THIRDPARTY.md` previously called
ANUGA GPL-2.0; upstream `LICENSE.txt` (GeoscienceAustralia/anuga_core) is Apache-2.0, and
the row is corrected.
