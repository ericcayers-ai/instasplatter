# ANUGA scientific flood sidecar

Isolated, pinned worker for the Standard geospatial flood engine
([ANUGA](https://github.com/GeoscienceAustralia/anuga_core), Apache-2.0).
**Do not vendor GPL solvers here.**

Install ANUGA into a local venv under this folder (or system Python), then
delete any install markers that claim the engine is absent. InstaSplatter
looks for launchers under:

`%LOCALAPPDATA%/InstaSplatter/engines/sidecars/anuga/`

Repo copies under `tools/sidecars/anuga/` are the development contract and may
be copied into the engines tree.

## Protocol

- **In:** JSON request on stdin (schema version 1 — see below).
- **Out:** JSON lines on stdout (`progress` / `done` / `error`).
- **Files:** full-resolution grids + decimated GeoJSON checkpoints under
  `outputDir` (usually `workspace/geo/runs/<runId>/`).

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | Success (scientific or labelled demo) |
| 2 | Engine missing and `demoMode` was false |
| 3 | Invalid / invalid input |
| 1 | Unexpected failure |

## Request schema (v1)

```json
{
  "schemaVersion": 1,
  "runId": "run_…",
  "workspace": "…/jobs/geo_…",
  "outputDir": "…/geo/runs/run_…",
  "demoMode": false,
  "dem": {
    "path": "…/geo/derived/dtm_flood.tif",
    "crs": "local-ENU-m",
    "cellSizeM": 2.0
  },
  "extent": {
    "boundsEnu": [0, 0, 400, 300],
    "meshMaxAreaM2": 25.0,
    "regionalMeshMaxAreaM2": 200.0
  },
  "scenario": {
    "id": "draft-site-rain",
    "name": "Draft site rain",
    "durationHours": 12,
    "rainfall": { "rateMmPerHour": 25 },
    "inflows": null,
    "infiltration": { "rateMmPerHour": 2 },
    "roughness": { "manningN": 0.035 },
    "structures": null,
    "drains": null,
    "boundaryConditions": null,
    "solverSettings": { "cfl": 0.9 }
  },
  "ensemble": {
    "realizationIndex": 0,
    "totalRealizations": 1,
    "rainfallScale": 1.0,
    "roughnessScale": 1.0,
    "infiltrationScale": 1.0
  },
  "swmm": { "enabled": false, "networkPath": null },
  "checkpointEveryS": 600
}
```

## Response lines

```json
{"kind":"progress","progress":0.42,"detail":"t=4.0 h","checkpoint":"…/checkpoints/t014400.geojson","simTimeHours":4.0}
{"kind":"done","mode":"anuga","resultPaths":["…"],"massBalance":0.0012,"engineVersion":"…"}
```

`mode` is `"anuga"` for a real solve or `"demo"` when ANUGA is not importable
and demo mode was allowed. Demo output is **not** scientifically authoritative.

## Pin

Target a known ANUGA release in your venv (document the exact git tag /
wheel in a local `PIN.txt` after install). The sidecar reports
`engineVersion` in the done payload when available.
