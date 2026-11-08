/**
 * Standalone preview of the Visual Positioning System panel.
 *
 * Used for visual receipts (`docs/assets/verify/vps-panel/`). The panel only touches Tauri IPC in
 * `getGeoReference` (wrapped in a catch) and on Save, so it renders in a plain browser; the "Run
 * self-test" action exercises the whole localiser with synthetic data.
 */

import { createRoot } from "react-dom/client";
import VpsPanel from "../../src/geospatial/VpsPanel";

const el = document.getElementById("root")!;
createRoot(el).render(
  <div className="rounded-lg border border-edge bg-panel p-3">
    <h2 className="mb-2 text-sm font-semibold">Localise</h2>
    <VpsPanel />
  </div>,
);

// Let the capture script drive the self-test and know when the render settled.
(window as unknown as { __PANEL_READY__: boolean }).__PANEL_READY__ = true;
