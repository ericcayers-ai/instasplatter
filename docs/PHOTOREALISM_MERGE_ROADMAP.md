# Photorealism Merge Roadmap: Digital-Earth SOTA rendering → instasplatter

Handoff spec for an autonomous coding agent. Ground-truthed against both
repos' actual source (file:line refs below) — not from memory, not from
READMEs. Follow this exactly. Do not invent scope beyond it. Do not leave
anything half-done: every item below has an explicit acceptance test; an
item is not done until its test passes for real.

## Non-negotiable rules

1. **No stubs.** Every function you write either works and is verified, or
   does not exist. Never commit a function that returns a placeholder value,
   silently no-ops, or is called nowhere. If something can't be finished in
   scope, say so in the PR description and leave it out — do not half-wire it.
2. **Reuse before you build.** This codebase already has: WGS84/ECEF/ENU
   geodesy (`src-tauri/src/geospatial/transforms.rs`), a DEM pipeline
   (`dem.rs`), a splat↔DEM fusion pair (`sat_splat.rs`, `splat_dem.rs`), a
   Tauri command-registration pattern (`src-tauri/src/lib.rs`, see the
   `#[tauri::command]` wrappers around L459, L479, L650), and a WebGL2 splat
   renderer (`src/splat/renderer.ts`). Extend these. Do not create a parallel
   coordinate system, a parallel command layer, or a second renderer.
3. **Don't overcomplicate.** Ship the minimum correct implementation of each
   item below. No new dependencies unless explicitly listed. No config
   options nobody asked for. No abstraction for a second implementation that
   doesn't exist. Keep token spend down: read only the files you need for
   the item you're on, don't re-derive context that's already given below.
4. **If a formula or algorithm detail below is underspecified, look it up —
   don't approximate from memory and don't guess.** Named papers/specs to
   pull if needed: Preetham et al. 1999 ("A Practical Analytic Model for
   Daylight"), Bruneton & Neyret 2008 ("Precomputed Atmospheric Scattering"),
   Schied et al. 2017 ("Spatiotemporal Variance-Guided Filtering" / SVGF),
   Karis 2014 ("High Quality Temporal Supersampling"), Heitz & Belcour 2019
   ("Screen-Space Blue-Noise Diffusion"), Jed Smith's OpenDRT spec
   (github.com/jedypod/open-display-transform), Troy Sobotka's AgX. A wrong
   formula that compiles is worse than an honest gap — verify against the
   source before shipping.
5. **Check before you add.** `src/geospatial/globe/floodOverlay.ts` exports
   `applyDemReliefOverlay`, imported into `CesiumGlobe.tsx` but not yet
   audited by anyone on this task. Read it first — it may already implement
   part of Item 5 (hillshade). Don't duplicate it.
6. **Test where the repo already tests.** `src-tauri` has `cargo test`
   (`npm run test:rust`) — all Rust math in this roadmap must ship with real
   `#[test]` cases (property/round-trip tests, not just "compiles"). The
   frontend (`src/`) has **no JS/TS test runner configured** — do not invent
   one for this task. Frontend/shader items are verified visually via
   `npm run dev` + the browser (screenshot before/after, or a documented
   manual check) instead of a fabricated automated test.
7. If this repo's tooling exposes a code-graph/search tool (check for
   `graphify-out/graph.json` or `.graphify/graph.json` and this repo's own
   `CLAUDE.md`), query it before grepping raw files for context. Fall back to
   plain search only if it doesn't have the answer.

## Repo map (what already exists — read, don't re-derive)

**instasplatter** (this repo), branch `feat-sat-splat`:
- `src/splat/renderer.ts` (~1052 lines) — WebGL2 (GLSL ES 300), NOT WebGPU
  despite naming elsewhere in the tree. Inline shaders: splat VS/FS (EWA
  covariance projection + Mip-Splatting low-pass), line VS/FS, point VS/FS.
  CPU depth sort in a Web Worker. Simple premultiplied-alpha blend. **No
  lighting, no BRDF, no tonemap, no post-processing exists here at all.**
  `src/geospatial/preview/webgpuSolver.ts` is an unused WebGPU adapter probe
  only — no WGSL anywhere in the repo. Do not target WebGPU; this repo has
  no compute-shader pipeline, and standing one up is out of scope here.
- `src-tauri/src/geospatial/transforms.rs` (255 lines) — `Geodetic`, `Ecef`,
  `Enu`, `EnuFrame::from_geodetic`, `ecef_to_enu`/`enu_to_ecef`, row-major 4×4
  matrix variants. Round-trip tested. This is the one coordinate contract —
  use it for any new geodesy, never reimplement.
- `src-tauri/src/geospatial/sat_splat.rs` (411 lines) and `splat_dem.rs` (583
  lines) — real, tested, DEM+imagery↔splat fusion. Purely geometric/
  statistical (2.5D), no lighting.
- `src-tauri/src/geospatial/dem.rs` (1213 lines) — DEM pipeline. No shading
  code was found in the prior review; confirm before assuming absence, then
  add hillshade here if genuinely missing (Item 5).
- `src-tauri/src/geospatial/digital_earth_export.rs` — writes the
  `digital-earth-splat-v1` manifest (PLY path, `Geodetic` origin,
  `origin_ecef`, row-major `enu_to_ecef_matrix`), consumed by Digital-Earth's
  `lib/splat_import.py`. This is the existing cross-repo bridge; reuse its
  conventions for any new shared data, don't invent a second format.
- `src/geospatial/globe/CesiumGlobe.tsx` (553 lines) — a real Cesium 3D
  globe with real terrain (local DEM heightmap / CTB / ion) and imagery
  draping already working. Atmosphere is **deliberately suppressed**:
  `fog.enabled = false`, forced flat background/globe colors,
  `enableLighting` never set — a stylized flat-dark look, not a missing
  feature. Item 3 re-enables it.
- Tauri command pattern: `src-tauri/src/lib.rs` wraps each geospatial module
  function in a `#[tauri::command]` (see the wrappers around L459 for
  `splat_dem::generate_splat_dem`, L479 for `sat_splat::generate_sat_splat`,
  L650 for `digital_earth_export::export_splat_for_digital_earth`). Any new
  Rust function the frontend needs must be wired the same way.
- `docs/` already holds `E2E-GEO-V010.md`, `E2E-VERIFICATION.md`,
  `RESEARCH-STACK.md`, `SMOKE-TEST.md` — match their format if you add a new
  doc; don't restructure existing ones.
- `npm run dev` starts Vite (frontend only, no Tauri build needed to preview
  UI/shader work in a browser). `npm run test:rust` runs `cargo test`.
  `npm run typecheck` / `npm run lint` exist — run both before calling any
  frontend item done.

**Digital-Earth** (separate repo, sibling directory), branch `main` — source
of the techniques being ported. Read only the files named per item below.

**sunset-gs** (separate repo, now archived/read-only on GitHub — reference
only, not a target of any commit): `src/sun_engine.py`, `src/dem.py`,
`src/splat_alignment.py`, `src/renderer.py`, `src/scene_graph.py`. Contains
real, tested sun-position / DEM-shadow-march / atmospheric-tint Python.
Read for algorithmic reference when porting Item 1 and Item 5 — do not
copy Python 1:1, re-derive correctly in Rust, but do reuse its verified
approach so you aren't deriving the sun-position algorithm from scratch.

## Phase 0 — finish Digital-Earth's own incomplete features first (prerequisite, done in the Digital-Earth repo)

No exclusions: everything Digital-Earth has started gets finished for real
in that repo before anything is imported into instasplatter. Do not port
a feature that's still a stub at the source — fix the source first.

### Item 0a — Make ReSTIR real

`lib/restir.py:40-103` (CPU reservoir dataclass, has self-tests) and
`L201-258` (GPU `ReSTIRField`, spatial/temporal reuse kernels) are correct
weighted-reservoir-sampling implementations. But `lib/pipeline.py:187-190`
only populates and spatially/temporally reuses the reservoirs — nothing
ever reads them back into the path tracer's sun/light sampling or final
color. The module's own docstring (L18-21) says this integration was
"left to a follow-up commit." Do that commit:
- Wire the reservoir's selected sample (direction + target-pdf weight) into
  the next-event-estimation step in `pathtracer.py` (the sun-sampling term
  around L408, `in_scattering += throughput * direct_transmittance *
  sun_irradiance * direct_phase`) so the reservoir's reused sample actually
  replaces or augments the current per-pixel sun sample.
- Confirm spatial and temporal reuse both measurably reduce noise at equal
  sample count (render the same scene with ReSTIR on/off, compare variance
  — Digital-Earth's own test/benchmark conventions if any exist, otherwise
  a documented before/after image comparison).
- Remove or update the stale "left to a follow-up commit" docstring once
  it's true.

**Acceptance (in Digital-Earth):** ReSTIR toggled on visibly reduces noise
vs. toggled off at the same sample count, in a real rendered frame — not
just "reservoirs are populated."

### Item 0b — Make the FFT ocean real

`lib/ocean.py` declares `OceanFFT`, `BubbleSystem`, `CausticPhotonMap` —
none are instantiated anywhere in the codebase (grep-confirmed dead code).
The wired path is a 4-term Gerstner sum-of-sines
(`_GERSTNER_WAVES`, L37-42) with finite-difference normals and
steepness-based foam (`gerstner_wave_normal_and_foam`, L64-91).
- Implement `OceanFFT` for real: a Tessendorf-style (Tessendorf 2001,
  "Simulating Ocean Water") statistical spectrum (Phillips or JONSWAP)
  synthesized via FFT into a height/displacement/normal field per frame,
  replacing or selectable alongside the Gerstner path.
- Instantiate and wire `BubbleSystem` and `CausticPhotonMap` into the
  render pipeline so they actually affect the composited frame (foam/
  bubble scattering, caustic light patterns on the seafloor/surface) —
  not just constructed and unused.
- Integrate with the existing BRDF in `lib/surface_rendering_models.py`
  (Schlick Fresnel, bathymetry roughness lerp, GGX widened by sun angular
  radius) rather than building a second shading path for ocean surfaces.

**Acceptance (in Digital-Earth):** A real rendered frame shows FFT-based
wave detail (not just the 4-term Gerstner sum), visible caustic patterns,
and visible foam/bubble scattering — each individually toggleable and
verified in an actual render, not just instantiated.

### Note — the Monte-Carlo path tracer itself is not ported, and that's architecture, not a stub

`pathtracer.py`'s delta-tracking, per-wavelength spectral sampling,
multi-bounce (up to 25, Russian roulette after bounce 3) path tracer is
real, finished, and not a stub — it's just fundamentally an offline/
progressive-accumulation technique requiring a compute pipeline
instasplatter's WebGL2 renderer doesn't have. This is a feasibility
boundary, not an unfinished feature, so it is not in scope for Phase 0.
Item 2 below ports the underlying *physical model* (phase functions,
density profiles, extinction spectra) into a real-time analytic
approximation instead (the same tradeoff Preetham/Bruneton-style real-time
atmospheres and Cesium's own atmosphere already make) — a different valid
technique, not a compromise.

## Phase 1 — ship this. Everything below is required, none of it is optional.

### Item 1 — Real sun-position astronomy (Rust)

Digital-Earth has **no astronomical sun position** — `renderer.py:87-88,
446-449` is two manually-set scalar angles (`sun_angle`, `sun_path_rot`), no
date/time input at all. Do not port that. Instead implement a real
geodetic sun-position function, referencing sunset-gs's `src/sun_engine.py`
(archived repo, reference only) for the verified approach, re-derived
correctly:

- New file `src-tauri/src/geospatial/sun.rs`. Function signature roughly:
  `fn sun_position(lat_deg: f64, lon_deg: f64, utc: DateTime<Utc>) -> SunPosition`
  returning at minimum azimuth, elevation/altitude, and a unit direction
  vector in ENU (reuse `transforms.rs`'s `Enu` type — do not add a second
  vector type).
  - If a well-tested solar-position crate is already an acceptable
    dependency for this repo (check `Cargo.toml` conventions — this repo
    prefers implementing geodesy itself, e.g. `transforms.rs`), implement
    the algorithm directly rather than pulling a new crate; if truly
    uncertain which is preferable, default to implementing it directly to
    match the existing repo convention (Rule 2 says: match existing
    patterns).
  - Real algorithm: NOAA Solar Position Algorithm or Meeus's low-precision
    solar position (accurate to ~0.01° over years ±2000, which is more than
    sufficient here). Look up the exact formula (Rule 4) rather than
    guessing — do not approximate with a linear day-angle hack.
- `#[test]` cases: known reference values (e.g. solar noon at the equator on
  an equinox should give elevation ≈ 90° minus a known offset; sunrise/
  sunset elevation ≈ 0° at a known lat/lon/date, cross-checked against a
  public reference calculator's output, not invented). Round-trip is not
  applicable here — use fixed known-good reference points instead.
- Wire a `#[tauri::command]` in `lib.rs` following the existing pattern
  (Rule 2) so the frontend can request sun position for a given
  lat/lon/time.

**Acceptance:** `cargo test` passes with real reference-value assertions
(not just "doesn't panic"). The Tauri command is callable from the frontend
console and returns a sane azimuth/elevation for a spot-checked
lat/lon/time (e.g. compare manually against a known almanac value once).

### Item 2 — Analytic real-time sky color (Rust, exposed to both consumers)

Port the *physical model*, not the path tracer, from Digital-Earth:
- Rayleigh phase function `3/(16π)(1+cos²θ)` —
  `Digital-Earth/lib/volume_rendering_models.py:61-63`.
- Mie phase (Klein-Nishina approximation, `mie_asymmetry=3000`) —
  same file, L66-97. (Note: real-time atmospheres more commonly use
  Henyey-Greenstein for Mie; if you swap to HG for real-time stability,
  document that deviation explicitly — don't silently change the model
  without saying so.)
- Density profiles: Rayleigh Gaussian-fit (L249-253), Mie/aerosol
  piecewise-exponential (L255-268), ozone two-Gaussian+cubic (L230-247).
- Extinction spectra: Rayleigh via refractive-index-of-air + King
  correction (L204-218), Mie via Junge-distribution turbidity (L195-201).

Implement as a real-time analytic single-scattering approximation (Preetham
1999 style is the simplest correct real-time option; Bruneton 2008 is more
accurate but requires precomputed LUTs — pick Preetham-style unless you have
clear headroom, per Rule 3's "don't overcomplicate"). New file
`src-tauri/src/geospatial/sky.rs`:
- `fn sky_color(sun_dir: Enu, view_dir: Enu, turbidity: f32) -> Rgb` (or
  equivalent) — a closed-form function, no iteration, cheap enough to call
  per-pixel or per-vertex.
- `#[test]`: sanity-check known cases — sun near zenith gives a blue-ish
  zenith color and warm horizon; sun near/below horizon gives orange/red
  dominant — assert on actual RGB relationships (e.g. "R > B near horizon at
  sunset"), not exact pixel values (no ground-truth image to compare against
  here, so test physically-reasonable relationships, not magic numbers).
- Expose via a `#[tauri::command]` for the Cesium globe to use (Item 3), and
  export the same values as a uniform (via IPC once per frame or on
  sun-time change, not per-vertex-recomputed-in-JS) for the WebGL2 splat
  renderer to use for outdoor-scene ambient tint (Item 4).

**Acceptance:** `cargo test` passes with the relationship assertions above.
Visually sane sunrise/noon/sunset gradient when driven through Item 3.

### Item 3 — Re-enable Cesium's native atmosphere, sun-driven

`src/geospatial/globe/CesiumGlobe.tsx` currently forces `fog.enabled =
false`, flat background/globe colors, and never sets `enableLighting`. This
is the single cheapest, highest-value item — Cesium ships real atmosphere
rendering, it's just switched off.
- Set `viewer.scene.globe.enableLighting = true`.
- Set `viewer.scene.light` to a Cesium directional light driven by Item 1's
  sun position (via `Cesium.SunLight`, or a `Cesium.DirectionalLight`
  constructed from the sun direction — check which is available in the
  installed Cesium version, `^1.143.0` per `package.json`).
- Re-enable `fog.enabled` and atmosphere rendering; remove/relax the forced
  flat colors so the sky/horizon reflect Item 2's sky color instead of a
  hardcoded flat value. If the existing stylized dark aesthetic is meant to
  be preserved as an option, gate it behind a toggle rather than deleting
  it outright — check with whoever reviews this before silently changing the
  default look if that seems risky; otherwise just make the atmosphere the
  new default, since that's the explicit ask.
- Read `applyDemReliefOverlay` in `floodOverlay.ts` first (Rule 5) — don't
  fight it or duplicate its lighting math.

**Acceptance:** Run `npm run dev`, open the globe view, confirm visually
(screenshot) that the sky/horizon changes color with a manually-adjusted
sun time (morning vs. noon vs. sunset), and that terrain shows real
directional shading instead of flat color. `npm run typecheck` and `npm run
lint` clean.

### Item 4 — Tonemap + FXAA + glare in the WebGL2 splat renderer

All three are per-pixel/post-process techniques — genuinely portable to
GLSL ES 300 without any compute pipeline. Add as a post-process pass in
`src/splat/renderer.ts` (render splats to an offscreen framebuffer texture
first if not already doing so, then run these as a final full-screen-quad
pass — check current structure before assuming a render target already
exists).

- **Tonemap**: port OpenDRT, the *live default* in Digital-Earth
  (`renderer.py:637`; `lib/OpenDRT.py` — GPL v3 port of Jed Smith's
  open-display-transform). Chromaticity-preserving: gamut matrix to XYZ, a
  Daniele-Siragusano-style exponential/power tonescale on the achromatic
  norm, then per-channel hue-shift/dechroma recompress (`hs_r/g/b`, `dch`).
  **License note**: `lib/OpenDRT.py` is GPL v3. Confirm instasplatter's own
  license before including GPL-derived code verbatim — if incompatible,
  re-derive from the public OpenDRT spec (github.com/jedypod/
  open-display-transform) rather than copying Digital-Earth's GPL file, and
  say explicitly in the PR which route you took and why.
  AgX (`lib/AgX.py`, 159 lines, simpler, non-chromaticity-preserving, MIT/
  BSD-style license — check) is the fallback if OpenDRT's license is a
  blocker: log2-encode relative to middle grey (0.18), sigmoid toe/shoulder
  per-channel, then a 1.4× saturation multiply.
- **FXAA**: Digital-Earth's own (`lib/fxaa.py`, 56 lines) is a simplified
  4-neighbor luminance-gradient edge detect + directional 3-tap blend, not
  the real NVIDIA FXAA 3.11 spec. Port this simplified version — it's real,
  self-contained, and matches what's actually shipping in Digital-Earth
  (Rule 4 warns against inventing detail; this is the one item where less
  detail than the "real" algorithm is the honest, correct choice, since
  that's what the source repo actually runs).
- **Glare**: `lib/glare.py` (96 lines) — radial exponential-falloff glow, 5
  ghost images along the sun→screen-center axis, 6-fold `|cos(6θ)|`
  diffraction streaks, purely procedural. Port directly as a GLSL
  post-process pass, driven by the sun's screen-space position (project
  Item 1's sun direction through the current view/projection matrix).

**Acceptance:** `npm run dev`, load a splat scene, visually confirm (before/
after screenshots): tonemapped output looks different from raw (less
clipped highlights, filmic roll-off), FXAA visibly softens jagged splat/line
edges when toggled, glare appears near the sun's screen position in an
outdoor/geospatial scene and disappears when the sun is off-screen. No
regression in existing splat rendering when all three are toggled off (add
a simple on/off flag for each, not a full settings UI unless one already
exists to extend).

### Item 5 — Sun-driven hillshade in the DEM pipeline (Rust)

Check `dem.rs` first (Rule 5 applies here too) — the prior review did not
find shading code but did not exhaustively rule it out. If genuinely
absent, add a hillshade function using Item 1's real sun position (not a
fixed default angle):
- Standard hillshade: `shade = max(0, cos(zenith)*cos(slope) +
  sin(zenith)*sin(slope)*cos(azimuth - aspect))`, where slope/aspect come
  from the DEM's existing height-gradient computation (reuse whatever
  gradient/normal computation `dem.rs` or `splat_dem.rs` already has —
  don't write a second one).
- `#[test]`: flat terrain facing the sun directly should shade near 1.0;
  terrain in self-shadow (facing away) should shade near 0.0 — assert the
  relationship on synthetic small DEM grids, not on real data files.

**Acceptance:** `cargo test` passes. Exported DEM/relief preview visibly
reflects the sun angle from Item 1 instead of a flat/fixed shade when
re-run with two different times of day.

## Phase 2 — stochastic light reuse, ocean, and the denoising they need (do only after Phase 0 and Phase 1 are both fully verified)

No exclusions means these get ported too, once Phase 0 has made them real
at the source. They share one piece of new infrastructure — WebGL2
render-to-texture ping-pong passes (motion vectors, history buffer) —
build it once, reuse it for all three:

- **Item 6 — Screen-space light reuse (ported from Item 0a's now-real
  ReSTIR).** Port the reservoir reuse pattern (spatial + temporal, not the
  Taichi kernel verbatim) as a WebGL2 fragment-shader pass over the splat
  renderer's direct-lighting term (the sun contribution added by Item 4).
  This is real, established practice — screen-space ReSTIR has shipped in
  fragment-shader-only web renderers before — but it introduces real
  per-frame sampling noise, which is exactly what Item 8 exists to clean up.
  Skip this item if the splat renderer only ever has a single directional
  light (the sun) at this point — ReSTIR's benefit is reusing samples
  across *many* candidate lights, and reservoir reuse over one light is a
  no-op with real code around it, which is its own kind of stub. Only build
  this once there's more than one light candidate (e.g. sky + sun + any
  area-light additions) to actually reuse across.
- **Item 7 — FFT ocean (ported from Item 0b's now-real ocean).** Port the
  Tessendorf FFT wave synthesis, caustics, and foam/bubble scattering into
  the WebGL2 splat/terrain rendering for any DEM cells classified as water
  (reuse whatever water/land classification `sat_splat.rs` or `dem.rs`
  already has — don't invent a second one). Skip entirely if no such
  classification exists yet and adding one is out of scope — say so rather
  than building ocean shading with no water mask to drive it.
- **Item 8 — SVGF + TAA denoising.** `Digital-Earth/lib/svgf.py:107-142`
  (à-trous wavelet, 4 levels, edge-stopping on luminance/depth/normal) and
  `lib/temporal.py:98-226` (reprojection, disocclusion test, min/max
  neighborhood clamp). Build this once Item 6 exists and actually produces
  noise to clean up — not before, and not against a synthetic/fake noisy
  signal invented just to justify building it.

**Acceptance:** each item independently toggleable; Item 6 visibly reduces
noise vs. off at equal sample budget; Item 7 shows real FFT wave detail,
caustics, and foam on water DEM cells; Item 8 visibly stabilizes Item 6's
output frame-to-frame without smearing moving splats.

## Order of work

Phase 0 (Items 0a, 0b — in Digital-Earth) has no dependency on
instasplatter and can run any time before Phase 2 starts, in parallel with
Phase 1. Within Phase 1: Item 1 → Item 2 → Item 3 (needs 1+2) → Item 5
(needs 1) can run in parallel with Item 4 (needs 1, not 2/3/5) once Item 1
lands. Don't start Item 3 or Item 4's glare sub-part before Item 1 exists —
both need a real sun direction, and a hardcoded placeholder direction is
exactly the kind of stub Rule 1 forbids. Phase 2 (Items 6-8) needs Phase 0
finished (can't port a stub) and Phase 1 Item 4 finished (Item 6 extends
Item 4's direct-lighting term); Item 8 needs Item 6 (needs real noise to
denoise).

## On using available tooling

If the execution environment has a code-graph/search skill, a
framework-specific pattern skill (Rust/Tauri, React, Vite, TypeScript,
shader/WebGL), or web search available, use them rather than guessing —
this repo ships its own skills for exactly this (`tauri-v2` for any
`#[tauri::command]`/IPC work, `typescript-advanced-types` and
`vite` for the frontend build, `react-best-practices` for `CesiumGlobe.tsx`
changes). If the target environment doesn't have equivalent tooling, fall
back to reading the source files named in this doc directly — don't skip
verification because a convenience tool isn't available.
