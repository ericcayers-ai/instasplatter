# DESIGN.md — InstaSplatter shell

## Direction
Midnight capture bay. Teal pigment is the brand; warm brass marks live progress. Surfaces stay quiet so the viewport is the stage.

## Tokens
- Accent / brand: teal `#2fc4b0` (light: `#0e7a6d`)
- Progress / live: brass `#e0a34a` (replaces purple Trace)
- Neutrals: cool ink glass `#0a0c10` / `#11151c` / `#171c26`
- Light theme: near-pure `#f5f6f8` surfaces (not cream)
- Dock chrome: dense all-caps section labels (`0.12em` tracking), teal ghost controls, brass progress spark on the status ribbon

## Shell IA (option 2)
- **Icon rail** (left): Capture · Scene · Settings · Log — mirrors `Ctrl+B` / focus · `Ctrl+,` · `Ctrl+L`
- **Left dock**: CAPTURE / RECONSTRUCTION / SCENE (Geo: LAYERS)
- **Main viewport**: recon splat stage or Geo map/globe/ENU
- **Right dock**: VISUALIZATION / STATS / LOG (Geo: scenario inspector)
- **Status ribbon**: Live label + brass progress spark + %; detailed metrics live in STATS

## Navigation QOL
- Suite segmented control (Recon / Geo) in slim TitleBar with Ready/Live pill
- Export / Cancel / New live under left RECONSTRUCTION (not TitleBar)
- Home: brand-first hero DropZone; rail Capture focuses left CAPTURE
- Shortcuts: `Ctrl+B` left dock, `Ctrl+,` settings (recon → RECONSTRUCTION; geo → right inspector), `Ctrl+L` log, `Esc` dismiss overlays

## Motion
150–220ms ease-out. Loading uses dual pulse rings + shimmer progress (transform scaleX). Status spark uses a short brass pulse. Reduced-motion honored.
