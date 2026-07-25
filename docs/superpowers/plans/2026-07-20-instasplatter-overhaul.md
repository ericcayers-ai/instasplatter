# InstaSplatter Comprehensive Overhaul Master Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan phase-by-phase. Each phase has independent tasks.

**Goal:** Complete end-to-end overhaul fixing all broken, stubbed, missing, poorly-presented, and gapped functionality across frontend, backend, CI, build system, and documentation.

**Architecture:** Six-phase approach: CI/Security Infrastructure → Frontend UI Overhaul → Backend Hardening → Tidy Up → Debugging Verification → Certification Testing. Each phase produces independently testable deliverables.

**Tech Stack:** Tauri 2 + React 18 + TypeScript 5.6 + Rust + Tailwind 4 + Vite 6 + Zustand 5 + Cesium 1.143 + MapLibre GL 5.24

## Global Constraints

- Preserve all existing functionality; no regressions
- Maintain Standard/Experimental mode gating
- Follow existing brand tokens (teal `#2fc4b0`, brass `#e0a34a`, midnight capture bay)
- Respect license gates (NC weights, GPL engines never bundled)
- Cross-platform: Windows primary, Linux/macOS best-effort
- All changes must pass: `cargo test --locked`, `npx tsc --noEmit`, `npm run build`
- No placeholder/stub text visible to end users after Phase 3

---

## Skill Guidance

The following skills are relevant to specific phases of this plan and should be invoked:

| Phase | Skills to Use |
|-------|---------------|
| Phase 0 | None (infrastructure/CI) |
| Phase 1 | `impeccable` critique/audit/polish, `frontend-design`, `accessibility`, `vercel-react-best-practices`, `vercel-composition-patterns`, `tailwind-css-patterns`, `typescript-advanced-types` |
| Phase 2 | `systematic-debugging`, `test-driven-development` |
| Phase 3 | `impeccable` clarify/distill/polish, `accessibility` |
| Phase 4 | `systematic-debugging` (root-cause tracing), `verification-before-completion` |
| Phase 5 | `test-driven-development`, `verification-before-completion` |

---

## Phase 0: CI / Security / Build Infrastructure

### Task 0.1: Add CI pull-request workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Purpose:** Run lint, type-check, build, and tests on every PR and push to main.

### Task 0.2: Harden release workflow

**Files:**
- Modify: `.github/workflows/release.yml`

**Purpose:** Fix `draft: true` (not `false`), pin all third-party actions by SHA, add `concurrency:`, validate `v*` tag regex, add artifact checksums, fix `cargo test --locked`.

### Task 0.3: Add development tooling configs

**Files:**
- Create: `.editorconfig`
- Create: `.prettierrc`
- Create: `eslint.config.js` (flat config)
- Create: `rustfmt.toml` (in src-tauri/)
- Create: `clippy.toml` (in src-tauri/)
- Create: `.nvmrc`
- Create: `rust-toolchain.toml` (in src-tauri/)
- Modify: `package.json` (add scripts: `lint`, `lint:fix`, `format`, `format:check`, `typecheck`, `test`, `audit`, `clippy`, `smoke`)

### Task 0.4: Fix tsconfig and vite config

**Files:**
- Create: `tsconfig.node.json`
- Modify: `tsconfig.json` (`noUnusedLocals: true`, `noUnusedParameters: true`)
- Modify: `vite.config.ts` (Cesium chunk split, expanded watch ignore, `clearScreen: false`)

### Task 0.5: Fix .gitignore

**Files:**
- Modify: `.gitignore`

**Purpose:** Add patterns for IDE files, secrets, installer outputs, `src-tauri/gen/`, log files, skill noise.

### Task 0.6: Add dependabot and CODEOWNERS

**Files:**
- Create: `.github/dependabot.yml`
- Create: `.github/CODEOWNERS`

### Task 0.7: Fix tauri.conf.json security

**Files:**
- Modify: `src-tauri/tauri.conf.json`

**Purpose:** Set real CSP, add `devtools: false` for release, add bundle metadata, split targets so non-Windows doesn't fail locally.

### Task 0.8: Fix stale verify artifacts

**Files:**
- Modify: `docs/assets/verify/v0.10/npm-build.log` (regenerate for v0.10)
- Modify: `docs/superpowers/plans/2026-07-17-minecraft-schematic-export.md` (update checkboxes)

### Task 0.9: Reconcile skills-lock.json vs tracked skill files

**Files:**
- Modify: `skills-lock.json`
- Modify: `.gitignore` (add `.claude/skills/` policy)

### Task 0.10: Add THIRDPARTY.md / NOTICE

**Files:**
- Create: `THIRDPARTY.md`
- Create: `NOTICE`

---

## Phase 1: Frontend UI Overhaul

**PREREQUISITE SKILLS:** Load `impeccable` (critique first, then polish command), `frontend-design`, `accessibility`, `vercel-react-best-practices`

### Task 1.1: Add Error Boundary and Suspense

**Files:**
- Create: `src/components/ErrorBoundary.tsx`
- Modify: `src/App.tsx`

### Task 1.2: Fix accessibility — modal focus traps

**Files:**
- Modify: `src/components/CaptureStart.tsx`
- Modify: `src/components/shell/AboutPanel.tsx`
- Modify: `src/components/shell/ExperimentalLicenseModal.tsx`

### Task 1.3: Fix canvas accessibility

**Files:**
- Modify: `src/components/Viewport.tsx`
- Modify: `src/geospatial/GeoWorkspace3D.tsx`
- Modify: `src/geospatial/GeoMap.tsx`
- Modify: `src/geospatial/globe/CesiumGlobe.tsx`

### Task 1.4: Fix global CSS issues

**Files:**
- Modify: `src/index.css`

### Task 1.5: Fix component accessibility — missing labels

**Files:**
- Modify: `src/components/shell/TitleBar.tsx`
- Modify: `src/components/shell/StatusBar.tsx`
- Modify: `src/components/shell/PropertiesPanel.tsx`
- Modify: `src/components/shell/RightDock.tsx`
- Modify: `src/components/shell/LeftDock.tsx`
- Modify: `src/components/BatchQueue.tsx`

### Task 1.6: Performance fixes — React patterns

**Files:**
- Modify: `src/state/store.ts` (split into slices)
- Modify: `src/components/shell/StatusBar.tsx` (remove dead `tick` state)
- Modify: `src/components/shell/RightDock.tsx` (fix `Date.now()` on every render)
- Modify: `src/components/shell/PropertiesPanel.tsx` (remove `JSON.stringify` comparison)
- Wrap key components in `React.memo`

### Task 1.7: Fix hardcoded colors and CSS inconsistencies

**Files:**
- Modify: `src/geospatial/globe/CesiumGlobe.tsx` (use CSS variable, not hardcoded `#0a0c10`)
- Modify: `src/geospatial/GeoMap.tsx` (use CSS variable, not hardcoded `#08121D`)
- Modify: `src/index.css` (consistent border-radius, define missing `--color-hazard`)

### Task 1.8: Fix stub/dead code visible to users

**Files:**
- Modify: `src/components/shell/RightDock.tsx` (line 279: remove EDL stub text)
- Modify: `src/geospatial/GeoToolbar.tsx` (add visual indicator for stub Measure/Profile tools)
- Modify: `src/components/shell/StatusBar.tsx` (fix `Ctrl+L` shortcut hint for Windows)

---

## Phase 2: Backend Hardening

**PREREQUISITE SKILLS:** Load `systematic-debugging` for error tracing, `test-driven-development`

### Task 2.1: Replace String error types with typed error enum

**Files:**
- Create: `src-tauri/src/error.rs`
- Modify: All `src-tauri/src/**/*.rs` (return `Result<_, AppError>` instead of `Result<_, String>`)

### Task 2.2: Fix mutex poisoning hazards

**Files:**
- Modify: `src-tauri/src/lib.rs` (replace `.lock().unwrap()` with `.lock().map_err()`)
- Modify: `src-tauri/src/queue.rs` (22 sites)
- Modify: `src-tauri/src/geospatial/hydro.rs`
- Modify: `src-tauri/src/pipeline/mod.rs`
- Modify: `src-tauri/src/pipeline/sidecars.rs`

### Task 2.3: Fix `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` panic hazards

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/queue.rs`
- Modify: `src-tauri/src/geospatial/hydro.rs`

### Task 2.4: Fix sidecar resolution for packaged installs

**Files:**
- Modify: `src-tauri/src/geospatial/hydro.rs`
- Modify: `src-tauri/src/pipeline/sidecars.rs`
- Modify: `src-tauri/src/pipeline/gsplat.rs`
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/tauri.conf.json` (bundle.resources)

### Task 2.5: Fix SWMM error swallowing

**Files:**
- Modify: `src-tauri/src/geospatial/hydro.rs` (emit errors, don't silently return `Vec::new()`)

### Task 2.6: Implement missing IPC commands

**Files:**
- Modify: `src-tauri/src/lib.rs` (add commands: `install_sidecar`, `pause_flood_run`, `delete_flood_scenario`, `verify_engine_signature`, `get_flood_engine_status` — fix hardcoded "demoAvailable")
- Modify: `src/lib/ipc.ts` (add typed bridge for new commands)

### Task 2.7: Fix `try_promote_to_standard` always returning Err

**Files:**
- Modify: `src-tauri/src/geospatial/hydro.rs`

### Task 2.8: Add COLMAP 4.1 camera model support

**Files:**
- Modify: `src-tauri/src/colmap.rs`

### Task 2.9: Add dependency checks

**Files:**
- Modify: `src-tauri/Cargo.toml` (remove unused `flate2`, pin `tauri` to specific minor)

---

## Phase 3: Tidy Up & Consistency

**PREREQUISITE SKILLS:** Load `impeccable` (clarify command for UX copy, distill for simplification)

### Task 3.1: Remove `window.confirm` for project deletion

**Files:**
- Modify: `src/components/shell/LeftDock.tsx` (replace with custom modal)

### Task 3.2: Fix `user-select` on log console

**Files:**
- Modify: `src/index.css` (scope `user-select: none`, override for log)
- Modify: `src/components/shell/LogConsole.tsx`

### Task 3.3: Add i18n infrastructure (keys only, English strings)

**Files:**
- Create: `src/lib/i18n.ts`
- Create: `src/lib/strings.ts`
- Modify: All UI components (use string keys)

### Task 3.4: Remove dead code

**Files:**
- Delete: `src/components/shell/SceneTree.tsx` (legacy re-export)
- Modify: `src/components/shell/PropertiesPanel.tsx` (remove deprecated default export)

### Task 3.5: Fix stale docs and verify artifacts

**Files:**
- Modify: `docs/E2E-VERIFICATION.md`
- Modify: `docs/assets/verify/v0.10/npm-build.log`
- Modify: `docs/superpowers/plans/2026-07-17-minecraft-schematic-export.md`

### Task 3.6: Add top-level tools/README.md

**Files:**
- Create: `tools/README.md`

### Task 3.7: Reconcile skill directory consistency

**Files:**
- Modify: `skills-lock.json`
- Modify: `.gitignore`

---

## Phase 4: End-to-End Debugging & Verification

**PREREQUISITE SKILLS:** Load `systematic-debugging` (root-cause tracing), `verification-before-completion`

### Task 4.1: Systematic debugging — known issues

1. `find_model_dir` never accepted `0/` directory (fixed per ROADMAP-V2 but verify)
2. `svd3` singular value edge case (verify fix)
3. `is_resumable` gap (verify fix)
4. HEIC input silently fails due to missing image crate feature
5. `COLOP::gen/` not fully ignored

### Task 4.2: Add structured logging across the app

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/pipeline/mod.rs`

### Task 4.3: Verify cancel/cleanup paths

**Files:**
- Read: `src-tauri/src/pipeline/mod.rs` cancel logic
- Read: `src-tauri/src/queue.rs` cancel logic

### Task 4.4: Verify license gates

**Files:**
- Read: `src-tauri/src/settings.rs` experimental/NC gates
- Read: `src-tauri/src/geospatial/hydro.rs` GPL refuse gates

---

## Phase 5: Testing Sequence & Certification

**PREREQUISITE SKILLS:** Load `test-driven-development`, `verification-before-completion`

### Task 5.1: Define rigorous testing sequence

Before certifying as satisfactory, execute this sequence in order:

1. **Static checks:**
   ```
   npx tsc --noEmit          # TypeScript: 0 errors
   cargo clippy -- -D warnings  # Rust: 0 warnings
   cargo fmt --check         # Rust: formatted
   npx prettier --check .    # Frontend: formatted
   npx eslint .              # Frontend: 0 errors
   ```

2. **Build checks:**
   ```
   npm run build             # Vite + tsc: produces dist/
   cargo test --locked       # Rust: all tests pass
   cargo build --release     # Rust: binary compiles
   ```

3. **Security checks:**
   ```
   cargo audit               # Rust: 0 vulnerabilities
   npm audit --audit-level=moderate  # NPM: 0 moderate+
   cargo deny check          # License: no GPL/NC in Standard bundle
   ```

4. **Manual end-to-end gates:**
   - Drop a video on Windows → COLMAP → Brush → splat in viewport → export PLY
   - Switch to Geospatial → draw AOI → run flood → export COG
   - Toggle Experimental → confirm license ack → verify banner
   - Test dark/light theme toggle persists across restart
   - Test keyboard navigation (Tab through all controls)
   - Test Ctrl+B, Ctrl+,, Ctrl+L, Esc shortcuts
   - Test project save/resume across app restart
   - Verify no stub/placeholder text visible in Standard mode
   - Test on Windows 11 with RTX GPU
   - Test with `ffmpeg` on PATH and without

5. **CI pipeline gates:**
   - All workflows green (ci.yml + release.yml)
   - Release publishes only as `draft: true`
   - Artifact checksums uploaded alongside installers

### Task 5.2: Build smoke test harness

**Files:**
- Modify: `tools/smoke-local.ps1` (add pre-flight checks for all sidecars, add structured output, add missing sidecar tests)

### Task 5.3: CI-ify the smoke test

**Files:**
- Create: `.github/workflows/smoke.yml`

### Task 5.4: Certification checklist

Final gate before declaring overhaul complete:

- [ ] `cargo test --locked` — 0 failures
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `npm run build` — succeeds, no chunk-size warnings
- [ ] `cargo clippy -- -D warnings` — 0 warnings
- [ ] `cargo fmt --check` — formatted
- [ ] `cargo audit` — 0 vulnerabilities
- [ ] `npm audit --audit-level=moderate` — 0 issues
- [ ] All modal dialogs trap focus
- [ ] All canvas elements have accessible labels
- [ ] No `unwrap()` on mutex or system clock in production paths
- [ ] Sidecar resolution works from packaged install path
- [ ] No `.stub` text visible to users
- [ ] CI workflow triggers on PR and passes
- [ ] Release workflow sets `draft: true`
- [ ] CSP is non-null in tauri.conf.json
- [ ] `devtools: false` in release builds

---
