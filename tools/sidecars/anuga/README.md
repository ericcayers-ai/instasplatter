# ANUGA flood compatibility scaffold

Isolated worker scaffold for exercising the ANUGA Python API
([ANUGA](https://github.com/GeoscienceAustralia/anuga_core), Apache-2.0).
**The current runner is not a scientific flood solver. Do not vendor GPL
solvers here.**

Installing ANUGA lets the compatibility smoke run; it does not enable an
authoritative result. The current path evolves a flat, fixed-stage bed with
reflective boundaries for at most 60 seconds. It does not sample the supplied
DEM, apply scenario rainfall/infiltration or configured boundary conditions,
or calibrate against observations. Its hydrograph and map products are display
scaffolds. Every run from this sidecar therefore reports `mode: scaffold`.

Install ANUGA into a local venv under this folder (or system Python) only to
exercise that compatibility path. InstaSplatter looks for launchers under:

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
| 0 | Success (labelled scaffold or demo) |
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
{"kind":"done","mode":"scaffold","resultPaths":["…"],"engineVersion":"…","label":"Scaffold — ... Not authoritative."}
```

`mode` is `"scaffold"` when ANUGA is installed, including when the bounded
flat-bed evolve smoke completes. `mode` is `"demo"` when ANUGA is not
importable and demo mode was allowed. Neither mode is scientifically
authoritative. This runner must not emit `mode: "anuga"` until real DEM
forcing, scenario boundary conditions, and calibration/validation are
implemented and tested.

## Pin

Target a known ANUGA release in your venv (document the exact git tag /
wheel in a local `PIN.txt` after install). The sidecar reports
`engineVersion` in the done payload when available; the version identifies
the imported package, not the authority of the generated output.
