# Contributing to InstaSplatter

Thanks for taking an interest in InstaSplatter. This guide covers how to build
the app, propose changes, and keep contributions aligned with the project's
license and roadmap.

Please also read the [Code of Conduct](CODE_OF_CONDUCT.md).

## Project shape

InstaSplatter is a **Tauri 2** desktop app:

| Layer | Stack | Location |
|---|---|---|
| UI | React 18, TypeScript, Vite, Tailwind, Zustand | `src/` |
| Native shell / pipeline | Rust (Tauri commands, COLMAP/Brush/gsplat orchestration, mesh, export) | `src-tauri/` |
| Sidecars / trainers | Optional engines and research tooling | `tools/` |
| Docs / plans | Research notes, roadmaps, release notes | `docs/`, `ROADMAP.md`, `ROADMAP-V2.md`, `RELEASE.md` |

Primary target platform today is **Windows 10/11**. Keep changes Windows-friendly unless a PR explicitly adds another OS.

## Before you start

1. Skim **[ROADMAP-V2.md](ROADMAP-V2.md)** (current V2 phases) and **[ROADMAP.md](ROADMAP.md)** (longer-range plan).
2. Prefer Apache-2.0 / MIT / BSD (or similarly redistributable) dependencies. Do **not** vendor Inria NC / GS-license-adjacent code into the default product path. See [docs/RESEARCH-STACK.md](docs/RESEARCH-STACK.md).
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

On first run the app downloads COLMAP and Brush (~200 MB) into app data.

## Testing

Before opening a PR, run what you can from:

```bash
cd src-tauri && cargo test
npx tsc --noEmit
node --experimental-strip-types src/splat/camera.ts
```

There is no separate frontend unit-test runner yet. Prefer `cargo test` for Rust pipeline/maths changes, `tsc --noEmit` for TypeScript, and a manual smoke of `npm run tauri dev` (drag video / image folder) for UI or pipeline wiring.

Optional Rust hygiene for native changes:

```bash
cd src-tauri && cargo clippy -- -D warnings
cargo fmt --check
```

## Code style

- **Rust**: idiomatic `rustfmt` formatting; prefer clear module boundaries under `src-tauri/src/` (pipeline, engines, mesh, export).
- **TypeScript / React**: keep components focused; match existing Zustand store patterns under `src/`. Avoid drive-by refactors outside the PR scope.
- **UI**: follow the existing visual language rather than introducing a new design system.
- Prefer small, reviewable PRs over mega-diffs.

## Pull request process

1. Fork (or branch from `main`) and keep your branch up to date.
2. Open an issue first for large features or architectural changes.
3. Use the PR template: summary, linked issue, test plan, checklist.
4. Describe **what** changed and **why**, especially for training/SfM/mesh quality defaults.
5. Do not bump the package version unless the maintainer asks; docs-only changes do not need a release.
6. Contributions are licensed under **Apache-2.0** (see [LICENSE](LICENSE)). By submitting a PR you agree your contribution is provided under that license.

## Reporting bugs / requesting features

Use the [bug report](https://github.com/ericcayers-ai/instasplatter/issues/new?template=bug_report.yml) and [feature request](https://github.com/ericcayers-ai/instasplatter/issues/new?template=feature_request.yml) issue forms. Include GPU/OS, InstaSplatter version, and a short repro when filing bugs.

## Questions

Open a GitHub issue and tag it as a question, or discuss on the relevant PR/issue thread.
