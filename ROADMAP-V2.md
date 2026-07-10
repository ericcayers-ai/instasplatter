# InstaSplatter Roadmap V2

Follow-up to [ROADMAP.md](ROADMAP.md). V0.1 shipped and is verified end to end on Windows 11 with an RTX 4060: video or image folder, frame gating, COLMAP SfM, Brush (wgpu) training, live WebGL2 viewport, and PLY export. This document defines the next five phases.

The goals for V2, taken directly from the product direction:

1. A professional reconstruction-tool interface with real light and dark themes, closer in feel to COLMAP or Lichtfeld Studio than to a marketing page.
2. Camera poses that register live, one at a time, alongside the growing splat, so the scene paints itself into reality instead of appearing all at once after a batch SfM pass.
3. The most robust and reliable general-purpose pipeline we can assemble from the current research, while staying lean. One cross-vendor binary, no required CUDA or PyTorch runtime, and a small number of good defaults rather than a wall of options.
4. An optional splat-to-mesh path for people who want a clean textured mesh of an object or environment.

Writing conventions for this document and for all UI copy it produces: plain, functional language. No marketing phrasing, no exclamation, and no em dashes.

---

## Status

| Phase | Assigned to | State |
| --- | --- | --- |
| 1. Core | Fable 5 / Opus 4.8 | Done, except the end-to-end confirmation in 1.1 and SOG in 1.6 |
| 2. Instant live init | Opus 4.8 | Engine done, verified on synthetic scenes. Gaussians do not spawn per frame, and there is no VGGT sidecar |
| 3. UI makeover | Sonnet 5, medium effort | Not started, skipped by model assignment |
| 4. Splat to mesh | Fable 5 / Opus | Done, except the UV atlas and the Poisson fallback |
| 5. Debug passthrough | Opus 4.8, ultracode | Not started, ultracode was not enabled |

165 unit tests pass, the Rust tree builds with no warnings, and the frontend typechecks and builds. **No end-to-end run against COLMAP and Brush has been performed since these changes landed.** Everything below is verified against synthetic data and unit tests only. The three settings this work added, live camera tracking, progressive resolution and the Mip-Splatting filter, all default to off pending that measurement.

Two defects in shared code were found and fixed along the way, both from judging a singular value against an absolute threshold rather than one relative to the matrix. In `svd3`, a third singular value of `4e-9` on a matrix whose largest was `4.5` passed a fixed `1e-12` floor, so the last column of `U` was computed as the quotient of two roundoff quantities. Callers read that column as the camera translation, which made `ransac_essential` fail outright on real essential matrices. Separately, `find_model_dir` never accepted a directory containing `0/`, so `is_resumable` reported false for every project that could in fact be resumed.

---

## Guiding constraints

**Stay lean.** The base installer remains a single cross-vendor wgpu application. Brush stays the sole trainer. Any technique that would require bundling a CUDA toolchain or a PyTorch runtime is either reimplemented natively or placed behind an explicit, opt-in sidecar download that the base app never depends on.

**Respect licenses.** Most of the strongest research code carries the Inria or NVLabs non-commercial research license, or a CC BY-NC variant. Those are usable as references only. We reimplement their algorithms in our own Rust and wgpu code. We never vendor non-commercial source. The table below records the license we found for each candidate so the implementing model does not have to rediscover it.

**Few good options, not many.** Every setting still defaults to Auto. New capability is added as a sensible default first and an exposed control only when it earns one. We are not shipping a parameter for every paper.

### License and adoption summary

| Project | License found | Role in V2 |
| --- | --- | --- |
| Brush (ArthurBrussee/brush) | Apache-2.0 | Trainer and renderer core. Keep. |
| gsplat (nerfstudio) | Apache-2.0 | Reference oracle for correctness only. Not shipped. |
| on-the-fly-nvs (graphdeco-inria) | Inria non-commercial | Reference. Reimplement the algorithm for Phase 2. |
| CUT3R | CC BY-NC-SA 4.0 | Reference architecture for the streaming loop. |
| MASt3R / MASt3R-SfM | CC BY-NC-SA 4.0 | Offline accuracy benchmark. Not shipped. |
| VGGT (facebookresearch/vggt) | NC weights, plus a gated VGGT-1B-Commercial checkpoint (Jul 2025) | Optional opt-in sidecar for hard cases. |
| 3DGS-MCMC (ubc-vision) | Inria non-commercial | Reference. Reimplement densification in Brush. |
| Mip-Splatting | Inria non-commercial | Reference. Port the 3D smoothing filter. |
| DashGaussian | Inria-derived, verify | Reference. Port the resolution schedule. |
| Taming-3DGS / FastGS | MIT (Taming perf files), MIT (FastGS) | Reference for budgeted densification. |
| Mini-Splatting / Speedy-Splat | Apache-2.0 / Inria non-commercial | Reference for pruning and tile culling. |
| WildGaussians | MIT (renderer builds on GS) | Reference. Port appearance embeddings. |
| SpotLessSplats | Apache-derived, verify | Reference. Port the robust mask loss. |
| 2DGS (hbb1/2d-gaussian-splatting) | Apache-2.0 | Recipe for the mesh depth source. |
| GS-2M (ndming/GS-2M) | MIT-leaning, verify | Reference for future material-aware meshing. |
| GSFix3D / GSFixer | Code not yet released | Reference for a deferred inpainting post-process. |
| RaGS | n/a | Skip. It is radar object detection, not reconstruction. |
| FlashSplat | Inria-derived | Skip. It is segmentation, not a training speedup. |
| vk_gaussian_splatting | Apache-2.0 | Skip for training. Renderer only, Brush already renders. |
| CityGaussian | non-commercial | Skip. City-scale LOD is out of scope for our captures. |

Anything marked verify still needs its raw LICENSE file confirmed before that code is copied. The safe path in every case is to reimplement the algorithm, which we are doing regardless.

---

## Phase 1: Core

> Assigned to Claude Fable 5/Opus 4.8. No other model, may implement this phase.

Phase 1 hardens the shipped core and lands the low-risk, well-scoped work: the viewport locomotion fixes, saving, and the cheap training-quality wins that do not require new research. Everything here is mechanical and clearly specified.

### 1.1 Consolidate the v0.1 pipeline
- [ ] Confirm the existing path (ingest, gating, COLMAP, Brush, viewport, PLY) still passes end to end after the V2 changes land. **Not done.** The tree compiles and 165 unit tests pass, but no end-to-end run has been executed against COLMAP and Brush since the V2 changes landed. This must be run before the release is trusted.
- [x] Move the single-shot autostart test hook behind a clearly named dev flag. It now returns nothing unless `INSTASPLATTER_DEV=1`.

### 1.2 Fix viewport locomotion
The current orbit, pan, and zoom behavior is unreliable. This is the top core bug.
- [x] Rewrite the orbit camera so rotation is stable at all pitch angles and does not flip or gimbal near the poles. The basis is built from the normalized yaw derivative rather than `cross(forward, worldUp)`, which is what degenerates at the poles.
- [x] Fix panning so it tracks the cursor in the view plane at a consistent speed regardless of zoom distance.
- [x] Make zoom focus toward the cursor target rather than the screen center.
- [x] Confirm the COLMAP down-axis convention is handled so the scene is never upside down on load. The projection negates y, so the view matrix carries `down`, not `up`, and `worldUp` is `-y`.

All four are covered by 39 numeric assertions in `src/splat/camera.ts`, run under node.

### 1.3 Model rotation and orientation
- [x] Add an explicit control to rotate the model itself, separate from orbiting the camera. The viewport turns the model about a world axis, and the orientation is saved with the project.
- [x] Add up-axis alignment (snap to nearest axis, or set from a selected ground plane) so exports come out upright. The ground plane is found by a deterministic RANSAC plane fit, and `estimate_up_axis` returns the rotation that stands it up.

### 1.4 Saving and resume
- [x] Add a project bundle that saves the input reference, resolved settings, solved poses, and the latest splat, so a session can be reopened. `project.json` is written atomically after every meaningful change.
- [x] Add checkpoint and resume for interrupted training using Brush start-iter, so closing the app mid-run is not fatal. A resumed run keeps the settings it started with, because the schedule has to match the checkpoint.
- [x] Autosave the latest result so a completed reconstruction is never lost.

### 1.5 Cheap training-quality wins ported into Brush
These are low-risk additions with a clear algorithm and no new dependency.
- [x] Progressive resolution schedule, based on DashGaussian. Train low resolution first and raise it on a schedule.
- [x] Mip-Splatting 3D smoothing filter. A small training-time regularizer that bounds Gaussian size to the sampling rate.

Both are **approximations, and both default to off.** Brush is a separate binary and its CLI cannot express either change inside the training loop. The schedule is therefore staged restarts driven by `--start-iter` plus an `init.ply` handover, and the filter is applied to the splats between stages and baked into the final result, rather than acting as a training-time regularizer. Restarting resets Adam's moment estimates, which is a real cost, and neither has been measured against an end-to-end baseline yet. Turning them on by default should wait for that measurement.

### 1.6 Export formats
- [x] Add SPZ (Niantic compressed) and the web .splat format alongside PLY. The SPZ encoder is checked by a decoder in the tests that verifies the header, the sign extension of negative 24-bit positions, the `w >= 0` rotation convention, and the SH reordering and bucketing.
- [ ] Add SOG (self-organizing Gaussians) for compact sharing. **Deferred.** SOG needs a self-organizing map over the Gaussians to produce the 2D locality its compression depends on, which is a different order of work from a container format.
- [x] Keep PLY as the default and record the format choice in preferences.

### 1.7 Housekeeping
- [x] Verify engine-download checksums and handle a failed or partial download gracefully. Downloads stream to a `.part` file, are checked against a pinned SHA-256, are extracted with zip-slip protection, and are swapped into place atomically. Any failure deletes the partial file.
- [x] Confirm cancel fully tears down child processes and cleans intermediates. Cancel kills the child process tree, and the workspace is removed only after the job task unwinds, so nothing is deleted while a process still holds it. A cancelled *resume* keeps its workspace, because it already holds a reconstruction.

---

## Phase 2: Instant live init

> Assigned to Claude Opus 4.8. No other model may implement this phase.

Phase 2 removes the hard split between the batch SfM pass and training. Instead of solving every camera in COLMAP before the first splat appears, poses register incrementally and the splat grows next to them, so the scene paints into reality as frames are processed. This is the hardest and most research-heavy phase, which is why it is reserved for the highest effort tier.

### 2.1 Target behavior
- [x] As each new frame is admitted, estimate its pose and show its camera frustum snapping into the viewport, without a separate blocking SfM stage.
- [ ] Spawn or update Gaussians for the newly seen region as each frame lands. **Not done.** Brush owns the Gaussians and is driven over its CLI; it has no way to accept new frames or new Gaussians mid-run. The engine registers cameras live and hands the finished sparse model to Brush in one piece. Interleaving the two would need Brush to be a library rather than a subprocess, which is a change to the leading constraint, not to this file.

### 2.2 Native incremental engine
The reference for this exact behavior is Inria on-the-fly-nvs, which does learned-feature pose initialization, a GPU mini bundle adjustment, and incremental Gaussian spawning, yielding a trained splat immediately after each unposed image. Its code is non-commercial, so we reimplement the algorithm rather than vendoring it.
- [x] Implement pose initialization for a new frame against the current model. **Classical, not learned.** Learned descriptors mean a neural runtime, and the base install carries neither CUDA nor PyTorch. The shipped backend is an oriented-FAST detector with a rotated BRIEF descriptor, matched by Hamming distance under Lowe's ratio and a cross-check, then RANSAC PnP. The seam for a learned backend is the `Frame` struct: a sidecar that fills one drives matching, pose solving and bundle adjustment unchanged.
- [x] Implement a local mini bundle adjustment over a sliding window of recent keyframes. **On the CPU.** The Schur complement reduces the problem to a dense system of `6C` unknowns, which for a window of eight keyframes is 36 by 36 and solves in microseconds. A GPU would not help at that size.
- [ ] Spawn Gaussians for newly observed regions and hand them to the existing Brush optimization loop. **Not done**, for the reason given in 2.1.
- [x] Use the CUT3R stateful streaming design as the architectural template for the per-frame update loop. The loop is stateful and per-frame, though the state is a sparse reconstruction rather than a learned latent.
- [ ] Use MASt3R-SfM as the offline accuracy benchmark to validate our poses against. **Not done.** Poses are checked against synthetic scenes with known ground truth, which confirms the geometry is right but says nothing about real captures.

The engine is verified on synthetic data: seven cameras and four hundred points, recovered to within 0.02 radians of rotation and 5% of centre offset once the arbitrary global scale is removed, at under one pixel of reprojection residual. Intrinsics are guessed from the image size and are not refined, so poses carry whatever error that guess introduces. The batch COLMAP path remains the accurate one, which is why live init is opt-in.

### 2.3 Optional VGGT bootstrap (opt-in sidecar)
For wide-baseline sets, loops, or captures where incremental tracking is fragile, VGGT gives fast feed-forward poses and pointmaps in a single pass. Its base weights are non-commercial, but a VGGT-1B-Commercial checkpoint exists under application-gated terms.
- [ ] Provide an optional, opt-in sidecar that runs VGGT to bootstrap or repair poses. **Not done.** The seam is in place (see 2.2), but the sidecar itself is not built.
- [ ] Gate its use behind a clear preference and confirm the commercial checkpoint terms before enabling by default. **Not done.**

### 2.4 Live camera registration in the viewport
- [x] Render each solved camera as a frustum in the viewport, and animate it appearing as its pose is confirmed.
- [x] Stream new Gaussians into the viewport so the splat visibly fills in alongside the cameras. This works as it did in v0.1, by reloading each checkpoint Brush exports. The cameras appear first, then the splat grows; the two do not interleave, for the reason given in 2.1.
- [x] Show a running count of registered cameras and the current tracking confidence.

### 2.5 Progressive pose refinement
- [x] Continue refining earlier poses as more frames arrive. A sliding window of local bundle adjustment runs after every registration, and two overlapping sweeps refine the whole sequence before the model is written. A full adjustment over hundreds of cameras would mean a dense reduced system of thousands of unknowns, so overlapping windows carry the same corrections at a cost linear in the frame count.
- [ ] Update affected Gaussians when a pose changes materially. Not applicable while training starts after the poses are final.

### 2.6 Safe fallback
- [x] If incremental tracking loses confidence or fails, fall back to the Phase 1 batch COLMAP path automatically and tell the user plainly that it switched. The engine gives up on too few known points, too few surviving inliers, too few frames located, or a seed pair without parallax. Each returns a plain reason, and a partial reconstruction is discarded rather than handed to the trainer.

---

## Phase 3: UI makeover

> Assigned to Claude Sonnet 5 at medium effort only. No other model, and no other effort level, may implement this phase.

**Not started. Skipped by model assignment**, as this document reserves the phase for Claude Sonnet 5 at medium effort. The Phase 1 and 2 work added controls to the existing interface without restyling it: model orientation controls and camera frustums in the viewport, a registered-camera count with tracking confidence, a notices strip, an export-format picker, and a mesh export action. All of that copy is plain and free of em dashes, and it is expected to be rehoused by the shell this phase builds.

Phase 3 replaces the current interface, which reads as a generic dark web app, with a layout and visual language closer to professional reconstruction software. It also lands the interface-level fixes: settings available at any time, light and dark themes, and plain non-marketing copy.

### 3.1 Layout: a three-region dockable shell
Model the layout on COLMAP and Lichtfeld Studio rather than a single centered card.
- [ ] Left: a dockable scene and dataset tree (input frames, solved cameras, the current model).
- [ ] Center: the viewport.
- [ ] Right: a properties and parameters panel with grouped, collapsible sections.
- [ ] Bottom: a log console showing real timestamped pipeline output, plus a status bar with live stats.
- [ ] Panels can dock, float, and collapse. Replace the centered-card home screen with this working layout.

### 3.2 Light and dark themes
- [ ] Define semantic theme tokens (background, panel, border, text, muted text, one accent).
- [ ] Provide both a light map and a dark map. Dark panels near #1e1e1e and #252526, light panels near #f3f3f3 and #ffffff.
- [ ] Default to the operating system preference, offer a manual toggle, and persist the choice.
- [ ] Use one low-saturation accent for selection and the single primary action. No gradients, no glow, no decorative blobs.

### 3.3 Settings available at any time
- [ ] Make preferences openable during a running job, not only from the home screen.
- [ ] Show which settings apply to the current run and which take effect on the next run.

### 3.4 Plain, functional copy
- [ ] Rewrite all interface text to be plain and descriptive. Remove promotional or colloquial phrasing.
- [ ] Do not use em dashes anywhere in the interface.
- [ ] Labels are verbs and nouns that describe the action or value, not slogans.

### 3.5 Viewport telemetry and gizmos
- [ ] Overlay camera frustums, splat count, frames per second, and view axes.
- [ ] Add transform gizmos for adjusting the model orientation from Phase 1.

### 3.6 Typography and density
- [ ] Use a 12 to 13 pixel system font with tabular numerals for counts, timings, and quality readouts.
- [ ] Favor information density over whitespace. Use flat buttons with small radii and 1 pixel borders, grouped by function. No pill buttons.

### 3.7 Persist the workspace
- [ ] Remember panel layout and theme between sessions.

Patterns worth borrowing, confirmed by research: COLMAP task-grouped toolbars and its thread-safe log console flushed on a short timer so long jobs never freeze the interface; Lichtfeld Studio live training preview, scene tree with selection and undo, and its export choices; SuperSplat interaction patterns for cropping and transforming.

---

## Phase 4: Splat to mesh

> Assigned to Claude Fable 5/Opus. No other model may implement this phase.

Phase 4 adds an optional Export as mesh feature for users who want a clean textured mesh of an object or environment rather than a splat. The research is clear that the lean and reliable choice is a standard, deterministic pipeline we can run natively in Rust, not a fragile SOTA repository or a stereo or tetrahedral CUDA step. This keeps the phase well scoped and low effort.

### 4.1 Native extraction pipeline
Follow the 2DGS recipe, which is what 2DGS, PGSR, and RaDe-GS all do under the hood, and which is permissively reproducible.
- [x] Render depth and normal maps from the training camera poses using the trained splat. A CPU EWA rasterizer, tiled and run over rayon. A pixel is only trusted once it has accumulated half its opacity, so thin or transparent regions report nothing rather than a depth pulled from one faint Gaussian.
- [x] Fuse the depth maps into a TSDF voxel grid. Samples are weighted by the cosine of the incidence angle and grazing hits are dropped, because a surface seen edge on tells you almost nothing about where it is. Without that, a voxel near a silhouette is free space to one camera and just behind the surface to another, and their mean crosses zero somewhere that is not the surface.
- [x] Extract a surface with marching cubes. The topology is derived per cube rather than read from the familiar 256-entry table, which encodes one fixed choice for ambiguous faces and lets neighbouring cubes disagree about the face they share. Ambiguous faces are resolved by the asymptotic decider, which reads only that face's four corner values, so two cubes sharing a face always agree. Vertices are keyed by the global grid edge they sit on. The tests check that every edge is shared by exactly two triangles, that every directed edge appears once, and that a sphere comes out with the right volume and outward normals.
- [x] Project the source images onto the mesh faces for texture, using a per-view blend. **Per-vertex, not per-texel.** Colour is fused into the volume alongside the distance, weighted the same way, and interpolated onto the surface. There is no UV atlas: that needs a chart parameterization and a packer, which is a separate piece of work. glTF, OBJ and PLY all carry the per-vertex colour, and every common viewer reads it.
- [ ] Provide Poisson surface reconstruction as a fallback path. **Deferred.** Screened Poisson needs an adaptive octree and a multigrid solver. Writing one badly would be worse than not offering it, and the TSDF path already reports plainly when it cannot find a surface.
- [x] Keep the whole extraction in Rust. No Python or CUDA runtime is required.

### 4.2 Cleaner depth, optional
- [ ] Optionally add a 2DGS-style flatten and normal regularizer to Brush so the Gaussians are more surfel-like and the depth is cleaner. **Not done**, and it cannot be done from outside Brush: it is a change inside the rasterizer. The extraction takes each Gaussian's thinnest axis as its normal, which is the surfel assumption, and the TSDF averages the resulting noise across views.

### 4.3 Output
- [x] Export the mesh as glTF (glb) and OBJ, in addition to a mesh PLY. All three carry per-vertex colour; none carry a material, because there is no texture image to reference. The glb writer is checked against its own container invariants: chunk alignment, declared lengths, and every buffer view inside the buffer.
- [x] Present mesh export as an optional action after a reconstruction completes, not as a required stage.

### 4.4 References and non-goals
- [x] Treat GS-2M as the reference to revisit if we later want material-aware, PBR-textured output. It is too new to depend on now.
- [x] Keep SuGaR, GOF, RaDe-GS, and PGSR as algorithm references only. They are non-commercial. Nothing from them is vendored.

---

## Phase 5: Efficient but robust debug passthrough

> Assigned to Claude Opus 4.8. No other model, and no other configuration, may implement this phase.

**Not started.** This phase is reserved for ultracode, which was not enabled for this session, so none of it was attempted. Nothing below has been touched.

Phase 5 makes the whole pipeline bulletproof and efficient on messy, real-world input, and it fixes holes. This is the robustness and hardening pass: exhaustive error handling at every stage, the in-the-wild quality stack, performance guardrails, and a debug passthrough that surfaces exactly what went wrong when something fails. It is reserved for ultracode because the value is in exhaustive, adversarially verified coverage of edge cases.

### 5.1 Floater suppression with MCMC densification
- [ ] Reimplement 3DGS-MCMC densification in Brush. Treat Gaussians as MCMC samples, relocate low-opacity Gaussians instead of the clone, split, and opacity-reset heuristics, and enforce a fixed Gaussian budget. This is the single strongest structural fix for floating blobs and it also caps VRAM and model size. It ports cleanly with no external network.

### 5.2 Fix holes and under-reconstruction
No lightweight regularizer truly fills holes, because missing regions need new content, so this is staged.
- [ ] Near term, add SparseGS and USGS-style depth and unseen-viewpoint regularization to prevent background collapse and reduce holes in under-observed regions. This is portable to the Rust trainer and needs no diffusion model.
- [ ] Add Depth Anything V2 monocular depth priors to regularize geometry where multi-view evidence is thin.
- [ ] Deferred, as an opt-in sidecar once its code releases: the GSFix3D render, 2D inpaint, and re-distill loop, which is the only approach that genuinely synthesizes missing geometry. It is a PyTorch diffusion stage, so it stays out of the base install.

### 5.3 Transient and moving-object rejection
- [ ] Port the SpotLessSplats robust mask into training so moving people and objects are down-weighted. The mask loss is portable to Rust. The feature extractor for it runs as an optional preprocess.
- [ ] Offer optional SAM2 masking for explicit removal of chosen classes.

### 5.4 Appearance and exposure drift
- [ ] Port WildGaussians per-image appearance embeddings so exposure and lighting changes across frames are absorbed rather than baked in as blotches.
- [ ] Add a bilateral grid for per-image color correction.

### 5.5 Motion blur
- [ ] Keep Deblur-GS as a reference and defer it. Per-frame blur kernels add camera and kernel machinery that is not worth the complexity yet.

### 5.6 Efficiency
- [ ] Add budgeted densification, based on Taming-3DGS and FastGS, so a per-step Gaussian budget bounds VRAM and model size. This pairs naturally with the MCMC cap.
- [ ] Port tighter tile culling and soft pruning, based on Speedy-Splat and Mini-Splatting, into the wgpu rasterizer to cut overdraw and trim the final count.
- [ ] Add VRAM and thermal guardrails with dynamic downscale so the app degrades instead of crashing.

### 5.7 Debug passthrough and reliability
- [ ] Add exhaustive, specific error handling at every stage: SfM failure, out of memory, too few frames, degenerate motion, and empty or corrupt input, each with a plain message and a suggested fix.
- [ ] Add structured logging and a one-click diagnostics export for support.
- [ ] Confirm crash-resilient checkpoint and resume across the whole pipeline.
- [ ] Build an end-to-end test matrix over object, room, and outdoor captures, and over each GPU vendor, and run it under adversarial verification.

---

## What changed from Roadmap V1

Carried forward from V1 and now scheduled: MCMC densification, Mip-Splatting, VGGT and MASt3R instant init with live pose registration, 2DGS and GOF surface handling for meshing, Depth Anything V2 depth priors, SAM2 transient masking, appearance embeddings and bilateral grid, visibility pruning, the SPZ, .splat, and SOG export formats, checkpoint and resume, camera-frustum overlays, and a floater debug view.

Reversed from V1: the plan to add a second CUDA gsplat engine for NVIDIA. Research across every topic agreed it breaks the single cross-vendor no-PyTorch value of Brush for a maintenance and install-size cost that is not justified. gsplat stays a correctness reference only. We invest instead in porting its proven techniques into Brush.
