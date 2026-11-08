# MASt3R-SfM benchmark — blocked (preconditions unmet)

- **Status: BLOCKED**
- Command: `python tools/bench/mast3r_sfm_bench.py --mast3r --images C:\Users\<you>\Videos\capture --receipt docs\evidence\mast3r-bench\2026-10-26-mast3r-blocked.md`
- When (UTC): 2026-10-26T01:59:24+00:00
- Machine: Windows-11-10.0.26300-SP0 · GPU NVIDIA GeForce RTX 4060, 8188 MiB · free VRAM 1992 MiB
- Python 3.13.15 · numpy 2.5.2

## What this run found

MASt3R-SfM could not run here: 0 image(s) in C:\Users\<you>\Videos\Capture; SfM needs at least 3 overlapping views; MASt3R weights/upstream not installed (C:\Users\<you>\OneDrive\Desktop\Programs\instasplatter\tools\sidecars\mast3r\.stub is present); the install steps are in C:\Users\<you>\OneDrive\Desktop\Programs\instasplatter\tools\sidecars\mast3r\README.md; only 1992 MiB of free VRAM measured; MASt3R-SfM needs about 6000 MiB.

## Full JSON

```json
{
  "detail": "MASt3R-SfM could not run here: 0 image(s) in C:\\Users\\ericc\\Videos\\Capture; SfM needs at least 3 overlapping views; MASt3R weights/upstream not installed (C:\\Users\\ericc\\OneDrive\\Desktop\\Programs\\instasplatter\\tools\\sidecars\\mast3r\\.stub is present); the install steps are in C:\\Users\\ericc\\OneDrive\\Desktop\\Programs\\instasplatter\\tools\\sidecars\\mast3r\\README.md; only 1992 MiB of free VRAM measured; MASt3R-SfM needs about 6000 MiB.",
  "environment": {
    "gpu": "NVIDIA GeForce RTX 4060, 8188 MiB",
    "gpu_free_mib": 1992,
    "numpy": "2.5.2",
    "platform": "Windows-11-10.0.26300-SP0",
    "python": "3.13.15",
    "when_utc": "2026-10-26T01:59:24+00:00"
  },
  "estimate": null,
  "images": "C:\\Users\\ericc\\Videos\\Capture",
  "mode": "mast3r",
  "preconditions_unmet": [
    "0 image(s) in C:\\Users\\ericc\\Videos\\Capture; SfM needs at least 3 overlapping views",
    "MASt3R weights/upstream not installed (C:\\Users\\ericc\\OneDrive\\Desktop\\Programs\\instasplatter\\tools\\sidecars\\mast3r\\.stub is present); the install steps are in C:\\Users\\ericc\\OneDrive\\Desktop\\Programs\\instasplatter\\tools\\sidecars\\mast3r\\README.md",
    "only 1992 MiB of free VRAM measured; MASt3R-SfM needs about 6000 MiB"
  ],
  "rerun": "python tools/bench/mast3r_sfm_bench.py --mast3r --images C:\\Users\\ericc\\Videos\\Capture --estimate <our sparse/0> --receipt docs/evidence/mast3r-bench/mast3r.md",
  "sidecar": "C:\\Users\\ericc\\OneDrive\\Desktop\\Programs\\instasplatter\\tools\\sidecars\\mast3r\\run.py",
  "status": "BLOCKED"
}
```
