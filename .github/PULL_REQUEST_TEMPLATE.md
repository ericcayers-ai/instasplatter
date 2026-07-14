## Summary

<!-- What changed and why? Link related issues (e.g. Fixes #123). -->

## Test plan

<!-- How did you verify this? -->

- [ ] `cd src-tauri && cargo test` (if Rust changed)
- [ ] `npx tsc --noEmit` (if TypeScript/UI changed)
- [ ] Manual smoke: `npm run tauri dev` — drag a short video or image folder (if UI/pipeline changed)
- [ ] Docs-only / no runtime impact

## Checklist

- [ ] Change matches [ROADMAP-V2.md](../ROADMAP-V2.md) / linked issue intent (or clearly documents an intentional deviation)
- [ ] No NC / GS-license-adjacent code introduced on the default product path
- [ ] No unrelated drive-by refactors
- [ ] README / docs updated when user-facing behavior changed
