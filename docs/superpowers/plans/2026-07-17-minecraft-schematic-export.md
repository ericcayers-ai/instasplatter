# Minecraft Schematic Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Experimental Mode export of a finished Gaussian splat to a Sponge Schematic v2 `.schem` file.

**Architecture:** Native Rust voxelizer + `fastnbt` Gzip writer under `splat/schematic.rs`; Tauri command gated on Experimental Mode; TitleBar action when Experimental is ON.

**Tech Stack:** Rust, fastnbt, flate2 (existing), Tauri 2, React/Zustand frontend.

## Global Constraints

- Experimental Mode only (Standard path unchanged)
- Sponge Schematic **v2** (not v3, not legacy `.schematic`)
- Vanilla concrete palette only (no mod blocks)
- Max schematic axis default 128, clamp 16–256
- Version bump to **0.9.1** with release notes

---

## File map

| File | Role |
|---|---|
| `src-tauri/Cargo.toml` | Add `fastnbt` |
| `src-tauri/src/splat/schematic.rs` | Core voxelize + write |
| `src-tauri/src/splat/mod.rs` | `pub mod schematic` |
| `src-tauri/src/lib.rs` | `export_minecraft_schematic` command |
| `src/lib/ipc.ts` | Typed API |
| `src/state/store.ts` | `exportSchematicAction` |
| `src/components/shell/TitleBar.tsx` | Button when Experimental + result |
| `src/components/shell/AboutPanel.tsx` | Mention export |
| `docs/RESEARCH-STACK.md`, `README.md`, `RELEASE.md` | Docs |
| `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` | Version 0.9.1 |

---

### Task 1: Core schematic module (TDD)

**Files:**
- Create: `src-tauri/src/splat/schematic.rs`
- Modify: `src-tauri/src/splat/mod.rs`, `src-tauri/Cargo.toml`

- [ ] Add `fastnbt` dependency
- [ ] Write failing tests: varint, palette nearest, synthetic cloud → file round-trip structure
- [ ] Implement `SchematicOptions`, `voxelize_cloud`, `write_schem`, concrete palette
- [ ] `cargo test schematic::` green
- [ ] Commit

### Task 2: IPC + Experimental gate

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] Command `export_minecraft_schematic` requiring experimental mode
- [ ] Reuse splat rotation bake from project / argument
- [ ] Register in invoke handler
- [ ] Commit

### Task 3: Frontend Experimental export action

**Files:**
- Modify: `src/lib/ipc.ts`, `src/state/store.ts`, `src/components/shell/TitleBar.tsx`, `AboutPanel.tsx`

- [ ] API + store action with `.schem` save dialog
- [ ] Button only when `resolved.experimentalMode && resultPath`
- [ ] `npx tsc --noEmit` green
- [ ] Commit

### Task 4: Docs + v0.9.1 release

**Files:**
- Modify: README, RESEARCH-STACK, RELEASE.md, version fields

- [ ] Document experimental schematic export
- [ ] Bump to 0.9.1
- [ ] Full `cargo test` + `tsc`
- [ ] Push branch, open PR, tag release after merge path
