## Summary

<!-- What changed and why? Link related issues (e.g. Fixes #123). Note Reconstruction / Geospatial / shared shell scope. -->

## Test plan

<!-- How did you verify this? -->

- [ ] `cd src-tauri && cargo test` (if Rust changed)
- [ ] `npx tsc --noEmit` (if TypeScript/UI changed)
- [ ] Manual smoke — Reconstruction: `npm run tauri dev` — drag a short video or image folder (if UI/pipeline changed)
- [ ] Manual smoke — Geospatial: open/create a geo project; flood labels and exports honest if touched
- [ ] Docs-only / no runtime impact

## Checklist

- [ ] Change matches [ROADMAP-V2.md](../ROADMAP-V2.md) / linked issue intent (or clearly documents an intentional deviation)
- [ ] No NC / GS-license-adjacent code introduced on the **Standard** (default) product path
- [ ] Flood demo/preview paths stay labelled non-authoritative; scientific path only when evolve + calibration allow
- [ ] Sidecar adapters fail clearly without weights / workers (no pretend-ready stubs)
- [ ] No unrelated drive-by refactors
- [ ] README / docs updated when user-facing behavior changed
