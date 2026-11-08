#!/usr/bin/env node
/**
 * Visual receipt for the VPS panel.
 *
 * Starts Vite, renders `tools/bench/vps-panel.html` at three widths, clicks "Run self-test" once,
 * and writes PNGs to docs/assets/verify/vps-panel/. Always stops the dev server.
 *
 * The self-test result is labelled synthetic in the UI; this receipt proves the panel renders and the
 * workflow is walkable, not that a real photo was localised.
 */

import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import { mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const outDir = path.join(repoRoot, "docs", "assets", "verify", "vps-panel");
const port = 5198;

async function waitForServer(url, timeoutMs = 60_000) {
  const start = Date.now();
  for (;;) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      /* not up yet */
    }
    if (Date.now() - start > timeoutMs) throw new Error(`dev server did not start: ${url}`);
    await delay(250);
  }
}

const vite = spawn(
  process.platform === "win32" ? "npx.cmd" : "npx",
  ["vite", "--port", String(port), "--strictPort", "--host", "127.0.0.1"],
  { cwd: repoRoot, stdio: "ignore", shell: process.platform === "win32" },
);

let browser;
try {
  mkdirSync(outDir, { recursive: true });
  await waitForServer(`http://127.0.0.1:${port}/`);
  const { chromium } = await import("playwright");
  browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 420, height: 900 } });
  await page.goto(`http://127.0.0.1:${port}/tools/bench/vps-panel.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.__PANEL_READY__ === true, undefined, { timeout: 60_000 });
  await page.getByRole("button", { name: "Run self-test" }).click();
  await page.waitForSelector('[role="alert"], dl', { timeout: 10_000 }).catch(() => {});
  await delay(400);

  const widths = [
    { w: 1440, h: 900, name: "1440" },
    { w: 768, h: 900, name: "768" },
    { w: 390, h: 844, name: "390" },
  ];
  const layout = [];
  for (const v of widths) {
    await page.setViewportSize({ width: v.w, height: v.h });
    await delay(250);
    // Mechanical overflow check: the wrap width is fixed, so any horizontal scroll means a row
    // exceeded the container (the "390px / long content" state).
    const overflow = await page.evaluate(() => {
      const root = document.getElementById("root");
      return root ? root.scrollWidth - root.clientWidth : 0;
    });
    layout.push({ width: v.w, horizontalOverflowPx: overflow });
    const file = path.join(outDir, `vps-panel-${v.name}.png`);
    await page.screenshot({ path: file, fullPage: true });
    console.log(`wrote ${path.relative(repoRoot, file)}`);
  }
  console.log(JSON.stringify({ layout }, null, 2));

  const summary = await page.evaluate(() => {
    const dl = document.querySelector("dl");
    const alert = document.querySelector('[role="alert"]');
    return {
      hasResultGrid: !!dl,
      alert: alert ? alert.textContent : null,
      rows: dl ? [...dl.children].map((c) => c.textContent) : [],
    };
  });
  console.log(JSON.stringify(summary, null, 2));
} finally {
  if (browser) await browser.close().catch(() => {});
  if (process.platform === "win32") {
    spawn("taskkill", ["/pid", String(vite.pid), "/T", "/F"], { stdio: "ignore" });
  } else {
    vite.kill("SIGTERM");
  }
  await delay(400);
}
