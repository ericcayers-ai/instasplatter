# Standard Cesium ion policy — static evidence

Verified 2026-07-17 from source (not Network tab):

- `src/geospatial/globe/ion.ts` — `blankCesiumIon()` sets `Ion.defaultAccessToken = ""`
- `applyCesiumIonPolicy(false)` never restores a token
- `CesiumGlobe.tsx` / `terrain.ts` call blank/policy before Viewer / World Terrain
- Experimental ion World Terrain only when Exp Mode + stored user token

Live Network assertion (zero `api.cesium.com`) remains on MANUAL-CAPTURE-CHECKLIST.
