# v0.3.0 draft smoke notes (not committed)

Local e2e targets (outputs stay under `%LOCALAPPDATA%/InstaSplatter/jobs` — never commit):

1. `DJI_20250623163523_0013_D.MP4`
2. `20250820_212300.mp4`
3. `VID_20220123_205403.MP4`

## Headless batch (agent / CI)

With `INSTASPLATTER_DEV=1`, write one path per line to
`%LOCALAPPDATA%/InstaSplatter/batch.txt` (or set `INSTASPLATTER_BATCH`), then
launch `instasplatter.exe`. Rust `setup` enqueues the batch and starts the first
GPU job without waiting on the WebView.

Suggested smoke settings for a fast gate (write to settings.json **without a UTF-8 BOM**,
or let the app save them; BOM used to make load fall back to Auto/High):

```json
{
  "preset": "draft",
  "denseInit": true,
  "progressiveResolution": true,
  "mipFilter": true,
  "maxFrames": 48,
  "totalSteps": 3000,
  "exportEvery": 750,
  "strictness": 0.45,
  "keepIntermediates": true
}
```

Full High/Max runs are the release quality target on RTX 4060 (~8 GB).
