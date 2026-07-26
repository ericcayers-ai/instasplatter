# Video Draft E2E — 20250820_212300.mp4

- Input: HEVC, 3840×2160, 81.43 s, 287.8 MB
- Hardware: NVIDIA GeForce RTX 4060 8 GB; CUDA COLMAP 4.1
- Run: Draft, 80 frames, 1024 px, 5,000 steps, live tracking ON, dense init ON
- Result: completed; 200,429 finite Gaussians, 32.9 MB
- Live behavior: incremental tracking emitted a growing sparse preview, then honestly fell back to CUDA COLMAP.
- Dense bootstrap: 5,398 points; no installed neural densifier added material geometry on this run.

## Verdict

**Pipeline PASS; visual quality POOR / not production-ready.**

The capture contains large camera-orientation changes and independently moving subjects (person and dog). The result preserves a recognizable partial room/recliner cluster, but the orthographic diagnostic shows fragmented geometry and broad floaters. A slower, landscape-oriented orbit around a mostly static scene is required for a useful splat.

## Evidence

- `video-draft-complete.png` — completed queue/recent-project UI state
- `video-draft-splat-pca.png` — three orthographic diagnostic views of the final Gaussian positions/colors
