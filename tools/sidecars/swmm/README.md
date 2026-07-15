# EPA SWMM coupling scaffold

Network / drainage exchange worker for the Standard flood suite.
Pairs with the ANUGA overland worker: depth/inflow at junctions ↔ surface
coupling terms.

**License:** EPA SWMM5 is public-domain. Do not bundle GPL network solvers.

## Protocol

JSON on stdin → JSON on stdout.

### Request

```json
{
  "schemaVersion": 1,
  "runId": "run_…",
  "outputDir": "…/geo/runs/run_…/swmm",
  "networkPath": "…/drains.inp",
  "surfaceExchange": {
    "fromAnuga": "…/checkpoints/latest_depth.geojson",
    "toAnuga": "…/swmm/outfalls.json"
  },
  "durationHours": 12,
  "demoMode": true
}
```

### Response

```json
{"kind":"done","mode":"stub","couplingPaths":[],"message":"SWMM launcher scaffold — install EPA SWMM and replace stub"}
```

Exit `2` when SWMM binaries are missing and `demoMode` is false.
