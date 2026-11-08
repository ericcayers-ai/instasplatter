# MASt3R-SfM accuracy benchmark

Scores our camera poses against a reference reconstruction, and answers the one question the
roadmap asks at `ROADMAP-V2.md` §2.2: *are the live-init poses right on a real capture, not just
on synthetic data?*

## What it measures

Two pose sets for the same images — a REFERENCE (MASt3R-SfM, or synthetic ground truth) and an
ESTIMATE (our sparse model) — after removing the similarity gauge that separates any two SfM
reconstructions. An SfM model is defined only up to scale, rotation and translation, so raw
comparison would measure the gauge, not the accuracy. `umeyama` (Umeyama 1991) recovers the
optimal scale + rotation + translation first; what is left is reconstruction error.

| Metric | Meaning | Threshold |
|---|---|---|
| `rotation_deg.median` | geodesic angle between aligned camera rotations | ≤ 2.0° |
| `centre_error.relative_median` | camera-centre error ÷ reference scene diameter | ≤ 2% |

Both thresholds are stated in the module (`ROTATION_MEDIAN_LIMIT_DEG`,
`CENTRE_RELATIVE_MEDIAN_LIMIT`) rather than tuned to whatever a run happens to score, and a run
below both is reported `PASS`, otherwise `FAIL` (exit 1).

## Running it

```powershell
# 1. Prove the metric code itself. No data, no GPU, no weights.
python tools/bench/mast3r_sfm_bench.py --selftest

# 2. Synthetic ground truth (this is the gate a machine without weights can always run).
python tools/bench/mast3r_sfm_bench.py --fixture --receipt docs/evidence/mast3r-bench/fixture.md

# 3. Score two existing sparse models.
python tools/bench/mast3r_sfm_bench.py --score `
    --reference <colmap sparse/0> --estimate <our sparse/0>

# 4. The real benchmark: MASt3R-SfM as the reference.
python tools/bench/mast3r_sfm_bench.py --mast3r --images <capture> `
    --estimate <our sparse/0> --receipt docs/evidence/mast3r-bench/mast3r.md
```

`--selftest` injects a known rotation error into a synthetic rig and requires the harness to
recover it, rejects a model that should fail the threshold, aligns a deliberately re-gauged model
back to zero error, and round-trips a model through the COLMAP `images.txt` format. It exists
because a benchmark that cannot detect a broken metric is worse than no benchmark.

## The weight-free path, and what blocked means

Mode 4 checks three preconditions before it starts anything and reports ALL of them when it
cannot run — a capture folder with at least three views, installed MASt3R weights (no `.stub`
marker in the sidecar folder), and at least `MAST3R_MIN_FREE_VRAM_MIB` (6000 MiB) of measured free
VRAM. Missing any of them writes a `BLOCKED` receipt naming the measured numbers and the exact
command to re-run; it never invents poses and never reports a PASS it did not compute. Free VRAM
is read from `nvidia-smi`, not `Win32_VideoController.AdapterRAM`, which reports 4 GB for this
8 GB card.

Modes 1 and 2 need none of that, so the accuracy claim stays testable while the weights are not
installed.

## Files

- `mast3r_sfm_bench.py` — the harness (single file; numpy only, used by
  `tools/sidecars/gsplat-train` too).
- Receipts land under `docs/evidence/mast3r-bench/`, each with the command, the machine, the
  measured free VRAM and the full JSON.
