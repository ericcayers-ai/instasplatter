# Contributing to InstaSplatter

Thanks for taking an interest in InstaSplatter. This guide covers how to build
the app, propose changes, and keep contributions aligned with the project's
license and roadmap.

Please also read the [Code of Conduct](CODE_OF_CONDUCT.md).

## Project shape

InstaSplatter is a **Tauri 2** desktop app with two product suites in one shell:

| Suite | Job |
|---|---|
| **Reconstruction** | Capture → cameras → dense evidence → live splat / mesh export |
| **Geospatial** | Georegistered scene → DEM/layers → flood scenarios → timed exports |

| Layer | Stack | Location |
|---|---|---|
| UI | React 18, TypeScript, Vite, Tailwind, Zustand, MapLibre | `src/` |
| Native shell / pipeline | Rust (Tauri commands, COLMAP/Brush/gsplat orchestration, georeg, flood, mesh, export) | `src-tauri/` |
| Sidecars / trainers | Optional densifiers, hydro workers, research tooling | `tools/sidecars/` |
| Docs / plans | Research notes, roadmaps, release notes | `docs/`, `ROADMAP.md`, `ROADMAP-V2.md`, `RELEASE.md` |

**Standard vs Experimental** applies in both suites: Standard stays commercially redistributable (adapters fail clearly without local weights); Experimental unlocks NC research paths after a one-time ack. GPL hydro engines stay plugin-only and are never bundled. See [docs/RESEARCH-STACK.md](docs/RESEARCH-STACK.md) and [tools/sidecars/README.md](tools/sidecars/README.md).

Primary target platform today is **Windows 10/11**. Keep changes Windows-friendly unless a PR explicitly adds another OS.

## Before you start

1. Skim **[ROADMAP-V2.md](ROADMAP-V2.md)** (current V2 phases) and **[ROADMAP.md](ROADMAP.md)** (longer-range plan).
2. Prefer Apache-2.0 / MIT / BSD (or similarly redistributable) dependencies. Do **not** vendor Inria NC / GS-license-adjacent code into the default product path. Research-only sidecars must stay gated behind Experimental Mode.
3. Search [existing issues](https://github.com/ericcayers-ai/instasplatter/issues) before opening a duplicate.

## Development setup

**Prerequisites**

- Windows 10/11 (64-bit)
- [Rust](https://rustup.rs/) stable + MSVC toolchain
- [Node.js](https://nodejs.org/) 20+
- [FFmpeg](https://ffmpeg.org/) on `PATH` (for video input), e.g. `winget install ffmpeg`

**Commands**

```bash
npm install
npm run tauri dev      # hot-reload development app
npm run tauri build    # NSIS installer under src-tauri/target/release/bundle
```

On first run the app downloads COLMAP and Brush (~200 MB) into app data. Optional scientific flood needs ANUGA/SWMM workers under `tools/sidecars/` — without them, geospatial uses a labelled demo/scaffold path (not authoritative).

## Testing

Before opening a PR, run what you can from:

```bash
cd src-tauri && cargo test
npx tsc --noEmit
```

There is no separate frontend unit-test runner yet. Prefer `cargo test` for Rust pipeline/maths/georeg/flood changes, `tsc --noEmit` for TypeScript, and a manual smoke of `npm run tauri dev` for UI or pipeline wiring:

- **Reconstruction**: drag a short video or image folder; confirm live viewport / export path if touched.
- **Geospatial**: open/create a geo project; confirm map layers, flood run labels (scientific vs demo/preview), and that exports do not claim authority when uncalibrated.

Optional Rust hygiene for native changes:

```bash
cd src-tauri && cargo clippy -- -D warnings
cargo fmt --check
```

## Code style

- **Rust**: idiomatic `rustfmt` formatting; prefer clear module boundaries under `src-tauri/src/` (pipeline, engines, mesh, geospatial, flood, export).
- **TypeScript / React**: keep components focused; match existing Zustand store patterns under `src/`. Avoid drive-by refactors outside the PR scope.
- **UI**: follow the existing visual language rather than introducing a new design system.
- Prefer small, reviewable PRs over mega-diffs.

## Pull request process

1. Fork (or branch from `main`) and keep your branch up to date.
2. Open an issue first for large features or architectural changes.
3. Use the PR template: summary, linked issue, test plan, checklist.
4. Describe **what** changed and **why**, especially for training/SfM/mesh quality defaults, suite routing, or flood authority labelling.
5. Do not bump the package version unless the maintainer asks; docs-only changes do not need a release.
6. Contributions are licensed under **Apache-2.0** (see [LICENSE](LICENSE)). By submitting a PR you agree your contribution is provided under that license.

## Reporting bugs / requesting features

Use the [bug report](https://github.com/ericcayers-ai/instasplatter/issues/new?template=bug_report.yml) and [feature request](https://github.com/ericcayers-ai/instasplatter/issues/new?template=feature_request.yml) issue forms. Include GPU/OS, InstaSplatter version, suite (Reconstruction / Geospatial), and a short repro when filing bugs.

## Questions

Open a GitHub issue and tag it as a question, or discuss on the relevant PR/issue thread.
