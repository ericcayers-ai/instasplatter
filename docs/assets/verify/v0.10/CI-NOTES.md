# CI-ready notes (Phase E → F handoff)

Phase F (**multiplatform-release**) owns version bump + [`.github/workflows/release.yml`](../../../../.github/workflows/release.yml).

Gates implemented in the release workflow:

```yaml
- name: Rust tests
  run: cargo test
  working-directory: src-tauri
- name: Typecheck
  run: npx tsc --noEmit
- name: Frontend build
  run: npm run build
- name: Tauri build
  run: npm run tauri build
```

Artifacts: Windows NSIS, Linux AppImage + deb, macOS dmg (unsigned / ad-hoc for v0.10).

Optional later: upload `docs/assets/verify/v0.10/` screenshots as release assets once MANUAL captures exist.

**Publish still blocked** until MANUAL GUI captures are filled or waived — see [VERIFY-SUMMARY.md](./VERIFY-SUMMARY.md).
